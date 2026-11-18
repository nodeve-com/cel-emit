//! Dyn at its edges: a dyn value read as a typed one, and a typed one entered as dyn.

use super::{narrow, Emitter, Vars};
use crate::functions::STD;
use crate::tree::Typed;
use crate::types::{Field, Message, Type, Width};
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
        let serde = self.fills_through_serde(kind);
        let f = match kind {
            _ if serde => {
                let to = (self.declare.deserializer()).unwrap_or(format!("{STD}::dynamic::to"));
                format!("{to}::<{}>", self.rust(kind)?)
            }
            Type::Opaque(rust) => self.named(rust, Does::Read)?,
            other => format!("{STD}::dynamic::{}", typed_read(other)?),
        };
        Ok(match wrap {
            Some(wrap) => {
                let x = self.optional_reference(x, vars)?;
                format!("{STD}::dynamic::{wrap}({x}, {f})?")
            }
            None => {
                let x = self.reference(x, false, vars)?;
                format!("{f}({x})?")
            }
        })
    }

    /// The function the embedder names for opaque type `rust`, which cel-emit neither reads nor
    /// writes itself.
    fn named(&self, rust: &str, does: Does) -> Result<String, Error> {
        let (named, what) = match does {
            Does::Read => (self.declare.reads(rust), "a dyn read as"),
            Does::Write => (self.declare.writes(rust), "json.encode of"),
        };
        named.ok_or_else(|| {
            Error::Unsupported(format!("{what} a {rust}: the embedder names no function"))
        })
    }

    /// Whether a dyn filling `ty` is read by `ty`'s own `Deserialize`: a message or a list of
    /// them whose messages all read themselves.
    fn fills_through_serde(&self, ty: &Type) -> bool {
        match ty {
            Type::Message(_) => self.reads_itself(ty),
            Type::List(of) => **of != Type::Dyn && self.reads_itself(ty),
            _ => false,
        }
    }

    /// Whether every message in `ty` reads itself and `ty` has no map, whose Rust is not a serde map.
    fn reads_itself(&self, ty: &Type) -> bool {
        match ty {
            Type::Message(message) => self.declare.deserializes(message),
            Type::List(t) | Type::Optional(t) | Type::Nullable(t) => self.reads_itself(t),
            Type::Map(..) | Type::Duration | Type::Bytes | Type::Number => false,
            Type::Opaque(_) => false,
            _ => true,
        }
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
            _ => return Ok(format!("{STD}::Value::from({inner})")),
        };
        Ok(format!("{STD}::{f}({inner})?"))
    }
}

/// Which of an opaque type's two functions.
#[derive(Clone, Copy)]
enum Does {
    Read,
    Write,
}

/// cel-emit-runtime's function reading a dyn as `ty`, a kind it reads itself.
fn typed_read(ty: &Type) -> Result<&'static str, Error> {
    Ok(match ty {
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
    })
}

/// What writing a value as a dyn carries down: the depth that names what each level binds.
#[derive(Clone, Copy)]
struct Writing {
    depth: usize,
}

impl Writing {
    fn next(self) -> Self {
        Writing {
            depth: self.depth.saturating_add(1),
        }
    }
}

impl Emitter {
    /// `x` as the dyn JSON writes it.
    pub(super) fn written(&mut self, x: &Typed, vars: &dyn Vars) -> Result<String, Error> {
        let code = self.expr(x, vars)?;
        self.dyn_of(&super::place::receiver(&code), &x.ty, Writing { depth: 0 })
    }

    /// The Rust giving `x`, a value of `ty`, as a dyn.
    fn dyn_of(&self, x: &str, ty: &Type, w: Writing) -> Result<String, Error> {
        let d = w.depth;
        Ok(match ty {
            Type::Nullable(t) => {
                let held = self.dyn_of(&format!("v{d}"), t, w.next())?;
                format!("match {x} {{ Some(v{d}) => {held}, None => {STD}::Value::Null }}")
            }
            Type::List(t) => {
                let each = self.dyn_of(&format!("e{d}"), t, w.next())?;
                format!(
                    "{STD}::Value::List({x}.into_iter().map(|e{d}| {STD}::ok({each})).collect::<{STD}::Result<Vec<_>>>()?)"
                )
            }
            Type::Map(k, v) if **k == Type::String => {
                let each = self.dyn_of(&format!("e{d}.1"), v, w.next())?;
                format!(
                    "{STD}::Value::Map({x}.into_iter().map(|e{d}| {STD}::ok((e{d}.0, {each}))).collect::<{STD}::Result<Vec<_>>>()?)"
                )
            }
            Type::Message(message) => self.dyn_message(x, message, w)?,
            Type::Opaque(rust) => format!("{}(&({x}))?", self.named(rust, Does::Write)?),
            _ => scalar_dyn(x, ty, d)?,
        })
    }

    /// The Rust giving message `x` of type `message` as a dyn map.
    fn dyn_message(&self, x: &str, message: &Message, w: Writing) -> Result<String, Error> {
        // An optional field holding nothing has no key, so entries may be left out.
        let fields = &message.fields;
        if self.declare.serializes(message) {
            return Ok(format!("{STD}::json::serialized(&({x}))?"));
        }
        let sparse = fields.iter().any(|f| matches!(f.ty, Type::Optional(_)));
        let names = self.declare.fields(message);
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
            format!("{STD}::Value::Map([{entries}].into_iter().flatten().collect())")
        } else {
            format!("{STD}::dynamic::fields(Vec::from([{entries}]))")
        };
        Ok(format!("{{ let r{d} = {x}; {map} }}"))
    }

    /// The entry of field `f`, named `name` in Rust, in its message's dyn map; for an optional
    /// holding nothing, `None`.
    fn dyn_entry(&self, name: &str, f: &Field, w: Writing) -> Result<String, Error> {
        let d = w.depth;
        let entry = |value: String| format!("(String::from({:?}), {value})", f.name);
        let held = |at: &str, ty: &Type| self.dyn_of(at, ty, w.next());
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
        | Type::Duration => format!("{STD}::Value::from({x})"),
        Type::Null => format!("{STD}::Value::Null"),
        Type::Bytes => format!("{STD}::Value::Str({STD}::conversions::base64_encode(&{x})?)"),
        Type::Narrow(Width::F32) => format!("{STD}::Value::Float32(({x}) as f32)"),
        Type::Narrow(_) => match narrow::convert(x, ty, narrow::To::Cel, d) {
            Some(wide) => format!("{STD}::Value::from({wide}?)"),
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
