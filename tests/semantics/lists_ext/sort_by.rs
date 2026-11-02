//! `sortBy`: the lists extension's macro, ordering a list by a key of each element.

use cel_core::Value::{Double as D, Int as I};
use cel_emit::{Field, Type, Width};

use super::{fail, gives, ints};
use crate::cases::{check, list, refusal, s, Cases, Got};

/// Only here: ascending and stable, over an empty and a single list, a field key, a computed
/// key, nesting, a receiver that is itself a call chain, and a key that is a comprehension.
#[test]
fn sort_by_orders_by_key() {
    check(|cases| {
        let xs = [("xs", ints(&[3, -1, 2, -3, 1]))];
        for (source, want) in [
            ("xs.sortBy(x, x)", ints(&[-3, -1, 1, 2, 3])),
            ("xs.sortBy(x, -x)", ints(&[3, 2, 1, -1, -3])),
            ("xs.sortBy(x, x * x)", ints(&[-1, 1, 2, 3, -3])),
            ("xs.sortBy(x, x % 2 == 0)", ints(&[3, -1, -3, 1, 2])),
            ("xs.sortBy(x, 0)", ints(&[3, -1, 2, -3, 1])),
            ("xs.sortBy(x, string(x))", ints(&[-1, -3, 1, 2, 3])),
            ("xs.sortBy(x, x).reverse()", ints(&[3, 2, 1, -1, -3])),
            (
                "xs.sortBy(x, -x).sortBy(y, y * y)",
                ints(&[1, -1, 2, 3, -3]),
            ),
            ("xs.sort().sortBy(x, -x)", ints(&[3, 2, 1, -1, -3])),
            (
                "xs.sortBy(x, xs.sortBy(y, -y).sortBy(z, z * z).size() - x)",
                ints(&[3, 2, 1, -1, -3]),
            ),
            (
                "xs.sortBy(x, [x, -x].sortBy(y, y)[0])",
                ints(&[3, -3, 2, -1, 1]),
            ),
            ("lists.range(5).sortBy(x, -x)", ints(&[4, 3, 2, 1, 0])),
            ("xs.slice(1, 4).sortBy(x, x)", ints(&[-3, -1, 2])),
            ("xs.sortBy(x, x).first().orValue(0)", I(-3)),
            ("size(xs.sortBy(x, x))", I(5)),
        ] {
            gives(cases, source, &xs, want);
        }
        gives(cases, "size([].sortBy(x, x))", &[], I(0));
        gives(cases, "[7].sortBy(x, -x)", &[], ints(&[7]));
        gives(cases, "[1, 2].sortBy(x, [x].size())", &[], ints(&[1, 2]));
        let none = [("e", list(vec![]))];
        gives(cases, "e.sortBy(x, x)", &none, list(vec![]));
        gives(cases, "e.sortBy(x, -x)", &none, list(vec![]));
        gives(cases, "e.sortBy(x, x).size()", &none, I(0));
    });
}

/// Only here: keys of each ordered kind, equal keys keeping their order, a double's negative
/// zero, and elements of another kind than their key.
#[test]
fn sort_by_orders_each_key_kind() {
    check(|cases| {
        let w = [("w", list(vec![s("bb"), s("a"), s("ccc"), s("dd"), s("e")]))];
        gives(
            cases,
            "w.sortBy(x, x.size())",
            &w,
            list(vec![s("a"), s("e"), s("bb"), s("dd"), s("ccc")]),
        );
        gives(
            cases,
            "w.sortBy(x, -x.size())",
            &w,
            list(vec![s("ccc"), s("bb"), s("dd"), s("a"), s("e")]),
        );
        gives(
            cases,
            "w.sortBy(x, x)",
            &w,
            list(vec![s("a"), s("bb"), s("ccc"), s("dd"), s("e")]),
        );
        gives(
            cases,
            "[3, 1, 2].sortBy(x, string(x))",
            &[],
            ints(&[1, 2, 3]),
        );
        gives(
            cases,
            "['b', 'a'].sortBy(x, x == 'a')",
            &[],
            list(vec![s("b"), s("a")]),
        );
        gives(
            cases,
            "[1.5, -2.5, 0.5].sortBy(x, x)",
            &[],
            list(vec![D(-2.5), D(0.5), D(1.5)]),
        );
        gives(
            cases,
            "[3u, 1u, 2u].sortBy(x, x)",
            &[],
            list(vec![
                cel_core::Value::UInt(1),
                cel_core::Value::UInt(2),
                cel_core::Value::UInt(3),
            ]),
        );
        gives(
            cases,
            "[3, 1, 2].sortBy(x, duration(string(x) + 's'))",
            &[],
            ints(&[1, 2, 3]),
        );
        gives(
            cases,
            "[3, 1, 2].sortBy(x, timestamp('2026-01-01T00:00:00Z') + duration(string(-x) + 's'))",
            &[],
            ints(&[3, 2, 1]),
        );
        gives(
            cases,
            "[3, 1, 2].sortBy(x, b'a' + bytes(string(x)))",
            &[],
            ints(&[1, 2, 3]),
        );
        let z = [("z", list(vec![D(0.0), D(-0.0), D(-3.0)]))];
        gives(
            cases,
            "z.sortBy(x, x)",
            &z,
            list(vec![D(-3.0), D(0.0), D(-0.0)]),
        );
        gives(
            cases,
            "z.sortBy(x, -x)",
            &z,
            list(vec![D(0.0), D(-0.0), D(-3.0)]),
        );
        let nan = [("n", list(vec![D(1.0), D(f64::NAN), D(0.5)]))];
        fail(
            cases,
            &["n.sortBy(x, x)", "n.sortBy(x, 1.0 / (x - x))"],
            &nan,
        );
        gives(cases, "n.sortBy(x, 0.0 - 1.0).size()", &nan, I(3));
        fail(cases, &["[0.0, 1.0].sortBy(x, x / 0.0 - x / 0.0)"], &[]);
        gives(cases, "[1.0, 2.0].sortBy(x, -x / 0.0).size()", &[], I(2));
    });
}

