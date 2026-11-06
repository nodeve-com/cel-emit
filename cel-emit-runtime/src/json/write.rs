//! A [`Value`] written as compact JSON text: serde_json's formatter writes each token, and this
//! walk decides what to write and refuses what JSON can't hold. A serde `Serialize` could stop
//! only with serde_json's text error, which links its message formatting into every program.

use serde::Serializer as _;
use serde_json::ser::{CompactFormatter, Formatter};

use crate::time::{duration_text, timestamp_text};
use crate::Value;
use crate::{Error, Result};

/// Writes `x` to `w`.
pub(super) fn value(w: &mut Vec<u8>, x: &Value) -> Result<()> {
    let f = &mut CompactFormatter;
    match *x {
        Value::Null => io(f.write_null(w)),
        Value::Bool(b) => io(f.write_bool(w, b)),
        Value::Int(i) => io(f.write_i64(w, i)),
        Value::UInt(u) => io(f.write_u64(w, u)),
        Value::Double(d) if d.is_finite() => io(f.write_f64(w, d)),
        Value::Float32(x) if x.is_finite() => io(f.write_f32(w, x)),
        Value::Double(_) | Value::Float32(_) => Err(Error::Range),
        Value::Str(ref t) => string(w, t),
        Value::Timestamp(t) => string(w, &timestamp_text(t)),
        Value::Duration(d) => string(w, &duration_text(d)),
        Value::List(ref xs) => list(w, f, xs),
        Value::Map(ref m) => map(w, f, m),
    }
}

/// Writes `xs` as an array.
fn list(w: &mut Vec<u8>, f: &mut CompactFormatter, xs: &[Value]) -> Result<()> {
    io(f.begin_array(w))?;
    for (i, x) in xs.iter().enumerate() {
        io(f.begin_array_value(w, i == 0))?;
        value(w, x)?;
        io(f.end_array_value(w))?;
    }
    io(f.end_array(w))
}

/// Writes `m` as an object.
fn map(w: &mut Vec<u8>, f: &mut CompactFormatter, m: &[(String, Value)]) -> Result<()> {
    io(f.begin_object(w))?;
    let mut first = true;
    for (k, v) in m {
        key(w, &mut first, k)?;
        value(w, v)?;
        io(f.end_object_value(w))?;
    }
    io(f.end_object(w))
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
