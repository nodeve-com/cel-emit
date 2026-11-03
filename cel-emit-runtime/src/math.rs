//! Int and double arithmetic, and CEL's math extension (`math.*`) as cel-go defines it, plus
//! `math.pow` and `math.sum`. Int arithmetic is checked; a double's follows IEEE 754. `math.pow`
//! runs through `libm` on every target, so builds give identical results. Uint arithmetic is in `uint`.

use std::cmp::Ordering::{self, Greater, Less};

use crate::{Error, Result};

pub use uint::{
    abs_uint, add_uint, bit_and_uint, bit_not_uint, bit_or_uint, bit_xor_uint, divide_uint,
    modulo_uint, multiply_uint, shift_left_uint, shift_right_uint, sign_uint, subtract_uint,
};

mod uint;

pub fn add_int(a: i64, b: i64) -> Result<i64> {
    a.checked_add(b).ok_or(Error::Overflow)
}

pub fn subtract_int(a: i64, b: i64) -> Result<i64> {
    a.checked_sub(b).ok_or(Error::Overflow)
}

pub fn multiply_int(a: i64, b: i64) -> Result<i64> {
    a.checked_mul(b).ok_or(Error::Overflow)
}

/// Toward zero: by zero a division error, `i64::MIN / -1` an overflow.
pub fn divide_int(a: i64, b: i64) -> Result<i64> {
    if b == 0 {
        return Err(Error::Division);
    }
    a.checked_div(b).ok_or(Error::Overflow)
}

/// The remainder of [`divide_int`], with `a`'s sign.
pub fn modulo_int(a: i64, b: i64) -> Result<i64> {
    if b == 0 {
        return Err(Error::Division);
    }
    a.checked_rem(b).ok_or(Error::Overflow)
}

pub fn negate_int(a: i64) -> Result<i64> {
    a.checked_neg().ok_or(Error::Overflow)
}

pub fn add_double(a: f64, b: f64) -> Result<f64> {
    Ok(a + b)
}

pub fn subtract_double(a: f64, b: f64) -> Result<f64> {
    Ok(a - b)
}

pub fn multiply_double(a: f64, b: f64) -> Result<f64> {
    Ok(a * b)
}

pub fn divide_double(a: f64, b: f64) -> Result<f64> {
    Ok(a / b)
}

pub fn negate_double(a: f64) -> Result<f64> {
    Ok(-a)
}

pub fn ceil(x: f64) -> Result<f64> {
    Ok(x.ceil())
}

pub fn floor(x: f64) -> Result<f64> {
    Ok(x.floor())
}

/// Half away from zero.
pub fn round(x: f64) -> Result<f64> {
    Ok(x.round())
}

pub fn trunc(x: f64) -> Result<f64> {
    Ok(x.trunc())
}

pub fn abs_int(x: i64) -> Result<i64> {
    x.checked_abs().ok_or(Error::Overflow)
}

pub fn abs_double(x: f64) -> Result<f64> {
    Ok(x.abs())
}

pub fn sign_int(x: i64) -> Result<i64> {
    Ok(x.signum())
}

/// NaN stays NaN; either zero gives +0.
pub fn sign_double(x: f64) -> Result<f64> {
    Ok(if x == 0.0 { 0.0 } else { x.signum() })
}

pub fn is_nan(x: f64) -> Result<bool> {
    Ok(x.is_nan())
}

pub fn is_inf(x: f64) -> Result<bool> {
    Ok(x.is_infinite())
}

pub fn is_finite(x: f64) -> Result<bool> {
    Ok(x.is_finite())
}

pub fn bit_and(a: i64, b: i64) -> Result<i64> {
    Ok(a & b)
}

pub fn bit_or(a: i64, b: i64) -> Result<i64> {
    Ok(a | b)
}

pub fn bit_xor(a: i64, b: i64) -> Result<i64> {
    Ok(a ^ b)
}

pub fn bit_not(a: i64) -> Result<i64> {
    Ok(!a)
}

/// `a` shifted left `bits`, zeros in: 64 or more shifts every bit out; `bits` < 0 a function
/// error.
pub fn shift_left(a: i64, bits: i64) -> Result<i64> {
    shift(bits, |b| a.checked_shl(b))
}

/// `a` shifted right `bits`, logically: zeros in.
pub fn shift_right(a: i64, bits: i64) -> Result<i64> {
    shift(bits, |b| {
        a.cast_unsigned().checked_shr(b).map(u64::cast_signed)
    })
}

/// `by` of `bits`: a negative count is a function error, a count `by` refuses gives zero.
fn shift<T: Default>(bits: i64, by: impl Fn(u32) -> Option<T>) -> Result<T> {
    crate::check((), bits >= 0, Error::Function)?;
    Ok(u32::try_from(bits).ok().and_then(by).unwrap_or_default())
}

/// `x` to the power `y`, as libm computes it on every target.
pub fn pow(x: f64, y: f64) -> Result<f64> {
    Ok(libm::pow(x, y))
}

/// The greatest of `xs` (`keep` is `Greater`) or the least (`Less`), the first of a tie: over
/// none, `empty`; a NaN meeting another value a function error.
fn extreme<T: PartialOrd + Copy>(
    xs: impl IntoIterator<Item = Result<T>>,
    keep: Ordering,
    empty: Error,
) -> Result<T> {
    let mut xs = xs.into_iter();
    let first = xs.next().ok_or(empty)??;
    xs.try_fold(first, |best, x| {
        let x = x?;
        Ok(match x.partial_cmp(&best).ok_or(Error::Function)? {
            o if o == keep => x,
            _ => best,
        })
    })
}

