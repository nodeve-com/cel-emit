//! Functions written in CEL, each one Rust function.

use std::fmt::Write as _;

use super::{Emitter, Vars};
use crate::functions::STD;
use crate::types::Type;
use crate::written::Body;
use crate::Error;

/// A written function's parameters, `p0`, `p1`, ..., each read as a place of its type.
struct Params<'a>(&'a [(String, Type)]);

impl Vars for Params<'_> {
    fn read(&self, var: usize) -> Result<String, Error> {
        let (_, ty) = (self.0.get(var)).ok_or_else(|| Error::Invalid("no such input".into()))?;
        Ok(match ty.copied() {
            true => format!("p{var}"),
            false => format!("(*p{var})"),
        })
    }

    fn has(&self, _: usize) -> Result<String, Error> {
        Ok("true".into())
    }
}

impl Emitter {
    /// Declares each function written in CEL, checked by [`crate::written::check`], which
    /// [`Emitter::items`] then writes, and each call of one runs. Call it before emitting an
    /// expression that calls one.
    pub fn define(&mut self, bodies: &[Body]) {
        for body in bodies {
            self.paths.add(&body.function());
            if self.borrowed(&body.tree) {
                self.lent.push(body.id.clone());
            }
            self.defined.push(body.clone());
        }
    }

    /// Whether a function written in CEL is overload `id`.
    pub(super) fn defines(&self, id: &str) -> bool {
        self.defined.iter().any(|b| b.id == id)
    }

    /// The Rust of each written function, each once.
    pub(super) fn written_items(&mut self) -> Result<String, Error> {
        let mut out = String::new();
        for body in std::mem::take(&mut self.defined) {
            let code = self.function(&body)?;
            let _ = writeln!(out, "{code}");
            self.defined.push(body);
        }
        Ok(out)
    }

    /// `body` as a function over its parameters: those not `Copy` by reference, text as `&String`.
    /// A body that is a part of them returns a reference to it, one lifetime tying them to it.
    fn function(&mut self, body: &Body) -> Result<String, Error> {
        let w = &body.written;
        let params = Params(&w.params);
        let lent = self.lent.contains(&body.id);
        let life = if lent { "'a " } else { "" };
        let mut takes = Vec::new();
        for (i, (_, ty)) in w.params.iter().enumerate() {
            takes.push(match ty {
                _ if ty.copied() => format!("p{i}: {}", self.rust(ty)?),
                _ => format!("p{i}: &{life}{}", self.rust(ty)?),
            });
        }
        let result = self.rust(&body.tree.ty)?;
        let (result, value) = match lent {
            true => (
                format!("&'a {result}"),
                self.reference(&body.tree, false, &params),
            ),
            false => (result, self.expr(&body.tree, &params)),
        };
        let value = value.map_err(|e| e.at(&w.name))?;
        let name = w.path.rsplit("::").next().unwrap_or(&w.path);
        let generics = if lent { "<'a>" } else { "" };
        Ok(format!(
            "pub fn {name}{generics}({}) -> {STD}::Result<{result}> {{ Ok({value}) }}",
            takes.join(", ")
        ))
    }
}
