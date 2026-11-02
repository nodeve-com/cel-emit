//! A failed call is its error kind, and an `||` of two failures reports the left one.

use cel_emit_runtime::Error;

use crate::cases::{check, list, Got};

/// Only here: the kind a failed call gives, overflow, division, or function, and which of two
/// failures an `||` reports.
#[test]
fn a_failed_call_is_its_error_kind() {
    check(|cases| {
        let inputs = [("xs", list(vec![]))];
        for (source, want) in [
            ("math.abs(-9223372036854775808)", Error::Overflow),
            ("math.bitShiftLeft(1, -2)", Error::Function),
            ("math.least(xs)", Error::Function),
            (
                "1 / 0 > 0 || math.abs(-9223372036854775808) > 0",
                Error::Division,
            ),
            (
                "math.abs(-9223372036854775808) > 0 || 1 / 0 > 0",
                Error::Overflow,
            ),
        ] {
            cases.gives(source, &inputs, Got::Status(want));
        }
    });
}
