//! Comprehensions: each iterates its list or map at the depth naming its `x` and `n`, its body
//! one deeper.

use super::place::receiver;
use super::{Emitter, Vars};
use crate::functions::STD;
use crate::tree::{Node, Typed};
use crate::types::Type;
use crate::{walk, Error};

impl Emitter {
    /// What a [`Node::Map`] or [`Node::Keep`] gives, as an iterator of each item's result.
    pub(super) fn mapped(&mut self, map: &Typed, vars: &dyn Vars) -> Result<String, Error> {
        match &map.node {
            Node::Map(list, body) => {
                let (iter, bind) = self.over(list, &[body], vars)?;
                let body = self.inside(body, vars)?;
                Ok(format!("{iter}.map(|{bind}| {STD}::ok({body}))"))
            }
            Node::Keep(list, pred, body) => {
                let (iter, bind) = self.over(list, &[pred, body], vars)?;
                let (pred, body) = (self.inside(pred, vars)?, self.inside(body, vars)?);
                Ok(format!(
                    "{iter}.map(|{bind}| {STD}::ok(if {pred} {{ Some({body}) }} else {{ None }})).filter_map({STD}::Result::transpose)"
                ))
            }
            _ => Err(Error::Invalid("a fold of other than a map".into())),
        }
    }

    /// The block writing the elements of `list`, where `pred` holds, each as the object `body`
    /// encodes, into one array.
    pub(super) fn encode_each(&mut self, t: &Typed, vars: &dyn Vars) -> Result<String, Error> {
        let Node::EncodeEach(list, pred, body) = &t.node else {
            return Err(Error::Invalid("an array that is no encoded items".into()));
        };
        let pred = pred.as_deref();
        let Node::Encode(object) = &body.node else {
            return Err(Error::Invalid(
                "an array of items that are no objects".into(),
            ));
        };
        let reads: Vec<&Typed> = pred.into_iter().chain([&**body]).collect();
        let (iter, bind) = self.over(list, &reads, vars)?;
        self.depth = self.depth.saturating_add(1);
        let pred = pred.map(|p| self.expr(p, vars)).transpose();
        let write = self.write_object(object, vars);
        self.depth = self.depth.saturating_sub(1);
        let write = format!("json.item(); {}", write?);
        let write = match pred? {
            Some(pred) => format!("if {pred} {{ {write}}} "),
            None => write,
        };
        Ok(format!(
            "match {iter} {{ it => {{ let mut json = {STD}::json::Text::with_capacity(it.size_hint().0.saturating_mul({}).saturating_add(2)); json.open_list(); for {bind} in it {{ {write}}} json.close_list(); json.done() }} }}",
            super::encode_parts::room(object)
        ))
    }

    /// A reference to the first element of `list` where `pred` holds, reading no further.
    pub(super) fn first(
        &mut self,
        list: &Typed,
        pred: &Typed,
        vars: &dyn Vars,
    ) -> Result<String, Error> {
        let (iter, bind) = self.over(list, &[pred], vars)?;
        let pred = self.inside(pred, vars)?;
        Ok(format!(
            "{STD}::collections::first({iter}, |{bind}| {STD}::ok({pred}))?"
        ))
    }

    /// An iterator over `list`, a list's elements or a map's entries, and the pattern binding at
    /// this depth what `bodies` read of each: the element, and its index.
    pub(super) fn over(
        &mut self,
        list: &Typed,
        bodies: &[&Typed],
        vars: &dyn Vars,
    ) -> Result<(String, String), Error> {
        if !matches!(list.ty, Type::List(_) | Type::Map(..)) {
            return Err(Error::Invalid(
                "a comprehension over other than a list or map".into(),
            ));
        }
        let d = self.depth;
        let read = |at: fn(&Node, usize) -> bool| bodies.iter().any(|b| walk::any(b, |n| at(n, d)));
        let item = match read(|n, d| matches!(n, Node::Item(i, _) if *i == d)) {
            true => format!("x{d}"),
            false => "_".to_string(),
        };
        let each = format!("{}.iter()", receiver(&self.place(list, vars)?));
        Ok(
            match read(|n, d| matches!(n, Node::Position(i) if *i == d)) {
                true => (format!("(0i64..).zip({each})"), format!("(n{d}, {item})")),
                false => (each, item),
            },
        )
    }

    /// `body` one comprehension deeper.
    pub(super) fn inside(&mut self, body: &Typed, vars: &dyn Vars) -> Result<String, Error> {
        self.depth = self.depth.saturating_add(1);
        let out = self.expr(body, vars);
        self.depth = self.depth.saturating_sub(1);
        out
    }
}
