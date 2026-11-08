//! The folds a [`super::Node::Reduce`] makes of a comprehension.

/// What a [`super::Node::Reduce`] folds its items to, each by a cel-emit-runtime function over an
/// iterator of results.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reduce {
    /// `math.greatest`: over none, absent.
    Greatest,
    /// `math.least`: over none, absent.
    Least,
    /// `math.sum`: over none, zero.
    Sum,
    /// `math.mean`: over none, an error.
    Mean,
    /// `math.variance`: of fewer than two, an error.
    Variance,
    /// `math.stddev`: of fewer than two, an error.
    Stddev,
    /// How many items it gives.
    Count,
}

impl Reduce {
    /// The fold a call to `name` with one list argument makes, if any.
    pub fn named(name: &str) -> Option<Reduce> {
        Some(match name {
            "math.greatest" => Reduce::Greatest,
            "math.least" => Reduce::Least,
            "math.sum" => Reduce::Sum,
            "math.mean" => Reduce::Mean,
            "math.variance" => Reduce::Variance,
            "math.stddev" => Reduce::Stddev,
            "size" => Reduce::Count,
            _ => return None,
        })
    }

    /// Its function's path under cel-emit-runtime.
    pub fn path(self) -> &'static str {
        match self {
            Reduce::Greatest => "math::greatest_of",
            Reduce::Least => "math::least_of",
            Reduce::Sum => "math::sum_of",
            Reduce::Mean => "math::mean_of",
            Reduce::Variance => "math::variance_of",
            Reduce::Stddev => "math::stddev_of",
            Reduce::Count => "collections::count",
        }
    }
}
