//! Dyn at its edges: a dyn value read as a typed one, and a typed one entered as dyn.

use super::{narrow, Emitter, Vars};
use crate::functions::STD;
use crate::tree::Typed;
use crate::types::{Type, Width};
use crate::Error;

impl Emitter {
    /// `x`, a dyn, as a `ty`; an optional dyn as an optional `ty` (absent stays absent), or as a
    /// nullable one (absent and null are null).
    pub(super) fn read_dyn(
        &mut self,
        x: &Typed,
        ty: &Type,
        vars: &dyn Vars,
    ) -> Result<String, Error> {
        let (kind, wrap) = match ty {
            Type::Optional(t) => (&**t, Some("optional")),
            Type::Nullable(t) => (&**t, Some("nullable")),
            t => (t, None),
        };
        let f = match kind {
            Type::Int => "to_int",
            Type::Uint => "to_uint",
            Type::Double => "to_double",
            Type::Bool => "to_bool",
            Type::String => "to_str",
            Type::Timestamp => "to_timestamp",
            Type::Duration => "to_duration",
            Type::Bytes => "to_bytes",
            other => {
                return Err(Error::Unsupported(format!(
                    "a dyn read as {}",
                    other.display()
                )))
            }
        };
        let x = self.expr(x, vars)?;
        Ok(match wrap {
            Some(wrap) => format!("{STD}::dynamic::{wrap}({x}, {STD}::dynamic::{f})?"),
            None => format!("{STD}::dynamic::{f}({x})?"),
        })
    }

    /// The text `text` decoded as `message`'s declared fields only, into its slots.
    pub(super) fn shaped(
        &mut self,
        text: &Typed,
        message: &Type,
        rest: Option<&str>,
        vars: &dyn Vars,
    ) -> Result<String, Error> {
        let text = self.place(text, vars)?;
        let mut slots = 1;
        let shape = shape(message, rest, &mut slots)?;
        Ok(format!(
            "{{ const SHAPE: &{STD}::json::Shape = &{shape}; {STD}::json::decode_shaped(&{text}, SHAPE, {slots})? }}"
        ))
    }

    /// `x` as `to`: a dyn, or a list or map of dyn.
    pub(super) fn enter_dyn(
        &mut self,
        x: &Typed,
        to: &Type,
        vars: &dyn Vars,
    ) -> Result<String, Error> {
        let inner = self.expr(x, vars)?;
        let unsupported =
            || Error::Unsupported(format!("a {} as {}", x.ty.display(), to.display()));
        if !x.ty.dyn_into(to) {
            return Err(unsupported());
        }
        let f = match (&x.ty, to) {
            (Type::List(_), Type::List(_)) => "dynamic::lift_list",
            (Type::Map(..), Type::Map(..)) => "dynamic::lift_map",
            (Type::List(_), _) => "dynamic::from_list",
            (Type::Map(k, _), _) if **k == Type::Dyn => "dynamic::from_keyed",
            (Type::Map(..), _) => "dynamic::from_map",
            _ => return Ok(format!("{STD}::Dyn::from({inner})")),
        };
        Ok(format!("{STD}::{f}({inner})?"))
    }
}

impl Emitter {
    /// `x` as the dyn JSON writes it; with `rest`, a message's string field of that name holds
    /// JSON object text, written as entries of the message's map.
    pub(super) fn written(
        &mut self,
        x: &Typed,
        rest: Option<&str>,
        vars: &dyn Vars,
    ) -> Result<String, Error> {
        let code = self.expr(x, vars)?;
        self.dyn_of(&code, &x.ty, rest, 0)
    }

