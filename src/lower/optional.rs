//! Optionals: a value that may be absent, lowered to an `Option` of its type. `optional.of`,
//! `none`, and the methods on one are calls cel-emit-runtime runs; `.?` and `[?]` select an optional
//! where a field, key, or index may hold nothing.

use super::has::{presence, select_field};
use super::parts::is_fields;
use super::*;

impl<'a> Lower<'a> {
    /// A call on an optional, if `name` is one: `optional.of`, `optional.none`,
    /// `optional.ofNonZeroValue`, or a method `hasValue`, `value`, `orValue`, or `or` on one.
    pub(crate) fn optional_form(
        &mut self,
        name: &str,
        receiver: Option<&'a SpannedExpr>,
        args: &'a [SpannedExpr],
    ) -> Result<Option<Node>, Error> {
        let node = match (name, receiver, args) {
            ("optional.of", None, [x]) => Node::Call("optional_of".into(), vec![self.expr(x)?]),
            ("optional.ofNonZeroValue", None, [x]) => self.of_non_zero(x)?,
            ("optional.none", None, []) => Node::None,
            ("hasValue", Some(o), []) if self.optional_type(o) => {
                Node::Call("optional_has_value".into(), vec![self.expr(o)?])
            }
            ("value", Some(o), []) if self.optional_type(o) => {
                Node::Call("optional_value".into(), vec![self.expr(o)?])
            }
            ("orValue", Some(o), [or]) if self.optional_type(o) => self.or_value(o, or)?,
            ("or", Some(o), [other]) if self.optional_type(o) => {
                let (o, other) = self.pair_as(o, other)?;
                Node::Call("optional_or_optional".into(), vec![o, other])
            }
            _ => return Ok(None),
        };
        Ok(Some(node))
    }

    /// `optional.ofNonZeroValue(x)`, for the types that have a zero.
    fn of_non_zero(&mut self, x: &'a SpannedExpr) -> Result<Node, Error> {
        let x = self.expr(x)?;
        let id = "optional_of_non_zero_value";
        if self.paths.path(id, std::slice::from_ref(&x.ty)).is_none() {
            return unsupported(format!("ofNonZeroValue of a {}", x.ty.display()));
        }
        Ok(Node::Call(id.into(), vec![x]))
    }

    /// `o.orValue(or)`, where the optional `o` holds the type of `or`.
    fn or_value(&mut self, o: &'a SpannedExpr, or: &'a SpannedExpr) -> Result<Node, Error> {
        let or = self.expr(or)?;
        let of = Type::Optional(Box::new(or.ty.clone()));
        let o = self.expr_as(o, Some(&of))?;
        let or = match &o.ty {
            Type::Optional(held) => super::dynamic::fits(or, held)?,
            _ => or,
        };
        if !matches!(&o.ty, Type::Optional(held) if **held == or.ty) {
            return unsupported("orValue across a dyn and another type");
        }
        Ok(Node::Call("optional_or_value".into(), vec![o, or]))
    }

    pub(crate) fn optional_type(&self, e: &SpannedExpr) -> bool {
        matches!(self.info.get_type(e.id), Some(CelType::Optional(_)))
    }

    /// The type an optional holds, of `e`, an optional.
    fn held(&self, e: &SpannedExpr) -> Result<Type, Error> {
        match self.ty(e)? {
            Type::Optional(t) => Ok(*t),
            _ => invalid("an optional of other than an optional"),
        }
    }

    /// `base.?field`: the field's value where it is present by proto3's rule, else absent. A
    /// name bound to a variable reads its presence from the embedder; a map's key from the map.
    pub(crate) fn optional_field(
        &mut self,
        e: &'a SpannedExpr,
        base: &'a SpannedExpr,
        field: &str,
    ) -> Result<Node, Error> {
        let held = self.held(e)?;
        if let Some(p) = self.path(e) {
            match self.found(&p) {
                Some(At::Constant(_)) => return unsupported("`.?` on a constant"),
                Some(At::Var(v)) => {
                    let present = Typed::new(Type::Bool, Node::Has(v));
                    return Ok(some_if(present, self.read_var(&p, v, held)));
                }
                None => {}
            }
        }
        if self.nullable_field(base, field) {
            return unsupported("`.?` on a nullable field");
        }
        if let Some(stored) = self.optional_stored(base)? {
            return self.select_optional(stored, field, held);
        }
        if let Some(value) = self.selected_field(e)?.filter(|v| !is_fields(v)) {
            return self.present(value);
        }
        let base = self.expr(base)?;
        self.select_optional(base, field, held)
    }

    /// Variable `v`, the declared name `path`, read as `held`: widened where it is stored at a
    /// width.
    fn read_var(&self, path: &str, v: usize, held: Type) -> Typed {
        match self.decls.names.get(path) {
            Some(name) if name.ty.narrow() => self.declared(path, v),
            _ => Typed::new(held, Node::Var(v)),
        }
    }

