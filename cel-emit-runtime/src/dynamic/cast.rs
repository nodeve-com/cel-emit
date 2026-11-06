//! CEL's casts of a dynamic value: `int(x)`, `uint(x)`, `double(x)`, `string(x)`, and `bool(x)`
//! pick the conversion by the value's kind, as CEL does, so a string converts here where
//! [`super::to_int`] and the rest, which fill a typed place, refuse it. A kind CEL has no
//! conversion from is [`Error::Cast`].

use super::Value;
use crate::{conversions, time, Error, Result};

pub fn to_int(x: &Value) -> Result<i64> {
    match x {
        Value::Int(i) => Ok(*i),
        Value::UInt(u) => conversions::uint_to_int(*u),
        Value::Double(d) => conversions::double_to_int(*d),
        Value::Float32(f) => conversions::double_to_int(f64::from(*f)),
        Value::Str(s) => conversions::string_to_int(s),
        Value::Timestamp(t) => time::to_seconds(*t),
        _ => Err(Error::Cast),
    }
}

pub fn to_uint(x: &Value) -> Result<u64> {
    match x {
        Value::UInt(u) => Ok(*u),
        Value::Int(i) => conversions::int_to_uint(*i),
        Value::Double(d) => conversions::double_to_uint(*d),
        Value::Float32(f) => conversions::double_to_uint(f64::from(*f)),
        Value::Str(s) => conversions::string_to_uint(s),
        _ => Err(Error::Cast),
    }
}

pub fn to_double(x: &Value) -> Result<f64> {
    match x {
        Value::Int(i) => conversions::int_to_double(*i),
        Value::UInt(u) => conversions::uint_to_double(*u),
        Value::Double(d) => Ok(*d),
        Value::Float32(f) => Ok(f64::from(*f)),
        Value::Str(s) => conversions::string_to_double(s),
        _ => Err(Error::Cast),
    }
}

pub fn to_str(x: &Value) -> Result<String> {
    match x {
        Value::Str(s) => Ok(s.clone()),
        Value::Bool(b) => Ok(String::from(if *b { "true" } else { "false" })),
        Value::Int(i) => conversions::int_to_string(*i),
        Value::UInt(u) => conversions::uint_to_string(*u),
        Value::Double(d) => conversions::double_to_string(*d),
        Value::Float32(f) => conversions::float32_to_string(*f),
        Value::Timestamp(t) => time::timestamp_string(*t),
        Value::Duration(d) => time::duration_string(*d),
        _ => Err(Error::Cast),
    }
}

pub fn to_bool(x: &Value) -> Result<bool> {
    match x {
        Value::Bool(b) => Ok(*b),
        Value::Str(s) => conversions::string_to_bool(s),
        _ => Err(Error::Cast),
    }
}
