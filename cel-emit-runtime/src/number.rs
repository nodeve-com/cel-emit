//! A number of any of CEL's numeric types as one value: what a mixed `math.greatest` or
//! `math.least` gives. It keeps the type of the argument it is, and compares with another number
//! by numeric value, exactly: an int is never rounded to a double to compare with it.

use std::cmp::Ordering::{self, Equal, Greater, Less};

use crate::{cmp, conversions, Error, Result};

/// `v` as a `T`, an error where `T` doesn't hold it: a message's field read or written.
pub fn fit<T: TryFrom<S>, S>(v: S) -> Result<T> {
    T::try_from(v).map_err(|_| Error::Range)
}

/// `v` itself, an error where a `T` doesn't hold it.
pub fn within<T: TryFrom<S>, S: Copy>(v: S) -> Result<S> {
    fit::<T, S>(v).map(|_| v)
}

/// 2^63 and 2^64: the doubles just past int64's and uint64's ranges, both exact.
const TWO_63: f64 = 9_223_372_036_854_775_808.0;
const TWO_64: f64 = 18_446_744_073_709_551_616.0;

#[derive(Clone, Copy, Debug)]
pub enum Number {
    Int(i64),
    Uint(u64),
    Double(f64),
}

impl PartialEq for Number {
    fn eq(&self, other: &Number) -> bool {
        self.partial_cmp(other) == Some(Equal)
    }
}

/// Unordered where a NaN is involved, as a double is.
impl PartialOrd for Number {
    fn partial_cmp(&self, other: &Number) -> Option<Ordering> {
        use Number::{Double, Int, Uint};
        match (*self, *other) {
            (Int(a), Int(b)) => Some(a.cmp(&b)),
            (Uint(a), Uint(b)) => Some(a.cmp(&b)),
            (Double(a), Double(b)) => a.partial_cmp(&b),
            (Int(a), Uint(b)) => Some(int_uint(a, b)),
            (Uint(a), Int(b)) => Some(int_uint(b, a).reverse()),
            (Int(a), Double(b)) => int_double(a, b),
            (Double(a), Int(b)) => int_double(b, a).map(Ordering::reverse),
            (Uint(a), Double(b)) => uint_double(a, b),
            (Double(a), Uint(b)) => uint_double(b, a).map(Ordering::reverse),
        }
    }
}

impl cmp::Equal for Number {
    fn equal(&self, other: &Number) -> Result<bool> {
        Ok(self == other)
    }
}

impl cmp::Order for Number {
    fn order(&self, other: &Number) -> Result<Option<Ordering>> {
        Ok(self.partial_cmp(other))
    }
}

fn int_uint(a: i64, b: u64) -> Ordering {
    match u64::try_from(a) {
        Ok(a) => a.cmp(&b),
        Err(_) => Less,
    }
}

/// `i` against `d`, exactly.
fn int_double(i: i64, d: f64) -> Option<Ordering> {
    if d.is_nan() {
        return None;
    }
    if d >= TWO_63 {
        return Some(Less);
    }
    if d < -TWO_63 {
        return Some(Greater);
    }
    // Within int64's range now, so the whole part is one: the cast is exact.
    let whole = d.trunc();
    Some(match i.cmp(&(whole as i64)) {
        Equal if d > whole => Less,
        Equal if d < whole => Greater,
        o => o,
    })
}

/// `u` against `d`, exactly.
fn uint_double(u: u64, d: f64) -> Option<Ordering> {
    if d.is_nan() {
        return None;
    }
    if d >= TWO_64 {
        return Some(Less);
    }
    if d < 0.0 {
        return Some(Greater);
    }
    // Within [0, 2^64) now, so the whole part is one: the cast is exact.
    let whole = d.trunc();
    Some(match u.cmp(&(whole as u64)) {
        Equal if d > whole => Less,
        o => o,
    })
}

/// Each numeric type as a number, which never fails.
pub fn of_int(i: i64) -> Result<Number> {
    Ok(Number::Int(i))
}

pub fn of_uint(u: u64) -> Result<Number> {
    Ok(Number::Uint(u))
}

pub fn of_double(d: f64) -> Result<Number> {
    Ok(Number::Double(d))
}

