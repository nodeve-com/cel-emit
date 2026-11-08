//! Functions written in CEL, each one Rust function.

use std::fmt::Write as _;

use super::{Emitter, Vars};
use crate::functions::STD;
use crate::types::Type;
use crate::written::Body;
use crate::{Decls, Error};

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
    /// Declares each function `decls` writes in CEL, which [`Emitter::items`] then writes, and
    /// each call of one runs. Call it before emitting an expression that calls one.
    pub fn define(&mut self, decls: &Decls) -> Result<(), Error> {
        for body in crate::written::bodies(decls)? {
            self.paths.add(&body.function());
            self.defined.push(body);
        }
        Ok(())
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
    fn function(&mut self, body: &Body) -> Result<String, Error> {
        let w = &body.written;
        let params = Params(&w.params);
        let mut takes = Vec::new();
        for (i, (_, ty)) in w.params.iter().enumerate() {
            takes.push(match ty {
                _ if ty.copied() => format!("p{i}: {}", self.rust(ty)?),
                _ => format!("p{i}: &{}", self.rust(ty)?),
            });
        }
        let result = self.rust(&body.tree.ty)?;
        let value = self.expr(&body.tree, &params).map_err(|e| e.at(&w.name))?;
        let name = w.path.rsplit("::").next().unwrap_or(&w.path);
        Ok(format!(
            "pub fn {name}({}) -> {STD}::Result<{result}> {{ Ok({value}) }}",
            takes.join(", ")
        ))
    }
}
