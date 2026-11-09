//! Comparisons and logic: `==` and the orderings, each operand of one type, and `&&`, `||`, `!`.

use std::cmp::Ordering;

use crate::{DateTime, Error, Null, Result, TimeDelta, Utc, Value};

/// A type `==` compares.
pub trait Equal {
    fn equal(&self, other: &Self) -> Result<bool>;
}

/// A type the orderings compare: none where two values are unordered, a NaN.
pub trait Order {
    fn order(&self, other: &Self) -> Result<Option<Ordering>>;
}

macro_rules! natively {
    ($($t:ty),*) => {$(
        impl Equal for $t {
            fn equal(&self, other: &Self) -> Result<bool> {
                Ok(self == other)
            }
        }

        impl Order for $t {
            fn order(&self, other: &Self) -> Result<Option<Ordering>> {
                Ok(self.partial_cmp(other))
            }
        }
    )*};
}

natively!(i64, u64, f64, bool, String, DateTime<Utc>, TimeDelta);

// Stored widths, compared at their own width: the answer is the widened one's.
natively!(i8, i16, i32, i128, u16, u32, u128, f32);

/// Text compares by code point, as `String` does.
impl Equal for str {
    fn equal(&self, other: &str) -> Result<bool> {
        Ok(self == other)
    }
}

impl Order for str {
    fn order(&self, other: &str) -> Result<Option<Ordering>> {
        Ok(Some(self.cmp(other)))
    }
}

/// A value by reference compares as the value.
impl<T: Equal + ?Sized> Equal for &T {
    fn equal(&self, other: &Self) -> Result<bool> {
        (**self).equal(other)
    }
}

impl<T: Order + ?Sized> Order for &T {
    fn order(&self, other: &Self) -> Result<Option<Ordering>> {
        (**self).order(other)
    }
}

/// Values of one kind order as that kind does, a float32 as a double; values of different kinds,
/// and null, lists, and maps, are [`Error::Function`]. Only `sort` reads this: `<` over a dyn
/// stays refused in lowering.
impl Order for Value {
    fn order(&self, other: &Value) -> Result<Option<Ordering>> {
        match (self, other) {
            (Value::Bool(a), Value::Bool(b)) => a.order(b),
            (Value::Int(a), Value::Int(b)) => a.order(b),
            (Value::UInt(a), Value::UInt(b)) => a.order(b),
            (Value::Double(a), Value::Double(b)) => a.order(b),
            (Value::Double(a), Value::Float32(b)) => a.order(&f64::from(*b)),
            (Value::Float32(a), Value::Double(b)) => f64::from(*a).order(b),
            (Value::Float32(a), Value::Float32(b)) => f64::from(*a).order(&f64::from(*b)),
            (Value::Str(a), Value::Str(b)) => a.order(b),
            (Value::Timestamp(a), Value::Timestamp(b)) => a.order(b),
            (Value::Duration(a), Value::Duration(b)) => a.order(b),
            _ => Err(Error::Function),
        }
    }
}

/// Null is null: every null equals every other.
impl Equal for Null {
    fn equal(&self, _: &Null) -> Result<bool> {
        Ok(true)
    }
}

/// A list of bytes is bytes: byte by byte, a shorter prefix first.
impl Equal for u8 {
    fn equal(&self, other: &u8) -> Result<bool> {
        Ok(self == other)
    }
}

impl Order for u8 {
    fn order(&self, other: &u8) -> Result<Option<Ordering>> {
        Ok(Some(self.cmp(other)))
    }
}

impl Order for Vec<u8> {
    fn order(&self, other: &Vec<u8>) -> Result<Option<Ordering>> {
        Ok(Some(self.cmp(other)))
    }
}

