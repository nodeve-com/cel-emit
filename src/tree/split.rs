//! A message-typed tree as its fields, for an embedder that lays a message out field by field.

use super::{Node, Typed};
use crate::types::Type;

/// A message's fields, and the locals they read.
#[derive(Clone, Debug, PartialEq)]
pub struct Split {
    /// Bind each around every field, outermost first: a value computed once, however many
    /// fields read it.
    pub lets: Vec<(usize, Typed)>,
    /// Each field by name, typed as the field. A flagged (`?`) entry of a struct literal stays
    /// an optional, absent where the field is unset.
    pub fields: Vec<(String, Typed)>,
}

/// `t` without the binds around it, and those binds, outermost first: each a value computed once,
/// however much of `t` reads it.
pub fn unbound(t: Typed) -> (Vec<(usize, Typed)>, Typed) {
    let mut lets = Vec::new();
    let mut body = t;
    while let Node::Let(n, value, rest) = body.node {
        lets.push((n, *value));
        body = *rest;
    }
    (lets, body)
}

/// `t`, a message, as its fields. A struct literal gives the fields it sets; a [`Node::Let`]
/// around a message hoists its binding into `lets`; a dyn read as a message reads each field from
/// the dyn's key. Any other message gives every field it declares, each a [`Node::Field`] of it.
/// A tree of other than a message gives none.
pub fn split(t: Typed) -> Split {
    let Type::Message(declared) = t.ty.clone() else {
        return Split {
            lets: Vec::new(),
            fields: Vec::new(),
        };
    };
    let name = |i: usize| declared.get(i).map(|f| f.name.clone());
    let (lets, t) = unbound(t);
    match t.node {
        Node::Message(entries) => Split {
            lets,
            fields: entries
                .into_iter()
                .filter_map(|(i, v, _)| Some((name(i)?, v)))
                .collect(),
        },
        Node::FromDyn(x, _) => Split {
            lets,
            fields: declared
                .iter()
                .map(|f| {
                    let key = Typed::string(&f.name);
                    let got = Typed::call("dyn_get", vec![(*x).clone(), key], Type::Dyn);
                    let read = Node::FromDyn(Box::new(got), f.ty.clone());
                    (f.name.clone(), Typed::new(f.ty.clone(), read))
                })
                .collect(),
        },
        node => {
            let whole = Typed { node, ..t };
            Split {
                lets,
                fields: declared
                    .iter()
                    .enumerate()
                    .map(|(i, f)| {
                        let read = Node::Field(Box::new(whole.clone()), i);
                        (f.name.clone(), Typed::stored(f.ty.clone(), read))
                    })
                    .collect(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Field;

    fn message() -> Type {
        let field = |name: &str| Field {
            name: name.into(),
            ty: Type::Int,
            required: false,
        };
        Type::Message(vec![field("a"), field("b")])
    }

    #[test]
    fn a_literal_gives_the_fields_it_sets() {
        let t = Typed::new(message(), Node::Message(vec![(1, Typed::int(7), false)]));
        let s = split(t);
        assert_eq!(s.fields, vec![("b".to_string(), Typed::int(7))]);
        assert!(s.lets.is_empty());
    }

    #[test]
    fn a_let_around_a_literal_hoists_its_binding() {
        let literal = Typed::new(message(), Node::Message(vec![(0, Typed::int(1), false)]));
        let t = Typed::new(
            message(),
            Node::Let(3, Box::new(Typed::int(5)), Box::new(literal)),
        );
        let s = split(t);
        assert_eq!(s.lets, vec![(3, Typed::int(5))]);
        assert_eq!(s.fields.len(), 1);
    }

    #[test]
    fn a_dyn_read_as_a_message_reads_each_field_from_its_key() {
        let x = Typed::new(Type::Dyn, Node::Local(0));
        let t = Typed::new(message(), Node::FromDyn(Box::new(x), message()));
        let names: Vec<_> = split(t).fields.into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["a", "b"]);
    }

    #[test]
    fn any_other_message_gives_every_declared_field() {
        let t = Typed::new(message(), Node::Local(0));
        assert_eq!(split(t).fields.len(), 2);
    }
}
