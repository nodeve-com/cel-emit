//! [`Node::Encode`](crate::Node::Encode): a block writing the object member by member into one
//! `json::Text`, a member that fails cut back out where flagged, else its error raised by `?`.
//! Where flagged members leave the object with none, the first error is raised.
//! The object's lets compute first, each holding its result: only a member reading one that
//! failed fails with it.

use std::fmt::Write as _;

use super::{Emitter, Vars};
use crate::functions::STD;
use crate::tree::{Member, Object, Out, Typed};
use crate::types::Type;
use crate::{walk, Error};

impl Emitter {
    /// `object`'s text, as a string.
    pub(super) fn encode(&mut self, object: &Object, vars: &dyn Vars) -> Result<String, Error> {
        let mut body = format!(
            "{{ let mut json = {STD}::json::Text::with_capacity({}); json.open(); ",
            room(object)
        );
        let skips = skips(object);
        if skips {
            body.push_str("let mut jf = None; ");
        }
        let mut block = Block {
            emitter: self,
            vars,
            body,
        };
        block.members(object, 0, &[])?;
        let mut body = block.body;
        if skips {
            let _ = write!(
                body,
                "if let (1, Some(e)) = (json.mark(), jf) {{ {STD}::fail::<()>(e)?; }} "
            );
        }
        body.push_str("json.close(); json.done() }");
        Ok(body)
    }
}

/// The block being written, and what its expressions emit with.
struct Block<'a> {
    emitter: &'a mut Emitter,
    vars: &'a dyn Vars,
    body: String,
}

/// One object's place in the block: its nesting, the lets it holds, and whether a member is a
/// rest.
struct Level {
    d: usize,
    held: Vec<usize>,
    rest: bool,
    /// The keys and flags of the members written so far, which a rest's entries leave out.
    own: Vec<String>,
}

/// One member's JSON key as a Rust literal, and the flag recording that it was written.
struct Slot {
    key: String,
    written: Option<String>,
}