/// `int(n)`: a double as `int(double)` gives it, a uint past int64's range overflows.
pub fn to_int(n: Number) -> Result<i64> {
    match n {
        Number::Int(i) => Ok(i),
        Number::Uint(u) => conversions::uint_to_int(u),
        Number::Double(d) => conversions::double_to_int(d),
    }
}

/// `uint(n)`, as the uint conversions give it.
pub fn to_uint(n: Number) -> Result<u64> {
    match n {
        Number::Int(i) => conversions::int_to_uint(i),
        Number::Uint(u) => Ok(u),
        Number::Double(d) => conversions::double_to_uint(d),
    }
}

/// `double(n)`: an int or uint correctly rounded, as the conversions round.
pub fn to_double(n: Number) -> Result<f64> {
    match n {
        Number::Int(i) => conversions::int_to_double(i),
        Number::Uint(u) => conversions::uint_to_double(u),
        Number::Double(d) => Ok(d),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    #[test]
    fn numbers_compare_by_value_across_types() {
        use Number::{Double, Int, Uint};
        assert_eq!(Int(1).partial_cmp(&Double(1.0)), Some(Equal));
        assert_eq!(Int(1), Uint(1));
        assert_eq!(Uint(3).partial_cmp(&Double(5.4)), Some(Less));
        assert_eq!(Double(-5.0).partial_cmp(&Int(-5)), Some(Equal));
        assert_eq!(Int(-1).partial_cmp(&Uint(0)), Some(Less));
        assert_eq!(Int(-1).partial_cmp(&Double(-0.5)), Some(Less));
        assert_eq!(Double(-0.0), Int(0));
        assert_eq!(Int(i64::MAX).partial_cmp(&Double(TWO_63)), Some(Less));
        assert_eq!(Int(i64::MIN).partial_cmp(&Double(-TWO_63)), Some(Equal));
        assert_eq!(Uint(u64::MAX).partial_cmp(&Double(TWO_64)), Some(Less));
        assert_eq!(Double(f64::NAN).partial_cmp(&Int(0)), None);
        assert_ne!(Double(f64::NAN), Double(f64::NAN));
    }

    #[test]
    fn int_against_double_is_exact_not_rounded() {
        use Number::{Double, Int};
        // 2^53 + 1 is not a double: rounding it would make the two equal.
        let big = 9_007_199_254_740_993;
        assert_eq!(
            Int(big).partial_cmp(&Double(9_007_199_254_740_992.0)),
            Some(Greater)
        );
        assert_eq!(
            Int(i64::MAX).partial_cmp(&Double(9_223_372_036_854_775_807.0)),
            Some(Less)
        );
    }

    #[test]
    fn conversions_truncate_and_overflow_as_cel_does() {
        use Number::{Double, Int, Uint};
        assert_eq!(to_int(Double(5.4)), Ok(5));
        assert_eq!(to_int(Double(-5.4)), Ok(-5));
        assert_eq!(to_int(Double(TWO_63)), Err(Error::Overflow));
        assert_eq!(to_int(Uint(u64::MAX)), Err(Error::Overflow));
        assert_eq!(to_int(Uint(3)), Ok(3));
        assert_eq!(to_uint(Double(-1.0)), Err(Error::Overflow));
        assert_eq!(to_uint(Double(TWO_64)), Err(Error::Overflow));
        assert_eq!(to_uint(Double(f64::NAN)), Err(Error::Overflow));
        assert_eq!(to_uint(Double(7.9)), Ok(7));
        assert_eq!(to_uint(Int(-1)), Err(Error::Overflow));
        assert_eq!(to_double(Uint(4)), Ok(4.0));
        assert_eq!(to_double(Int(-4)), Ok(-4.0));
    }

    #[test]
    fn a_field_holds_only_its_width() {
        assert_eq!(within::<i32, _>(2_147_483_647_i64), Ok(2_147_483_647));
        assert_eq!(within::<i32, _>(2_147_483_648_i64), Err(Error::Range));
        assert_eq!(within::<u8, _>(256_u64), Err(Error::Range));
        assert_eq!(within::<u128, _>(7_u64), Ok(7));
        assert_eq!(fit::<i64, _>(i128::from(i64::MAX) + 1), Err(Error::Range));
        assert_eq!(fit::<u64, _>(5_u128), Ok(5));
    }
}