/// Only here: a key that is a decoded value, a dyn, orders among its own kind; keys of mixed
/// kinds are an error. A list of dyn elements takes `sortBy` too.
#[test]
fn sort_by_orders_dyn_keys() {
    check(|cases| {
        let t = |items: &[&str]| [("t", list(items.iter().map(|i| s(i)).collect()))];
        let order = "t.sortBy(x, json.decode(x))";
        let first = |items: &[&str], want: &str| (format!("{order}[0]"), t(items), s(want));
        for (source, inputs, want) in [
            first(&["3", "-1", "2"], "-1"),
            first(&["\"b\"", "\"a\""], "\"a\""),
            first(&["true", "false"], "false"),
            first(&["2.5", "1.5"], "1.5"),
        ] {
            gives(cases, &source, &inputs, want);
        }
        let texts = Type::list(Type::String);
        let decoded = "v.sortBy(x, json.decode(x)).size()";
        cases.stored(decoded, texts, list(vec![]), Got::Value(I(0)));
        gives(cases, &format!("{order}.size()"), &t(&["7"]), I(1));
        gives(cases, "t.sortBy(x, 1)[0]", &t(&["1", "[1]"]), s("1"));
        for items in [
            &["1", "\"a\""][..],
            &["1", "2.5"],
            &["1", "null"],
            &["[1]"],
            &["{}"],
            &["null", "null"],
        ] {
            fail(cases, &[&format!("{order}.size()")], &t(items));
        }
        gives(cases, "[1, 'a'].sortBy(x, 0).size()", &[], I(2));
    });
}

fn field(name: &str, ty: Type) -> Field {
    Field {
        name: name.into(),
        ty,
        required: false,
    }
}

/// Registers the message `M` the stored cases build.
fn stored(cases: &mut Cases) {
    let fields = vec![
        field("xs", Type::list(Type::Narrow(Width::I32))),
        field("fs", Type::list(Type::Narrow(Width::F32))),
    ];
    cases.message("M", Type::Message(fields));
}

/// Only here: a list stored at a narrow width, as elements and as a key.
#[test]
fn sort_by_takes_stored_lists() {
    check(|cases| {
        stored(cases);
        let m = "M{xs: [3, -1, 2, 3], fs: [2.5, 0.5]}";
        gives(
            cases,
            &format!("{m}.xs.sortBy(x, -x)"),
            &[],
            ints(&[3, 3, 2, -1]),
        );
        gives(
            cases,
            &format!("{m}.xs.sortBy(x, x)"),
            &[],
            ints(&[-1, 2, 3, 3]),
        );
        gives(
            cases,
            &format!("{m}.fs.sortBy(x, x)"),
            &[],
            list(vec![D(0.5), D(2.5)]),
        );
        gives(
            cases,
            &format!("{m}.fs.sortBy(x, -x)"),
            &[],
            list(vec![D(2.5), D(0.5)]),
        );
    });
}

/// Only here: what lowering refuses stays refused, and a bad first argument is a parse error.
#[test]
fn sort_by_refuses_what_it_cannot_order() {
    let message = Type::Message(vec![field("a", Type::Int)]);
    let names = [
        ("rs", Type::list(message)),
        ("ls", Type::list(Type::list(Type::Int))),
        ("a", Type::Dyn),
        ("t", Type::String),
    ];
    for source in [
        "rs.sortBy(r, r)",
        "ls.sortBy(l, l)",
        "[1].sortBy(x, [x])",
        "[1].sortBy(x, {'a': x})",
        "[1].sortBy(x, null)",
        "[1].sortBy(x, optional.of(x))",
        "[1].sortBy(x + 1, x)",
        "[1].sortBy('x', x)",
        "[1].sortBy(x)",
        "[1].sortBy(x, x, x)",
        "[1].sortBy(x.y, x)",
        "1.sortBy(x, x)",
        "'a'.sortBy(x, x)",
        "a.sortBy(x, x)",
        "json.decode(t).sortBy(x, x)",
        "xs.sortBy(x, x)",
    ] {
        assert!(refusal(source, &names).is_some(), "{source}");
    }
    for source in [
        "rs.sortBy(r, r.a).size()",
        "ls.sortBy(l, l.size()).size()",
        "ls.sortBy(l, l.size()).map(l, l.size())",
    ] {
        assert_eq!(refusal(source, &names), None, "{source}");
    }
}