    /// `base.?field` on a lowered `base`, giving an optional of `held`: on an optional, its
    /// value's where it holds one, else absent.
    fn select_optional(&self, base: Typed, field: &str, held: Type) -> Result<Node, Error> {
        match base.ty.clone() {
            Type::Optional(inner) if matches!(*inner, Type::Message(_) | Type::Map(..)) => {
                let has = Typed::call("optional_has_value", vec![base.clone()], Type::Bool);
                let value = Typed::call("optional_value", vec![base], *inner);
                let ty = Type::Optional(Box::new(held.clone()));
                let some = Typed::new(ty.clone(), self.select_optional(value, field, held)?);
                Ok(Node::Cond(
                    Box::new(has),
                    Box::new(some),
                    Box::new(Typed::new(ty, Node::None)),
                ))
            }
            Type::Map(k, _) => {
                let key = super::dynamic::field_key(&k, field)?;
                Ok(Node::Call("optional_index_map".into(), vec![base, key]))
            }
            Type::Dyn => Ok(Node::Call(
                "dyn_optional_get".into(),
                vec![base, Typed::string(field)],
            )),
            Type::Optional(held) if *held == Type::Dyn => Ok(Node::Call(
                "dyn_chain".into(),
                vec![base, Typed::string(field)],
            )),
            Type::Message(fields) => self.present(select_field(base, &fields, field)?),
            _ => unsupported("`.?` on other than a name, a map, a message, or an optional of one"),
        }
    }

    /// `a.?b.c`: a selection the checker types optional because a `.?` before it did. Only a
    /// name the embedder reads answers, present where the variable holds a value, or a dotted
    /// constant the embedder declares optional.
    pub(crate) fn optional_chain(&mut self, e: &'a SpannedExpr) -> Result<Node, Error> {
        let held = self.held(e)?;
        let path = self.path(e);
        match path.as_deref().and_then(|p| self.found(p)) {
            Some(At::Var(v)) => Ok(some_if(
                Typed::new(Type::Bool, Node::Has(v)),
                self.read_var(path.as_deref().unwrap_or_default(), v, held),
            )),
            // A dotted name declared optional is itself.
            Some(At::Constant(c)) if c.ty == Type::Optional(Box::new(held)) => Ok(c.node),
            _ => unsupported("a selection after `.?` other than a name"),
        }
    }

    /// `base[?index]`: the element or entry where the index holds one, else absent.
    pub(crate) fn optional_index(
        &mut self,
        base: &'a SpannedExpr,
        index: &'a SpannedExpr,
    ) -> Result<Node, Error> {
        let (base, index) = (self.expr(base)?, self.expr(index)?);
        match base.ty.clone() {
            Type::List(_) => Ok(Node::Call("optional_index_list".into(), vec![base, index])),
            Type::Map(k, _) => {
                let index = super::dynamic::fits(index, &k)?;
                Ok(Node::Call("optional_index_map".into(), vec![base, index]))
            }
            Type::Dyn if index.ty == Type::String => {
                Ok(Node::Call("dyn_optional_get".into(), vec![base, index]))
            }
            _ => unsupported("`[?]` on other than a list or map"),
        }
    }

    /// `value` as an optional holding it where proto3 says it is present.
    fn present(&self, value: Typed) -> Result<Node, Error> {
        // A message's optional field is the optional itself.
        if matches!(value.ty, Type::Optional(_)) {
            return Ok(value.node);
        }
        let present = Typed::new(Type::Bool, presence(value.clone())?);
        Ok(some_if(present, value))
    }
}

/// A list literal of these items, flagged (`?`) where optional, whose element the checker gives
/// as `elem`.
pub(super) fn list(made: Vec<(Typed, bool)>, elem: &Type) -> Result<Typed, Error> {
    let elem = elem.clone();
    let held = Type::Optional(Box::new(elem.clone()));
    let mut items = Vec::with_capacity(made.len());
    for (x, optional) in made {
        let x = match optional {
            true if x.ty != held => return unsupported("an optional list element of another type"),
            true => x,
            false => super::dynamic::fits(x, &elem)?,
        };
        items.push((x, optional));
    }
    Ok(Typed::new(Type::list(elem), Node::ListOpt(items)))
}

/// `value` where `present` holds, else absent: an optional of `value`'s type.
fn some_if(present: Typed, value: Typed) -> Node {
    let ty = Type::Optional(Box::new(value.ty.clone()));
    Node::Cond(
        Box::new(present),
        Box::new(Typed::new(ty.clone(), Node::Some(Box::new(value)))),
        Box::new(Typed::new(ty, Node::None)),
    )
}
