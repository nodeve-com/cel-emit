//! Messages in the emitted crate, each a type the embedder declares: what it declares, each
//! type's equality, and a struct literal's value.

use std::fmt::Write as _;

use super::{narrow, Emitter, Vars};
use crate::functions::STD;
use crate::tree::Typed;
use crate::types::{Field, Type};
use crate::Error;

/// Names each message's Rust, as a CEL environment declares a message type.
pub trait Messages {
    /// The Rust type of `fields`, the `i`th message type met, asked once.
    fn path(&mut self, i: usize, fields: &[Field]) -> String;
    /// The Rust name of each of `fields`.
    fn fields(&self, fields: &[Field]) -> Vec<String>;
    /// Whether the message of `fields` writes itself: its type implements `serde::Serialize`, and
    /// `json.encode` writes what that gives, wherever the message sits in the value. Else
    /// `json.encode` writes each of its fields.
    fn serializes(&self, _fields: &[Field]) -> bool {
        false
    }
    /// Whether the message of `fields` reads itself: its type implements `serde::Deserialize`, and
    /// a dynamic value filling a place of that message, or a list of it, is read through it.
    /// Else such a fill is [`Error::Unsupported`].
    fn deserializes(&self, _fields: &[Field]) -> bool {
        false
    }
}

/// A message the embedder writes: `Clone`, `Default`, and `Debug`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declared {
    pub rust: String,
    /// Each field's Rust name and type.
    pub fields: Vec<(String, String)>,
}

/// What [`Emitter::items`] gives the crate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Items {
    pub code: String,
    pub messages: Vec<Declared>,
}

impl Emitter {
    /// Each message type met, adding `==` to `out` for those whose fields CEL compares.
    pub(super) fn declared(&mut self, out: &mut String) -> Result<Vec<Declared>, Error> {
        let mut declared = Vec::new();
        let mut i = 0;
        // Emitting a field's type may add a message.
        while let Some((fields, rust)) = self.messages.get(i).cloned() {
            declared.push(self.declare_one(fields, rust, out)?);
            i = i.saturating_add(1);
        }
        Ok(declared)
    }

    /// Notes each message type in `ty`, and in its fields, as compared: it gets `==`.
    pub(super) fn compares(&mut self, ty: &Type) {
        match ty {
            Type::Message(fields) if !self.compared.contains(fields) => {
                self.compared.push(fields.clone());
                fields.iter().for_each(|f| self.compares(&f.ty));
            }
            Type::List(t) | Type::Optional(t) | Type::Nullable(t) => self.compares(t),
            Type::Map(k, v) => {
                self.compares(k);
                self.compares(v);
            }
            _ => {}
        }
    }

    /// One message of `fields` as Rust type `rust`, adding its `==` to `out` where an expression
    /// compares it and CEL compares its fields.
    fn declare_one(
        &mut self,
        fields: Vec<Field>,
        rust: String,
        out: &mut String,
    ) -> Result<Declared, Error> {
        let names = self.declare.fields(&fields);
        let mut held_fields = Vec::with_capacity(fields.len());
        let mut same = String::new();
        for (name, f) in names.into_iter().zip(&fields) {
            let held = match &f.ty {
                Type::Optional(t) | Type::Nullable(t) => &**t,
                t => t,
            };
            if !held.laid_out() {
                return Err(Error::Unsupported("an absent field of a message".into()));
            }
            let _ = write!(
                same,
                "&& {STD}::cmp::Equal::equal(&self.{name}, &other.{name})? "
            );
            held_fields.push((name, self.rust(&f.ty)?));
        }
        if self.compared.contains(&fields) && crate::stdlib::equal(&Type::Message(fields)) {
            let _ = writeln!(
                out,
                "impl {STD}::cmp::Equal for {rust} {{ fn equal(&self, other: &Self) -> {STD}::Result<bool> {{ Ok(true {same}) }} }}"
            );
        }
        Ok(Declared {
            rust,
            fields: held_fields,
        })
    }

    /// A struct literal: the message, defaulted, then each entry set in order, as its field
    /// stores it.
    pub(super) fn message(
        &mut self,
        t: &Typed,
        entries: &[(usize, Typed, bool)],
        vars: &dyn Vars,
    ) -> Result<String, Error> {
        let mut body = format!("let mut rec = {}::default(); ", self.rust(&t.ty)?);
        let names = self.declare.fields(t.ty.fields());
        for (i, v, flagged) in entries {
            let name = names
                .get(*i)
                .ok_or_else(|| Error::Invalid("a field past its message".into()))?;
            let field = t.ty.fields().get(*i);
            let v = match field {
                Some(f) => self.stored(v, &f.ty, vars)?,
                None => self.expr(v, vars)?,
            };
            let holds_option =
                field.is_some_and(|f| matches!(f.ty, Type::Nullable(_) | Type::Optional(_)));
            if *flagged && !holds_option {
                let _ = write!(body, "if let Some(val) = {v} {{ rec.{name} = val; }} ");
            } else {
                let _ = write!(body, "rec.{name} = {v}; ");
            }
        }
        Ok(format!("{{ {body}rec }}"))
    }

    /// `v`, a CEL value, as a field of type `field` stores it: a stored width kept exactly, an
    /// optional field holding an optional value as it is.
    pub fn stored(&mut self, v: &Typed, field: &Type, vars: &dyn Vars) -> Result<String, Error> {
        if let Some(stored) = v.read_as(field) {
            return self.expr(stored, vars);
        }
        let code = self.expr(v, vars)?;
        Ok(match narrow::convert(&code, field, narrow::To::Stored, 0) {
            Some(stored) => format!("{stored}?"),
            None => code,
        })
    }
}
