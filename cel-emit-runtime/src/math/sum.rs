//! `math.sum` and `math.mean` of a list, or of what a comprehension gives. `math` re-exports
//! each, so `math::sum_of` names it.

use super::add_int;
use crate::{Error, Result};

/// A number `math.sum` adds, an int checked and a double as IEEE 754 does, and `math.mean`
/// averages as a double.
pub trait Sum: Copy {
    const ZERO: Self;
    fn plus(self, other: Self) -> Result<Self>;
    fn double(self) -> f64;
}

impl Sum for i64 {
    const ZERO: i64 = 0;
    fn plus(self, other: i64) -> Result<i64> {
        add_int(self, other)
    }
    fn double(self) -> f64 {
        self as f64
    }
}

impl Sum for f64 {
    const ZERO: f64 = 0.0;
    fn plus(self, other: f64) -> Result<f64> {
        Ok(self + other)
    }
    fn double(self) -> f64 {
        self
    }
}

/// `math.sum` of what a comprehension gives, in order: over none, zero.
pub fn sum_of<T: Sum>(xs: impl IntoIterator<Item = Result<T>>) -> Result<T> {
    xs.into_iter().try_fold(T::ZERO, |total, x| total.plus(x?))
}

pub fn sum_list<T: Sum>(xs: &[T]) -> Result<T> {
    sum_of(xs.iter().copied().map(Ok))
}

/// `math.mean` of what a comprehension gives, in order, each added as a double: over none, an
/// error.
pub fn mean_of<T: Sum>(xs: impl IntoIterator<Item = Result<T>>) -> Result<f64> {
    let (total, count) = xs.into_iter().try_fold((0.0, 0.0), |(total, count), x| {
        Ok::<_, Error>((total + x?.double(), count + 1.0))
    })?;
    match count > 0.0 {
        true => Ok(total / count),
        false => Err(Error::Function),
    }
}

pub fn mean_list<T: Sum>(xs: &[T]) -> Result<f64> {
    mean_of(xs.iter().copied().map(Ok))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_sums_are_checked() {
        assert_eq!(sum_of([Ok(i64::MAX), Ok(1)]), Err(Error::Overflow));
        assert_eq!(sum_of(Vec::<Result<f64>>::new()), Ok(0.0));
    }

    #[test]
    fn mean_adds_as_doubles_and_refuses_none() {
        assert_eq!(mean_list(&[1i64, 2]), Ok(1.5));
        assert_eq!(mean_list(&[i64::MAX, i64::MAX]), Ok(i64::MAX as f64));
        assert_eq!(mean_list(&[0.5, -1.5, 4.0]), Ok(1.0));
        assert_eq!(mean_list(&Vec::<f64>::new()), Err(Error::Function));
        assert_eq!(mean_of([Ok(1.0), Err(Error::Range)]), Err(Error::Range));
    }
}
