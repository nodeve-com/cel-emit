//! CEL's lists extension: `flatten`, `slice`, `reverse`, `distinct`, `sort`, and
//! `lists.range`. A bad argument is [`Error::Function`], never a panic.

use std::cmp::Ordering;

use crate::cmp::{Equal, Order};
use crate::{Error, Result, Value};

/// `xs.flatten()` of a list of lists: one level.
pub fn flatten<T>(xs: Vec<Vec<T>>) -> Result<Vec<T>> {
    Ok(xs.into_iter().flatten().collect())
}

/// `xs.flatten()` of a dynamic list: an element that is a list spreads, any other stays.
pub fn flatten_dyn(xs: Vec<Value>) -> Result<Vec<Value>> {
    Ok(spread(xs))
}

/// `xs.flatten(depth)`: `depth` levels, or fewer where nothing is a list. A negative depth is an
/// error. It loops once per level and stops when a level spreads nothing, so a large depth costs
/// what the value holds.
pub fn flatten_depth(xs: Vec<Value>, depth: i64) -> Result<Vec<Value>> {
    if depth < 0 {
        return Err(Error::Function);
    }
    let mut level = xs;
    for _ in 0..depth {
        if !level.iter().any(|x| matches!(x, Value::List(_))) {
            break;
        }
        level = spread(level);
    }
    Ok(level)
}

/// `items` with each list element replaced by its elements.
fn spread(items: Vec<Value>) -> Vec<Value> {
    let mut out = Vec::new();
    for item in items {
        match item {
            Value::List(inner) => out.extend(inner),
            other => out.push(other),
        }
    }
    out
}

/// `xs.slice(start, end)`: `[start, end)`; a negative index, start past end, or end past the
/// list is an error.
pub fn slice<T: Clone>(xs: &[T], start: i64, end: i64) -> Result<Vec<T>> {
    let (start, end) = (index(start)?, index(end)?);
    let part = xs.get(start..end).ok_or(Error::Function)?;
    Ok(part.to_vec())
}

fn index(i: i64) -> Result<usize> {
    usize::try_from(i).map_err(|_| Error::Function)
}

/// `xs.reverse()`.
pub fn reverse<T>(mut xs: Vec<T>) -> Result<Vec<T>> {
    xs.reverse();
    Ok(xs)
}

/// `xs.distinct()`: each element's first occurrence, in order. Each is compared with those kept,
/// so the cost grows with the square of the length.
pub fn distinct<T: Equal>(xs: Vec<T>) -> Result<Vec<T>> {
    let mut kept: Vec<T> = Vec::new();
    for x in xs {
        let mut seen = false;
        for k in &kept {
            if k.equal(&x)? {
                seen = true;
                break;
            }
        }
        if !seen {
            kept.push(x);
        }
    }
    Ok(kept)
}

/// `xs.sort()`: ascending, equal elements in their order. A NaN, or a dynamic list mixing kinds,
/// is an error.
pub fn sort<T: Order + Clone>(xs: Vec<T>) -> Result<Vec<T>> {
    sort_by_keys(xs.clone(), xs)
}

/// `xs` ordered by `keys`, one per element: ascending, equal keys in `xs`'s order. Lists of
/// unequal length, a NaN key, or keys of mixed kinds are an error.
pub fn sort_by_keys<T: Clone, K: Order>(xs: Vec<T>, keys: Vec<K>) -> Result<Vec<T>> {
    if xs.len() != keys.len() {
        return Err(Error::Function);
    }
    let ranks = ranked(&keys)?;
    Ok(ranks
        .into_iter()
        .filter_map(|i| xs.get(i).cloned())
        .collect())
}

/// The positions of `keys` in ascending order of key, ties by position. Every key must order
/// with itself and with the first, so the comparison is a total order; Rust's sort may panic
/// on one that isn't.
fn ranked<K: Order>(keys: &[K]) -> Result<Vec<usize>> {
    if let Some(first) = keys.first() {
        for k in keys {
            k.order(k)?.ok_or(Error::Function)?;
            k.order(first)?.ok_or(Error::Function)?;
        }
    }
    let mut at: Vec<usize> = (0..keys.len()).collect();
    at.sort_by(|a, b| match (keys.get(*a), keys.get(*b)) {
        (Some(x), Some(y)) => x.order(y).ok().flatten().unwrap_or(Ordering::Equal),
        _ => Ordering::Equal,
    });
    Ok(at)
}

