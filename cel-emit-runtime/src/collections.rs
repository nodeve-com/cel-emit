//! Lists and maps: sizes, indexing, membership, and what CEL's macros (`all`, `exists`, `map`, …)
//! expand to, each comprehension's body a closure over one element.

use std::borrow::Borrow;

use crate::cmp::Equal;
use crate::{Error, Result};

pub fn list_size<T>(xs: &[T]) -> Result<i64> {
    i64::try_from(xs.len()).map_err(|_| Error::Overflow)
}

pub fn map_size<K, V>(m: &[(K, V)]) -> Result<i64> {
    list_size(m)
}

/// `xs[i]`: outside the list, a function error, as cel-go has it.
pub fn index_list<T: Clone>(xs: &[T], i: i64) -> Result<T> {
    item(xs, i).cloned()
}

/// [`index_list`], no copy made.
pub fn item<T>(xs: &[T], i: i64) -> Result<&T> {
    let i = usize::try_from(i).map_err(|_| Error::Function)?;
    xs.get(i).ok_or(Error::Function)
}

/// The first element `pred` holds for, what `filter(…)[0]` gives, read no further: with none,
/// the error of an index outside a list.
pub fn first<'a, T>(
    items: impl IntoIterator<Item = &'a T>,
    mut pred: impl FnMut(&'a T) -> Result<bool>,
) -> Result<&'a T> {
    for item in items {
        if pred(item)? {
            return Ok(item);
        }
    }
    Err(Error::Function)
}

/// `m[key]`: a key it lacks is no such key.
pub fn index_map<K: Equal + Clone, V: Clone>(m: &[(K, V)], key: impl Borrow<K>) -> Result<V> {
    index_map_with(m, key, Ok, Ok)
}

/// `m[key]` over a map stored at other widths: each stored key and the found value widened by
/// `wide_key` and `wide`.
pub fn index_map_with<K: Clone, V: Clone, T: Equal, W>(
    m: &[(K, V)],
    key: impl Borrow<T>,
    wide_key: impl Fn(K) -> Result<T>,
    wide: impl Fn(V) -> Result<W>,
) -> Result<W> {
    for (k, v) in m {
        if wide_key(k.clone())?.equal(key.borrow())? {
            return wide(v.clone());
        }
    }
    Err(Error::NoSuchKey)
}

/// `x in xs`.
pub fn in_list<T: Equal + Clone>(x: impl Borrow<T>, xs: &[T]) -> Result<bool> {
    in_list_with(x, xs, Ok)
}

