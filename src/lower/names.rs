//! Names: declared, constant, a comprehension's, or a message's field by its address.

use super::parts::is_fields;
use super::scope::Scoped;
use super::*;

impl<'a> Lower<'a> {
    /// A declared name, dotted or not; a constant; a comprehension's element, index, key, or
    /// message field; or a field of a message by its address.
    pub(crate) fn name(&mut self, e: &'a SpannedExpr) -> Result<Node, Error> {
        let path = self.path(e);
        let root = path.iter().flat_map(|p| p.split('.')).next();
        let by_bind = matches!(root.map(|r| self.local(r)), Some(Some(Scoped::Bind(_))));
        if !by_bind {
            if let Some(node) = self.loop_name(path.as_deref())? {
                return Ok(node);
            }
            if let Some(node) = path.as_deref().and_then(|p| self.declared_name(p)) {
                return Ok(node);
            }
        }
        // A field of a message a literal or a bound name gives.
        if let Some(v) = self.selected_field(e)? {
            return match is_fields(&v) {
                false => Ok(self.read(e, v)),
                true => Ok(super::message::whole(v)?.node),
            };
        }
        // A field of a message a value gives: a list's element, say.
        match &e.node {
            Expr::Member { expr, field, .. } => self.value_field(expr, field),
            _ => unsupported("field access on a value"),
        }
    }

    /// The comprehension's index, key, or element that `path` names, innermost first, or a field
    /// of the element.
    fn loop_name(&self, path: Option<&str>) -> Result<Option<Node>, Error> {
        let mut parts = path.iter().flat_map(|p| p.split('.'));
        let (root, field) = (parts.next(), parts.next());
        if root.is_none() {
            return Ok(None);
        }
        for (n, l) in self.loops.iter().enumerate().rev() {
            if root == l.index.as_deref() && field.is_none() {
                return Ok(Some(match &l.over {
                    Over::List(_) => Node::Position(n),
                    Over::Map(key, _) => Typed::stored(key.clone(), Node::Item(n, Part::Key)).node,
                }));
            }
            if root == l.item.as_deref() {
                let (ty, part) = loop_item(&l.over);
                let item = Typed::stored(ty.clone(), Node::Item(n, part));
                let rest: Vec<&str> = path.iter().flat_map(|p| p.split('.')).skip(1).collect();
                return self.fields(item, &rest).map(Some);
            }
        }
        Ok(None)
    }

    /// The constant or variable the name `path` declares.
    fn declared_name(&self, path: &str) -> Option<Node> {
        match self.found(path)? {
            At::Constant(c) => Some(c.node),
            At::Var(v) => Some(self.declared(path, v).node),
        }
    }

    /// `field` of the message, map, or dyn that `expr` gives.
    fn value_field(&mut self, expr: &'a SpannedExpr, field: &str) -> Result<Node, Error> {
        let base = self.expr(expr)?;
        match &base.ty {
            Type::Message(_) | Type::Nullable(_) => self.fields(base, &[field]),
            // A string-keyed map's field is its entry: a key it lacks is no such key.
            Type::Map(k, _) if matches!(**k, Type::String | Type::Dyn) => {
                let key = super::dynamic::field_key(k, field)?;
                Ok(Node::Call("index_map".into(), vec![base, key]))
            }
            Type::Dyn => {
                let key = Typed::string(field);
                Ok(Node::Call("dyn_get".into(), vec![base, key]))
            }
            _ => unsupported("field access on a value"),
        }
    }

    /// `path` of fields into `message`, a message by its address.
    pub(crate) fn fields(&self, message: Typed, path: &[&str]) -> Result<Node, Error> {
        let Some((field, rest)) = path.split_first() else {
            return Ok(message.node);
        };
        if message.ty == Type::Dyn {
            let key = Typed::string(field);
            let read = Node::Call("dyn_get".into(), vec![message, key]);
            return self.fields(Typed::new(Type::Dyn, read), rest);
        }
        // A message that may lack a value has no field to read where it does: the read fails.
        let message = match message.ty.clone() {
            Type::Optional(held) | Type::Nullable(held) => {
                Typed::call("optional_held", vec![message], *held)
            }
            _ => message,
        };
        let fields = message.ty.fields();
        let i = fields
            .iter()
            .position(|f| f.name == *field)
            .ok_or_else(|| Error::Invalid(format!("no field {field}")))?;
        let ty = fields.get(i).map(|f| f.ty.clone()).unwrap_or(Type::Int);
        let field = Typed::stored(ty, Node::Field(Box::new(message), i));
        // An optional field read plainly is its value, or absent.
        if let (Type::Optional(_), true) = (&field.ty, rest.is_empty()) {
            return Ok(Node::Call("optional_value".into(), vec![field]));
        }
        self.fields(field, rest)
    }

    /// The declared name `path`, variable `v`, read: widened where it is stored at a width.
    pub(crate) fn declared(&self, path: &str, v: usize) -> Typed {
        let ty = self.decls.names.get(path).map(|n| n.ty.clone());
        Typed::stored(ty.unwrap_or(Type::Int), Node::Var(v))
    }

    /// `e` as a dotted name, `.?` read as `.`: `prev.?Smooth.watts` is `prev.Smooth.watts`.
    pub(crate) fn path(&self, e: &SpannedExpr) -> Option<String> {
        match &e.node {
            Expr::Ident(name) => Some(name.clone()),
            Expr::Member { expr, field, .. } | Expr::MemberTestOnly { expr, field } => {
                Some(format!("{}.{field}", self.path(expr)?))
            }
            _ => None,
        }
    }

    /// What a declared name is.
    pub(crate) fn found(&self, path: &str) -> Option<At> {
        self.decls.names.get(path).map(|n| n.at.clone())
    }
}

/// The type and part of a comprehension's element, of a list or map.
fn loop_item(over: &Over) -> (&Type, Part) {
    match over {
        Over::List(ty) => (ty, Part::Whole),
        Over::Map(_, ty) => (ty, Part::Value),
    }
}
