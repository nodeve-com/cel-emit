//! `json.decode` and `json.encode`: text to a [`Dyn`] and back, through `serde_json`, building and
//! walking the value in place.
//!
//! A whole number decodes as an int, a uint past int64, any other number as a double; malformed
//! text or nesting past serde_json's depth limit is [`Error::Cast`]. A repeated object key keeps
//! its last value, in the place of its first.
//! Encoding writes null as `null`, a float32 in its own shortest digits, a timestamp and a
//! duration as their text, and refuses what JSON
//! can't hold: NaN and infinities are [`Error::Range`]. A [`Dyn::Rest`] entry of a map writes
//! its object text's entries into that map, each as the text has it.

use core::fmt;

use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

use crate::dynamic::Dyn;
use crate::{conversions, Error, Result};

/// Parses `text` as one JSON value and nothing after it.
pub fn decode(text: &str) -> Result<Dyn> {
    serde_json::from_str(text).map_err(|_| Error::Cast)
}

/// `x` as compact JSON text.
pub fn encode(x: &Dyn) -> Result<String> {
    let mut text = Vec::new();
    write::value(&mut text, x)?;
    String::from_utf8(text).map_err(|_| Error::Cast)
}

/// The entries of the map `x` whose keys are not in `keys`, as compact JSON object text; absent
/// where none remain. Each value is written as [`encode`] writes it.
pub fn encode_without(x: &Dyn, keys: &[String]) -> Result<Option<String>> {
    let Dyn::Map(map) = x else {
        return Err(Error::Cast);
    };
    let kept: Vec<_> = map
        .iter()
        .filter(|(k, _)| !keys.contains(k))
        .cloned()
        .collect();
    if kept.is_empty() {
        return Ok(None);
    }
    encode(&Dyn::Map(kept)).map(Some)
}

/// `text`, JSON object text, as an entry of a map that writes its entries as the map's own, each
/// value as the text has it: a key the map has is kept, and text that is no object adds nothing.
pub fn rest(text: String) -> Result<Dyn> {
    Ok(Dyn::Rest(text))
}

impl<'de> Deserialize<'de> for Dyn {
    fn deserialize<D: Deserializer<'de>>(d: D) -> core::result::Result<Dyn, D::Error> {
        d.deserialize_any(Build(&Shape::Plain))
    }
}

/// Builds a [`Dyn`], reading numbers and objects as its shape has them.
struct Build(&'static Shape);

impl<'de> Visitor<'de> for Build {
    type Value = Dyn;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_unit<E: de::Error>(self) -> core::result::Result<Dyn, E> {
        Ok(Dyn::Null)
    }

    fn visit_bool<E: de::Error>(self, b: bool) -> core::result::Result<Dyn, E> {
        Ok(Dyn::Bool(b))
    }

    fn visit_i64<E: de::Error>(self, i: i64) -> core::result::Result<Dyn, E> {
        Ok(match self.0 {
            Shape::Uint => u64::try_from(i).map_or(Dyn::Int(i), Dyn::UInt),
            Shape::Double => Dyn::Double(i as f64),
            _ => Dyn::Int(i),
        })
    }

    fn visit_u64<E: de::Error>(self, u: u64) -> core::result::Result<Dyn, E> {
        Ok(match self.0 {
            Shape::Double => Dyn::Double(u as f64),
            Shape::Uint => Dyn::UInt(u),
            _ => i64::try_from(u).map_or(Dyn::UInt(u), Dyn::Int),
        })
    }

    fn visit_f64<E: de::Error>(self, d: f64) -> core::result::Result<Dyn, E> {
        let whole = d.fract() == 0.0;
        Ok(match self.0 {
            Shape::Int if whole => conversions::double_to_int(d).map_or(Dyn::Double(d), Dyn::Int),
            Shape::Uint if whole => {
                conversions::double_to_uint(d).map_or(Dyn::Double(d), Dyn::UInt)
            }
            _ => Dyn::Double(d),
        })
    }

    fn visit_str<E: de::Error>(self, s: &str) -> core::result::Result<Dyn, E> {
        self.visit_string(s.into())
    }

    fn visit_string<E: de::Error>(self, s: String) -> core::result::Result<Dyn, E> {
        Ok(Dyn::Str(s))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> core::result::Result<Dyn, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element::<Dyn>()? {
            items.push(item);
        }
        Ok(Dyn::List(items))
    }

    /// Every key: a message shape reads its object's keys itself, never through here.
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> core::result::Result<Dyn, A::Error> {
        let mut entries = Entries::default();
        while let Some(key) = map.next_key::<String>()? {
            entries.put(key, map.next_value::<Dyn>()?);
        }
        Ok(entries.into_dyn())
    }
}

/// An object's entries as they arrive: a key seen again replaces the value it had. `at` holds
/// each key's place in `list`, sorted by key.
#[derive(Default)]
pub(crate) struct Entries {
    list: Vec<(String, Dyn)>,
    at: Vec<(String, usize)>,
}

impl Entries {
    pub(crate) fn put(&mut self, key: String, value: Dyn) {
        match self.at.binary_search_by(|(k, _)| k.as_str().cmp(&key)) {
            Ok(i) => {
                let place = self.at.get(i).map(|(_, p)| *p);
                if let Some(e) = place.and_then(|p| self.list.get_mut(p)) {
                    e.1 = value;
                }
            }
            Err(i) => {
                self.at.insert(i, (key.clone(), self.list.len()));
                self.list.push((key, value));
            }
        }
    }

    pub(crate) fn into_dyn(self) -> Dyn {
        Dyn::Map(self.list)
    }
}

mod scan;
mod shaped;
mod text;
mod write;
pub use shaped::{decode_shaped, field, has_field, optional_field, Key, Shape, Slots};
pub use text::Text;

#[cfg(test)]
mod tests;
