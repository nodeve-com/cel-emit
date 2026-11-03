//! The lists extension over lists of dyn, and over a list `json.decode` gives. A dyn result
//! is written as JSON text, which `json.encode` gives.

use cel_core::Value::{Bool as B, Int as I};

use super::{fail, gives, ints};
use crate::cases::{check, s, Cases};
use cel_core::Value;

/// `source`, a dyn, encoded over `inputs` gives the JSON text `want`.
fn json(cases: &mut Cases, source: &str, inputs: &[(&str, Value)], want: &str) {
    gives(cases, &format!("json.encode({source})"), inputs, s(want));
}

/// Only here: flatten spreads a list of lists one level, a list of dyn by the kind of each
/// element, and `flatten(depth)` stops once nothing is a list; a negative depth is an error.
#[test]
fn flatten_spreads_nested_lists() {
    check(|cases| {
        flatten_typed(cases);
        flatten_depths(cases);
        flatten_negative_depths(cases);
        flatten_decoded(cases);
    });
}

/// Flatten of a list of lists of ints.
fn flatten_typed(cases: &mut Cases) {
    let nested = [(
        "xss",
        Value::list(vec![ints(&[1, 2]), ints(&[]), ints(&[3, 2, 1])]),
    )];
    gives(cases, "xss.flatten()", &nested, ints(&[1, 2, 3, 2, 1]));
    gives(
        cases,
        "xss.flatten().distinct().sort()",
        &nested,
        ints(&[1, 2, 3]),
    );
    gives(
        cases,
        "xss.flatten().map(x, x + 1)",
        &nested,
        ints(&[2, 3, 4, 3, 2]),
    );
    gives(cases, "size(xss.flatten())", &nested, I(5));
    gives(cases, "xss.flatten().first().orValue(0)", &nested, I(1));
}

/// Flatten of a list of dyn, by depth.
fn flatten_depths(cases: &mut Cases) {
    let mixed = "[[1, [2]], 3, [4]]";
    gives(cases, &format!("size({mixed}.flatten())"), &[], I(4));
    for (depth, want) in [
        (0, "[[1,[2]],3,[4]]"),
        (1, "[1,[2],3,4]"),
        (2, "[1,2,3,4]"),
        (3, "[1,2,3,4]"),
        (i64::MAX, "[1,2,3,4]"),
    ] {
        json(cases, &format!("{mixed}.flatten({depth})"), &[], want);
    }
    json(
        cases,
        &format!("{mixed}.flatten(2).sort()"),
        &[],
        "[1,2,3,4]",
    );
    json(cases, "[[], [[]], 1].flatten(5)", &[], "[1]");
    json(cases, "[[1, 'a'], 'b'].flatten()", &[], "[1,\"a\",\"b\"]");
}

/// A negative depth is an error.
fn flatten_negative_depths(cases: &mut Cases) {
    let min = "-9223372036854775807 - 1";
    let bad = [
        "json.encode([[1], 2].flatten(-1))".to_string(),
        format!("json.encode([1, 2.5].flatten({min}))"),
    ];
    fail(cases, &[&bad[0], &bad[1]], &[]);
}

/// Flatten of a decoded list.
fn flatten_decoded(cases: &mut Cases) {
    let dyns = [("t", s("[[1, [2]], 3, [4]]"))];
    json(
        cases,
        "json.decode(t).flatten(2).sort()",
        &dyns,
        "[1,2,3,4]",
    );
    json(cases, "json.decode(t).flatten()", &dyns, "[1,[2],3,4]");
    json(
        cases,
        "json.decode(t).flatten(9223372036854775807)",
        &dyns,
        "[1,2,3,4]",
    );
    json(
        cases,
        "json.decode(t).flatten(2).distinct().reverse()",
        &dyns,
        "[4,3,2,1]",
    );
    gives(cases, "json.decode(t).flatten().size()", &dyns, I(4));
    fail(cases, &["json.encode(json.decode(t).flatten(-1))"], &dyns);
}

/// Only here: a list `json.decode` gives, a dyn, takes the methods; a mixed list or one that is
/// no list is an error.
#[test]
fn a_decoded_list_takes_the_methods() {
    check(|cases| {
        decoded_methods(cases);
        decoded_sorts(cases);
        decoded_mixed_errors(cases);
        decoded_distinct(cases);
    });
}

/// The methods on a decoded list of numbers.
fn decoded_methods(cases: &mut Cases) {
    let t = |text: &str| [("t", s(text))];
    let numbers = t("[3, 1, 2, 3]");
    for (call, want) in [
        ("sort()", "[1,2,3,3]"),
        ("distinct()", "[3,1,2]"),
        ("reverse()", "[3,2,1,3]"),
        ("slice(1, 3)", "[1,2]"),
        ("slice(0, 0)", "[]"),
        ("distinct().sort()", "[1,2,3]"),
        ("first().orValue(0)", "3"),
        ("last().orValue(0)", "3"),
        ("sort().first().orValue(0)", "1"),
    ] {
        json(cases, &format!("json.decode(t).{call}"), &numbers, want);
    }
    gives(
        cases,
        "json.decode(t).first().hasValue()",
        &numbers,
        B(true),
    );
    gives(cases, "json.decode(t).distinct().size()", &numbers, I(3));
    json(cases, "json.decode(t).first().orValue(0)", &t("[]"), "0");
}

/// Sorts of decoded lists of one kind.
fn decoded_sorts(cases: &mut Cases) {
    let t = |text: &str| [("t", s(text))];
    for (text, want) in [
        ("[]", "[]"),
        ("[4]", "[4]"),
        ("[\"b\", \"a\"]", "[\"a\",\"b\"]"),
        ("[2.5, 1.5]", "[1.5,2.5]"),
        ("[true, false]", "[false,true]"),
    ] {
        json(cases, "json.decode(t).sort()", &t(text), want);
    }
}

/// A mixed list, or no list, is an error.
fn decoded_mixed_errors(cases: &mut Cases) {
    let t = |text: &str| [("t", s(text))];
    let numbers = [("t", s("[3, 1, 2, 3]"))];
    let mixed = ["[1, \"a\"]", "[1, 2.5]", "[1, null]", "[[1], [2]]", "[{}]"];
    for text in mixed {
        let sort = "json.encode(json.decode(t).sort())";
        fail(cases, &[sort], &t(text));
    }
    for text in ["{}", "3", "\"a\"", "null"] {
        fail(
            cases,
            &[
                "json.encode(json.decode(t).reverse())",
                "json.encode(json.decode(t).flatten())",
                "json.encode(json.decode(t).slice(0, 0))",
                "json.decode(t).first().hasValue()",
            ],
            &t(text),
        );
    }
    fail(
        cases,
        &[
            "json.encode(json.decode(t).slice(0, 5))",
            "json.encode(json.decode(t).slice(-1, 1))",
        ],
        &numbers,
    );
}

/// Distinct over a decoded mixed list.
fn decoded_distinct(cases: &mut Cases) {
    let t = |text: &str| [("t", s(text))];
    let mixed = t("[1, 1.0, \"1\", null, null]");
    gives(cases, "json.decode(t).distinct().size()", &mixed, I(3));
}
