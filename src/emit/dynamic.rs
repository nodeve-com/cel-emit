//! Dyn at its edges: a dyn value read as a typed one, and a typed one entered as dyn.

use super::{narrow, Emitter, Vars};
use crate::functions::STD;
use crate::tree::Typed;
use crate::types::{Field, Type, Width};
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
            Type::List(of) if **of == Type::Dyn => "to_list",
            other => {
                return Err(Error::Unsupported(format!(
                    "a dyn read as {}",
                    other.display()
                )))
            }
        };
        Ok(match wrap {
            Some(wrap) => {
                let x = self.optional_reference(x, vars)?;
                format!("{STD}::dynamic::{wrap}({x}, {STD}::dynamic::{f})?")
            }
            None => {
                let x = self.reference(x, false, vars)?;
                format!("{STD}::dynamic::{f}({x})?")
            }
        })
    }

    /// The text `text` decoded as `shape`.
    pub(super) fn shaped(
        &mut self,
        text: &Typed,
        shape: &Shaped,
        vars: &dyn Vars,
    ) -> Result<String, Error> {
        let text = self.reference(text, false, vars)?;
        let Shaped { code, slots } = shape;
        Ok(format!(
            "{{ const SHAPE: &{STD}::json::Shape = &{code}; {STD}::json::decode_shaped({text}, SHAPE, {slots})? }}"
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

/// What writing a value as a dyn carries down: the field holding the other keys' JSON text, and
/// the depth that names what each level binds.
#[derive(Clone, Copy)]
struct Writing<'a> {
    rest: Option<&'a str>,
    depth: usize,
}

impl Writing<'_> {
    fn next(self) -> Self {
        Writing {
            depth: self.depth.saturating_add(1),
            ..self
        }
    }

    /// Whether `f` is the field holding the other keys' text.
    fn holds_rest(self, f: &Field) -> bool {
        self.rest == Some(f.name.as_str())
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
        self.dyn_of(
            &super::place::receiver(&code),
            &x.ty,
            Writing { rest, depth: 0 },
        )
    }

    /// The Rust giving `x`, a value of `ty`, as a dyn.
    fn dyn_of(&self, x: &str, ty: &Type, w: Writing) -> Result<String, Error> {
        let d = w.depth;
        Ok(match ty {
            Type::Nullable(t) => {
                let held = self.dyn_of(&format!("v{d}"), t, w.next())?;
                format!("match {x} {{ Some(v{d}) => {held}, None => {STD}::Dyn::Null }}")
            }
            Type::List(t) => {
                let each = self.dyn_of(&format!("e{d}"), t, w.next())?;
                format!(
                    "{STD}::Dyn::List({x}.into_iter().map(|e{d}| {STD}::ok({each})).collect::<{STD}::Result<Vec<_>>>()?)"
                )
            }
            Type::Map(k, v) if **k == Type::String => {
                let each = self.dyn_of(&format!("e{d}.1"), v, w.next())?;
                format!(
                    "{STD}::Dyn::Map({x}.into_iter().map(|e{d}| {STD}::ok((e{d}.0, {each}))).collect::<{STD}::Result<Vec<_>>>()?)"
                )
            }
            Type::Message(fields) => self.dyn_message(x, fields, w)?,
            _ => scalar_dyn(x, ty, d)?,
        })
    }

    /// The Rust giving message `x` of `fields` as a dyn map.
    fn dyn_message(&self, x: &str, fields: &[Field], w: Writing) -> Result<String, Error> {
        // An optional field holding nothing has no key, so entries may be left out.
        let sparse = fields.iter().any(|f| matches!(f.ty, Type::Optional(_)));
        let names = self.declare.fields(fields);
        let mut entries = Vec::with_capacity(fields.len());
        for (name, f) in names.iter().zip(fields) {
            let entry = self.dyn_entry(name, f, w)?;
            let always = !matches!(f.ty, Type::Optional(_));
            entries.push(if sparse && always {
                format!("Some({entry})")
            } else {
                entry
            });
        }
        let d = w.depth;
        let entries = entries.join(", ");
        let map = if sparse {
            format!("{STD}::Dyn::Map([{entries}].into_iter().flatten().collect())")
        } else {
            format!("{STD}::dynamic::fields(Vec::from([{entries}]))")
        };
        Ok(format!("{{ let r{d} = {x}; {map} }}"))
    }

    /// The entry of field `f`, named `name` in Rust, in its message's dyn map; for an optional
    /// holding nothing, `None`. A string field named for the other keys' text writes them.
    fn dyn_entry(&self, name: &str, f: &Field, w: Writing) -> Result<String, Error> {
        let d = w.depth;
        let entry = |value: String| format!("(String::from({:?}), {value})", f.name);
        let held = |at: &str, ty: &Type| {
            if w.holds_rest(f) && *ty == Type::String {
                return Ok(format!("{STD}::json::rest({at})?"));
            }
            self.dyn_of(at, ty, w.next())
        };
        if let Type::Optional(t) = &f.ty {
            let entry = entry(held(&format!("v{d}"), t)?);
            return Ok(format!(
                "match r{d}.{name} {{ Some(v{d}) => Some({entry}), None => None }}"
            ));
        }
        Ok(entry(held(&format!("r{d}.{name}"), &f.ty)?))
    }
}

/// The Rust giving `x`, a value of the non-composite `ty`, as a dyn, naming what it binds with
/// depth `d`.
fn scalar_dyn(x: &str, ty: &Type, d: usize) -> Result<String, Error> {
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
        other => {
            return Err(Error::Unsupported(format!(
                "a {} written as JSON",
                other.display()
            )))
        }
    })
}

/// A `json::Shape` and the slots it fills.
pub(super) struct Shaped {
    code: String,
    slots: usize,
}

/// `message` as the shape that reads a document into its declared fields only; with `rest`, the
/// other keys are kept under that field.
pub(super) fn shaped(message: &Type, rest: Option<&str>) -> Result<Shaped, Error> {
    let mut slots = 1;
    let code = shape(message, rest, &mut slots)?;
    Ok(Shaped { code, slots })
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
        Type::Message(fields) => message_shape(fields, rest, slots)?,
        _ => kind("Plain"),
    })
}

/// A message's `json::Shape`: each field keyed to its slot, the one named `rest` open.
fn message_shape(fields: &[Field], rest: Option<&str>, slots: &mut usize) -> Result<String, Error> {
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
        let inner = shape(&f.ty, rest, slots)?;
        keys.push(format!("({:?}, {slot}, {inner})", f.name));
    }
    let kind = |k: &str| format!("{STD}::json::Shape::{k}");
    Ok(match opens {
        Some(slot) => format!("{}(&[{}], {slot})", kind("Open"), keys.join(", ")),
        None => format!("{}(&[{}])", kind("Message"), keys.join(", ")),
    })
}
