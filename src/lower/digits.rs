//! A stored float32 written as text, left unwidened to keep its own digits.

use super::*;
use crate::types::Width;

/// `id` on a stored float32 it writes as text, left as stored to keep its own digits: `string` of
/// one, or `format` of a stored list or a list literal of nothing else.
pub(super) fn own_digits<'i>(id: &'i str, operands: &mut [Typed]) -> &'i str {
    match (id, operands) {
        ("double_to_string", [x]) => match unwidened(x) {
            Some(stored) if stored.ty == Type::Narrow(Width::F32) => {
                *x = stored;
                "float32_to_string"
            }
            _ => id,
        },
        ("string_format", [_, list]) => {
            let stored = match &list.node {
                Node::List(items) => items
                    .iter()
                    .map(unwidened)
                    .collect::<Option<Vec<_>>>()
                    .and_then(|items| {
                        let of = items.first()?.ty.clone();
                        Some(Typed::new(Type::list(of), Node::List(items)))
                    }),
                _ => unwidened(list),
            };
            if let Some(stored) = stored {
                *list = stored;
            }
            id
        }
        _ => id,
    }
}

/// What `t` reads, where it reads a float32 or a list of them as stored.
fn unwidened(t: &Typed) -> Option<Typed> {
    match &t.node {
        Node::Widen(stored) if float32s(&stored.ty) => Some((**stored).clone()),
        _ => None,
    }
}

/// Whether `ty` is a float32, or a list of them at any depth.
fn float32s(ty: &Type) -> bool {
    match ty {
        Type::Narrow(Width::F32) => true,
        Type::List(t) => float32s(t),
        _ => false,
    }
}
