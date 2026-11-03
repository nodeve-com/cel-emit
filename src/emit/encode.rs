//! [`Node::Encode`](crate::Node::Encode): a block writing the object member by member into one
//! `json::Text`, a member that fails cut back out where flagged, else its error raised by `?`.
//! Where flagged members leave the object with none, the first error is raised.
//! The object's lets compute first, each holding its result: only a member reading one that
//! failed fails with it.

use std::fmt::Write as _;

use super::encode_parts::{kept_member, reads, room, skips};
use super::{Emitter, Vars};
use crate::functions::STD;
use crate::tree::{Member, Node, Object, Out, Typed};
use crate::types::Type;
use crate::Error;

impl Emitter {
    /// `object`'s text, as a string.
    pub(super) fn encode(&mut self, object: &Object, vars: &dyn Vars) -> Result<String, Error> {
        let members = self.write_object(object, vars)?;
        Ok(format!(
            "{{ let mut json = {STD}::json::Text::with_capacity({}); {members}json.done() }}",
            room(object)
        ))
    }

    /// The statements writing `object` onto the `json` in scope.
    pub(super) fn write_object(
        &mut self,
        object: &Object,
        vars: &dyn Vars,
    ) -> Result<String, Error> {
        let mut body = String::new();
        let skips = skips(object);
        if skips {
            body.push_str("let jb = json.mark(); let mut jf = None; ");
        }
        body.push_str("json.open(); ");
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
                "if let (true, Some(e)) = (json.bare(jb), jf) {{ {STD}::fail::<()>(e)?; }} "
            );
        }
        body.push_str("json.close(); ");
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

    /// The statement writing the dyn `x`: a scalar entering as a dyn is written as it is.
    fn write(&mut self, x: &Typed) -> Result<String, Error> {
        if let Node::ToDyn(inner) = &x.node {
            let f = match inner.ty {
                Type::String => Some("str"),
                Type::Int => Some("int"),
                Type::Uint => Some("uint"),
                Type::Double => Some("double"),
                Type::Bool => Some("bool"),
                _ => None,
            };
            if let Some(f) = f {
                let value = match inner.ty {
                    Type::String => self.emitter.reference(inner, true, self.vars)?,
                    _ => self.expr(inner)?,
                };
                return Ok(format!("json.{f}({value})?; "));
            }
        }
        Ok(format!("json.value({})?; ", self.expr(x)?))
    }

    /// Computes `object`'s lets, each holding its result; `held` and the lets.
    fn lets(&mut self, object: &Object, held: &[usize]) -> Result<Vec<usize>, Error> {
        let mut held = held.to_vec();
        for (n, value) in &object.lets {
            let code = match super::place::rooted(value) {
                true => self.emitter.reference(value, false, self.vars)?,
                false => self.expr(value)?,
            };
            let scope = reads(value, &held);
            let _ = write!(
                self.body,
                "let jl{n} = (|| {{ {scope}{STD}::ok({code}) }})(); "
            );
            held.push(*n);
        }
        Ok(held)
    }

    /// Writes `object`'s members, at nesting `d`, into the block.
    fn members(&mut self, object: &Object, d: usize, held: &[usize]) -> Result<(), Error> {
        let held = self.lets(object, held)?;
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
            Out::Value(..) | Out::Optional { .. } => self.scalar(out, slot, held)?,
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

    /// Writes a member whose `inner` statements run where `guard`, if any, holds. Where `kept`, a
    /// closure holds a failure; else the block raises it with `?`.
    fn plain(
        &mut self,
        (scope, guard, inner): (&str, Option<&str>, &str),
        kept: bool,
        slot: &Slot,
    ) {
        if kept {
            let code = match guard {
                Some(g) => {
                    format!("{scope}{g} {{ {inner}return {STD}::ok(true); }} {STD}::ok(false)")
                }
                None => format!("{scope}{inner}{STD}::ok(true)"),
            };
            return kept_member(&mut self.body, &code, slot.written.as_deref());
        }
        let flag = (slot.written.as_ref()).map_or(String::new(), |w| format!("{w} = true; "));
        let _ = match guard {
            Some(g) => write!(self.body, "{{ {scope}{g} {{ {inner}{flag}}} }} "),
            None => write!(self.body, "{{ {scope}{inner}{flag}}} "),
        };
    }

    /// Writes a member of a value or an optional.
    fn scalar(&mut self, out: &Out, slot: &Slot, held: &[usize]) -> Result<(), Error> {
        let key = &slot.key;
        match out {
            Out::Value(x, kept) => {
                let inner = format!("json.key({key}); {}", self.write(x)?);
                let scope = reads(x, held);
                self.plain((&scope, None, &inner), *kept, slot);
            }
            Out::Optional {
                optional,
                local,
                value,
                kept,
            } => {
                let (opt, value) = (self.expr(optional)?, self.write(value)?);
                let guard = format!("if let Some(l{local}) = &{opt}");
                let inner = format!("json.key({key}); {value}");
                let scope = reads(optional, held);
                let guard = Some(guard.as_str());
                self.plain((&scope, guard, &inner), *kept, slot);
            }
            _ => {}
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
