//! Walks over a typed tree: the variables it reads or tests, each node, a cast.

use crate::tree::{Node, Object, Out, Typed};
use crate::types::Type;
use crate::Error;

/// A pattern for the nodes with a literal's Rust and no child.
macro_rules! literal {
    () => {
        Node::Int(_)
            | Node::Uint(_)
            | Node::Double(_)
            | Node::Bool(_)
            | Node::Str(_)
            | Node::Bytes(_)
            | Node::Null
            | Node::Timestamp(..)
            | Node::Duration(..)
            | Node::Position(_)
    };
}
pub(crate) use literal;

/// A pattern for the nodes with no child.
macro_rules! leaf {
    () => {
        $crate::walk::literal!()
            | Node::None
            | Node::Var(_)
            | Node::Has(_)
            | Node::Item(..)
            | Node::Local(_)
            | Node::Fail(_)
    };
}
pub(crate) use leaf;

/// A pattern for the nodes over one child, `$x`, that cost nothing past reading it.
macro_rules! thin {
    ($x:ident) => {
        Node::Field($x, _)
            | Node::Within($x, _)
            | Node::Store($x)
            | Node::Widen($x)
            | Node::FromDyn($x, _)
            | Node::ToDyn($x)
            | Node::Some($x)
    };
}
pub(crate) use thin;

/// A pattern for the nodes that read a place, call, or test.
macro_rules! access {
    () => {
        Node::Var(_)
            | Node::Local(_)
            | Node::Item(..)
            | Node::Field(..)
            | Node::First(..)
            | Node::Has(_)
            | Node::Call(..)
            | Node::Matches(..)
    };
}
pub(crate) use access;

/// A pattern for the nodes that choose, bind, or fail.
macro_rules! control {
    () => {
        Node::And(..) | Node::Or(..) | Node::Cond(..) | Node::Let(..) | Node::Fail(_)
    };
}
pub(crate) use control;

/// A pattern for the nodes that change a value between forms or write it as JSON.
macro_rules! converting {
    () => {
        Node::Within(..)
            | Node::Store(_)
            | Node::Widen(_)
            | Node::FromDyn(..)
            | Node::Shaped(..)
            | Node::ToDyn(_)
            | Node::Written(..)
            | Node::Encode(_)
    };
}
pub(crate) use converting;

/// A pattern for the comprehensions and the folds over them.
macro_rules! iterating {
    () => {
        Node::Map(..)
            | Node::Keep(..)
            | Node::All(..)
            | Node::Extreme(..)
            | Node::Sum(_)
            | Node::Count(_)
    };
}
pub(crate) use iterating;

/// A pattern for the optionals and the list, map, and message literals.
macro_rules! collecting {
    () => {
        Node::Some(_)
            | Node::None
            | Node::ListOpt(_)
            | Node::MapOpt(_)
            | Node::Message(_)
            | Node::List(_)
            | Node::MapOf(_)
    };
}
pub(crate) use collecting;

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
        leaf!() => {}
        thin!(x)
        | Node::Shaped(x, ..)
        | Node::Written(x, _)
        | Node::Matches(x, ..)
        | Node::Extreme(_, x)
        | Node::Sum(x)
        | Node::Count(x) => nodes(x, f),
        Node::ListOpt(items) => items.iter().for_each(|(a, _)| nodes(a, f)),
        Node::MapOpt(entries) => entries.iter().for_each(|(k, v, _)| each([k, v], f)),
        Node::And(a, b)
        | Node::Or(a, b)
        | Node::Map(a, b)
        | Node::All(_, a, b)
        | Node::First(a, b)
        | Node::Let(_, a, b) => each([a, b], f),
        Node::Cond(c, a, b) | Node::Keep(c, a, b) => each([c, a, b], f),
        Node::Call(_, args) | Node::List(args) => each(args, f),
        Node::Message(entries) => entries.iter().for_each(|(_, v, _)| nodes(v, f)),
        Node::MapOf(entries) => entries.iter().for_each(|(k, v)| each([k, v], f)),
        Node::Encode(object) => members(object, f),
    }
}

/// `nodes` on each of `xs`, in order.
fn each<X: AsRef<Typed>>(xs: impl IntoIterator<Item = X>, f: &mut impl FnMut(&Typed)) {
    xs.into_iter().for_each(|x| nodes(x.as_ref(), f));
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
