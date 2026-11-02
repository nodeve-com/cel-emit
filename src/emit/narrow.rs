//! Stored widths ([`Type::Narrow`]): a stored value widened to what CEL computes in, a written
//! one held to its widths, and the calls that widen one element at a time rather than a whole
//! collection.

use super::{Emitter, Vars};
use crate::functions::STD;
use crate::tree::Typed;
use crate::types::{Type, Width};
use crate::Error;

/// Where [`convert`] takes a value.
#[derive(Clone, Copy)]
pub(super) enum To {
    /// A stored value widened to CEL's type.
    Cel,
    /// A CEL value held to the stored widths, still of CEL's type; a float rounds to its own.
    Within,
    /// A CEL value as the stored type itself, which a message's field holds.
    Stored,
}

/// A `Result` expression converting `code` between the stored type `ty` and its CEL type, as
/// `to` says. None where `ty` holds no width. A nested conversion binds `w{d}`.
pub(super) fn convert(code: &str, ty: &Type, to: To, d: usize) -> Option<String> {
    if !ty.narrow() {
        return None;
    }
    let w = format!("w{d}");
    let inner = |t: &Type| closure(t, to, d.saturating_add(1));
    Some(match ty {
        Type::Narrow(Width::F32) => match to {
            To::Cel => format!("{STD}::ok(f64::from({code}))"),
            To::Within => format!("{STD}::ok(f64::from({code} as f32))"),
            To::Stored => format!("{STD}::ok({code} as f32)"),
        },
        Type::Narrow(n) => match to {
            To::Cel => {
                let wide = if n.cel() == Type::Uint { "u64" } else { "i64" };
                format!("{STD}::number::fit::<{wide}, _>({code})")
            }
            To::Within => format!("{STD}::number::within::<{}, _>({code})", n.rust()),
            To::Stored => format!("{STD}::number::fit::<{}, _>({code})", n.rust()),
        },
        Type::List(t) => format!("{STD}::collections::map_list({code}, {})", inner(t)),
        Type::Map(k, v) => format!(
            "{STD}::collections::map_entries({code}, {}, {})",
            inner(k),
            inner(v)
        ),
        Type::Optional(t) | Type::Nullable(t) => {
            let body = convert(&w, t, to, d.saturating_add(1))?;
            format!("{code}.map(|{w}| {body}).transpose()")
        }
        _ => return None,
    })
}

/// [`convert`] as a closure, the identity where `ty` holds no width.
fn closure(ty: &Type, to: To, d: usize) -> String {
    let w = format!("w{d}");
    match convert(&w, ty, to, d.saturating_add(1)) {
        Some(c) => format!("|{w}| {c}"),
        None => format!("{STD}::ok"),
    }
}

/// Whether `id` takes a stored float32 as it is, to write it in its own digits.
pub(super) fn writes_stored(id: &str) -> bool {
    matches!(id, "float32_to_string" | "string_format")
}

impl Emitter {
    /// A call on a stored collection, widening each element it reads: sizes need none, an index
    /// widens what it finds, and a membership or key lookup widens each element it compares. An optional's
    /// value widens too.
    pub(super) fn stored_call(
        &mut self,
        id: &str,
        args: &[Typed],
        vars: &dyn Vars,
    ) -> Result<String, Error> {
        let path = self.path(id, args)?;
        let types: Vec<Type> = args.iter().map(|a| a.ty.clone()).collect();
        let code = self.args(id, args, vars)?;
        let refused = || Error::Invalid(format!("{id} on a stored collection"));
        // The collection is the second operand of a membership, else the first.
        let at = usize::from(matches!(id, "in_list" | "in_map"));
        let (collection, c) = types.get(at).zip(code.get(at)).ok_or_else(refused)?;
        if matches!(id, "size_list" | "list_size" | "size_map" | "map_size") {
            return Ok(format!("{path}({c})?"));
        }
        if let Type::Optional(t) = collection {
            // An optional of a stored width: presence needs no widening, the value does.
            return match id {
                "optional_has_value" => Ok(format!("{path}({c})?")),
                "optional_value" => {
                    let found = format!("{path}({c})?");
                    Ok(convert(&found, t, To::Cel, 0).map_or(found, |w| format!("{w}?")))
                }
                _ => Err(refused()),
            };
        }
        let x = code.get(1usize.saturating_sub(at)).ok_or_else(refused)?;
        Ok(match (id, collection) {
            ("index_list", Type::List(t)) => {
                let found = format!("{path}({c}, {x})?");
                convert(&found, t, To::Cel, 0).map_or(found, |w| format!("{w}?"))
            }
            ("in_list", Type::List(t)) => format!(
                "{STD}::collections::in_list_with({x}, {c}, {})?",
                closure(t, To::Cel, 0)
            ),
            ("in_map", Type::Map(k, _)) => format!(
                "{STD}::collections::in_map_with({x}, {c}, {})?",
                closure(k, To::Cel, 0)
            ),
            ("index_map", Type::Map(k, v)) => format!(
                "{STD}::collections::index_map_with({c}, {x}, {}, {})?",
                closure(k, To::Cel, 0),
                closure(v, To::Cel, 0)
            ),
            _ => return Err(refused()),
        })
    }
}
