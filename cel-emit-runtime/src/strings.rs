//! Strings: CEL's standard functions on them and its strings extension.

use std::sync::Mutex;

use regex_lite::Regex;

use crate::{Error, Pattern, Result};

mod format;
pub use format::format;
mod rewrite;
pub use rewrite::*;

pub fn contains(s: &str, sub: &str) -> Result<bool> {
    Ok(s.contains(sub))
}

pub fn starts_with(s: &str, prefix: &str) -> Result<bool> {
    Ok(s.starts_with(prefix))
}

pub fn ends_with(s: &str, suffix: &str) -> Result<bool> {
    Ok(s.ends_with(suffix))
}

/// In code points.
pub fn size(s: &str) -> Result<i64> {
    i64::try_from(s.chars().count()).map_err(|_| Error::Overflow)
}

pub fn concat(a: &str, b: &str) -> Result<String> {
    Ok([a, b].concat())
}

pub fn trim(s: &str) -> Result<String> {
    Ok(s.trim().to_string())
}

/// Byte offset where code point `at` of `s` starts: `s`'s length at its end. [`Error::Function`]
/// below 0 or past the end.
fn byte_at(s: &str, at: i64) -> Result<usize> {
    let at = usize::try_from(at).map_err(|_| Error::Function)?;
    s.char_indices()
        .map(|(b, _)| b)
        .chain(std::iter::once(s.len()))
        .nth(at)
        .ok_or(Error::Function)
}

/// `s` from byte `at`, a char boundary of `s`.
fn rest(s: &str, at: usize) -> Result<&str> {
    s.get(at..).ok_or(Error::Function)
}

/// Code points of `s` before byte `at`, a char boundary of `s`.
fn chars_before(s: &str, at: usize) -> Result<i64> {
    let head = s.get(..at).ok_or(Error::Function)?;
    i64::try_from(head.chars().count()).map_err(|_| Error::Overflow)
}

/// The code point index of `sub`'s first occurrence in `s`, from code point `offset`; -1 where
/// none. [`Error::Function`] where `offset` is past `s`.
pub fn index_of_from(s: &str, sub: &str, offset: i64) -> Result<i64> {
    let tail = rest(s, byte_at(s, offset)?)?;
    match tail.find(sub) {
        Some(b) => offset
            .checked_add(chars_before(tail, b)?)
            .ok_or(Error::Overflow),
        None => Ok(-1),
    }
}

pub fn index_of(s: &str, sub: &str) -> Result<i64> {
    index_of_from(s, sub, 0)
}

/// The code point index of `sub`'s last occurrence starting at or before byte `limit`; -1
/// where none.
fn last_before(s: &str, sub: &str, limit: usize) -> Result<i64> {
    let found = s
        .char_indices()
        .map(|(b, _)| b)
        .chain(std::iter::once(s.len()))
        .rev()
        .find(|b| *b <= limit && s.get(*b..).is_some_and(|t| t.starts_with(sub)));
    match found {
        Some(b) => chars_before(s, b),
        None => Ok(-1),
    }
}

/// The code point index of `sub`'s last occurrence in `s`; -1 where none.
pub fn last_index_of(s: &str, sub: &str) -> Result<i64> {
    last_before(s, sub, s.len())
}

/// As [`last_index_of`], but an occurrence must start at or before code point `offset`.
/// [`Error::Function`] where `offset` is past `s`.
pub fn last_index_of_from(s: &str, sub: &str, offset: i64) -> Result<i64> {
    last_before(s, sub, byte_at(s, offset)?)
}

/// `s` from code point `from` to its end. [`Error::Function`] where `from` is past `s`.
pub fn substring(s: &str, from: i64) -> Result<String> {
    Ok(rest(s, byte_at(s, from)?)?.to_string())
}

/// `s` from code point `from` up to code point `to`. [`Error::Function`] where either is past
/// `s`, or `to` comes before `from`.
pub fn substring_between(s: &str, from: i64, to: i64) -> Result<String> {
    let (a, b) = (byte_at(s, from)?, byte_at(s, to)?);
    if from > to {
        return Err(Error::Function);
    }
    Ok(s.get(a..b).ok_or(Error::Function)?.to_string())
}

/// The code point at `at` as a string; empty at `s`'s end. [`Error::Function`] past it.
pub fn char_at(s: &str, at: i64) -> Result<String> {
    Ok(rest(s, byte_at(s, at)?)?.chars().take(1).collect())
}

/// Whether `s` matches anywhere by `pattern`, a constant pattern's table. [`Error::Function`]
/// where the table is damaged or its search can't finish.
pub fn matches_table(pattern: &'static Pattern, s: &str) -> Result<bool> {
    pattern.is_match(s).map_err(|_| Error::Function)
}

/// Patterns the cache holds at most.
const SLOTS: usize = 8;

/// Each pattern compiled; never freed.
static CACHE: Mutex<[Option<Regex>; SLOTS]> = Mutex::new([const { None }; SLOTS]);

