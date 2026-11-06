//! CEL's operations on a dynamic [`Value`]: conversions from it to a typed place, each failing as
//! CEL fills a typed place does: a wrong kind, a number of another kind included,
//! [`Error::Cast`], a missing key [`Error::NoSuchKey`]. Nothing converts across kinds.

use chrono::{DateTime, TimeDelta, Utc};

use crate::cmp::Equal;
use crate::{collections, conversions, strings, Error, Result, Value};

/// A typed list as a dynamic one.
pub fn from_list<T: Into<Value>>(xs: Vec<T>) -> Result<Value> {
    Ok(Value::List(lift_list(xs)?))
}

/// A typed map, string-keyed, as a dynamic one.
pub fn from_map<T: Into<Value>>(m: Vec<(String, T)>) -> Result<Value> {
    Ok(Value::Map(lift_map(m)?))
}

/// A dynamic-keyed map as a dynamic one: a key of other than a string is [`Error::Cast`].
pub fn from_keyed(m: Vec<(Value, Value)>) -> Result<Value> {
    let entries = m.into_iter().map(|(k, v)| match k {
        Value::Str(k) => Ok((k, v)),
        _ => Err(Error::Cast),
    });
    Ok(Value::Map(entries.collect::<Result<_>>()?))
}

/// A message's fields as a dynamic map. Not generic, so no program compiles its own copy.
pub fn fields(entries: Vec<(String, Value)>) -> Value {
    Value::Map(entries)
}

/// A typed list's elements as dynamic ones.
pub fn lift_list<T: Into<Value>>(xs: Vec<T>) -> Result<Vec<Value>> {
    Ok(xs.into_iter().map(Into::into).collect())
}

/// A typed map's values as dynamic ones.
pub fn lift_map<K, T: Into<Value>>(m: Vec<(K, T)>) -> Result<Vec<(K, Value)>> {
    Ok(m.into_iter().map(|(k, v)| (k, v.into())).collect())
}

pub fn to_bool(x: &Value) -> Result<bool> {
    match x {
        Value::Bool(b) => Ok(*b),
        _ => Err(Error::Cast),
    }
}

/// `x`'s text, no copy made.
pub fn str_of(x: &Value) -> Result<&str> {
    match x {
        Value::Str(s) => Ok(s),
        _ => Err(Error::Cast),
    }
}

pub fn to_str(x: &Value) -> Result<String> {
    str_of(x).map(String::from)
}

/// A timestamp, or one from RFC 3339 text, as JSON carries it.
pub fn to_timestamp(x: &Value) -> Result<DateTime<Utc>> {
    match x {
        Value::Timestamp(t) => Ok(*t),
        x => crate::time::parse_timestamp(str_of(x)?),
    }
}

/// A duration, or one from text, as JSON carries it: `1.5s`, or `humantime`'s other units.
pub fn to_duration(x: &Value) -> Result<TimeDelta> {
    match x {
        Value::Duration(d) => Ok(*d),
        x => crate::time::parse_duration(str_of(x)?),
    }
}

/// Bytes from base64 text, as JSON carries them.
pub fn to_bytes(x: &Value) -> Result<Vec<u8>> {
    conversions::base64_decode(str_of(x)?)
}

/// `x`'s elements, no copy made.
pub fn list_of(x: &Value) -> Result<&Vec<Value>> {
    match x {
        Value::List(xs) => Ok(xs),
        _ => Err(Error::Cast),
    }
}

pub fn to_list(x: &Value) -> Result<Vec<Value>> {
    list_of(x).cloned()
}

/// `x`'s entries, no copy made.
pub fn map_of(x: &Value) -> Result<&Vec<(String, Value)>> {
    match x {
        Value::Map(m) => Ok(m),
        _ => Err(Error::Cast),
    }
}

pub fn to_map(x: &Value) -> Result<Vec<(String, Value)>> {
    map_of(x).cloned()
}

/// A double, as a typed double field takes it: a double, or a whole number. Another kind is
/// [`Error::Cast`].
pub fn to_double(x: &Value) -> Result<f64> {
    to(x)
}

/// An int, as a typed int field takes it: an int, a uint that fits, or a double with no
/// fraction. A fractional double or another kind is [`Error::Cast`]; a number past int64 is
/// [`Error::Range`].
pub fn to_int(x: &Value) -> Result<i64> {
    to(x)
}

/// A uint, as a typed uint field takes it: a uint, a non-negative int, or a double with no
/// fraction. A fractional double or another kind is [`Error::Cast`]; a negative or past uint64
/// is [`Error::Range`].
pub fn to_uint(x: &Value) -> Result<u64> {
    to(x)
}

/// `x` read by `read`, where it is there: absent stays absent.
pub fn optional<T>(x: Option<&Value>, read: fn(&Value) -> Result<T>) -> Result<Option<T>> {
    x.map(read).transpose()
}

