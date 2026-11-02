//! Optionals: a value that may be absent, as an `Option` of its type. Absence is `None`; `null`
//! is a value of its own ([`crate::Null`]), so the two stay apart.

use std::borrow::Borrow;

use crate::cmp::Equal;
use crate::{Error, Result};

/// Two optionals are equal when both hold equal values, or neither holds one.
impl<T: Equal> Equal for Option<T> {
    fn equal(&self, other: &Self) -> Result<bool> {
        match (self, other) {
            (Some(a), Some(b)) => a.equal(b),
            (None, None) => Ok(true),
            _ => Ok(false),
        }
    }
}

/// `optional.of(x)`.
pub fn of<T>(x: T) -> Result<Option<T>> {
    Ok(Some(x))
}

/// `optional.ofNonZeroValue(x)`: absent where `x` is its type's zero.
pub fn of_non_zero_value<T: Equal + Default>(x: T) -> Result<Option<T>> {
    Ok(if x.equal(&T::default())? {
        None
    } else {
        Some(x)
    })
}

/// `x.hasValue()`.
pub fn has_value<T>(x: &Option<T>) -> Result<bool> {
    Ok(x.is_some())
}

/// `x.value()`: no value, an error.
pub fn value<T>(x: Option<T>) -> Result<T> {
    x.ok_or(Error::Absent)
}

/// What `x` holds, no copy made.
pub fn held<T>(x: &Option<T>) -> Result<&T> {
    x.as_ref().ok_or(Error::Absent)
}

/// `x.orValue(default)`.
pub fn or_value<T>(x: Option<T>, default: T) -> Result<T> {
    Ok(x.unwrap_or(default))
}

/// `x.or(other)`: `x` where it holds a value, else `other`.
pub fn or<T>(x: Option<T>, other: Option<T>) -> Result<Option<T>> {
    Ok(x.or(other))
}

/// `xs[?i]`: absent outside the list.
pub fn index_list<T: Clone>(xs: &[T], i: i64) -> Result<Option<T>> {
    let Ok(i) = usize::try_from(i) else {
        return Ok(None);
    };
    Ok(xs.get(i).cloned())
}

/// `xs.first()`: absent over no elements.
pub fn first<T: Clone>(xs: &[T]) -> Result<Option<T>> {
    Ok(xs.first().cloned())
}

/// `xs.last()`: absent over no elements.
pub fn last<T: Clone>(xs: &[T]) -> Result<Option<T>> {
    Ok(xs.last().cloned())
}

/// `m[?key]`: absent where the map lacks the key.
pub fn index_map<K: Equal, V: Clone>(m: &[(K, V)], key: impl Borrow<K>) -> Result<Option<V>> {
    for (k, v) in m {
        if k.equal(key.borrow())? {
            return Ok(Some(v.clone()));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_lends_what_it_holds() {
        assert_eq!(held(&Some(4i64)), Ok(&4));
        assert_eq!(held(&None::<i64>), Err(Error::Absent));
    }

    #[test]
    fn first_and_last_are_absent_over_no_elements() {
        let xs = [4i64, 5];
        assert_eq!((first(&xs), last(&xs)), (Ok(Some(4)), Ok(Some(5))));
        let none: [i64; 0] = [];
        assert_eq!((first(&none), last(&none)), (Ok(None), Ok(None)));
    }
}
