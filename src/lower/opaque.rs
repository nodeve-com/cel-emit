//! An opaque value ([`Type::Opaque`]): an expression moves it and never reads into it. The
//! checker refuses most reads, as no overload takes one. What it lets through is refused here:
//! an overload of any type reading its argument, and equality, against null too.

use super::*;

/// The overloads handing an argument on unread: an optional made of it, or its fallback.
const MOVES: &[&str] = &["optional_of", "optional_or_value"];

impl<'a> Lower<'a> {
    /// The operands of `e`, which the checker resolved to overload `id`: refused where one of
    /// cel-emit's own functions would read an opaque one. An embedder's function takes what it
    /// declares.
    pub(crate) fn moved(&mut self, e: &'a SpannedExpr, id: &str) -> Result<Vec<Typed>, Error> {
        let operands = self.operands(e)?;
        if self.paths.own(id) {
            return Ok(operands);
        }
        let read = operands.iter().find_map(|o| match &o.ty {
            Type::Opaque(_) if !MOVES.contains(&id) => Some(o.ty.display()),
            ty if crate::stdlib::uses_equality(id) => held(ty),
            _ => None,
        });
        match read {
            Some(opaque) => unsupported(format!(
                "{} reads an {opaque}, which an expression only moves",
                self.unresolved(e)
            )),
            None => Ok(operands),
        }
    }
}

/// Refuses a choice between `a` and `b` of two types where either holds an opaque one: the
/// checker lets an opaque type meet another.
pub(crate) fn alike(a: &Typed, b: &Typed) -> Result<(), Error> {
    match held(&a.ty).or_else(|| held(&b.ty)) {
        Some(opaque) if a.ty.cel() != b.ty.cel() => unsupported(format!(
            "a choice between an {opaque} and a value of another type"
        )),
        _ => Ok(()),
    }
}

/// The opaque type `ty` holds, anywhere in it.
fn held(ty: &Type) -> Option<String> {
    match ty {
        Type::Opaque(_) => Some(ty.display()),
        Type::List(t) | Type::Optional(t) | Type::Nullable(t) => held(t),
        Type::Map(k, v) => held(k).or_else(|| held(v)),
        Type::Message(m) => m.fields.iter().find_map(|f| held(&f.ty)),
        _ => None,
    }
}
