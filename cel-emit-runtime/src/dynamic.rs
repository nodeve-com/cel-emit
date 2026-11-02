//! A dynamic value: any one of what JSON holds, and conversions from it to a typed place, each
//! failing as CEL fills a typed place does: a wrong kind, a number of another kind included,
//! [`Error::Cast`], a missing key [`Error::NoSuchKey`]. Nothing converts across kinds.

use chrono::{DateTime, TimeDelta, Utc};

use crate::cmp::Equal;
use crate::{collections, conversions, strings, Error, Null, Result};

/// A value whose kind is known only when read. A float32 is a double that keeps its own
/// shortest digits when written.
#[derive(Clone, Debug)]
pub enum Dyn {
    Null,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Double(f64),
    Float32(f32),
    Str(String),
    Timestamp(DateTime<Utc>),
    Duration(TimeDelta),
    List(Vec<Dyn>),
    Map(Vec<(String, Dyn)>),
    /// JSON object text whose entries the map holding it takes as its own when written, never
    /// replacing a key the map has. Only an entry of a map, built by [`crate::json::rest`]; no
    /// expression reads one.
    Rest(String),
}

macro_rules! from {
    ($($t:ty => $v:ident),*) => {$(
        impl From<$t> for Dyn {
            fn from(x: $t) -> Dyn {
                Dyn::$v(x)
            }
        }
    )*};
}

from!(bool => Bool, i64 => Int, u64 => UInt, f64 => Double, f32 => Float32, String => Str,
    DateTime<Utc> => Timestamp, TimeDelta => Duration, Vec<Dyn> => List, Vec<(String, Dyn)> => Map);

impl From<Null> for Dyn {
    fn from(_: Null) -> Dyn {
        Dyn::Null
    }
}

/// A typed list as a dynamic one.
pub fn from_list<T: Into<Dyn>>(xs: Vec<T>) -> Result<Dyn> {
    Ok(Dyn::List(lift_list(xs)?))
}

/// A typed map, string-keyed, as a dynamic one.
pub fn from_map<T: Into<Dyn>>(m: Vec<(String, T)>) -> Result<Dyn> {
    Ok(Dyn::Map(lift_map(m)?))
}

/// A dynamic-keyed map as a dynamic one: a key of other than a string is [`Error::Cast`].
pub fn from_keyed(m: Vec<(Dyn, Dyn)>) -> Result<Dyn> {
    let entries = m.into_iter().map(|(k, v)| Ok((to_str(k)?, v)));
    Ok(Dyn::Map(entries.collect::<Result<_>>()?))
}

/// A message's fields as a dynamic map. Not generic, so no program compiles its own copy.
pub fn fields(entries: Vec<(String, Dyn)>) -> Dyn {
    Dyn::Map(entries)
}

/// A typed list's elements as dynamic ones.
pub fn lift_list<T: Into<Dyn>>(xs: Vec<T>) -> Result<Vec<Dyn>> {
    Ok(xs.into_iter().map(Into::into).collect())
}

/// A typed map's values as dynamic ones.
pub fn lift_map<K, T: Into<Dyn>>(m: Vec<(K, T)>) -> Result<Vec<(K, Dyn)>> {
    Ok(m.into_iter().map(|(k, v)| (k, v.into())).collect())
}

pub fn to_bool(x: Dyn) -> Result<bool> {
    match x {
        Dyn::Bool(b) => Ok(b),
        _ => Err(Error::Cast),
    }
}

pub fn to_str(x: Dyn) -> Result<String> {
    match x {
        Dyn::Str(s) => Ok(s),
        _ => Err(Error::Cast),
    }
}

/// A timestamp, or one from RFC 3339 text, as JSON carries it.
pub fn to_timestamp(x: Dyn) -> Result<DateTime<Utc>> {
    match x {
        Dyn::Timestamp(t) => Ok(t),
        x => crate::time::parse_timestamp(&to_str(x)?),
    }
}

/// A duration, or one from text, as JSON carries it: `1.5s`, or `humantime`'s other units.
pub fn to_duration(x: Dyn) -> Result<TimeDelta> {
    match x {
        Dyn::Duration(d) => Ok(d),
        x => crate::time::parse_duration(&to_str(x)?),
    }
}

/// Bytes from base64 text, as JSON carries them.
pub fn to_bytes(x: Dyn) -> Result<Vec<u8>> {
    conversions::base64_decode(&to_str(x)?)
}

pub fn to_list(x: Dyn) -> Result<Vec<Dyn>> {
    match x {
        Dyn::List(xs) => Ok(xs),
        _ => Err(Error::Cast),
    }
}

pub fn to_map(x: Dyn) -> Result<Vec<(String, Dyn)>> {
    match x {
        Dyn::Map(m) => Ok(m),
        _ => Err(Error::Cast),
    }
}

