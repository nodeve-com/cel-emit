//! Lists and maps where cel-emit follows cel-go over cel-core, and the `math` calls on them the
//! cel-spec suite lacks.

use cel_core::Value::{self, Bool as B, Double as D, Int as I};
use cel_emit::Type;

use crate::cases::{check, list, map, refusal, s, Got};

/// Only here: a negative index errs in `xs[i]` and holds nothing in `xs[?i]`, where cel-core
/// counts from the end; `math.pow` and `math.sum` over lists and maps match cel-core, a sum of
/// an empty list is zero, and an int sum overflows; a fold of no values is absent.
#[test]
fn lists_follow_cel_go_where_cel_core_differs() {
    check(|cases| {
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
        cases.absent("math.greatest(xs.map(x, x * 2.0))", &[("xs", list(vec![]))]);
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
    });
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
