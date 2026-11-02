//! The lists extension over ranges, stored widths, and what lowering refuses.

use cel_core::Value::{Double as D, Int as I};
use cel_emit::{Field, Type, Width};
use cel_emit_runtime::Error;

use super::{gives, ints};
use crate::cases::{check, list, refusal, s, Got};

/// Only here: `lists.range` counts from zero, empty for no count, and a count past memory is a
/// Range status, not a trap.
#[test]
fn range_counts_and_refuses_what_memory_cannot_hold() {
    check(|cases| {
        gives(cases, "lists.range(4)", &[], ints(&[0, 1, 2, 3]));
        gives(cases, "lists.range(1)", &[], ints(&[0]));
        gives(cases, "lists.range(0)", &[], ints(&[]));
        gives(cases, "lists.range(-3).size()", &[], I(0));
        gives(
            cases,
            "lists.range(5).map(x, x * x)",
            &[],
            ints(&[0, 1, 4, 9, 16]),
        );
        gives(cases, "size(lists.range(1000))", &[], I(1000));
        gives(cases, "lists.range(n)", &[("n", I(i64::MIN))], ints(&[]));
        gives(
            cases,
            "lists.range(n).reverse().first().orValue(-1)",
            &[("n", I(3))],
            I(2),
        );
        gives(
            cases,
            "lists.range(n).slice(1, 3)",
            &[("n", I(4))],
            ints(&[1, 2]),
        );
        gives(
            cases,
            "lists.range(4).filter(x, x % 2 == 0)",
            &[],
            ints(&[0, 2]),
        );
        for n in [i64::MAX, i64::MAX - 1, 1 << 62] {
            cases.gives("lists.range(n)", &[("n", I(n))], Got::Status(Error::Range));
        }
    });
}

fn field(name: &str, ty: Type) -> Field {
    Field {
        name: name.into(),
        ty,
        required: false,
    }
}

/// Only here: a list stored at a narrow width, or a list of messages, takes the methods as the
/// wide list does, or is refused.
#[test]
fn stored_lists_take_the_methods() {
    check(|cases| {
        let narrow = Type::list(Type::Narrow(Width::I32));
        let fields = vec![
            field("xs", narrow.clone()),
            field("fs", Type::list(Type::Narrow(Width::F32))),
        ];
        cases.message("M", Type::Message(fields));
        let m = "M{xs: [3, -1, 2, 3], fs: [2.5, 0.5]}";
        gives(
            cases,
            &format!("[0, 1].map(i, {m}.xs[i])"),
            &[],
            ints(&[3, -1]),
        );
        for (source, want) in [
            ("sort()", ints(&[-1, 2, 3, 3])),
            ("distinct()", ints(&[3, -1, 2])),
            ("reverse()", ints(&[3, 2, -1, 3])),
            ("slice(1, 3)", ints(&[-1, 2])),
        ] {
            gives(cases, &format!("{m}.xs.{source}"), &[], want);
        }
        gives(cases, &format!("{m}.xs.first().orValue(0)"), &[], I(3));
        gives(
            cases,
            &format!("{m}.xs.sort().first().orValue(0)"),
            &[],
            I(-1),
        );
        gives(
            cases,
            &format!("{m}.fs.sort()"),
            &[],
            list(vec![D(0.5), D(2.5)]),
        );
        gives(cases, &format!("size({m}.xs.distinct())"), &[], I(3));
        gives(
            cases,
            &format!("{m}.xs.distinct().map(x, x + 3000000000)[0]"),
            &[],
            I(3000000003),
        );
        cases.stored(
            "v.sort()",
            narrow.clone(),
            ints(&[5, -5, 0]),
            Got::Value(ints(&[-5, 0, 5])),
        );
        cases.stored(
            "v.reverse()",
            narrow.clone(),
            ints(&[5, -5]),
            Got::Value(ints(&[-5, 5])),
        );
        cases.stored("v.first().orValue(9)", narrow, ints(&[]), Got::Value(I(9)));
        cases.stored(
            "v.sort()",
            Type::list(Type::String),
            list(vec![s("b"), s("a")]),
            Got::Value(list(vec![s("a"), s("b")])),
        );
    });
}

/// Only here: what lowering refuses stays refused, with its reason.
#[test]
fn what_the_methods_cannot_take_is_refused() {
    let message = Type::Message(vec![field("a", Type::Int)]);
    let names = [
        ("rs", Type::list(message)),
        ("xss", Type::list(Type::list(Type::list(Type::Int)))),
        ("ls", Type::list(Type::list(Type::Int))),
        ("a", Type::Dyn),
        ("b", Type::Dyn),
    ];
    for source in [
        "rs.sort()",
        "xss.sort()",
        "ls.sort()",
        "[[1], [2]].sort()",
        "[{'a': 1}].sort()",
        "xss.flatten(1)",
        "ls.flatten(1)",
        "a < b",
    ] {
        assert!(refusal(source, &names).is_some(), "{source}");
    }
    for source in [
        "ls.flatten()",
        "xss.flatten()",
        "ls.distinct()",
        "rs.reverse().size()",
    ] {
        assert_eq!(refusal(source, &names), None, "{source}");
    }
}
