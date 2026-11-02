//! Dyn: where a value of no one kind meets a typed place. A dyn operand is read as the kind its
//! place declares ([`Node::FromDyn`]) only for a string or bool, where CEL's own dispatch on the
//! value's kind agrees; a number, arithmetic, ordering, and a loop over a dyn are refused. A
//! typed value entering a dyn place is [`Node::ToDyn`].

use super::*;

/// `x` in a place of type `want`: a typed value entering dyn, or a list or map of it, is
/// [`Node::ToDyn`]. Any other mismatch is left to the caller.
pub(crate) fn fits(x: Typed, want: &Type) -> Result<Typed, Error> {
    let dyn_in = |ty: &Type| match (&x.ty, ty) {
        (_, Type::Dyn) => x.ty != Type::Dyn,
        (Type::List(a), Type::List(b)) => **b == Type::Dyn && **a != Type::Dyn,
        (Type::Map(k, a), Type::Map(l, b)) => k == l && **b == Type::Dyn && **a != Type::Dyn,
        _ => false,
    };
    if dyn_in(want) {
        if !x.ty.dyn_into(want) {
            return unsupported(format!("a {} as {}", x.ty.display(), want.display()));
        }
        return Ok(Typed::new(want.clone(), Node::ToDyn(Box::new(x))));
    }
    Ok(x)
}

/// `t` typed by what its call gives on its operands, where one holds a dyn: the checker, which
/// joins a dyn with a typed operand to the typed one, may type the result narrower than the dyn
/// it is.
pub(crate) fn retype(t: Typed) -> Typed {
    let Node::Call(id, args) = &t.node else {
        return t;
    };
    if !args.iter().any(|a| a.ty.has_dyn()) {
        return t;
    }
    let first = args.first().map(|a| &a.ty);
    let ty = match (id.as_str(), first, args.get(1)) {
        ("index_list", Some(Type::List(of)), _) => Some((**of).clone()),
        ("index_map", Some(Type::Map(_, of)), _) => Some((**of).clone()),
        ("add_list_list" | "optional_or_optional", Some(list), _) => Some(list.clone()),
        ("optional_or_value", _, Some(or)) => Some(or.ty.clone()),
        ("optional_value", Some(Type::Optional(held)), _) => Some((**held).clone()),
        ("optional_of", Some(held), _) => Some(Type::Optional(Box::new(held.clone()))),
        ("optional_index_list", Some(Type::List(of)), _)
        | ("optional_index_map", Some(Type::Map(_, of)), _) => Some(Type::Optional(of.clone())),
        _ => None,
    };
    match ty {
        Some(ty) => Typed { ty, ..t },
        None => t,
    }
}

/// The key `field` is, in a map of `key`s: a string, or a dyn that holds one.
pub(crate) fn field_key(key: &Type, field: &str) -> Result<Typed, Error> {
    fits(Typed::string(field), key)
}

/// A dyn `x` filling a field of type `want`, as CEL does: each kind takes only its own, an int
/// field no double and a double field no int.
pub(crate) fn filled(x: Typed, want: &Type) -> Result<Typed, Error> {
    if x.ty != Type::Dyn || *want == Type::Dyn {
        return Ok(x);
    }
    match want {
        Type::Int | Type::Uint | Type::Double | Type::Bool | Type::String => Ok(Typed::new(
            want.clone(),
            Node::FromDyn(Box::new(x), want.clone()),
        )),
        other => unsupported(format!("a dyn filling {}", other.display())),
    }
}

/// A dyn `x` as an operand of type `want`: a string or bool only.
fn operand(x: Typed, want: &Type, id: &str) -> Result<Typed, Error> {
    match (&x.ty, want) {
        (Type::Dyn, Type::String | Type::Bool) => Ok(Typed::new(
            want.clone(),
            Node::FromDyn(Box::new(x), want.clone()),
        )),
        (Type::Dyn, Type::Dyn) => Ok(x),
        (Type::Dyn, other) => unsupported(format!("{id} of a dyn read as {}", other.display())),
        (_, Type::Dyn) => fits(x, want),
        _ => Ok(x),
    }
}

/// `x` read as a bool, where it is a dyn.
pub(crate) fn boolean(x: Typed) -> Result<Typed, Error> {
    operand(x, &Type::Bool, "a condition")
}

impl<'a> Lower<'a> {
    /// Whether the checker types `e` dyn.
    pub(crate) fn dyn_typed(&self, e: &SpannedExpr) -> bool {
        matches!(self.info.get_type(e.id), Some(CelType::Dyn))
    }

    /// `e`'s type, when the checker settled it: none holding dyn.
    pub(crate) fn settled(&self, e: &SpannedExpr) -> Option<Type> {
        self.ty(e).ok().filter(|t| !t.has_dyn())
    }

    /// `e` as a bool.
    pub(crate) fn sub_bool(&mut self, e: &'a SpannedExpr) -> Result<Box<Typed>, Error> {
        boolean(self.expr(e)?).map(Box::new)
    }

