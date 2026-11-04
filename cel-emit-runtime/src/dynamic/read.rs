//! A [`Dyn`] read by any serde `Deserialize`: a type fills itself from a dynamic value, no copy
//! of the value made. A timestamp or duration reads as the text `json.encode` writes for it.

use core::fmt;

use serde::de::value::{MapDeserializer, SeqDeserializer};
use serde::de::{self, DeserializeOwned, IntoDeserializer, Visitor};
use serde::forward_to_deserialize_any;

use super::Dyn;
use crate::time::{duration_text, timestamp_text};
use crate::{Error, Result};

/// A `T` filled from `x`; a value `T` does not accept is [`Error::Cast`], and a number past the
/// width of the integer or float32 it fills is [`Error::Range`].
pub fn to<T: DeserializeOwned>(x: &Dyn) -> Result<T> {
    T::deserialize(x).map_err(|e| e.0)
}

/// What a read fails with: an error of its own for a number out of range, else a cast.
#[derive(Debug)]
pub struct ReadError(Error);

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl de::StdError for ReadError {}

impl de::Error for ReadError {
    fn custom<T: fmt::Display>(_: T) -> Self {
        ReadError(Error::Cast)
    }
}

/// `x` as a whole number: an int, a uint, or a double with no fraction. A fractional double or
/// another kind is [`Error::Cast`]; a double past `i128` is [`Error::Range`].
pub(super) fn whole(x: &Dyn) -> Result<i128> {
    let d = match x {
        Dyn::Int(i) => return Ok(i128::from(*i)),
        Dyn::UInt(u) => return Ok(i128::from(*u)),
        Dyn::Double(d) => *d,
        Dyn::Float32(f) => f64::from(*f),
        _ => return Err(Error::Cast),
    };
    if d.fract() != 0.0 {
        return Err(Error::Cast);
    }
    // 2^100 is far past every width; the cast below stays exact.
    if d.abs() >= 2f64.powi(100) {
        return Err(Error::Range);
    }
    Ok(d as i128)
}

/// `x` as a double: any int, uint, or double.
pub(super) fn real(x: &Dyn) -> Result<f64> {
    match x {
        Dyn::Int(i) => Ok(*i as f64),
        Dyn::UInt(u) => Ok(*u as f64),
        Dyn::Double(d) => Ok(*d),
        Dyn::Float32(f) => Ok(f64::from(*f)),
        _ => Err(Error::Cast),
    }
}

macro_rules! integers {
    ($($method:ident $visit:ident $ty:ty),*) => {$(
        fn $method<V: Visitor<'de>>(self, v: V) -> core::result::Result<V::Value, ReadError> {
            let n = whole(self).map_err(ReadError)?;
            v.$visit(<$ty>::try_from(n).map_err(|_| ReadError(Error::Range))?)
        }
    )*};
}

impl<'de> IntoDeserializer<'de, ReadError> for &'de Dyn {
    type Deserializer = &'de Dyn;

    fn into_deserializer(self) -> &'de Dyn {
        self
    }
}

impl<'de> de::Deserializer<'de> for &'de Dyn {
    type Error = ReadError;

    fn deserialize_any<V: Visitor<'de>>(self, v: V) -> core::result::Result<V::Value, ReadError> {
        match self {
            Dyn::Null => v.visit_unit(),
            Dyn::Bool(b) => v.visit_bool(*b),
            Dyn::Int(i) => v.visit_i64(*i),
            Dyn::UInt(u) => v.visit_u64(*u),
            Dyn::Double(d) => v.visit_f64(*d),
            Dyn::Float32(f) => v.visit_f32(*f),
            Dyn::Str(s) => v.visit_borrowed_str(s),
            Dyn::Timestamp(t) => v.visit_string(timestamp_text(*t)),
            Dyn::Duration(d) => v.visit_string(duration_text(*d)),
            Dyn::List(xs) => v.visit_seq(SeqDeserializer::new(xs.iter())),
            Dyn::Map(m) => {
                let entries = m.iter().map(|(k, v)| (k.as_str(), v));
                v.visit_map(MapDeserializer::new(entries))
            }
        }
    }

    fn deserialize_option<V: Visitor<'de>>(
        self,
        v: V,
    ) -> core::result::Result<V::Value, ReadError> {
        match self {
            Dyn::Null => v.visit_none(),
            x => v.visit_some(x),
        }
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        v: V,
    ) -> core::result::Result<V::Value, ReadError> {
        match self {
            Dyn::Str(s) => v.visit_enum(s.as_str().into_deserializer()),
            x => x.deserialize_any(v),
        }
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        v: V,
    ) -> core::result::Result<V::Value, ReadError> {
        v.visit_newtype_struct(self)
    }

    integers! {
        deserialize_i8 visit_i8 i8, deserialize_i16 visit_i16 i16,
        deserialize_i32 visit_i32 i32, deserialize_i64 visit_i64 i64,
        deserialize_i128 visit_i128 i128, deserialize_u8 visit_u8 u8,
        deserialize_u16 visit_u16 u16, deserialize_u32 visit_u32 u32,
        deserialize_u64 visit_u64 u64, deserialize_u128 visit_u128 u128
    }

    fn deserialize_f64<V: Visitor<'de>>(self, v: V) -> core::result::Result<V::Value, ReadError> {
        v.visit_f64(real(self).map_err(ReadError)?)
    }

    fn deserialize_f32<V: Visitor<'de>>(self, v: V) -> core::result::Result<V::Value, ReadError> {
        let d = real(self).map_err(ReadError)?;
        if d.is_finite() && d.abs() > f64::from(f32::MAX) {
            return Err(ReadError(Error::Range));
        }
        v.visit_f32(d as f32)
    }

    forward_to_deserialize_any! {
        bool char str string bytes byte_buf unit
        unit_struct seq tuple tuple_struct map struct identifier ignored_any
    }
}

#[cfg(test)]
mod tests;