impl<T: Equal> Equal for Vec<T> {
    fn equal(&self, other: &Vec<T>) -> Result<bool> {
        if self.len() != other.len() {
            return Ok(false);
        }
        for (a, b) in self.iter().zip(other) {
            if !a.equal(b)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// Maps are equal holding the same keys, each with an equal value, in any order.
impl<K: Equal, V: Equal> Equal for Vec<(K, V)> {
    fn equal(&self, other: &Self) -> Result<bool> {
        if self.len() != other.len() {
            return Ok(false);
        }
        for (key, value) in self {
            let Some(v) = find_equal(other, key)? else {
                return Ok(false);
            };
            if !value.equal(v)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// The value of the first entry whose key equals `key`.
fn find_equal<'a, K: Equal, V>(entries: &'a [(K, V)], key: &K) -> Result<Option<&'a V>> {
    for (k, v) in entries {
        if key.equal(k)? {
            return Ok(Some(v));
        }
    }
    Ok(None)
}

pub fn eq<T: Equal>(a: T, b: T) -> Result<bool> {
    a.equal(&b)
}

pub fn ne<T: Equal>(a: T, b: T) -> Result<bool> {
    Ok(!a.equal(&b)?)
}

pub fn lt<T: Order>(a: T, b: T) -> Result<bool> {
    Ok(a.order(&b)? == Some(Ordering::Less))
}

pub fn le<T: Order>(a: T, b: T) -> Result<bool> {
    Ok(matches!(
        a.order(&b)?,
        Some(Ordering::Less | Ordering::Equal)
    ))
}

pub fn gt<T: Order>(a: T, b: T) -> Result<bool> {
    Ok(a.order(&b)? == Some(Ordering::Greater))
}

pub fn ge<T: Order>(a: T, b: T) -> Result<bool> {
    Ok(matches!(
        a.order(&b)?,
        Some(Ordering::Greater | Ordering::Equal)
    ))
}

pub fn not(a: bool) -> Result<bool> {
    Ok(!a)
}

/// `a && b`: a side that is false decides, absorbing the other's error; else `a`'s error, then
/// `b`'s. `b` runs only where `a` doesn't decide.
pub fn and(a: impl FnOnce() -> Result<bool>, b: impl FnOnce() -> Result<bool>) -> Result<bool> {
    logic(a, b, false)
}

/// `a || b`, as [`and`] with true deciding.
pub fn or(a: impl FnOnce() -> Result<bool>, b: impl FnOnce() -> Result<bool>) -> Result<bool> {
    logic(a, b, true)
}

fn logic(
    a: impl FnOnce() -> Result<bool>,
    b: impl FnOnce() -> Result<bool>,
    decides: bool,
) -> Result<bool> {
    let a = a();
    if a == Ok(decides) {
        return a;
    }
    let b = b();
    if b == Ok(decides) {
        return b;
    }
    a.and(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    #[test]
    fn a_deciding_side_absorbs_the_others_error() {
        let fails = || Err(Error::Division);
        assert_eq!(and(fails, || Ok(false)), Ok(false));
        assert_eq!(and(|| Ok(true), fails), Err(Error::Division));
        assert_eq!(or(fails, || Ok(true)), Ok(true));
        assert_eq!(or(|| Err(Error::Overflow), fails), Err(Error::Overflow));
        assert_eq!(or(|| Ok(false), || Ok(false)), Ok(false));
    }

    #[test]
    fn strings_order_by_code_point_and_nan_by_nothing() {
        assert_eq!(lt(String::from("a"), String::from("b")), Ok(true));
        assert_eq!(gt(String::from("é"), String::from("z")), Ok(true));
        assert_eq!(le(f64::NAN, 1.0), Ok(false));
        assert_eq!(ne(f64::NAN, f64::NAN), Ok(true));
    }

    #[test]
    fn text_by_reference_compares_as_strings_do() {
        assert_eq!(eq("a", "a"), Ok(true));
        assert_eq!(ne("a", "a"), Ok(false));
        assert_eq!(lt("a", "b"), Ok(true));
        assert_eq!(gt("é", "z"), Ok(true));
    }

    #[test]
    fn maps_are_equal_in_any_order() {
        let m = |e: Vec<(&'static str, i64)>| -> Vec<(String, i64)> {
            e.into_iter().map(|(k, v)| (String::from(k), v)).collect()
        };
        assert_eq!(
            eq(m(vec![("a", 1), ("b", 2)]), m(vec![("b", 2), ("a", 1)])),
            Ok(true)
        );
        assert_eq!(eq(m(vec![("a", 1)]), m(vec![("a", 2)])), Ok(false));
    }
}
