//! `json.decode(text, Message)`: the text read as a [`Shape`] into [`Slots`], keeping only the keys
//! it declares.
//!
//! Every other key's value is skipped unread, however odd: its bytes scanned for its extent and
//! syntax, never parsed or built, and so without a depth limit (a value read
//! keeps serde_json's). A [`Shape::Open`] object instead keeps them, as one JSON object text of
//! the other keys, each value as it arrived and scanned as a skipped one is, in the slot the
//! shape gives. A repeated key keeps its last value. A number fills its place as JSON Schema
//! has it: an integral one such as `2.0` is an int where the shape wants an int or uint, and any
//! number is a double where it wants a double. A value of any other kind is left as it is for
//! the read to refuse.
//!
//! Each declared key has its own slot, which the shape names; slot 0 holds the document. A
//! key's slot holds its value where the text gives it. A message's slot holds an empty map where
//! the value is an object, and the value itself where it is not, which each message inside it
//! then holds too. The embedder's field reads index the slots, each checking its message's slot
//! first, as reading a key of a map would.

use std::borrow::Cow;

use serde::de::DeserializeSeed;
use serde::Deserializer;

use super::{scan, Build};
use crate::dynamic::Dyn;
use crate::{Error, Result};

#[cfg(test)]
mod tests;

/// A decoded document's values, one slot each, empty where the text gives none.
pub type Slots = Vec<Option<Dyn>>;

