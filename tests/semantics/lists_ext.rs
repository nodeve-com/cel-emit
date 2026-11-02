//! CEL's lists extension, where cel-core has none: expected values are stated by hand, as cel-go
//! gives them.

use cel_core::Value::{self, Bool as B, Double as D, Int as I, UInt as U};

use crate::cases::{check, list, s, Cases, Got};

mod dynamic;
mod sort_by;
mod stored;

fn ints(xs: &[i64]) -> Value {
    list(xs.iter().map(|x| I(*x)).collect())
}

fn gives(cases: &mut Cases, source: &str, inputs: &[(&str, Value)], want: Value) {
    cases.gives(source, inputs, Got::Value(want));
}

/// Sources that are errors over `inputs`.
fn fail(cases: &mut Cases, sources: &[&str], inputs: &[(&str, Value)]) {
    for source in sources {
        cases.gives(source, inputs, Got::Error);
    }
}

/// Only here: reverse, slice, first, distinct, and sort over typed lists, empty and single
/// ones, and chains that fill an optional or feed a comprehension.
#[test]
fn lists_reshape_typed_lists() {
    check(|cases| {
        let xs = [("xs", ints(&[3, 1, 2, 3, 1]))];
        for (source, want) in [
            ("xs.reverse()", ints(&[1, 3, 2, 1, 3])),
            ("xs.slice(1, 3)", ints(&[1, 2])),
            ("xs.slice(0, 5)", ints(&[3, 1, 2, 3, 1])),
            ("xs.slice(2, 2)", ints(&[])),
            ("xs.slice(5, 5)", ints(&[])),
            ("xs.sort()", ints(&[1, 1, 2, 3, 3])),
            ("xs.distinct()", ints(&[3, 1, 2])),
            ("xs.sort().distinct()", ints(&[1, 2, 3])),
            ("xs.distinct().sort().reverse()", ints(&[3, 2, 1])),
            ("xs.distinct().map(x, x * 2)", ints(&[6, 2, 4])),
            ("xs.sort().slice(1, 4).reverse()", ints(&[3, 2, 1])),
            (
                "xs.sort() + xs.reverse()",
                ints(&[1, 1, 2, 3, 3, 1, 3, 2, 1, 3]),
            ),
        ] {
            gives(cases, source, &xs, want);
        }
        for (source, want) in [
            ("size(xs.distinct())", I(3)),
            ("size(xs.slice(1, 4))", I(3)),
            ("xs.first().orValue(-1)", I(3)),
            ("xs.sort().first().orValue(-1)", I(1)),
            ("xs.first().hasValue()", B(true)),
            ("xs.distinct().sort()[2]", I(3)),
            ("xs.sort().exists(x, x == 3)", B(true)),
        ] {
            gives(cases, source, &xs, want);
        }
        fail(
            cases,
            &[
                "xs.slice(-1, 2)",
                "xs.slice(0, -1)",
                "xs.slice(2, 1)",
                "xs.slice(0, 6)",
                "xs.slice(6, 6)",
                "xs.slice(0, 9223372036854775807)",
                "xs.slice(9223372036854775807, 9223372036854775807)",
                "xs.slice(-9223372036854775807 - 1, 9223372036854775807)",
                "xs.slice(-9223372036854775807 - 1, -9223372036854775807 - 1)",
            ],
            &xs,
        );
        let none = [("e", list(vec![]))];
        gives(cases, "e.first().orValue(1.5)", &none, D(1.5));
        gives(cases, "e.first().hasValue()", &none, B(false));
        for source in ["e.reverse()", "e.sort()", "e.distinct()", "e.slice(0, 0)"] {
            gives(cases, source, &none, list(vec![]));
        }
        fail(cases, &["e.slice(0, 1)"], &none);
        let one = [("one", ints(&[7]))];
        for source in [
            "one.sort()",
            "one.reverse()",
            "one.distinct()",
            "one.slice(0, 1)",
        ] {
            gives(cases, source, &one, ints(&[7]));
        }
        gives(cases, "one.first().orValue(0)", &one, I(7));
        gives(cases, "one.slice(1, 1)", &one, ints(&[]));
        for (source, want) in [
            ("size([].reverse())", I(0)),
            ("size([].sort())", I(0)),
            ("size([].distinct())", I(0)),
            ("size([].slice(0, 0))", I(0)),
            ("[].first().hasValue()", B(false)),
            (
                "[[1], [1], [2]].distinct()",
                list(vec![ints(&[1]), ints(&[2])]),
            ),
            ("[[1, 2], [3]].flatten()", ints(&[1, 2, 3])),
        ] {
            gives(cases, source, &[], want);
        }
    });
}

