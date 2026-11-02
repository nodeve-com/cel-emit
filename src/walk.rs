//! Walks over a typed tree: the variables it reads or tests, each node, a cast.

use crate::tree::{Node, Object, Out, Typed};
use crate::types::Type;
use crate::Error;

/// A node of type `to` from `x`, as a declared type casts a result: an int to a double, a value
/// to the stored type of its CEL type; any other change is a mistake in the declarations.
pub fn cast(x: Typed, to: &Type) -> Result<Typed, Error> {
    match (&x.ty, to) {
        (from, to) if from == to => Ok(x),
        (Type::Int, Type::Double) => Ok(Typed::call("int_to_double", vec![x], Type::Double)),
        (from, to) if to.narrow() && *from == to.cel() => {
            Ok(Typed::new(to.clone(), Node::Store(Box::new(x))))
        }
        (from, to) => Err(Error::Invalid(format!(
            "a {} result for datatype {}",
            from.display(),
            to.display()
        ))),
    }
}

/// Every variable `t` reads, in order.
pub fn vars(t: &Typed) -> Vec<usize> {
    let mut out = Vec::new();
    nodes(t, &mut |n| {
        if let Node::Var(v) | Node::Has(v) = n.node {
            out.push(v)
        }
    });
    out
}

/// Every variable `t` tests with [`Node::Has`]: it computes without them.
pub fn tested(t: &Typed) -> Vec<usize> {
    let mut out = Vec::new();
    nodes(t, &mut |n| {
        if let Node::Has(v) = n.node {
            out.push(v)
        }
    });
    out
}

/// Every local `t` reads.
pub fn locals(t: &Typed) -> Vec<usize> {
    let mut out = Vec::new();
    nodes(t, &mut |n| {
        if let Node::Local(l) = n.node {
            out.push(l)
        }
    });
    out
}

/// Whether `f` holds for `t` or a node under it.
pub fn any(t: &Typed, f: impl Fn(&Node) -> bool) -> bool {
    let mut found = false;
    nodes(t, &mut |n| found = found || f(&n.node));
    found
}

/// `f` on `t` and each node under it, parents first, in order.
pub fn nodes(t: &Typed, f: &mut impl FnMut(&Typed)) {
    f(t);
    match &t.node {
        Node::Int(_)
        | Node::Uint(_)
        | Node::Double(_)
        | Node::Bool(_)
        | Node::Str(_)
        | Node::Bytes(_)
        | Node::Null
        | Node::None
        | Node::Timestamp(..)
        | Node::Duration(..)
        | Node::Var(_)
        | Node::Has(_)
        | Node::Item(..)
        | Node::Position(_)
        | Node::Local(_)
        | Node::Fail(_) => {}
        Node::Field(x, _)
        | Node::Within(x, _)
        | Node::Store(x)
        | Node::Widen(x)
        | Node::FromDyn(x, _)
        | Node::Shaped(x, ..)
        | Node::ToDyn(x)
        | Node::Written(x, _)
        | Node::Matches(x, ..)
        | Node::Extreme(_, x)
        | Node::Sum(x)
        | Node::Count(x)
        | Node::Some(x) => nodes(x, f),
        Node::ListOpt(items) => items.iter().for_each(|(a, _)| nodes(a, f)),
        Node::MapOpt(entries) => entries.iter().for_each(|(k, v, _)| {
            nodes(k, f);
            nodes(v, f);
        }),
        Node::And(a, b)
        | Node::Or(a, b)
        | Node::Map(a, b)
        | Node::All(_, a, b)
        | Node::First(a, b)
        | Node::Let(_, a, b) => {
            nodes(a, f);
            nodes(b, f);
        }
        Node::Cond(c, a, b) | Node::Keep(c, a, b) => {
            nodes(c, f);
            nodes(a, f);
            nodes(b, f);
        }
        Node::Call(_, args) | Node::List(args) => args.iter().for_each(|a| nodes(a, f)),
        Node::Message(entries) => entries.iter().for_each(|(_, v, _)| nodes(v, f)),
        Node::MapOf(entries) => entries.iter().for_each(|(k, v)| {
            nodes(k, f);
            nodes(v, f);
        }),
        Node::Encode(object) => members(object, f),
    }
}

/// `f` on each node of `object`'s members, as [`nodes`] walks them.
fn members(object: &Object, f: &mut impl FnMut(&Typed)) {
    for (_, value) in &object.lets {
        nodes(value, f);
    }
    for m in &object.members {
        if let Some(present) = &m.present {
            nodes(present, f);
        }
        match &m.out {
            Out::Value(x, _) | Out::Rest(x, _) => nodes(x, f),
            Out::Optional {
                optional, value, ..
            } => {
                nodes(optional, f);
                nodes(value, f);
            }
            Out::Object(o) => members(o, f),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Width;

    #[test]
    fn a_value_casts_to_the_stored_type_of_its_cel_type() {
        let stored = Type::list(Type::Narrow(Width::F32));
        let list = Typed::new(Type::list(Type::Double), Node::List(Vec::new()));
        let made = cast(list, &stored).unwrap();
        assert_eq!(made.ty, stored);
        assert!(matches!(made.node, Node::Store(_)));
        assert!(cast(Typed::string("a"), &stored).is_err());
    }
}