/// `x in xs` over a list stored at another width: each element widened by `wide`.
pub fn in_list_with<S: Clone, T: Equal>(
    x: impl Borrow<T>,
    xs: &[S],
    wide: impl Fn(S) -> Result<T>,
) -> Result<bool> {
    for y in xs {
        if x.borrow().equal(&wide(y.clone())?)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `key in m`.
pub fn in_map<K: Equal + Clone, V>(key: impl Borrow<K>, m: &[(K, V)]) -> Result<bool> {
    in_map_with(key, m, Ok)
}

/// `key in m` over a map stored at another key width: each key widened by `wide`.
pub fn in_map_with<K: Clone, V, T: Equal>(
    key: impl Borrow<T>,
    m: &[(K, V)],
    wide: impl Fn(K) -> Result<T>,
) -> Result<bool> {
    for (k, _) in m {
        if key.borrow().equal(&wide(k.clone())?)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `xs` with each element passed through `f`, the first error standing: a stored list widened,
/// or a list checked against the widths it is written at.
pub fn map_list<S, T>(xs: Vec<S>, f: impl FnMut(S) -> Result<T>) -> Result<Vec<T>> {
    xs.into_iter().map(f).collect()
}

/// `m` with each key passed through `fk` and each value through `fv`.
pub fn map_entries<K, V, L, W>(
    m: Vec<(K, V)>,
    fk: impl Fn(K) -> Result<L>,
    fv: impl Fn(V) -> Result<W>,
) -> Result<Vec<(L, W)>> {
    m.into_iter().map(|(k, v)| Ok((fk(k)?, fv(v)?))).collect()
}

/// A map literal's entries: a key twice is a function error.
pub fn map_of<K: Equal, V>(entries: impl IntoIterator<Item = (K, V)>) -> Result<Vec<(K, V)>> {
    let mut kept: Vec<(K, V)> = Vec::new();
    for (key, value) in entries {
        for (k, _) in &kept {
            if k.equal(&key)? {
                return Err(Error::Function);
            }
        }
        kept.push((key, value));
    }
    Ok(kept)
}

/// `a + b`.
pub fn concat<T: Clone>(a: &[T], b: &[T]) -> Result<Vec<T>> {
    Ok([a, b].concat())
}

/// `b"…" + b"…"`: a's bytes, then b's.
pub fn bytes_concat(a: &[u8], b: &[u8]) -> Result<Vec<u8>> {
    concat(a, b)
}

/// `size(b)`: its number of bytes.
pub fn bytes_size(b: &[u8]) -> Result<i64> {
    list_size(b)
}

/// `all` (`every` true) or `exists`: an element deciding it, false for `all` and true for
/// `exists`, absorbs every error; else the first error stands.
pub fn all<I>(
    items: impl IntoIterator<Item = I>,
    every: bool,
    mut body: impl FnMut(I) -> Result<bool>,
) -> Result<bool> {
    let mut first = None;
    for item in items {
        match body(item) {
            Ok(b) if b != every => return Ok(b),
            Ok(_) => {}
            Err(e) => {
                first.get_or_insert(e);
            }
        }
    }
    first.map_or(Ok(every), Err)
}

/// How many items a comprehension gives, each one's error first.
pub fn count<T>(items: impl IntoIterator<Item = Result<T>>) -> Result<i64> {
    let mut n = 0i64;
    for item in items {
        item?;
        n = n.checked_add(1).ok_or(Error::Overflow)?;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_index_outside_the_list_fails() {
        let xs = [1.5, -2.0];
        assert_eq!(index_list(&xs, 1), Ok(-2.0));
        assert_eq!(index_list(&xs, 2), Err(Error::Function));
        assert_eq!(index_list(&xs, -1), Err(Error::Function));
    }

    #[test]
    fn the_first_match_reads_nothing_past_it() {
        let reads = |x: &i64| {
            let quotient = 4i64.checked_div(*x).ok_or(Error::Division)?;
            Ok(quotient == 2)
        };
        assert_eq!(first(&[1, 2, 0], reads), Ok(&2));
        assert_eq!(first(&[0], reads), Err(Error::Division));
        assert_eq!(first(&[1], reads), Err(Error::Function));
    }

    #[test]
    fn a_key_the_map_lacks_is_no_such_key() {
        let m = map_of([(0i64, 1.5)]).unwrap();
        assert_eq!(index_map(&m, 0), Ok(1.5));
        assert_eq!(index_map(&m, 1), Err(Error::NoSuchKey));
    }

    #[test]
    fn a_stored_collection_is_read_through_its_widening() {
        let wide = |x: i32| Ok(i64::from(x));
        let xs = vec![5i32, -7];
        assert_eq!(in_list_with(-7i64, &xs, wide), Ok(true));
        assert_eq!(in_list_with(3_000_000_000i64, &xs, wide), Ok(false));
        let m = vec![(2i32, 250u8)];
        let value = |v: u8| Ok(u64::from(v));
        assert_eq!(index_map_with(&m, 2i64, wide, value), Ok(250u64));
        assert_eq!(index_map_with(&m, 3i64, wide, value), Err(Error::NoSuchKey));
        assert_eq!(in_map_with(2i64, &m, wide), Ok(true));
        let widened = map_list(xs, wide).unwrap();
        assert_eq!(widened, [5i64, -7]);
        let narrowed = map_list(widened, crate::number::within::<i8, _>);
        assert_eq!(narrowed.unwrap(), [5i64, -7]);
        let too_wide = map_list(vec![128i64], crate::number::within::<i8, _>);
        assert_eq!(too_wide.err(), Some(Error::Range));
        let entries = map_entries(m, wide, value).unwrap();
        assert_eq!(entries, [(2i64, 250u64)]);
    }

    #[test]
    fn a_deciding_element_absorbs_the_others_errors() {
        let xs = [Err(Error::Division), Ok(false), Ok(true)];
        assert_eq!(all(xs, true, |x| x), Ok(false));
        assert_eq!(all(xs, false, |x| x), Ok(true));
        assert_eq!(
            all([Ok(true), Err(Error::Overflow)], true, |x| x),
            Err(Error::Overflow)
        );
        assert_eq!(all(Vec::<Result<bool>>::new(), false, |x| x), Ok(false));
    }
}
