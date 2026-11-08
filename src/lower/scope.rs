//! Lexical scope: a `cel.bind` name and a comprehension variable resolve together, the innermost
//! first. A bind's initializer is lowered once, where it is defined, and held in a local.

use super::parts::{is_message, parts, rebuild};
use super::select::is_literal;
use super::*;

/// A `cel.bind` around what is lowering.
pub(crate) struct Bind<'a> {
    name: String,
    /// How many comprehensions enclosed the bind where it was defined.
    depth: usize,
    init: &'a SpannedExpr,
    /// What the name stands for: its initializer's value as locals.
    what: Typed,
}

/// What a name resolves to.
pub(crate) enum Scoped<'s, 'a> {
    /// A comprehension's variable.
    Loop,
    Bind(&'s Bind<'a>),
}

/// `t` with each of `lets` bound around it, the first outermost.
pub(crate) fn wrap(t: Typed, lets: &[(usize, Typed)]) -> Typed {
    lets.iter().rev().fold(t, |body, (n, value)| {
        Typed::new(
            body.ty.clone(),
            Node::Let(*n, Box::new(value.clone()), Box::new(body)),
        )
    })
}

impl<'a> Lower<'a> {
    /// What `name` resolves to: a comprehension's variable or a bind, whichever is innermost.
    pub(crate) fn local(&self, name: &str) -> Option<Scoped<'_, 'a>> {
        let looped = (self.loops.iter())
            .rposition(|l| l.index.as_deref() == Some(name) || l.item.as_deref() == Some(name));
        let bound = self.binds.iter().rfind(|b| b.name == name);
        match (looped, bound) {
            (Some(n), Some(b)) if b.depth <= n => Some(Scoped::Loop),
            (_, Some(b)) => Some(Scoped::Bind(b)),
            (Some(_), None) => Some(Scoped::Loop),
            (None, None) => None,
        }
    }

    /// What `name` stands for, if a bind is what it resolves to.
    pub(crate) fn bound(&self, name: &str) -> Option<&Typed> {
        match self.local(name)? {
            Scoped::Bind(b) => Some(&b.what),
            Scoped::Loop => None,
        }
    }

    /// The initializer of the bind `name` resolves to.
    pub(crate) fn bound_init(&self, name: &str) -> Option<&'a SpannedExpr> {
        match self.local(name)? {
            Scoped::Bind(b) => Some(b.init),
            Scoped::Loop => None,
        }
    }

    /// `name` bound to `init`, lowered here once, around what `body` lowers: the locals to bind
    /// around the result. An initializer's error is the result's, used or not.
    pub(crate) fn binding<T>(
        &mut self,
        name: &str,
        init: &'a SpannedExpr,
        body: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<(T, Vec<(usize, Typed)>), Error> {
        let mut lets = Vec::new();
        let value = match self.selected(init)? {
            Some(message) => message,
            None => self.expr(init)?,
        };
        let what = self.locals(value, &mut lets)?;
        self.binds.push(Bind {
            name: name.to_string(),
            depth: self.loops.len(),
            init,
            what,
        });
        let out = body(self);
        self.binds.pop();
        Ok((out?, lets))
    }

    /// `v` with each value that is no literal held in a new local, pushed on `lets`.
    fn locals(&mut self, v: Typed, lets: &mut Vec<(usize, Typed)>) -> Result<Typed, Error> {
        if is_message(&v) {
            let ty = v.ty.clone();
            let mut held = Vec::new();
            for (name, field) in parts(v) {
                held.push((name, self.locals(field, lets)?));
            }
            return rebuild(&ty, held);
        }
        if is_literal(&v.node) {
            return Ok(v);
        }
        let n = self.local_number()?;
        let local = Typed::new(v.ty.clone(), Node::Local(n));
        lets.push((n, v));
        Ok(local)
    }

    /// The next number for a local.
    pub(crate) fn local_number(&mut self) -> Result<usize, Error> {
        let n = self.next;
        self.next = n
            .checked_add(1)
            .ok_or(Error::Unsupported("too many locals".into()))?;
        Ok(n)
    }
}
