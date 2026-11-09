//! `json.encode` of an object literal, each member written as it computes.

use cel_core::Value::Int as I;
use cel_emit_runtime::Error as Fail;

use crate::cases::{check, s, Got};

/// With `skip_failed`, a member that fails is left out, and so is a nested object every member of
/// which is; its siblings after it are still written. An object left with nothing fails.
#[test]
fn a_nested_object_whose_members_all_fail_is_left_out() {
    check(|cases| {
        let inputs = [("n", I(3))];
        let source = "json.encode({'z': n, 'o': {'a': 1 / (n - 3)}, 'p': {'b': n, 'c': 1 / 0}})";
        cases.gives_skipping(source, &inputs, Got::Value(s(r#"{"z":3,"p":{"b":3}}"#)));
        let source = "json.encode({'o': {'a': 1 / (n - 3)}})";
        cases.gives_skipping(source, &inputs, Got::Status(Fail::Division));
    });
}
