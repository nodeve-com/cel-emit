//! A [`Dyn`] written as compact JSON text: serde_json's formatter writes each token, and this
//! walk decides what to write and refuses what JSON can't hold. A serde `Serialize` could stop
//! only with serde_json's text error, which links its message formatting into every program.

use serde::Serializer as _;
use serde_json::ser::{CompactFormatter, Formatter};

use super::shaped::Raw;
use crate::dynamic::Dyn;
use crate::time::{duration_text, timestamp_text};
use crate::{Error, Result};

/// Writes `x` to `w`.
pub(super) fn value(w: &mut Vec<u8>, x: &Dyn) -> Result<()> {
    let f = &mut CompactFormatter;
    match *x {
        Dyn::Null => io(f.write_null(w)),
        Dyn::Bool(b) => io(f.write_bool(w, b)),
        Dyn::Int(i) => io(f.write_i64(w, i)),
        Dyn::UInt(u) => io(f.write_u64(w, u)),
        Dyn::Double(d) if d.is_finite() => io(f.write_f64(w, d)),
        Dyn::Float32(x) if x.is_finite() => io(f.write_f32(w, x)),
        Dyn::Double(_) | Dyn::Float32(_) => Err(Error::Range),
        Dyn::Str(ref t) => string(w, t),
        Dyn::Timestamp(t) => string(w, &timestamp_text(t)),
        Dyn::Duration(d) => string(w, &duration_text(d)),
        Dyn::List(ref xs) => list(w, f, xs),
        Dyn::Map(ref m) => map(w, f, m),
        Dyn::Rest(_) => Err(Error::Cast),
    }
}

/// Writes `xs` as an array.
fn list(w: &mut Vec<u8>, f: &mut CompactFormatter, xs: &[Dyn]) -> Result<()> {
    io(f.begin_array(w))?;
    for (i, x) in xs.iter().enumerate() {
        io(f.begin_array_value(w, i == 0))?;
        value(w, x)?;
        io(f.end_array_value(w))?;
    }
    io(f.end_array(w))
}

/// Writes `m` as an object: its own members, then each member a `Rest` text holds that no own
/// member names.
fn map(w: &mut Vec<u8>, f: &mut CompactFormatter, m: &[(String, Dyn)]) -> Result<()> {
    io(f.begin_object(w))?;
    let (mut own, mut rest, mut first) = (Vec::new(), Vec::new(), true);
    for (k, v) in m {
        match v {
            Dyn::Rest(text) => rest.push(text),
            v => {
                let k = k.as_str();
                own.push(k);
                key(w, &mut first, k)?;
                value(w, v)?;
                io(f.end_object_value(w))?;
            }
        }
    }
    for text in rest {
        // Text that is no object adds nothing.
        let Some(raw) = super::shaped::object(text.as_bytes()) else {
            continue;
        };
        for (k, v) in raw.iter().filter(|(k, _)| !own.contains(k)) {
            key(w, &mut first, k)?;
            w.extend_from_slice(v.as_bytes());
            io(f.end_object_value(w))?;
        }
    }
    io(f.end_object(w))
}

/// `raw` as one JSON object text, each value as it arrived: UTF-8, as each part is.
pub(super) fn object(raw: &Raw) -> Result<String> {
    let (mut w, f, mut first) = (Vec::new(), &mut CompactFormatter, true);
    io(f.begin_object(&mut w))?;
    for (k, v) in raw.iter() {
        key(&mut w, &mut first, k)?;
        w.extend_from_slice(v.as_bytes());
        io(f.end_object_value(&mut w))?;
    }
    io(f.end_object(&mut w))?;
    String::from_utf8(w).map_err(|_| Error::Cast)
}

/// Writes an object's key `k`, up to where its value goes.
fn key(w: &mut Vec<u8>, first: &mut bool, k: &str) -> Result<()> {
    let f = &mut CompactFormatter;
    io(f.begin_object_key(w, core::mem::take(first)))?;
    string(w, k)?;
    io(f.end_object_key(w))?;
    io(f.begin_object_value(w))
}

/// Writes `s` quoted, escaped as serde_json escapes it.
pub(super) fn string(w: &mut Vec<u8>, s: &str) -> Result<()> {
    (&mut serde_json::Serializer::new(w))
        .serialize_str(s)
        .map_err(|_| Error::Cast)
}

/// A write's failure: a writer in memory never fails.
fn io(r: std::io::Result<()>) -> Result<()> {
    r.map_err(|_| Error::Cast)
}
