//! A failed call is its error kind, and an `||` of two failures reports the left one. A bool
//! converts to text.

use cel_core::Value;
use cel_emit_runtime::Error;

use crate::cases::{check, list, s, Got};

/// Only here: `string()` of a bool, a constant or one read, is `true` or `false`, as cel-core
/// gives it.
#[test]
fn a_bool_converts_to_text() {
    check(|cases| {
        for (b, text) in [(true, "true"), (false, "false")] {
            let inputs = [("b", Value::Bool(b))];
            cases.gives("string(b)", &inputs, Got::Value(s(text)));
            cases.agrees("string(b) + string(!b)", &inputs);
        }
        cases.gives("string(1 > 2)", &[], Got::Value(s("false")));
    });
}

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