impl Block<'_> {
    fn expr(&mut self, x: &Typed) -> Result<String, Error> {
        self.emitter.expr(x, self.vars)
    }

    /// Writes `object`'s members, at nesting `d`, into the block.
    fn members(&mut self, object: &Object, d: usize, held: &[usize]) -> Result<(), Error> {
        let mut held = held.to_vec();
        for (n, value) in &object.lets {
            let code = self.expr(value)?;
            let scope = reads(value, &held);
            let _ = write!(
                self.body,
                "let jl{n} = (|| {{ {scope}{STD}::ok({code}) }})(); "
            );
            held.push(*n);
        }
        // Which members were written, where a rest's entries must leave their keys out.
        let rest = object
            .members
            .iter()
            .any(|m| matches!(m.out, Out::Rest(..)));
        if rest {
            let _ = write!(self.body, "let mut jr{d} = None::<String>; ");
        }
        let mut level = Level {
            d,
            held,
            rest,
            own: Vec::new(),
        };
        for (i, m) in object.members.iter().enumerate() {
            self.member(i, m, &mut level)?;
        }
        if rest {
            let _ = write!(
                self.body,
                "if let Some(t) = jr{d} {{ json.rest(t, &[{}])?; }} ",
                level.own.join(", ")
            );
        }
        Ok(())
    }

    /// Writes member `m`, the `i`th of its object.
    fn member(&mut self, i: usize, m: &Member, level: &mut Level) -> Result<(), Error> {
        let key = serde_json::to_string(&m.key).map_err(|e| Error::Invalid(e.to_string()))?;
        let key = format!("{:?}", format!("{key}:"));
        let flag = format!("jw{}_{i}", level.d);
        let written = (level.rest && !matches!(m.out, Out::Rest(..))).then(|| {
            level.own.push(format!("({:?}, {flag})", m.key));
            flag
        });
        if let Some(w) = &written {
            let _ = write!(self.body, "let mut {w} = false; ");
        }
        if let Some(present) = &m.present {
            let present = self.expr(present)?;
            let _ = write!(self.body, "if {present} {{ ");
        }
        self.out(&m.out, &Slot { key, written }, level)?;
        if m.present.is_some() {
            self.body.push_str("} ");
        }
        Ok(())
    }

    /// Writes what a member gives.
    fn out(&mut self, out: &Out, slot: &Slot, level: &Level) -> Result<(), Error> {
        let held = &level.held;
        let key = &slot.key;
        match out {
            Out::Value(x, kept) => {
                let scope = reads(x, held);
                let x = self.expr(x)?;
                let write = format!("json.key({key}); json.value({x})?; ");
                let code = format!("{scope}{write}{STD}::ok(true)");
                member(&mut self.body, &code, *kept, slot.written.as_deref());
            }
            Out::Optional {
                optional,
                local,
                value,
                kept,
            } => {
                let scope = reads(optional, held);
                let (optional, value) = (self.expr(optional)?, self.expr(value)?);
                let code = format!(
                    "{scope}if let Some(l{local}) = &{optional} {{ json.key({key}); json.value({value})?; return {STD}::ok(true); }} {STD}::ok(false)"
                );
                member(&mut self.body, &code, *kept, slot.written.as_deref());
            }
            Out::Rest(x, kept) => self.rest(x, *kept, level)?,
            Out::Object(o) => {
                let _ = write!(self.body, "{{ let jm = json.key({key}); json.open(); ");
                self.members(o, level.d.saturating_add(1), held)?;
                let _ = match &slot.written {
                    Some(w) => write!(self.body, "{w} = json.close_or_cut(jm); }} "),
                    None => write!(self.body, "json.close_or_cut(jm); }} "),
                };
            }
        }
        Ok(())
    }

    /// Writes a rest: the text whose other keys' entries follow the members.
    fn rest(&mut self, x: &Typed, kept: bool, level: &Level) -> Result<(), Error> {
        let d = level.d;
        let scope = reads(x, &level.held);
        let some = if matches!(x.ty, Type::Optional(_)) {
            ""
        } else {
            "Some"
        };
        let x = self.expr(x)?;
        match kept {
            true => write!(
                self.body,
                "jr{d} = match (|| {{ {scope}{STD}::ok({x}) }})() {{ Ok(t) => {some}(t), Err(e) => {{ jf.get_or_insert(e); None }} }}; "
            ),
            false => write!(self.body, "jr{d} = {{ {scope}{some}({x}) }}; "),
        }
        .map_err(|e| Error::Invalid(e.to_string()))
    }
}

/// Writes a member: `code` is a closure's body giving whether it wrote one. Where `kept`, a
/// failure is cut back out and kept in `jf`; `written` takes the answer.
fn member(body: &mut String, code: &str, kept: bool, written: Option<&str>) {
    let to = written.map_or("let _ = ".to_string(), |w| format!("{w} = "));
    let call = format!("(|| -> {STD}::Result<bool> {{ {code} }})()");
    let _ = match kept {
        true => write!(
            body,
            "let jm = json.mark(); {to}match {call} {{ Ok(b) => b, Err(e) => {{ jf.get_or_insert(e); json.cut(jm); false }} }}; "
        ),
        false => write!(body, "{to}{call}?; "),
    };
}

/// Whether a member of `object` can be left out for failing.
fn skips(object: &Object) -> bool {
    object.members.iter().any(|m| match &m.out {
        Out::Value(_, kept) | Out::Rest(_, kept) | Out::Optional { kept, .. } => *kept,
        Out::Object(o) => skips(o),
    })
}

/// The statements binding each of `held` that `t` reads, raising its error where it failed.
fn reads(t: &Typed, held: &[usize]) -> String {
    let mut read = walk::locals(t);
    read.retain(|n| held.contains(n));
    read.sort_unstable();
    read.dedup();
    read.iter().fold(String::new(), |mut out, n| {
        let _ = write!(out, "let l{n} = jl{n}.as_ref().map_err(|e| *e)?; ");
        out
    })
}

/// Bytes enough for `object`'s text where each value is short: its keys, and room for each value.
fn room(object: &Object) -> usize {
    object.members.iter().fold(2, |n, m| {
        let value = match &m.out {
            Out::Object(o) => room(o),
            _ => 16,
        };
        n.saturating_add(m.key.len())
            .saturating_add(4)
            .saturating_add(value)
    })
}
