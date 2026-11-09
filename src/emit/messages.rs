//! Messages in the emitted crate, each a type the embedder declares: what it declares, each
//! type's equality, and a struct literal's value.

use std::fmt::Write as _;

use super::{narrow, Emitter, Vars};
use crate::functions::STD;
use crate::tree::Typed;
use crate::types::{Message, Type};
use crate::Error;

/// Names each message's Rust, as a CEL environment declares a message type.
pub trait Messages {
    /// The Rust type of `message`, the `i`th message type met, asked once. Messages differing in
    /// name or fields are different types.
    fn path(&mut self, i: usize, message: &Message) -> String;
    /// The Rust name of each field of `message`.
    fn fields(&self, message: &Message) -> Vec<String>;
    /// Whether `message` writes itself: its type implements `serde::Serialize`, and
    /// `json.encode` writes what that gives, wherever the message sits in the value. Else
    /// `json.encode` writes each of its fields.
    fn serializes(&self, _message: &Message) -> bool {
        false
    }
    /// Whether `message` reads itself: its type implements `serde::Deserialize`, and
    /// a dynamic value filling a place of that message, or a list of it, is read through it.
    /// Else such a fill is [`Error::Unsupported`].
    fn deserializes(&self, _message: &Message) -> bool {
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
        while let Some((message, rust)) = self.messages.get(i).cloned() {
            declared.push(self.declare_one(message, rust, out)?);
            i = i.saturating_add(1);
        }
        Ok(declared)
    }

    /// Notes each message type in `ty`, and in its fields, as compared: it gets `==`.
    pub(super) fn compares(&mut self, ty: &Type) {
        match ty {
            Type::Message(m) if !self.compared.contains(m) => {
                self.compared.push(m.clone());
                m.fields.iter().for_each(|f| self.compares(&f.ty));
            }
            Type::List(t) | Type::Optional(t) | Type::Nullable(t) => self.compares(t),
            Type::Map(k, v) => {
                self.compares(k);
                self.compares(v);
            }
            _ => {}
        }
    }

    /// One `message` as Rust type `rust`, adding its `==` to `out` where an expression
    /// compares it and CEL compares its fields.
    fn declare_one(
        &mut self,
        message: Message,
        rust: String,
        out: &mut String,
    ) -> Result<Declared, Error> {
        let names = self.declare.fields(&message);
        let mut held_fields = Vec::with_capacity(message.fields.len());
        let mut same = String::new();
        for (name, f) in names.into_iter().zip(&message.fields) {
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
        if self.compared.contains(&message) && crate::stdlib::equal(&Type::Message(message)) {
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
        let Type::Message(message) = &t.ty else {
            return Err(Error::Invalid("a struct literal of a non-message".into()));
        };
        let names = self.declare.fields(message);
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
