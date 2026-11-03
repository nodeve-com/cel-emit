//! `json.decode` and `json.encode` as the interpreter runs them, for the differential harness to
//! compare with cel-emit-runtime's: numbers decode as integer text to int (uint past int64), other
//! numbers to double, a repeated object key keeping its last value; a timestamp and a duration
//! encode as their text; malformed text or a value JSON can't hold is an error. The shaped `json.decode(text, Message)` is declared without a body:
//! the interpreter has no message values to build.

use std::fmt;
use std::sync::Arc;

use cel_core::{CelType, FunctionDecl, OverloadDecl, Value};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

pub(super) fn declarations() -> [FunctionDecl; 2] {
    let shaped = OverloadDecl::function(
        "json_decode_shaped",
        vec![CelType::String, CelType::type_of(CelType::type_param("T"))],
        CelType::type_param("T"),
    )
    .with_type_params(vec!["T".to_string()]);
    let decode = OverloadDecl::function("json_decode", vec![CelType::String], CelType::Dyn)
        .with_impl(|args| match args {
            [Value::String(text)] => decoded(text),
            _ => Value::error("json.decode takes a string"),
        });
    let encode = OverloadDecl::function("json_encode", vec![CelType::Dyn], CelType::String)
        .with_impl(|args| match args {
            [v] => match written(v) {
                Some(j) => Value::String(Arc::from(j.to_string())),
                None => Value::error("json.encode: not JSON"),
            },
            _ => Value::error("json.encode takes one value"),
        });
    [
        FunctionDecl::new("json.decode").with_overloads([decode, shaped]),
        FunctionDecl::new("json.encode").with_overload(encode),
    ]
}

/// JSON `text` as the value `json.decode` gives, or its error.
pub fn decoded(text: &str) -> Value {
    match serde_json::from_str::<Parsed>(text) {
        Ok(Parsed(v)) => v,
        Err(e) => Value::error(format!("json.decode: {e}")),
    }
}

/// `v` as JSON, if JSON holds it.
fn written(v: &Value) -> Option<serde_json::Value> {
    use serde_json::Value as J;
    Some(match v {
        Value::Null => J::Null,
        Value::Bool(b) => J::Bool(*b),
        Value::Int(i) => J::from(*i),
        Value::UInt(u) => J::from(*u),
        Value::Double(d) => J::Number(serde_json::Number::from_f64(*d)?),
        Value::String(s) => J::String(s.to_string()),
        Value::Timestamp(t) => {
            J::String(cel_emit_runtime::time::timestamp_text(t.to_datetime_utc()?))
        }
        Value::Duration(d) => J::String(cel_emit_runtime::time::duration_text(d.to_chrono())),
        Value::List(items) => J::Array(items.iter().map(written).collect::<Option<_>>()?),
        Value::Map(m) => J::Object(
            m.iter()
                .map(|(k, v)| match k {
                    cel_core::MapKey::String(k) => Some((k.to_string(), written(v)?)),
                    _ => None,
                })
                .collect::<Option<_>>()?,
        ),
        _ => return None,
    })
}

struct Parsed(Value);

impl<'de> Deserialize<'de> for Parsed {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Parsed, D::Error> {
        d.deserialize_any(Build)
    }
}

struct Build;

impl<'de> Visitor<'de> for Build {
    type Value = Parsed;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_unit<E: de::Error>(self) -> Result<Parsed, E> {
        Ok(Parsed(Value::Null))
    }

    fn visit_bool<E: de::Error>(self, b: bool) -> Result<Parsed, E> {
        Ok(Parsed(Value::Bool(b)))
    }

    fn visit_i64<E: de::Error>(self, i: i64) -> Result<Parsed, E> {
        Ok(Parsed(Value::Int(i)))
    }

    fn visit_u64<E: de::Error>(self, u: u64) -> Result<Parsed, E> {
        Ok(Parsed(i64::try_from(u).map_or(Value::UInt(u), Value::Int)))
    }

    fn visit_f64<E: de::Error>(self, d: f64) -> Result<Parsed, E> {
        Ok(Parsed(Value::Double(d)))
    }

    fn visit_str<E: de::Error>(self, s: &str) -> Result<Parsed, E> {
        Ok(Parsed(Value::String(Arc::from(s))))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Parsed, A::Error> {
        let mut items = Vec::new();
        while let Some(Parsed(item)) = seq.next_element()? {
            items.push(item);
        }
        Ok(Parsed(Value::List(items.into())))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Parsed, A::Error> {
        let mut out = cel_core::ValueMap::new();
        while let Some(key) = map.next_key::<String>()? {
            let key = cel_core::MapKey::from(key);
            let Parsed(value) = map.next_value()?;
            out.insert(key, value);
        }
        Ok(Parsed(Value::Map(Arc::new(out))))
    }
}
