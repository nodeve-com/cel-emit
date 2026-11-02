//! Nullable message fields: null or a value. A literal sets one to null as `None`; a value it
//! sets reads back as the value itself, and a read in an expression holds it as `Some`.

use super::*;

impl<'a> Lower<'a> {
    /// Whether `base.field` selects a nullable field of a message.
    pub(crate) fn nullable_field(&self, base: &SpannedExpr, field: &str) -> bool {
        match self.ty(base) {
            Ok(Type::Message(fields)) => fields
                .iter()
                .any(|f| f.name == field && matches!(f.ty, Type::Nullable(_))),
            _ => false,
        }
    }

    /// A message's field read as an expression: a value known to be set, held as `Some` where the
    /// checker types the read nullable.
    pub(crate) fn read(&self, e: &SpannedExpr, t: Typed) -> Node {
        match self.ty(e) {
            Ok(Type::Nullable(inner)) if *inner == t.ty => Node::Some(Box::new(t)),
            _ => t.node,
        }
    }
}

/// `value` filling a field of `ty`. For a nullable one, null is `None` and any other value is
/// itself. An optional one takes a value of its inner type.
pub(crate) fn fit(value: Typed, ty: &Type) -> Result<Typed, Error> {
    if value.ty == Type::Dyn {
        return match ty {
            Type::Nullable(_) | Type::Optional(_) => {
                unsupported("a dyn into a nullable or optional field")
            }
            _ => super::dynamic::filled(value, ty),
        };
    }
    if let Type::Optional(inner) = ty {
        return match &value.ty {
            t if t == ty => Ok(value),
            _ => crate::walk::cast(value, inner),
        };
    }
    let Type::Nullable(inner) = ty else {
        return crate::walk::cast(value, ty);
    };
    match &value.ty {
        Type::Null => Ok(Typed::new(ty.clone(), Node::None)),
        t if t == ty => Ok(value),
        _ => crate::walk::cast(value, inner),
    }
}

/// A comparison's `operands` with a nullable side's other side made nullable too: `null` is
/// `None`, a value is `Some`.
pub(crate) fn compare(operands: &mut [Typed]) {
    let [a, b] = operands else { return };
    let (nullable, other) = match (&a.ty, &b.ty) {
        (Type::Nullable(_), _) => (a.ty.clone(), b),
        (_, Type::Nullable(_)) => (b.ty.clone(), a),
        _ => return,
    };
    let Type::Nullable(inner) = &nullable else {
        return;
    };
    if other.ty == Type::Null {
        *other = Typed::new(nullable, Node::None);
    } else if other.ty == **inner {
        let value = other.clone();
        *other = Typed::new(nullable, Node::Some(Box::new(value)));
    }
}