/// `lists.range(n)`: `0` up to `n`, none for `n <= 0`. A length past memory is [`Error::Range`].
pub fn range(n: i64) -> Result<Vec<i64>> {
    let end = n.max(0);
    let len = usize::try_from(end).map_err(|_| Error::Range)?;
    let mut out: Vec<i64> = Vec::new();
    out.try_reserve_exact(len).map_err(|_| Error::Range)?;
    out.extend(0..end);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ints(xs: &[i64]) -> Vec<i64> {
        xs.to_vec()
    }

    fn got<T>(xs: Result<Vec<T>>) -> Vec<T> {
        xs.unwrap()
    }

    fn dyn_list(xs: Vec<Value>) -> Value {
        Value::List(xs)
    }

    fn ints_of(xs: &[Value]) -> Vec<Option<i64>> {
        xs.iter()
            .map(|x| match x {
                Value::Int(i) => Some(*i),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn flatten_spreads_one_level() {
        let nested = vec![ints(&[1, 2]), ints(&[]), ints(&[3])];
        assert_eq!(got(flatten(nested)), [1, 2, 3]);
        assert!(got(flatten(Vec::<Vec<i64>>::new())).is_empty());
        let mixed = vec![
            Value::Int(1),
            dyn_list(vec![Value::Int(2), dyn_list(vec![Value::Int(3)])]),
        ];
        let once = flatten_dyn(mixed).unwrap();
        assert_eq!(ints_of(&once), [Some(1), Some(2), None]);
    }

    #[test]
    fn flatten_depth_stops_when_nothing_spreads() {
        let deep = vec![dyn_list(vec![dyn_list(vec![Value::Int(1)])]), Value::Int(2)];
        assert_eq!(
            ints_of(&flatten_depth(deep.clone(), 0).unwrap()),
            [None, Some(2)]
        );
        assert_eq!(
            ints_of(&flatten_depth(deep.clone(), 1).unwrap()),
            [None, Some(2)]
        );
        assert_eq!(
            ints_of(&flatten_depth(deep.clone(), 2).unwrap()),
            [Some(1), Some(2)]
        );
        assert_eq!(
            ints_of(&flatten_depth(deep.clone(), i64::MAX).unwrap()),
            [Some(1), Some(2)]
        );
        assert_eq!(flatten_depth(deep.clone(), -1).err(), Some(Error::Function));
        assert_eq!(flatten_depth(deep, i64::MIN).err(), Some(Error::Function));
    }

    #[test]
    fn slice_takes_a_range_and_refuses_the_rest() {
        let xs = ints(&[1, 2, 3]);
        assert_eq!(got(slice(&xs, 1, 3)), [2, 3]);
        assert!(got(slice(&xs, 2, 2)).is_empty());
        assert!(got(slice(&xs, 3, 3)).is_empty());
        for (a, b) in [
            (-1, 2),
            (0, -1),
            (2, 1),
            (0, 4),
            (4, 4),
            (i64::MIN, i64::MAX),
            (i64::MAX, i64::MAX),
            (0, i64::MAX),
            (i64::MIN, i64::MIN),
        ] {
            assert_eq!(slice(&xs, a, b).err(), Some(Error::Function), "{a} {b}");
        }
    }

    #[test]
    fn reverse_flips_the_order() {
        assert_eq!(got(reverse(ints(&[1, 2, 3]))), [3, 2, 1]);
        assert!(got(reverse(ints(&[]))).is_empty());
    }

    #[test]
    fn distinct_keeps_first_occurrences() {
        assert_eq!(got(distinct(ints(&[3, 1, 3, 2, 1]))), [3, 1, 2]);
        assert!(got(distinct(ints(&[]))).is_empty());
        let nan = vec![f64::NAN, f64::NAN];
        assert_eq!(got(distinct(nan)).len(), 2);
        let mixed = vec![
            Value::Int(1),
            Value::UInt(1),
            Value::Double(1.0),
            Value::Null,
        ];
        assert_eq!(got(distinct(mixed)).len(), 2);
    }

    #[test]
    fn sort_is_ascending_and_stable() {
        assert_eq!(got(sort(ints(&[3, -1, 2, i64::MIN]))), [i64::MIN, -1, 2, 3]);
        assert_eq!(got(sort(ints(&[5]))), [5]);
        let words = vec![String::from("b"), String::from("a"), String::from("é")];
        let sorted = got(sort(words));
        assert_eq!(sorted.first().map(String::as_str), Some("a"));
        assert_eq!(sorted.last().map(String::as_str), Some("é"));
        let marks = ints(&[10, 20, 30, 40, 50]);
        let keys = vec![1i64, 0, 1, 0, 1];
        assert_eq!(got(sort_by_keys(marks, keys)), [20, 40, 10, 30, 50]);
    }

    #[test]
    fn sort_refuses_what_has_no_order() {
        let nan = vec![1.0, f64::NAN, 0.0];
        assert_eq!(sort(nan).err(), Some(Error::Function));
        let only = vec![f64::NAN];
        assert_eq!(sort(only).err(), Some(Error::Function));
        let mixed = vec![Value::Int(2), Value::Str(String::from("a")), Value::Int(1)];
        assert_eq!(sort(mixed).err(), Some(Error::Function));
        let numbers = vec![Value::Int(2), Value::UInt(1)];
        assert_eq!(sort(numbers).err(), Some(Error::Function));
        let maps = vec![dyn_list(vec![])];
        assert_eq!(sort(maps).err(), Some(Error::Function));
        let same = vec![Value::Int(2), Value::Int(1)];
        assert_eq!(ints_of(&sort(same).unwrap()), [Some(1), Some(2)]);
        assert_eq!(
            sort_by_keys(ints(&[1, 2]), ints(&[1])).err(),
            Some(Error::Function)
        );
        let keys = vec![0.0, f64::NAN];
        assert_eq!(
            sort_by_keys(ints(&[1, 2]), keys).err(),
            Some(Error::Function)
        );
    }

    #[test]
    fn range_counts_from_zero() {
        assert_eq!(got(range(3)), [0, 1, 2]);
        assert!(got(range(0)).is_empty());
        assert!(got(range(-1)).is_empty());
        assert!(got(range(i64::MIN)).is_empty());
        assert_eq!(range(i64::MAX).err(), Some(Error::Range));
    }
}
