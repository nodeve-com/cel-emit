//! The statistics of a list, or of what a comprehension gives: `math.sum`, `math.mean`,
//! `math.variance`, `math.stddev`, `math.median`, and `math.percentile`. `math` re-exports each,
//! so `math::sum_of` names it. Each but `math.sum` gives a double, its ints read as doubles, so
//! none overflows.

use super::add_int;
use crate::{Error, Result};

/// A number `math.sum` adds, an int checked and a double as IEEE 754 does, and the other
/// statistics read as a double.
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

/// `math.mean` of what a comprehension gives, in order, each added as a double, the total over
/// how many: over none, an error.
pub fn mean_of<T: Sum>(xs: impl IntoIterator<Item = Result<T>>) -> Result<f64> {
    let (total, n) = xs.into_iter().try_fold((0.0, 0.0), |(total, n), x| {
        Ok::<_, Error>((total + x?.double(), n + 1.0))
    })?;
    if n > 0.0 {
        Ok(total / n)
    } else {
        Err(Error::Function)
    }
}

pub fn mean_list<T: Sum>(xs: &[T]) -> Result<f64> {
    mean_of(xs.iter().copied().map(Ok))
}

/// `math.variance` of what a comprehension gives: a sample's, its squared distances from the
/// mean over one fewer than how many there are. Of fewer than two, an error.
pub fn variance_of<T: Sum>(xs: impl IntoIterator<Item = Result<T>>) -> Result<f64> {
    let (n, squares) = spread(xs)?;
    if n > 1.0 {
        Ok(squares / (n - 1.0))
    } else {
        Err(Error::Function)
    }
}

pub fn variance_list<T: Sum>(xs: &[T]) -> Result<f64> {
    variance_of(xs.iter().copied().map(Ok))
}

/// `math.stddev` of what a comprehension gives: the square root of its `math.variance`.
pub fn stddev_of<T: Sum>(xs: impl IntoIterator<Item = Result<T>>) -> Result<f64> {
    variance_of(xs).map(f64::sqrt)
}

pub fn stddev_list<T: Sum>(xs: &[T]) -> Result<f64> {
    stddev_of(xs.iter().copied().map(Ok))
}

/// How many, and their squared distances from their mean summed, in one pass (Welford's
/// method), so a large mean loses no precision.
fn spread<T: Sum>(xs: impl IntoIterator<Item = Result<T>>) -> Result<(f64, f64)> {
    let (n, _, squares) = xs
        .into_iter()
        .try_fold((0.0, 0.0, 0.0), |(n, mean, squares), x| {
            let x = x?.double();
            let n = n + 1.0;
            let step = x - mean;
            let mean = mean + step / n;
            Ok::<_, Error>((n, mean, squares + step * (x - mean)))
        })?;
    Ok((n, squares))
}

/// `math.median`: the middle value, or the mean of the two in the middle. Of none, an error.
pub fn median_list<T: Sum>(xs: &[T]) -> Result<f64> {
    percentile_list(xs, 50.0)
}

/// `math.percentile`: the value `p` percent of the way from the least to the greatest, `p` from
/// 0 to 100, between two values by a straight line, as spreadsheets and numpy give it. Of none,
/// or with `p` outside 0 to 100, an error.
pub fn percentile_list<T: Sum, P: Sum>(xs: &[T], p: P) -> Result<f64> {
    let p = p.double();
    if !(0.0..=100.0).contains(&p) {
        return Err(Error::Range);
    }
    let mut sorted: Vec<f64> = xs.iter().map(|x| x.double()).collect();
    sorted.sort_by(f64::total_cmp);
    let last = sorted.len().checked_sub(1).ok_or(Error::Function)?;
    let rank = p / 100.0 * last as f64;
    let below = rank.floor();
    let at = |i: f64| sorted.get(i as usize).copied().ok_or(Error::Function);
    let (low, high) = (at(below)?, at(rank.ceil())?);
    Ok(low + (high - low) * (rank - below))
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

    #[test]
    fn variance_is_a_samples() {
        assert_eq!(variance_list(&[2i64, 4, 4, 4, 5, 5, 7, 9]), Ok(32.0 / 7.0));
        assert_eq!(stddev_list(&[1.0, 3.0]), Ok(2f64.sqrt()));
        assert_eq!(variance_list(&[1e9 + 1.0, 1e9 + 3.0]), Ok(2.0));
        assert_eq!(variance_list(&[5i64]), Err(Error::Function));
        assert_eq!(stddev_of(Vec::<Result<f64>>::new()), Err(Error::Function));
    }

    #[test]
    fn percentiles_interpolate() {
        assert_eq!(median_list(&[3i64, 1, 2]), Ok(2.0));
        assert_eq!(median_list(&[4i64, 1, 3, 2]), Ok(2.5));
        assert_eq!(percentile_list(&[1.0, 2.0, 3.0, 4.0, 5.0], 90i64), Ok(4.6));
        assert_eq!(percentile_list(&[10i64, 20], 0.0), Ok(10.0));
        assert_eq!(percentile_list(&[10i64, 20], 100.0), Ok(20.0));
        assert_eq!(percentile_list(&[7.0], 25.0), Ok(7.0));
        assert_eq!(percentile_list(&[1.0], 101.0), Err(Error::Range));
        assert_eq!(median_list(&Vec::<i64>::new()), Err(Error::Function));
    }
}