    /// An index or `in` over a dyn.
    pub(crate) fn dyn_overload(&mut self, e: &'a SpannedExpr) -> Result<Option<Node>, Error> {
        match &e.node {
            Expr::Index {
                expr,
                index,
                optional: false,
            } if self.dyn_typed(expr) => {
                let (base, index) = (self.expr(expr)?, self.expr(index)?);
                let id = match index.ty {
                    Type::String => "dyn_get",
                    Type::Int => "dyn_at",
                    _ => return unsupported("a dyn indexed by other than a string or int"),
                };
                Ok(Some(Node::Call(id.into(), vec![base, index])))
            }
            Expr::Binary {
                op: BinaryOp::In,
                left,
                right,
            } if self.dyn_typed(left) || self.dyn_typed(right) => {
                let (x, container) = (self.expr(left)?, self.expr(right)?);
                let (x, container) = (fits(x, &Type::Dyn)?, fits(container, &Type::Dyn)?);
                Ok(Some(Node::Call("dyn_contains".into(), vec![x, container])))
            }
            _ => Ok(None),
        }
    }

    /// A call the checker resolves on a dyn to several overloads: `size`, the casts, and `dyn()`.
    pub(crate) fn dyn_form(
        &mut self,
        name: &str,
        receiver: Option<&'a SpannedExpr>,
        args: &'a [SpannedExpr],
    ) -> Result<Option<Node>, Error> {
        let node = match (name, receiver, args) {
            ("size", Some(x), []) | ("size", None, [x]) if self.dyn_typed(x) => {
                Node::Call("dyn_size".into(), vec![self.expr(x)?])
            }
            ("int" | "uint" | "double" | "string" | "bool", None, [x]) if self.dyn_typed(x) => {
                let x = self.expr(x)?;
                // An operand lowered to a typed value (a `math` extreme over agreeing operands)
                // is cast by its own type's overload.
                let id = match &x.ty {
                    Type::Dyn => format!("dyn_to_{name}"),
                    ty @ (Type::Int | Type::Uint | Type::Double) => {
                        format!("{}_to_{name}", ty.display())
                    }
                    ty => return unsupported(format!("{name} of a {}", ty.display())),
                };
                Node::Call(id, vec![x])
            }
            (
                "flatten" | "slice" | "reverse" | "first" | "last" | "distinct" | "sort",
                Some(x),
                _,
            ) if self.dyn_typed(x) => return self.dyn_list_method(name, x, args),
            ("dyn", None, [x]) => {
                let x = fits(self.expr(x)?, &Type::Dyn)?;
                x.node
            }
            _ => return Ok(None),
        };
        Ok(Some(node))
    }

    /// A list method called on a dyn: the receiver read as a list of dyn, which is a cast error
    /// where it is none, then the list overload. The checker leaves `reverse` ambiguous with
    /// the string one on a dyn, so the overload is picked here by name.
    fn dyn_list_method(
        &mut self,
        name: &str,
        receiver: &'a SpannedExpr,
        args: &'a [SpannedExpr],
    ) -> Result<Option<Node>, Error> {
        let id = match (name, args.len()) {
            ("flatten", 1) => "list_flatten_depth".to_string(),
            _ => format!("list_{name}"),
        };
        let list = Typed::call(
            "dyn_list",
            vec![self.expr(receiver)?],
            Type::list(Type::Dyn),
        );
        let mut operands = vec![list];
        for arg in args {
            operands.push(operand(self.expr(arg)?, &Type::Int, &id)?);
        }
        Ok(Some(Node::Call(id, operands)))
    }

    /// The operands of the overload `id`, each fitted to the type it declares where dyn meets
    /// a typed place.
    pub(crate) fn settle(&self, id: &str, operands: &mut [Typed]) -> Result<(), Error> {
        match (id, operands) {
            ("equals" | "not_equals", [a, b]) => {
                if a.ty == Type::Dyn || b.ty == Type::Dyn {
                    *a = fits(a.clone(), &Type::Dyn)?;
                    *b = fits(b.clone(), &Type::Dyn)?;
                }
                if a.ty != b.ty && (a.ty.has_dyn() || b.ty.has_dyn()) {
                    return unsupported("a comparison of a dyn with other than a dyn");
                }
            }
            (
                "in_list",
                [x, Typed {
                    ty: Type::List(of), ..
                }],
            ) => {
                *x = fits(x.clone(), of)?;
            }
            (
                "in_map",
                [x, Typed {
                    ty: Type::Map(k, _),
                    ..
                }],
            )
            | (
                "index_map" | "optional_index_map",
                [Typed {
                    ty: Type::Map(k, _),
                    ..
                }, x],
            ) => {
                *x = fits(x.clone(), k)?;
            }
            ("add_list_list", [a, b]) if a.ty != b.ty => {
                *b = fits(b.clone(), &a.ty)?;
                *a = fits(a.clone(), &b.ty)?;
                if a.ty != b.ty {
                    return unsupported("lists of different types");
                }
            }
            (_, operands) => {
                let Some(params) = self.params.get(id) else {
                    return Ok(());
                };
                for (x, p) in operands.iter_mut().zip(params) {
                    let want = of_cel(p, &self.messages);
                    *x = match want {
                        Some(want) => operand(x.clone(), &want, id)?,
                        None if x.ty == Type::Dyn => {
                            return unsupported(format!("{id} of a dyn"));
                        }
                        None => x.clone(),
                    };
                }
            }
        }
        Ok(())
    }
}