/// Only here: sort puts each ordered kind ascending, strings by code point, equal doubles in
/// their given order, and refuses a NaN; distinct compares as `==` does.
#[test]
fn sort_orders_each_kind() {
    check(|cases| {
        let at = "timestamp('2026-01-0";
        let times = format!("[{at}2T00:00:00Z'), {at}1T00:00:00Z'), {at}3T00:00:00Z')]");
        let first = format!("{times}.sort()[0] == {at}1T00:00:00Z')");
        let last = format!("{times}.sort()[2] == {at}3T00:00:00Z')");
        let spans = "[duration('2h'), duration('-1s'), duration('90m')]";
        let words = [("w", list(vec![s("b"), s("a"), s("é"), s("B"), s("a")]))];
        gives(
            cases,
            "w.sort()",
            &words,
            list(vec![s("B"), s("a"), s("a"), s("b"), s("é")]),
        );
        gives(
            cases,
            "w.distinct()",
            &words,
            list(vec![s("b"), s("a"), s("é"), s("B")]),
        );
        gives(
            cases,
            "w.reverse()",
            &words,
            list(vec![s("a"), s("B"), s("é"), s("a"), s("b")]),
        );
        gives(cases, "w.sort().first().orValue('')", &words, s("B"));
        gives(
            cases,
            "[3u, 1u, 2u].sort()",
            &[],
            list(vec![U(1), U(2), U(3)]),
        );
        gives(
            cases,
            "[true, false, true].sort()",
            &[],
            list(vec![B(false), B(true), B(true)]),
        );
        gives(
            cases,
            "[b'b', b'a', b'ab'].sort()[1] == b'ab'",
            &[],
            B(true),
        );
        gives(cases, &first, &[], B(true));
        gives(cases, &last, &[], B(true));
        gives(
            cases,
            &format!("{spans}.sort()[0] == duration('-1s')"),
            &[],
            B(true),
        );
        gives(
            cases,
            &format!("{spans}.sort()[2] == duration('2h')"),
            &[],
            B(true),
        );
        gives(
            cases,
            "[2.5, -1.0, 0.0, 1e300].sort()",
            &[],
            list(vec![D(-1.0), D(0.0), D(2.5), D(1e300)]),
        );
        let zeros = |a: f64, b: f64| [("z", list(vec![D(a), D(b), D(-3.0)]))];
        gives(
            cases,
            "z.sort()",
            &zeros(0.0, -0.0),
            list(vec![D(-3.0), D(0.0), D(-0.0)]),
        );
        gives(
            cases,
            "z.sort()",
            &zeros(-0.0, 0.0),
            list(vec![D(-3.0), D(-0.0), D(0.0)]),
        );
        gives(
            cases,
            "z.distinct()",
            &zeros(0.0, -0.0),
            list(vec![D(0.0), D(-3.0)]),
        );
        let nan = |at: usize| {
            let mut v = vec![D(1.0), D(0.5), D(2.0)];
            v.insert(at, D(f64::NAN));
            [("n", list(v))]
        };
        for at in 0..4 {
            fail(cases, &["n.sort()"], &nan(at));
        }
        gives(cases, "n.size()", &nan(0), I(4));
        gives(cases, "n.distinct().size()", &nan(1), I(4));
        gives(cases, "n.reverse().size()", &nan(2), I(4));
        gives(cases, "n.slice(1, 2).size()", &nan(2), I(1));
        let big = [("b", ints(&[i64::MAX, i64::MIN, 0]))];
        gives(cases, "b.sort()", &big, ints(&[i64::MIN, 0, i64::MAX]));
        gives(cases, "[1, 1.0, 1u].distinct().size()", &[], I(1));
        gives(cases, "[1, 'a', 1, 'a', 2.5].distinct().size()", &[], I(3));
        gives(cases, "[1, 2].map(x, [x, x]).distinct().size()", &[], I(2));
        gives(cases, "[[3], [1]].map(x, x[0]).sort()", &[], ints(&[1, 3]));
    });
}
