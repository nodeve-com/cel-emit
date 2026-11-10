//! Computed properties as a program walks them: an index is a list of [`Entry`], one per
//! property, in the order they fill. cel-emit writes each entry; the program walking them is
//! written once, for every record type `R` and params type `P`.

use std::borrow::Cow;

use crate::Error;

/// One computed property of a record `R`, its expression reading the caller's params `P` beside
/// the record.
#[derive(Clone, Copy, Debug)]
pub struct Entry<R: 'static, P: 'static> {
    /// The property it fills.
    pub key: &'static str,
    /// The properties its expression reads, in key order.
    pub reads: &'static [&'static str],
    /// Whether the record needs it: what its caller said, for the walk to judge by.
    pub required: bool,
    /// Computes the value, checks it, and stores it in the record.
    pub fill: fn(&mut R, &P) -> Outcome,
}

/// What filling one property came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The property holds its new value.
    Filled,
    /// Something its expression read was empty, so it didn't run: the property is as it was.
    Skipped,
    /// Its expression or its check failed: the property is as it was.
    Failed(Failure),
}

/// Where a value failed, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// The property, or a place inside its value: `total`, `label.text`.
    pub place: Cow<'static, str>,
    pub reason: Reason,
}

/// Why a property's value failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// Its expression raised a CEL error.
    Expression(Error),
    /// Its check refused the value, in the check's own words.
    Check(Cow<'static, str>),
}

impl Failure {
    /// The expression filling `key` raised `error`.
    pub fn expression(key: &'static str, error: Error) -> Failure {
        Failure {
            place: Cow::Borrowed(key),
            reason: Reason::Expression(error),
        }
    }

    /// A check refused the value at `place`, for `reason`.
    pub fn check(
        place: impl Into<Cow<'static, str>>,
        reason: impl Into<Cow<'static, str>>,
    ) -> Failure {
        Failure {
            place: place.into(),
            reason: Reason::Check(reason.into()),
        }
    }
}

/// What an expression's result becomes before its check: an empty input is no failure.
pub fn computed<T>(key: &'static str, value: crate::Result<T>) -> Result<T, Outcome> {
    match value {
        Ok(value) => Ok(value),
        Err(Error::Absent) => Err(Outcome::Skipped),
        Err(error) => Err(Outcome::Failed(Failure::expression(key, error))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_input_skips_and_any_other_error_fails_at_the_key() {
        assert_eq!(computed("total", Ok(1)), Ok(1));
        assert_eq!(
            computed::<i64>("total", Err(Error::Absent)),
            Err(Outcome::Skipped)
        );
        let failure = Failure::expression("total", Error::Overflow);
        assert_eq!(
            computed::<i64>("total", Err(Error::Overflow)),
            Err(Outcome::Failed(failure))
        );
    }

    #[test]
    fn a_check_failure_names_its_own_place() {
        let failure = Failure::check("label.text", "range");
        assert_eq!(failure.place, "label.text");
        assert_eq!(failure.reason, Reason::Check("range".into()));
    }
}