/// `x` read by `read`, unless it is absent or null, which are null.
pub fn nullable<T>(x: Option<&Value>, read: fn(&Value) -> Result<T>) -> Result<Option<T>> {
    match x {
        None | Some(Value::Null) => Ok(None),
        Some(x) => read(x).map(Some),
    }
}

/// `m[key]` of a dynamic map, no copy made: a key it lacks is [`Error::NoSuchKey`].
pub fn get<'a>(x: &'a Value, key: &str) -> Result<&'a Value> {
    optional_get(x, key)?.ok_or(Error::NoSuchKey)
}

/// `m[i]` of a dynamic list, no copy made: outside it, as [`collections::item`] has it.
pub fn at(x: &Value, i: i64) -> Result<&Value> {
    collections::item(list_of(x)?, i)
}

/// `has(m.key)` of a dynamic map.
pub fn has(x: &Value, key: &str) -> Result<bool> {
    Ok(optional_get(x, key)?.is_some())
}

/// `m[?key]` of a dynamic map, no copy made: absent where it lacks the key.
pub fn optional_get<'a>(x: &'a Value, key: &str) -> Result<Option<&'a Value>> {
    let found = map_of(x)?.iter().find(|(k, _)| k == key);
    Ok(found.map(|(_, v)| v))
}

/// `o.?key`, `o` an optional dynamic map: absent where `o` is, or lacks the key.
pub fn chain<'a>(o: Option<&'a Value>, key: &str) -> Result<Option<&'a Value>> {
    match o {
        Some(x) => optional_get(x, key),
        None => Ok(None),
    }
}

/// `x in container`: a list holds an equal element, a map the string key.
pub fn contains(x: &Value, container: &Value) -> Result<bool> {
    match container {
        Value::List(xs) => collections::in_list(x, xs),
        Value::Map(m) => match x {
            Value::Str(key) => Ok(m.iter().any(|(k, _)| k == key)),
            _ => Ok(false),
        },
        _ => Err(Error::Cast),
    }
}

/// `size(x)` of a string (code points), list, or map.
pub fn size(x: &Value) -> Result<i64> {
    match x {
        Value::Str(s) => strings::size(s),
        Value::List(xs) => collections::list_size(xs),
        Value::Map(m) => collections::map_size(m),
        _ => Err(Error::Cast),
    }
}

/// Numbers equal by value across int, uint, and double; other kinds equal only to their own.
impl Equal for Value {
    fn equal(&self, other: &Value) -> Result<bool> {
        Ok(match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Timestamp(a), Value::Timestamp(b)) => a == b,
            (Value::Duration(a), Value::Duration(b)) => a == b,
            (Value::List(a), Value::List(b)) => a.equal(b)?,
            (Value::Map(a), Value::Map(b)) => a.equal(b)?,
            (a, b) => number(a).zip(number(b)).is_some_and(|(a, b)| a.equals(b)),
        })
    }
}

/// A number's value, exact.
#[derive(Clone, Copy)]
enum Num {
    Int(i64),
    UInt(u64),
    Double(f64),
}

fn number(x: &Value) -> Option<Num> {
    Some(match *x {
        Value::Int(i) => Num::Int(i),
        Value::UInt(u) => Num::UInt(u),
        Value::Double(d) => Num::Double(d),
        Value::Float32(f) => Num::Double(f64::from(f)),
        _ => return None,
    })
}

impl Num {
    fn equals(self, other: Num) -> bool {
        match (self, other) {
            (Num::Int(a), Num::Int(b)) => a == b,
            (Num::UInt(a), Num::UInt(b)) => a == b,
            (Num::Double(a), Num::Double(b)) => a == b,
            (Num::Int(i), Num::UInt(u)) | (Num::UInt(u), Num::Int(i)) => {
                u64::try_from(i).is_ok_and(|i| i == u)
            }
            (Num::Int(i), Num::Double(d)) | (Num::Double(d), Num::Int(i)) => {
                d.fract() == 0.0 && conversions::double_to_int(d) == Ok(i)
            }
            (Num::UInt(u), Num::Double(d)) | (Num::Double(d), Num::UInt(u)) => {
                d.fract() == 0.0 && conversions::double_to_uint(d) == Ok(u)
            }
        }
    }
}

pub mod cast;

/// A `T` filled from `x`; a value `T` does not accept is [`Error::Cast`], and a number past the
/// width of the integer or float32 it fills is [`Error::Range`].
pub fn to<T: serde::de::DeserializeOwned>(x: &Value) -> Result<T> {
    any_value::to(x).map_err(|e| match e {
        any_value::Error::Kind => Error::Cast,
        any_value::Error::Range => Error::Range,
    })
}

#[cfg(test)]
mod tests;