    /// The Rust giving `x`, a value of `ty`, as a dyn, naming what it binds with depth `d`.
    fn dyn_of(&self, x: &str, ty: &Type, rest: Option<&str>, d: usize) -> Result<String, Error> {
        let next = d.saturating_add(1);
        Ok(match ty {
            Type::Dyn => x.into(),
            Type::Int
            | Type::Uint
            | Type::Double
            | Type::Bool
            | Type::String
            | Type::Timestamp
            | Type::Duration => format!("{STD}::Dyn::from({x})"),
            Type::Null => format!("{STD}::Dyn::Null"),
            Type::Bytes => format!("{STD}::Dyn::Str({STD}::conversions::base64_encode(&{x})?)"),
            Type::Narrow(Width::F32) => format!("{STD}::Dyn::Float32(({x}) as f32)"),
            Type::Narrow(_) => match narrow::convert(x, ty, narrow::To::Cel, d) {
                Some(wide) => format!("{STD}::Dyn::from({wide}?)"),
                None => return Err(Error::Invalid("a width without a conversion".into())),
            },
            Type::Nullable(t) => {
                let held = self.dyn_of(&format!("v{d}"), t, rest, next)?;
                format!("match {x} {{ Some(v{d}) => {held}, None => {STD}::Dyn::Null }}")
            }
            Type::List(t) => {
                let each = self.dyn_of(&format!("e{d}"), t, rest, next)?;
                format!(
                    "{STD}::Dyn::List({x}.into_iter().map(|e{d}| {STD}::ok({each})).collect::<{STD}::Result<Vec<_>>>()?)"
                )
            }
            Type::Map(k, v) if **k == Type::String => {
                let each = self.dyn_of(&format!("e{d}.1"), v, rest, next)?;
                format!(
                    "{STD}::Dyn::Map({x}.into_iter().map(|e{d}| {STD}::ok((e{d}.0, {each}))).collect::<{STD}::Result<Vec<_>>>()?)"
                )
            }
            Type::Message(fields) => {
                let mut entries = Vec::with_capacity(fields.len());
                // An optional field holding nothing has no key, so entries may be left out.
                let sparse = fields.iter().any(|f| matches!(f.ty, Type::Optional(_)));
                let names = self.declare.fields(fields);
                for (name, f) in names.iter().zip(fields) {
                    let entry = |value: String| format!("(String::from({:?}), {value})", f.name);
                    if let Type::Optional(t) = &f.ty {
                        let kept = format!("v{d}");
                        let held = match rest == Some(f.name.as_str()) && **t == Type::String {
                            true => format!("{STD}::json::rest({kept})?"),
                            false => self.dyn_of(&kept, t, rest, next)?,
                        };
                        let held = entry(held);
                        entries.push(format!(
                            "match r{d}.{name} {{ Some(v{d}) => Some({held}), None => None }}"
                        ));
                        continue;
                    }
                    let at = format!("r{d}.{name}");
                    let value = if rest == Some(f.name.as_str()) && f.ty == Type::String {
                        format!("{STD}::json::rest({at})?")
                    } else {
                        self.dyn_of(&at, &f.ty, rest, next)?
                    };
                    let entry = entry(value);
                    entries.push(if sparse {
                        format!("Some({entry})")
                    } else {
                        entry
                    });
                }
                let map = if sparse {
                    format!(
                        "{STD}::Dyn::Map([{}].into_iter().flatten().collect())",
                        entries.join(", ")
                    )
                } else {
                    format!(
                        "{STD}::dynamic::fields(Vec::from([{}]))",
                        entries.join(", ")
                    )
                };
                format!("{{ let r{d} = {x}; {map} }}")
            }
            other => {
                return Err(Error::Unsupported(format!(
                    "a {} written as JSON",
                    other.display()
                )))
            }
        })
    }
}

/// `ty` as the `json::Shape` that reads a document into it, each field's slot the next of
/// `slots` in field order, a message's before its own fields'; a message, however deep, keeping
/// the other keys under `rest` gives that field's slot to them.
fn shape(ty: &Type, rest: Option<&str>, slots: &mut usize) -> Result<String, Error> {
    let kind = |k: &str| format!("{STD}::json::Shape::{k}");
    Ok(match ty {
        Type::Nullable(t) | Type::Optional(t) => shape(t, rest, slots)?,
        Type::Int | Type::Narrow(Width::I8 | Width::I16 | Width::I32 | Width::I128) => kind("Int"),
        Type::Uint | Type::Narrow(Width::U8 | Width::U16 | Width::U32 | Width::U128) => {
            kind("Uint")
        }
        Type::Double | Type::Narrow(Width::F32) => kind("Double"),
        Type::Message(fields) => {
            let mut keys = Vec::with_capacity(fields.len());
            let mut opens = None;
            for f in fields {
                let slot = *slots;
                *slots = slot
                    .checked_add(1)
                    .ok_or(Error::Unsupported("too many fields".into()))?;
                if rest == Some(f.name.as_str()) {
                    opens = Some(slot);
                    continue;
                }
                keys.push(format!(
                    "({:?}, {slot}, {})",
                    f.name,
                    shape(&f.ty, rest, slots)?
                ));
            }
            match opens {
                Some(slot) => format!("{}(&[{}], {slot})", kind("Open"), keys.join(", ")),
                None => format!("{}(&[{}])", kind("Message"), keys.join(", ")),
            }
        }
        _ => kind("Plain"),
    })
}
