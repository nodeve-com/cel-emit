//! What a native program of cases needs beside `program`: bindings as Rust expressions, a result
//! written as text, and a run's lines read back as answers.

use cel_emit::emit::Vars;
use cel_emit::{Error, Type};

/// The bindings, each already a Rust expression.
pub struct Bound(pub Vec<String>);

impl Vars for Bound {
    fn read(&self, var: usize) -> Result<String, Error> {
        self.0
            .get(var)
            .cloned()
            .ok_or_else(|| Error::Invalid("no such input".into()))
    }
    fn has(&self, _: usize) -> Result<String, Error> {
        Ok("true".into())
    }
}

/// A Rust expression of type `String` writing `v`, a value of `ty`, as `values::show` writes it,
/// its doubles through the program's `dbl`.
pub fn show(ty: &Type, v: &str, depth: usize) -> String {
    let x = format!("x{depth}");
    match ty {
        Type::Int => format!("format!(\"{{}}\", {v})"),
        Type::Uint => format!("format!(\"{{}}u\", {v})"),
        Type::Bool => format!("format!(\"{{}}\", {v})"),
        Type::Double => format!("dbl({v})"),
        Type::String => format!("format!(\"{{:?}}\", {v})"),
        Type::Bytes => format!("format!(\"b{{:?}}\", {v})"),
        Type::Timestamp => {
            format!("format!(\"t{{}}.{{}}\", {v}.timestamp(), {v}.timestamp_subsec_nanos())")
        }
        Type::Duration => format!("format!(\"d{{}}.{{}}\", {v}.num_seconds(), {v}.subsec_nanos())"),
        Type::Narrow(cel_emit::Width::F32) => format!("dbl(f64::from({v}))"),
        Type::Narrow(w) if matches!(w.cel(), Type::Uint) => {
            format!("format!(\"{{}}u\", u64::from({v}))")
        }
        Type::Narrow(_) => format!("format!(\"{{}}\", i64::from({v}))"),
        Type::List(t) => {
            let one = show(t, &format!("(*{x})"), depth + 1);
            format!(
                "format!(\"[{{}}]\", {v}.iter().map(|{x}| {one}).collect::<Vec<String>>().join(\",\"))"
            )
        }
        Type::Map(_, t) => {
            let one = show(t, &format!("{x}.1"), depth + 1);
            format!(
                "{{ let mut e: Vec<String> = {v}.iter().map(|{x}| format!(\"{{:?}}:{{}}\", {x}.0, {one})).collect(); e.sort(); format!(\"{{{{{{}}}}}}\", e.join(\",\")) }}"
            )
        }
        // A message in a list: only that it is one, as no other result holds one.
        Type::Message(_) => "String::from(\"<message>\")".into(),
        other => unreachable!("a result of {other:?}"),
    }
}

/// Each line of a run as its case's number and answer.
pub fn answers(lines: Vec<String>) -> Vec<(usize, String)> {
    lines
        .iter()
        .filter_map(|l| {
            let (n, v) = l.split_once('\t')?;
            Some((n.parse().ok()?, v.to_string()))
        })
        .collect()
}
