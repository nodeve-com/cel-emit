//! Reads of a stored width ([`Node::Widen`]): widened where CEL computes, and kept as stored
//! where what is built from them doesn't depend on the width.

use super::{Node, Typed};
use crate::types::Type;

impl Typed {
    /// A read of a value stored as `ty`, as CEL computes it: widened where `ty` holds a width.
    pub fn stored(ty: Type, node: Node) -> Typed {
        if ty.narrow() {
            Typed::new(ty.cel(), Node::Widen(Box::new(Typed::new(ty, node))))
        } else {
            Typed::new(ty, node)
        }
    }

    /// What this reads, where that is stored as `ty`, so a place of `ty` takes it as it is.
    pub fn read_as(&self, ty: &Type) -> Option<&Typed> {
        match &self.node {
            Node::Widen(stored) if stored.ty == *ty => Some(stored),
            _ => None,
        }
    }

    /// The stored type each of `parts` reads, where every one is a read of that same type.
    pub fn read_alike<'t>(parts: impl IntoIterator<Item = &'t Typed>) -> Option<Type> {
        let mut alike: Option<Type> = None;
        for p in parts {
            let Node::Widen(stored) = &p.node else {
                return None;
            };
            match &alike {
                Some(t) if *t != stored.ty => return None,
                Some(_) => {}
                None => alike = Some(stored.ty.clone()),
            }
        }
        alike
    }

    /// What this reads as stored, where it widens a read; else itself.
    pub fn unwidened(self) -> Typed {
        match self.node {
            Node::Widen(stored) => *stored,
            _ => self,
        }
    }

    /// `make` over `parts`. Where every part reads one stored type `t`, `make` takes them as
    /// stored, and what it builds, of the stored type `of(t)`, is read widened once.
    pub fn over_stored<const N: usize>(
        parts: [Typed; N],
        of: impl FnOnce(Type) -> Type,
        make: impl FnOnce([Typed; N]) -> Node,
    ) -> Node {
        match Typed::read_alike(&parts) {
            Some(t) => {
                let built = make(parts.map(Typed::unwidened));
                Node::Widen(Box::new(Typed::new(of(t), built)))
            }
            None => make(parts),
        }
    }
}