/// `math.greatest` of a list: over none, a function error.
pub fn greatest_list<T: PartialOrd + Copy>(xs: &[T]) -> Result<T> {
    extreme(xs.iter().copied().map(Ok), Greater, Error::Function)
}

pub fn least_list<T: PartialOrd + Copy>(xs: &[T]) -> Result<T> {
    extreme(xs.iter().copied().map(Ok), Less, Error::Function)
}

/// `math.greatest` of what a comprehension gives, each item's error first: over none, absent.
pub fn greatest_of<T: PartialOrd + Copy>(xs: impl IntoIterator<Item = Result<T>>) -> Result<T> {
    extreme(xs, Greater, Error::Absent)
}

pub fn least_of<T: PartialOrd + Copy>(xs: impl IntoIterator<Item = Result<T>>) -> Result<T> {
    extreme(xs, Less, Error::Absent)
}

/// `math.greatest` of its `N` arguments, at least one.
pub fn greatest<T: PartialOrd + Copy, const N: usize>(xs: [T; N]) -> Result<T> {
    const { assert!(N >= 1) };
    extreme(xs.into_iter().map(Ok), Greater, Error::Function)
}

/// `math.least` of its `N` arguments, at least one.
pub fn least<T: PartialOrd + Copy, const N: usize>(xs: [T; N]) -> Result<T> {
    const { assert!(N >= 1) };
    extreme(xs.into_iter().map(Ok), Less, Error::Function)
}

/// A number `math.sum` adds: an int checked, a double as IEEE 754 does.
pub trait Sum: Copy {
    const ZERO: Self;
    fn plus(self, other: Self) -> Result<Self>;
}

impl Sum for i64 {
    const ZERO: i64 = 0;
    fn plus(self, other: i64) -> Result<i64> {
        add_int(self, other)
    }
}

impl Sum for f64 {
    const ZERO: f64 = 0.0;
    fn plus(self, other: f64) -> Result<f64> {
        Ok(self + other)
    }
}

/// `math.sum` of what a comprehension gives, in order: over none, zero.
pub fn sum_of<T: Sum>(xs: impl IntoIterator<Item = Result<T>>) -> Result<T> {
    xs.into_iter().try_fold(T::ZERO, |total, x| total.plus(x?))
}

pub fn sum_list<T: Sum>(xs: &[T]) -> Result<T> {
    sum_of(xs.iter().copied().map(Ok))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extremes_keep_the_first_of_a_tie_and_cant_order_a_nan() {
        assert_eq!(greatest([3, -3, 7, 7]), Ok(7));
        assert_eq!(least([3, -3, 7]), Ok(-3));
        assert_eq!(greatest_list(&Vec::<i64>::new()), Err(Error::Function));
        assert_eq!(greatest_of(Vec::<Result<i64>>::new()), Err(Error::Absent));
        assert_eq!(least([0.0, -0.0]).map(f64::is_sign_positive), Ok(true));
        assert!(greatest([f64::NAN]).is_ok_and(f64::is_nan));
        assert_eq!(greatest([1.0, f64::NAN]), Err(Error::Function));
        assert_eq!(least([f64::NAN, 1.0]), Err(Error::Function));
    }

    #[test]
    fn pow_takes_ieee_edges() {
        assert_eq!(pow(0.5, 3.0), Ok(0.125));
        assert_eq!(pow(2.0, -1.0), Ok(0.5));
        assert!(pow(-8.0, 1.0 / 3.0).is_ok_and(f64::is_nan));
        assert_eq!(pow(f64::NAN, 0.0), Ok(1.0));
        assert_eq!(pow(0.0, -1.0), Ok(f64::INFINITY));
    }

    #[test]
    fn sign_keeps_nan_and_zeroes_both_zeros() {
        assert_eq!(sign_double(-2.5), Ok(-1.0));
        assert!(sign_double(-0.0).is_ok_and(f64::is_sign_positive));
        assert!(sign_double(f64::NAN).is_ok_and(f64::is_nan));
    }

    #[test]
    fn shifts_put_zeros_in_and_refuse_a_negative_count() {
        assert_eq!(shift_left(1, 2), Ok(4));
        assert_eq!(shift_left(-1, 64), Ok(0));
        assert_eq!(shift_left(1, i64::MAX), Ok(0));
        assert_eq!(shift_right(-1024, 3), Ok(2_305_843_009_213_693_824));
        assert_eq!(shift_right(1024, 64), Ok(0));
        assert_eq!(shift_left(1, -2), Err(Error::Function));
        assert_eq!(shift_right(1, -2), Err(Error::Function));
    }

    #[test]
    fn int_arithmetic_is_checked() {
        assert_eq!(add_int(i64::MAX, 1), Err(Error::Overflow));
        assert_eq!(divide_int(1, 0), Err(Error::Division));
        assert_eq!(divide_int(i64::MIN, -1), Err(Error::Overflow));
        assert_eq!(modulo_int(-7, 2), Ok(-1));
        assert_eq!(sum_of([Ok(i64::MAX), Ok(1)]), Err(Error::Overflow));
        assert_eq!(sum_of(Vec::<Result<f64>>::new()), Ok(0.0));
    }
}
