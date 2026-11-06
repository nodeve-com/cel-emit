//! `json.decode` and `json.encode`: text to a [`Value`] and back, through `serde_json`, building and
//! walking the value in place.
//!
//! How a value builds from text, and how a number keeps its kind, is [`Value`]'s `Deserialize`
//! (any-value). Malformed text, or nesting past serde_json's depth limit, is [`Error::Cast`].
//! Encoding writes a value as any-value's `Serialize` does, but refuses what JSON can't hold: NaN
//! and infinities are [`Error::Range`].

use crate::{Error, Result, Value};

/// Parses `text` as one JSON value and nothing after it.
pub fn decode(text: &str) -> Result<Value> {
    serde_json::from_str(text).map_err(|_| Error::Cast)
}

/// `x` as compact JSON text.
pub fn encode(x: &Value) -> Result<String> {
    let mut text = Vec::new();
    write::value(&mut text, x)?;
    String::from_utf8(text).map_err(|_| Error::Cast)
}

/// What `x`'s own `Serialize` writes, as compact JSON text: `json.encode` of a message that
/// serializes itself.
pub fn own<T: serde::Serialize + ?Sized>(x: &T) -> Result<String> {
    serde_json::to_string(x).map_err(|_| Error::Cast)
}

/// What `x`'s own `Serialize` writes, as a dynamic value: a message that serializes itself,
/// inside what `json.encode` writes.
pub fn serialized<T: serde::Serialize + ?Sized>(x: &T) -> Result<Value> {
    let text = serde_json::to_vec(x).map_err(|_| Error::Cast)?;
    serde_json::from_slice(&text).map_err(|_| Error::Cast)
}

mod text;
mod write;
pub use text::Text;

#[cfg(test)]
mod tests;
