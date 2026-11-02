//! CEL's casts of a dynamic value: `int(x)`, `uint(x)`, `double(x)`, `string(x)`, and `bool(x)`
//! pick the conversion by the value's kind, as CEL does, so a string converts here where
//! [`super::to_int`] and the rest, which fill a typed place, refuse it. A kind CEL has no
//! conversion from is [`Error::Cast`].

use super::Dyn;
use crate::{conversions, time, Error, Result};

pub fn to_int(x: Dyn) -> Result<i64> {
    match x {
        Dyn::Int(i) => Ok(i),
        Dyn::UInt(u) => conversions::uint_to_int(u),
        Dyn::Double(d) => conversions::double_to_int(d),
        Dyn::Float32(f) => conversions::double_to_int(f64::from(f)),
        Dyn::Str(s) => conversions::string_to_int(&s),
        Dyn::Timestamp(t) => time::to_seconds(t),
        _ => Err(Error::Cast),
    }
}

pub fn to_uint(x: Dyn) -> Result<u64> {
    match x {
        Dyn::UInt(u) => Ok(u),
        Dyn::Int(i) => conversions::int_to_uint(i),
        Dyn::Double(d) => conversions::double_to_uint(d),
        Dyn::Float32(f) => conversions::double_to_uint(f64::from(f)),
        Dyn::Str(s) => conversions::string_to_uint(&s),
        _ => Err(Error::Cast),
    }
}

pub fn to_double(x: Dyn) -> Result<f64> {
    match x {
        Dyn::Int(i) => conversions::int_to_double(i),
        Dyn::UInt(u) => conversions::uint_to_double(u),
        Dyn::Double(d) => Ok(d),
        Dyn::Float32(f) => Ok(f64::from(f)),
        Dyn::Str(s) => conversions::string_to_double(&s),
        _ => Err(Error::Cast),
    }
}

pub fn to_str(x: Dyn) -> Result<String> {
    match x {
        Dyn::Str(s) => Ok(s),
        Dyn::Bool(b) => Ok(String::from(if b { "true" } else { "false" })),
        Dyn::Int(i) => conversions::int_to_string(i),
        Dyn::UInt(u) => conversions::uint_to_string(u),
        Dyn::Double(d) => conversions::double_to_string(d),
        Dyn::Float32(f) => conversions::float32_to_string(f),
        Dyn::Timestamp(t) => time::timestamp_string(t),
        Dyn::Duration(d) => time::duration_string(d),
        _ => Err(Error::Cast),
    }
}

pub fn to_bool(x: Dyn) -> Result<bool> {
    match x {
        Dyn::Bool(b) => Ok(b),
        Dyn::Str(s) => conversions::string_to_bool(&s),
        _ => Err(Error::Cast),
    }
}
