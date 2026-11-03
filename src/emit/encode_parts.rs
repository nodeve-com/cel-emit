//! What [`super::encode`]'s block writes with: the statements around a member, and the room an
//! object's text takes.

use std::fmt::Write as _;

use crate::functions::STD;
use crate::tree::{Object, Out, Typed};
use crate::walk;

/// Writes a member that may be left out for failing: `code` is a closure's body giving whether
/// it wrote one. A failure is cut back out and kept in `jf`; `written` takes the answer.
pub(super) fn kept_member(body: &mut String, code: &str, written: Option<&str>) {
    let to = written.map_or("let _ = ".to_string(), |w| format!("{w} = "));
    let call = format!("(|| -> {STD}::Result<bool> {{ {code} }})()");
    let _ = write!(
        body,
        "let jm = json.mark(); {to}match {call} {{ Ok(b) => b, Err(e) => {{ jf.get_or_insert(e); json.cut(jm); false }} }}; "
    );
}

/// Whether a member of `object` can be left out for failing.
pub(super) fn skips(object: &Object) -> bool {
    object.members.iter().any(|m| match &m.out {
        Out::Value(_, kept) | Out::Rest(_, kept) | Out::Optional { kept, .. } => *kept,
        Out::Object(o) => skips(o),
    })
}

/// The statements binding each of `held` that `t` reads, raising its error where it failed.
pub(super) fn reads(t: &Typed, held: &[usize]) -> String {
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
pub(super) fn room(object: &Object) -> usize {
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