/// A declared key: its name, its slot, and its shape.
pub type Key = (&'static str, usize, Shape);

/// What a place in the document is read as.
#[derive(Clone, Copy, Debug)]
pub enum Shape {
    Int,
    Uint,
    Double,
    /// A bool, a string, or a value read as it is.
    Plain,
    /// An object: its declared keys.
    Message(&'static [Key]),
    /// A message that also keeps the keys it does not declare, as one JSON object text of them
    /// in the slot given; a document with no such key leaves that slot empty.
    Open(&'static [Key], usize),
}

/// Parses `text` as one JSON value and nothing after it, read as `shape` into `slots` slots. A
/// string's bytes are UTF-8 by construction; serde_json checks those of each value it reads.
pub fn decode_shaped(text: &str, shape: &'static Shape, slots: usize) -> Result<Slots> {
    let mut out = vec![None; slots];
    place(text.as_bytes(), shape, 0, &mut out)?;
    Ok(out)
}

/// The value of the key in slot `at` of the message in slot `of`: [`Error::NoSuchKey`] where
/// either is absent, [`Error::Cast`] where the message's value is no object.
pub fn field(slots: &[Option<Dyn>], of: i64, at: i64) -> Result<Dyn> {
    optional_field(slots, of, at)?.ok_or(Error::NoSuchKey)
}

/// Whether the message in slot `of` has the key in slot `at`, read as [`field`] reads it.
pub fn has_field(slots: &[Option<Dyn>], of: i64, at: i64) -> Result<bool> {
    Ok(optional_field(slots, of, at)?.is_some())
}

/// The value of the key in slot `at`, absent where the message in slot `of` lacks it.
pub fn optional_field(slots: &[Option<Dyn>], of: i64, at: i64) -> Result<Option<Dyn>> {
    let slot = |i: i64| {
        let i = usize::try_from(i).map_err(|_| Error::Cast)?;
        slots.get(i).ok_or(Error::Cast)
    };
    match slot(of)? {
        None => Err(Error::NoSuchKey),
        Some(Dyn::Map(_)) => slot(at).cloned(),
        Some(_) => Err(Error::Cast),
    }
}

/// `shape`'s keys and its slot for the others, where it is a message.
fn message(shape: &'static Shape) -> Option<(&'static [Key], Option<usize>)> {
    match *shape {
        Shape::Message(keys) => Some((keys, None)),
        Shape::Open(keys, rest) => Some((keys, Some(rest))),
        _ => None,
    }
}

fn put(out: &mut [Option<Dyn>], slot: usize, value: Option<Dyn>) -> Result<()> {
    *out.get_mut(slot).ok_or(Error::Cast)? = value;
    Ok(())
}

/// `value`, one JSON value and nothing after it, read as `shape` into `slot`: an object a message
/// shape reads key by key, anything else through serde_json. A message read again first empties
/// what its keys held.
fn place(value: &[u8], shape: &'static Shape, slot: usize, out: &mut [Option<Dyn>]) -> Result<()> {
    let Some((keys, rest)) = message(shape) else {
        return put(out, slot, Some(parse(value, shape)?));
    };
    if matches!(out.get(slot), Some(Some(_))) {
        empty(keys, rest, out)?;
    }
    let Some(body) = scan::ws(value).strip_prefix(b"{") else {
        let other = parse(value, shape)?;
        put(out, slot, Some(other.clone()))?;
        return refuse(keys, &other, out);
    };
    put(out, slot, Some(Dyn::Map(Vec::new())))?;
    let after = members(body, keys, rest, out)?;
    match scan::ws(after).is_empty() {
        true => Ok(()),
        false => Err(Error::Cast),
    }
}

/// Every slot of `keys` and of the messages inside them emptied.
fn empty(keys: &'static [Key], rest: Option<usize>, out: &mut [Option<Dyn>]) -> Result<()> {
    for (_, slot, shape) in keys {
        put(out, *slot, None)?;
        if let Some((keys, rest)) = message(shape) {
            empty(keys, rest, out)?;
        }
    }
    rest.map_or(Ok(()), |rest| put(out, rest, None))
}

/// Each message inside `keys` given `other`, the value no object their message has.
fn refuse(keys: &'static [Key], other: &Dyn, out: &mut [Option<Dyn>]) -> Result<()> {
    for (_, slot, shape) in keys {
        if let Some((keys, _)) = message(shape) {
            put(out, *slot, Some(other.clone()))?;
            refuse(keys, other, out)?;
        }
    }
    Ok(())
}

/// `text` built as `shape` by serde_json.
fn parse(text: &[u8], shape: &'static Shape) -> Result<Dyn> {
    let mut de = serde_json::Deserializer::from_slice(text);
    let read = Seed(shape)
        .deserialize(&mut de)
        .and_then(|d| de.end().map(|()| d));
    read.map_err(|_| Error::Cast)
}

struct Seed(&'static Shape);

impl<'de> DeserializeSeed<'de> for Seed {
    type Value = Dyn;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> core::result::Result<Dyn, D::Error> {
        d.deserialize_any(Build(self.0))
    }
}

/// The bytes of `from` that `to`, a part of it running to its end, doesn't hold.
fn part<'t>(from: &'t [u8], to: &[u8]) -> Result<&'t [u8]> {
    from.get(..from.len().wrapping_sub(to.len()))
        .ok_or(Error::Cast)
}

/// `text`'s members, where it is one JSON object and nothing after it.
pub(super) fn object(text: &[u8]) -> Option<Raw> {
    let body = scan::ws(text).strip_prefix(b"{")?;
    let mut raw = Raw::default();
    let after = scan_members(body, |key, value| {
        raw.put(key, value);
        Ok(())
    })
    .ok()?;
    scan::ws(after).is_empty().then_some(raw)
}

/// An object's members by key, sorted, each value the text it arrived as: a key seen again keeps
/// its last value.
#[derive(Default)]
pub(super) struct Raw(Vec<(String, String)>);

impl Raw {
    fn put(&mut self, key: Cow<'_, [u8]>, value: &[u8]) -> Option<()> {
        let key = String::from_utf8(key.into_owned()).ok()?;
        let value = String::from_utf8(value.into()).ok()?;
        match self.0.binary_search_by(|(k, _)| k.as_str().cmp(&key)) {
            Ok(i) => self.0.get_mut(i)?.1 = value,
            Err(i) => self.0.insert(i, (key, value)),
        }
        Some(())
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

/// The members of the object `body`, just past its `{`, starts with, read into `out` as `keys`
/// declare, and what follows it; the others are kept in `rest`'s slot where there is one.
fn members<'t>(
    body: &'t [u8],
    keys: &'static [Key],
    rest: Option<usize>,
    out: &mut [Option<Dyn>],
) -> Result<&'t [u8]> {
    let mut others = Raw::default();
    let after = scan_members(body, |key, value| {
        match keys.iter().find(|(name, ..)| name.as_bytes() == &*key) {
            Some((_, slot, shape)) => place(value, shape, *slot, out)?,
            None if rest.is_some() => others.put(key, value).ok_or(Error::Cast)?,
            None => {}
        }
        Ok(())
    })?;
    if let (Some(rest), false) = (rest, others.0.is_empty()) {
        put(out, rest, Some(Dyn::Str(super::write::object(&others)?)))?;
    }
    Ok(after)
}

/// Each member of the object `body`, just past its `{`, starts with, its key's own bytes and its
/// value's text handed to `each`; what follows the object.
fn scan_members<'t>(
    mut body: &'t [u8],
    mut each: impl FnMut(Cow<'_, [u8]>, &'t [u8]) -> Result<()>,
) -> Result<&'t [u8]> {
    body = scan::ws(body);
    if let Some(after) = body.strip_prefix(b"}") {
        return Ok(after);
    }
    loop {
        let quoted = body.strip_prefix(b"\"").ok_or(Error::Cast)?;
        let (named, escaped) = scan::string(quoted).ok_or(Error::Cast)?;
        // The key's own bytes, its escapes decoded where it has any.
        let key: Cow<'_, [u8]> = match escaped {
            false => {
                let (_, key) = part(quoted, named)?.split_last().ok_or(Error::Cast)?;
                Cow::Borrowed(key)
            }
            true => match serde_json::from_slice(part(body, named)?) {
                Ok(Dyn::Str(key)) => Cow::Owned(key.as_bytes().to_vec()),
                _ => return Err(Error::Cast),
            },
        };
        let colon = scan::ws(named).strip_prefix(b":").ok_or(Error::Cast)?;
        let start = scan::ws(colon);
        let end = scan::value(start).ok_or(Error::Cast)?;
        each(key, part(start, end)?)?;
        match scan::ws(end).split_first() {
            Some((b',', next)) => body = scan::ws(next),
            Some((b'}', after)) => return Ok(after),
            _ => return Err(Error::Cast),
        }
    }
}
