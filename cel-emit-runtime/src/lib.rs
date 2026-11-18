#![doc = include_str!("../README.md")]

pub mod cmp;
pub mod collections;
pub mod computed;
pub mod conversions;
pub mod dynamic;
pub mod fields;
pub mod json;
pub mod lists;
pub mod math;
pub mod number;
pub mod optional;
pub mod strings;
pub mod time;

use std::borrow::Cow;

/// A value of any one kind, and null: what emitted code names for them.
pub use any_value::{Null, Value};
pub use chrono::{DateTime, TimeDelta, Utc};
/// A constant `matches` pattern's table, as emitted code holds it in a `static`.
pub use pattern_emit::{Pattern, Unusable};

/// A CEL error: what a computation gives instead of a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// No value: a variable not bound.
    Absent,
    /// A map selected or indexed by a key it lacks: CEL's "no such key", an error, not absence.
    NoSuchKey,
    /// Int arithmetic or a conversion past its type's range.
    Overflow,
    /// Division or modulus by zero.
    Division,
    /// A function given an argument it can't take.
    Function,
    /// A string that won't convert to the value asked of it.
    Cast,
    /// A value a check refuses.
    Range,
    /// A dynamic value read as a type that fails at a place inside it.
    Unfit(Box<Unfit>),
}

/// Where inside a dynamic value it fails to fit the type it's read as, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unfit {
    /// The place, as a path that follows the value's own: `.n`, `.pair[0]`.
    pub place: Cow<'static, str>,
    /// Why, in a word the reader picks, as `cast`.
    pub reason: &'static str,
}

impl Error {
    /// A value failing at `place` inside it, for `reason`.
    pub fn unfit(place: impl Into<Cow<'static, str>>, reason: &'static str) -> Error {
        Error::Unfit(Box::new(Unfit {
            place: place.into(),
            reason,
        }))
    }

    /// Each error that names no place.
    pub const ALL: [Error; 7] = [
        Error::Absent,
        Error::NoSuchKey,
        Error::Overflow,
        Error::Division,
        Error::Function,
        Error::Cast,
        Error::Range,
    ];
}

pub type Result<T> = core::result::Result<T, Error>;

/// A computed value read by reference gives up its error as itself.
impl From<&Error> for Error {
    fn from(e: &Error) -> Error {
        e.clone()
    }
}

/// `value`, as a closure computing it gives it.
pub fn ok<T>(value: T) -> Result<T> {
    Ok(value)
}

/// `error`, as a value of whatever type is wanted.
pub fn fail<T>(error: Error) -> Result<T> {
    Err(error)
}

/// `value` where `ok` holds, else `error`.
pub fn check<T>(value: T, ok: bool, error: Error) -> Result<T> {
    if ok {
        Ok(value)
    } else {
        Err(error)
    }
}
