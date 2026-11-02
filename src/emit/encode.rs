//! [`Node::Encode`](crate::Node::Encode): a block writing the object member by member into one
//! `json::Text`, a member that fails cut back out where flagged, else its error raised by `?`.
//! Where flagged members leave the object with none, the first error is raised.
//! The object's lets compute first, each holding its result: only a member reading one that
//! failed fails with it.

use std::fmt::Write as _;

use super::{Emitter, Vars};
use crate::functions::STD;
use crate::tree::{Object, Out, Typed};
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
        self.members(&mut body, object, 0, vars, &[])?;
        if skips {
            let _ = write!(
                body,
                "if let (1, Some(e)) = (json.mark(), jf) {{ {STD}::fail::<()>(e)?; }} "
            );
        }
        body.push_str("json.close(); json.done() }");
        Ok(body)
    }

    /// Writes `object`'s members, at nesting `d`, into the block `body`.
    fn members(
        &mut self,
        body: &mut String,
        object: &Object,
        d: usize,
        vars: &dyn Vars,
        held: &[usize],
    ) -> Result<(), Error> {
        let mut held = held.to_vec();
        for (n, value) in &object.lets {
            let code = self.expr(value, vars)?;
            let scope = reads(value, &held);
            let _ = write!(body, "let jl{n} = (|| {{ {scope}{STD}::ok({code}) }})(); ");
            held.push(*n);
        }
        // Which members were written, where a rest's entries must leave their keys out.
        let rest = object
            .members
            .iter()
            .any(|m| matches!(m.out, Out::Rest(..)));
        let flag = |i: usize| format!("jw{d}_{i}");
        if rest {
            let _ = write!(body, "let mut jr{d} = None::<String>; ");
        }
        let mut own = Vec::new();
        for (i, m) in object.members.iter().enumerate() {
            let key = serde_json::to_string(&m.key).map_err(|e| Error::Invalid(e.to_string()))?;
            let key = format!("{:?}", format!("{key}:"));
            let written = match &m.out {
                Out::Rest(..) => None,
                _ if rest => {
                    own.push(format!("({:?}, {})", m.key, flag(i)));
                    let _ = write!(body, "let mut {} = false; ", flag(i));
                    Some(flag(i))
                }
                _ => None,
            };
            if let Some(present) = &m.present {
                let _ = write!(body, "if {} {{ ", self.expr(present, vars)?);
            }
            match &m.out {
                Out::Value(x, kept) => {
                    let scope = reads(x, &held);
                    let x = self.expr(x, vars)?;
                    let write = format!("json.key({key}); json.value({x})?; ");
                    let code = format!("{scope}{write}{STD}::ok(true)");
                    member(body, &code, *kept, written.as_deref());
                }
                Out::Optional {
                    optional,
                    local,
                    value,
                    kept,
                } => {
                    let scope = reads(optional, &held);
                    let (optional, value) = (self.expr(optional, vars)?, self.expr(value, vars)?);
                    let code = format!(
                        "{scope}if let Some(l{local}) = &{optional} {{ json.key({key}); json.value({value})?; return {STD}::ok(true); }} {STD}::ok(false)"
                    );
                    member(body, &code, *kept, written.as_deref());
                }
                Out::Rest(x, kept) => {
                    let scope = reads(x, &held);
                    let some = if matches!(x.ty, Type::Optional(_)) {
                        ""
                    } else {
                        "Some"
                    };
                    let x = self.expr(x, vars)?;
                    match kept {
                        true => write!(
                            body,
                            "jr{d} = match (|| {{ {scope}{STD}::ok({x}) }})() {{ Ok(t) => {some}(t), Err(e) => {{ jf.get_or_insert(e); None }} }}; "
                        ),
                        false => write!(body, "jr{d} = {{ {scope}{some}({x}) }}; "),
                    }
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                }
                Out::Object(o) => {
                    let _ = write!(body, "{{ let jm = json.key({key}); json.open(); ");
                    self.members(body, o, d.saturating_add(1), vars, &held)?;
                    let _ = match &written {
                        Some(w) => write!(body, "{w} = json.close_or_cut(jm); }} "),
                        None => write!(body, "json.close_or_cut(jm); }} "),
                    };
                }
            }
            if m.present.is_some() {
                body.push_str("} ");
            }
        }
        if rest {
            let _ = write!(
                body,
                "if let Some(t) = jr{d} {{ json.rest(t, &[{}])?; }} ",
                own.join(", ")
            );
        }
        Ok(())
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