/// A double, as CEL fills a double place: an int or uint is [`Error::Cast`].
pub fn to_double(x: Dyn) -> Result<f64> {
    match x {
        Dyn::Double(d) => Ok(d),
        Dyn::Float32(f) => Ok(f64::from(f)),
        _ => Err(Error::Cast),
    }
}

/// An int, as CEL fills an int place: any other kind is [`Error::Cast`].
pub fn to_int(x: Dyn) -> Result<i64> {
    match x {
        Dyn::Int(i) => Ok(i),
        _ => Err(Error::Cast),
    }
}

/// A uint, as CEL fills a uint place: any other kind is [`Error::Cast`].
pub fn to_uint(x: Dyn) -> Result<u64> {
    match x {
        Dyn::UInt(u) => Ok(u),
        _ => Err(Error::Cast),
    }
}

/// `x` read by `read`, where it is there: absent stays absent.
pub fn optional<T>(x: Option<Dyn>, read: fn(Dyn) -> Result<T>) -> Result<Option<T>> {
    x.map(read).transpose()
}

/// `x` read by `read`, unless it is absent or null, which are null.
pub fn nullable<T>(x: Option<Dyn>, read: fn(Dyn) -> Result<T>) -> Result<Option<T>> {
    match x {
        None | Some(Dyn::Null) => Ok(None),
        Some(x) => read(x).map(Some),
    }
}

/// `x` as a map, as [`to_map`] reads it, by reference.
fn map_of(x: &Dyn) -> Result<&[(String, Dyn)]> {
    match x {
        Dyn::Map(m) => Ok(m),
        _ => Err(Error::Cast),
    }
}

/// `m[key]` of a dynamic map: a key it lacks is [`Error::NoSuchKey`].
pub fn get(x: &Dyn, key: &str) -> Result<Dyn> {
    optional_get(x, key)?.ok_or(Error::NoSuchKey)
}

/// `m[i]` of a dynamic list: outside it is [`Error::Range`].
pub fn at(x: &Dyn, i: i64) -> Result<Dyn> {
    match x {
        Dyn::List(xs) => collections::index_list(xs, i),
        _ => Err(Error::Cast),
    }
}

/// `has(m.key)` of a dynamic map.
pub fn has(x: &Dyn, key: &str) -> Result<bool> {
    Ok(map_of(x)?.iter().any(|(k, _)| k == key))
}

/// `m[?key]` of a dynamic map: absent where it lacks the key.
pub fn optional_get(x: &Dyn, key: &str) -> Result<Option<Dyn>> {
    let found = map_of(x)?.iter().find(|(k, _)| k == key);
    Ok(found.map(|(_, v)| v.clone()))
}

/// `o.?key`, `o` an optional dynamic map: absent where `o` is, or lacks the key.
pub fn chain(o: &Option<Dyn>, key: &str) -> Result<Option<Dyn>> {
    match o {
        Some(x) => optional_get(x, key),
        None => Ok(None),
    }
}

/// `x in container`: a list holds an equal element, a map the string key.
pub fn contains(x: &Dyn, container: &Dyn) -> Result<bool> {
    match container {
        Dyn::List(xs) => collections::in_list(x, xs),
        Dyn::Map(m) => match x {
            Dyn::Str(key) => Ok(m.iter().any(|(k, _)| k == key)),
            _ => Ok(false),
        },
        _ => Err(Error::Cast),
    }
}

/// `size(x)` of a string (code points), list, or map.
pub fn size(x: &Dyn) -> Result<i64> {
    match x {
        Dyn::Str(s) => strings::size(s),
        Dyn::List(xs) => collections::list_size(xs),
        Dyn::Map(m) => collections::map_size(m),
        _ => Err(Error::Cast),
    }
}

/// Numbers equal by value across int, uint, and double; other kinds equal only to their own.
impl Equal for Dyn {
    fn equal(&self, other: &Dyn) -> Result<bool> {
        Ok(match (self, other) {
            (Dyn::Null, Dyn::Null) => true,
            (Dyn::Bool(a), Dyn::Bool(b)) => a == b,
            (Dyn::Str(a), Dyn::Str(b)) => a == b,
            (Dyn::Timestamp(a), Dyn::Timestamp(b)) => a == b,
            (Dyn::Duration(a), Dyn::Duration(b)) => a == b,
            (Dyn::List(a), Dyn::List(b)) => a.equal(b)?,
            (Dyn::Map(a), Dyn::Map(b)) => a.equal(b)?,
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

fn number(x: &Dyn) -> Option<Num> {
    Some(match *x {
        Dyn::Int(i) => Num::Int(i),
        Dyn::UInt(u) => Num::UInt(u),
        Dyn::Double(d) => Num::Double(d),
        Dyn::Float32(f) => Num::Double(f64::from(f)),
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

#[cfg(test)]
mod tests;
