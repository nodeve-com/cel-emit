//! Lists and maps where cel-emit follows cel-go over cel-core, and the `math` calls on them the
//! cel-spec suite lacks.

use cel_core::Value::{self, Bool as B, Double as D, Int as I};
use cel_emit::Type;

use crate::cases::{check, list, map, refusal, s, Cases, Got};

/// Only here: a negative index errs in `xs[i]` and holds nothing in `xs[?i]`, where cel-core
/// counts from the end; `math.pow` and `math.sum` over lists and maps match cel-core, a sum of
/// an empty list is zero, and an int sum overflows; a fold of no values is absent; `math.mean`
/// adds as doubles, and of no values is an error.
#[test]
fn lists_follow_cel_go_where_cel_core_differs() {
    check(|cases| {
        negative_indexes(cases);
        powers(cases);
        cases.absent("math.greatest(xs.map(x, x * 2.0))", &[("xs", list(vec![]))]);
        map_sums(cases);
        means(cases);
    });
}

fn negative_indexes(cases: &mut Cases) {
    let strings = list(vec![s("a"), s("bc")]);
    let doubles = list(vec![D(1.5), D(-2.0)]);
    for xs in [&strings, &doubles] {
        for i in [-1, i64::MIN] {
            cases.gives("xs[i]", &[("xs", xs.clone()), ("i", I(i))], Got::Error);
        }
    }
    let inputs = [("xs", doubles), ("i", I(-1))];
    cases.gives("xs[?i].orValue(7.0)", &inputs, Got::Value(D(7.0)));
    cases.gives("xs[?i].hasValue()", &inputs, Got::Value(B(false)));
}

fn powers(cases: &mut Cases) {
    for (x, y) in [
        (0.5, 3.0),
        (2.0, -1.0),
        (10.0, 0.1),
        (-8.0, 1.0 / 3.0),
        (f64::NAN, 0.0),
        (0.0, -1.0),
        (1.0000001, 1e9),
    ] {
        cases.agrees("math.pow(x, y)", &[("x", D(x)), ("y", D(y))]);
    }
}

fn map_sums(cases: &mut Cases) {
    let maps = [
        map(&[]),
        map(&[("a", D(1.5))]),
        map(&[("a", D(1.5)), ("bc", D(-2.0)), ("b", D(4.0))]),
    ];
    for m in maps {
        let inputs = [("m", m.clone())];
        // cel-core picks an empty list's overload as it runs: int, not the double checked.
        let empty = matches!(&m, Value::Map(e) if e.is_empty());
        for source in [
            "math.sum(m.map(x, m[x]))",
            "math.sum(m.transformList(x, v, v * 2.0))",
        ] {
            match empty {
                true => cases.gives(source, &inputs, Got::Value(D(0.0))),
                false => cases.agrees(source, &inputs),
            }
        }
    }
    let ints = [("m", map(&[("a", I(i64::MAX)), ("b", I(1))]))];
    cases.agrees("math.sum(m.transformList(k, v, v))", &ints);
    cases.gives("math.sum(m.map(k, m[k]))", &ints, Got::Error);
}

fn means(cases: &mut Cases) {
    let ints = [("xs", list(vec![I(i64::MAX), I(i64::MAX), I(1)]))];
    let doubles = [("xs", list(vec![D(0.5), D(-1.5), D(4.0)]))];
    let empty = [("xs", list(vec![]))];
    cases.gives("math.mean([1, 2])", &[], Got::Value(D(1.5)));
    cases.gives("math.mean(xs)", &doubles, Got::Value(D(1.0)));
    cases.gives(
        "math.mean(xs.map(x, x * 2.0))",
        &doubles,
        Got::Value(D(2.0)),
    );
    cases.gives(
        "math.mean(xs.filter(x, x > 0.0))",
        &doubles,
        Got::Value(D(2.25)),
    );
    cases.gives("math.mean(xs.map(x, x * 2.0))", &empty, Got::Error);
    let big = (i64::MAX as f64 * 2.0 + 1.0) / 3.0;
    cases.gives("math.mean(xs)", &ints, Got::Value(D(big)));
}

/// Only here: compile refuses a `math` call whose operands no single declared type holds.
#[test]
fn math_cel_emit_cant_type_is_rejected_at_compile() {
    let names = [("a", Type::Int), ("x", Type::Double)];
    for source in [
        "math.greatest(a, x)",
        "math.greatest(a, 2u)",
        "math.bitAnd(1u, 2)",
        "math.sqrt(x)",
        "math.greatest(x, x, x, x, x, x, x)",
    ] {
        assert!(refusal(source, &names).is_some(), "{source}");
    }
}

/// A field of a value no name reaches, read inside a comprehension, is that field: never the
/// loop's index or element, which have no path to differ from.
#[test]
fn a_field_of_a_call_in_a_comprehension_is_that_field() {
    check(|cases| {
        let t = list(vec![s("{\"k\": 2}"), s("{\"k\": 1}")]);
        let inputs = [("t", t)];
        for (source, want) in [
            ("json.encode(t.map(x, json.decode(x).k))", "[2,1]"),
            ("json.encode(t.map(x, json.decode(t[1]).k))", "[1,1]"),
            (
                "json.encode(t.transformList(i, x, json.decode(x).k))",
                "[2,1]",
            ),
        ] {
            cases.gives(source, &inputs, Got::Value(s(want)));
        }
        let m = map(&[("a", s("{\"k\": 2}"))]);
        cases.gives(
            "json.encode(m.map(n, json.decode(m[n]).k))",
            &[("m", m)],
            Got::Value(s("[2]")),
        );
    });
}

/// A `map` whose body is a map literal of constant string keys encodes as an array of objects,
/// in key order, a filter leaving items out and a failing field failing the whole expression.
#[test]
fn a_map_of_literal_objects_encodes_as_an_array() {
    check(|cases| {
        let xs = list(vec![I(1), I(2), I(3)]);
        let inputs = [("xs", xs)];
        for (source, want) in [
            (
                "json.encode(xs.map(x, {'b': x, 'a': 'k', 'c': x > 1}))",
                r#"[{"b":1,"a":"k","c":false},{"b":2,"a":"k","c":true},{"b":3,"a":"k","c":true}]"#,
            ),
            (
                "json.encode(xs.filter(x, x != 2).map(i, {'v': i * 2}))",
                r#"[{"v":2},{"v":6}]"#,
            ),
            ("json.encode(xs.map(x, {}))", "[{},{},{}]"),
            ("json.encode([].map(x, {'a': x}))", "[]"),
        ] {
            cases.gives(source, &inputs, Got::Value(s(want)));
        }
        cases.gives(
            "json.encode(xs.map(x, {'a': 1 / (x - 2)}))",
            &inputs,
            Got::Error,
        );
    });
}
