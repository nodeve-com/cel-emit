//! Comprehensions: each iterates its list or map at the depth naming its `x` and `n`, its body
//! one deeper.

use super::{Emitter, Vars};
use crate::functions::STD;
use crate::tree::{Node, Typed};
use crate::types::Type;
use crate::Error;

impl Emitter {
    /// What a [`Node::Map`] or [`Node::Keep`] gives, as an iterator of each item's result.
    pub(super) fn mapped(&mut self, map: &Typed, vars: &dyn Vars) -> Result<String, Error> {
        match &map.node {
            Node::Map(list, body) => {
                let (iter, bind) = self.over(list, vars)?;
                let body = self.inside(body, vars)?;
                Ok(format!("{iter}.map(|{bind}| {STD}::ok({body}))"))
            }
            Node::Keep(list, pred, body) => {
                let (iter, bind) = self.over(list, vars)?;
                let (pred, body) = (self.inside(pred, vars)?, self.inside(body, vars)?);
                Ok(format!(
                    "{iter}.filter_map(|{bind}| (|| {STD}::ok(if {pred} {{ Some({body}) }} else {{ None }}))().transpose())"
                ))
            }
            _ => Err(Error::Invalid("a fold of other than a map".into())),
        }
    }

    /// An iterator over `list`, a list's elements or a map's entries, each with its index, and
    /// the pattern binding them at this depth.
    pub(super) fn over(
        &mut self,
        list: &Typed,
        vars: &dyn Vars,
    ) -> Result<(String, String), Error> {
        if !matches!(list.ty, Type::List(_) | Type::Map(..)) {
            return Err(Error::Invalid(
                "a comprehension over other than a list or map".into(),
            ));
        }
        let d = self.depth;
        Ok((
            format!("({}).iter().enumerate()", self.place(list, vars)?),
            format!("(n{d}, x{d})"),
        ))
    }

    /// `body` one comprehension deeper.
    pub(super) fn inside(&mut self, body: &Typed, vars: &dyn Vars) -> Result<String, Error> {
        self.depth = self.depth.saturating_add(1);
        let out = self.expr(body, vars);
        self.depth = self.depth.saturating_sub(1);
        out
    }
}
