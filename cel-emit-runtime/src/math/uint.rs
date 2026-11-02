//! Uint arithmetic, checked as int's is: past the range an overflow, by zero a division error.
//! `math` re-exports each function, so `math::add_uint` names it.

use super::shift;
use crate::{Error, Result};

pub fn add_uint(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b).ok_or(Error::Overflow)
}

pub fn subtract_uint(a: u64, b: u64) -> Result<u64> {
    a.checked_sub(b).ok_or(Error::Overflow)
}

pub fn multiply_uint(a: u64, b: u64) -> Result<u64> {
    a.checked_mul(b).ok_or(Error::Overflow)
}

/// Toward zero: by zero a division error, which `checked_div` gives as none.
pub fn divide_uint(a: u64, b: u64) -> Result<u64> {
    a.checked_div(b).ok_or(Error::Division)
}

/// The remainder of [`divide_uint`].
pub fn modulo_uint(a: u64, b: u64) -> Result<u64> {
    a.checked_rem(b).ok_or(Error::Division)
}

pub fn abs_uint(a: u64) -> Result<u64> {
    Ok(a)
}

/// Zero or one.
pub fn sign_uint(a: u64) -> Result<u64> {
    Ok(u64::from(a != 0))
}

pub fn bit_and_uint(a: u64, b: u64) -> Result<u64> {
    Ok(a & b)
}

pub fn bit_or_uint(a: u64, b: u64) -> Result<u64> {
    Ok(a | b)
}

pub fn bit_xor_uint(a: u64, b: u64) -> Result<u64> {
    Ok(a ^ b)
}

pub fn bit_not_uint(a: u64) -> Result<u64> {
    Ok(!a)
}

/// `a` shifted left `bits`, as `shift_left` does an int.
pub fn shift_left_uint(a: u64, bits: i64) -> Result<u64> {
    shift(bits, |b| a.checked_shl(b))
}

/// `a` shifted right `bits`, zeros in, as `shift_right` does an int.
pub fn shift_right_uint(a: u64, bits: i64) -> Result<u64> {
    shift(bits, |b| a.checked_shr(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uint_arithmetic_is_checked() {
        assert_eq!(add_uint(u64::MAX, 1), Err(Error::Overflow));
        assert_eq!(subtract_uint(0, 1), Err(Error::Overflow));
        assert_eq!(multiply_uint(1 << 32, 1 << 32), Err(Error::Overflow));
        assert_eq!(divide_uint(7, 2), Ok(3));
        assert_eq!(divide_uint(1, 0), Err(Error::Division));
        assert_eq!(modulo_uint(7, 0), Err(Error::Division));
        assert_eq!(modulo_uint(u64::MAX, 2), Ok(1));
    }

    #[test]
    fn uint_bits_and_shifts_take_zeros_in() {
        assert_eq!(sign_uint(0), Ok(0));
        assert_eq!(sign_uint(100), Ok(1));
        assert_eq!(bit_not_uint(0), Ok(u64::MAX));
        assert_eq!(shift_left_uint(1, 200), Ok(0));
        assert_eq!(shift_right_uint(1024, 2), Ok(256));
        assert_eq!(shift_left_uint(1, -1), Err(Error::Function));
        assert_eq!(shift_right_uint(1, -1), Err(Error::Function));
    }
}