/// Whether `re`, RE2 syntax, matches anywhere in `s`: [`Error::Function`] when `re` isn't a
/// pattern. A pattern compiles once while the cache has room; a cached one is only ever cloned,
/// sharing its compiled program, its scratch from the call's heap.
pub fn matches(s: &str, re: &str) -> Result<bool> {
    let compiled = match cached(re) {
        Some(r) => r,
        None => Regex::new(re).map_err(|_| Error::Function)?,
    };
    Ok(compiled.is_match(s))
}

/// `re` from the cache, compiled into it where there is room; none where it isn't a pattern or
/// can't be cached.
fn cached(re: &str) -> Option<Regex> {
    let mut cache = CACHE.try_lock().ok()?;
    if let Some(r) = cache.iter().flatten().find(|r| r.as_str() == re) {
        return Some(r.clone());
    }
    let slot = cache.iter_mut().find(|e| e.is_none())?;
    let r = Regex::new(re).ok()?;
    *slot = Some(r.clone());
    Some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &'static str) -> &'static str {
        v
    }

    #[test]
    fn size_counts_code_points() {
        assert_eq!(size(s("")), Ok(0));
        assert_eq!(size(s("héllo")), Ok(5));
        assert_eq!(size(s("😀")), Ok(1));
    }

    #[test]
    fn matches_searches_anywhere() {
        assert_eq!(matches(s("ACME STORE 12"), s(r"^ACME STORE\b")), Ok(true));
        assert_eq!(matches(s("the ACME"), s("ACME")), Ok(true));
        assert_eq!(matches(s("acme"), s("^ACME")), Ok(false));
        assert_eq!(matches(s("x"), s("(")), Err(Error::Function));
    }

    #[test]
    fn a_damaged_table_is_a_function_error() {
        static DAMAGED: Pattern = Pattern::new(b"not a table");
        assert_eq!(matches_table(&DAMAGED, s("x")), Err(Error::Function));
    }

    #[test]
    fn concat_and_trim_make_new_strings() {
        assert_eq!(concat(s("ab"), s("cd")), Ok("abcd".to_string()));
        assert_eq!(trim(s("  a b ")), Ok("a b".to_string()));
    }

    #[test]
    fn index_of_counts_code_points() {
        assert_eq!(index_of(s("ta©o©αT"), s("©")), Ok(2));
        assert_eq!(index_of(s("tacocat"), s("")), Ok(0));
        assert_eq!(index_of(s("tacocat"), s("none")), Ok(-1));
        assert_eq!(index_of_from(s("ta©o©αT"), s("©"), 3), Ok(4));
        assert_eq!(index_of_from(s("ta©o©αT"), s("©α"), 5), Ok(-1));
        assert_eq!(
            index_of_from(s("tacocat"), s("a"), 30),
            Err(Error::Function)
        );
        assert_eq!(
            index_of_from(s("tacocat"), s("a"), -1),
            Err(Error::Function)
        );
    }

    #[test]
    fn last_index_of_finds_the_last_start_at_or_before_offset() {
        assert_eq!(last_index_of(s("tacocat"), s("")), Ok(7));
        assert_eq!(last_index_of(s("tacocat"), s("at")), Ok(5));
        assert_eq!(last_index_of(s("tacocat"), s("none")), Ok(-1));
        assert_eq!(last_index_of_from(s("bananananana"), s("nana"), 7), Ok(6));
        assert_eq!(last_index_of_from(s("ta©o©αT"), s("©"), 3), Ok(2));
        assert_eq!(
            last_index_of_from(s("tacocat"), s("a"), 30),
            Err(Error::Function)
        );
        assert_eq!(
            last_index_of_from(s("tacocat"), s("a"), -1),
            Err(Error::Function)
        );
    }

    #[test]
    fn substring_slices_by_code_points() {
        assert_eq!(substring(s("tacocat"), 4).as_deref(), Ok("cat"));
        assert_eq!(substring(s("tacocat"), 7).as_deref(), Ok(""));
        assert_eq!(substring(s("tacocat"), 8), Err(Error::Function));
        assert_eq!(substring_between(s("ta©o©αT"), 2, 6).as_deref(), Ok("©o©α"));
        assert_eq!(substring_between(s("tacocat"), 4, 4).as_deref(), Ok(""));
        assert_eq!(substring_between(s("tacocat"), 4, 3), Err(Error::Function));
        assert_eq!(substring_between(s("tacocat"), 1, 50), Err(Error::Function));
        assert_eq!(substring_between(s("tacocat"), -1, 3), Err(Error::Function));
    }

    #[test]
    fn char_at_is_one_code_point() {
        assert_eq!(char_at(s("©αT"), 1).as_deref(), Ok("α"));
        assert_eq!(char_at(s("tacocat"), 7).as_deref(), Ok(""));
        assert_eq!(char_at(s("tacocat"), 30), Err(Error::Function));
    }
}
