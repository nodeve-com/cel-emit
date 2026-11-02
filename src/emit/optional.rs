//! A list or map literal with `?` items: each optional kept where it holds a value, as a block
//! building the list or map in order, so each item's error stops the block as it comes.

use std::fmt::Write as _;

use super::{Emitter, Vars};
use crate::functions::STD;
use crate::tree::Typed;
use crate::Error;

impl Emitter {
    /// `items` as a list: a `?` item adds its value where it holds one.
    pub(super) fn list_opt(
        &mut self,
        items: &[(Typed, bool)],
        vars: &dyn Vars,
    ) -> Result<String, Error> {
        let mut body = String::from("{ let mut e = Vec::new(); ");
        for (item, optional) in items {
            let x = self.expr(item, vars)?;
            if *optional {
                let _ = write!(body, "if let Some(x) = {x} {{ e.push(x); }} ");
            } else {
                let _ = write!(body, "e.push({x}); ");
            }
        }
        body.push_str("e }");
        Ok(body)
    }

    /// `entries` as a map: a `?` entry's value, where it holds one, is kept with its key.
    pub(super) fn map_opt(
        &mut self,
        entries: &[(Typed, Typed, bool)],
        vars: &dyn Vars,
    ) -> Result<String, Error> {
        let mut body = String::from("{ let mut e = Vec::new(); ");
        for (key, value, optional) in entries {
            let k = self.expr(key, vars)?;
            let v = self.expr(value, vars)?;
            if *optional {
                let _ = write!(
                    body,
                    "let k = {k}; if let Some(x) = {v} {{ e.push((k, x)); }} "
                );
            } else {
                let _ = write!(body, "let k = {k}; e.push((k, {v})); ");
            }
        }
        let _ = write!(body, "{STD}::collections::map_of(e)? }}");
        Ok(body)
    }
}
