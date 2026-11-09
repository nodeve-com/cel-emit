//! An optional message field read through a message value: `has()`, `.?`, and a plain select
//! see the field absent or present, and a plain select of an absent field stops.

use cel_core::Value::{Bool as B, Int as I};
use cel_emit::{Field, Type};

use crate::cases::{check, list, map, Cases, Got, Stored};

fn field(name: &str, ty: Type) -> Field {
    Field {
        name: name.into(),
        ty,
        required: false,
    }
}

/// The type of `M`: an int `n`, and an optional message field `o` of an int `f`.
fn outer() -> Type {
    let inner = Type::message(vec![field("f", Type::Int)]);
    Type::message(vec![
        field("n", Type::Int),
        field("o", Type::Optional(Box::new(inner))),
    ])
}

fn run(cases: &mut Cases, v: cel_core::Value, wants: [Option<Got>; 5]) {
    let sources = [
        "has(v.o)",
        "has(v.o.f)",
        "v.o.?f.hasValue()",
        "v.o.?f.orValue(7)",
        "v.o.f",
    ];
    for (source, want) in sources.into_iter().zip(wants) {
        match want {
            Some(want) => cases.stored(source, Stored::new(outer(), v.clone()), want),
            None => cases.stored_absent(source, outer(), v.clone()),
        }
    }
}

/// Only here: with `M.o` absent, `has(M.o)` and `has(M.o.f)` are false, `M.o.?f` is none, and
/// `M.o.f` stops; with it present they read the field.
#[test]
fn an_absent_optional_message_field_has_nothing_in_it() {
    let value = |v| Some(Got::Value(v));
    check(|cases| {
        let absent = map(&[("n", I(1))]);
        run(
            cases,
            absent,
            [
                value(B(false)),
                value(B(false)),
                value(B(false)),
                value(I(7)),
                None,
            ],
        );
        let present = map(&[("o", map(&[("f", I(5))]))]);
        run(
            cases,
            present,
            [
                value(B(true)),
                value(B(true)),
                value(B(true)),
                value(I(5)),
                value(I(5)),
            ],
        );
    });
}

/// Only here: an empty list literal falls back for an optional of a list of messages, taking
/// the list type the optional holds.
#[test]
fn an_empty_list_falls_back_for_an_optional_list_of_messages() {
    let sample = Type::message(vec![field("f", Type::Int)]);
    let ty = Type::message(vec![field("w", Type::list(sample))]);
    let items = list(vec![map(&[("f", I(5))])]);
    check(|cases| {
        let source = "v.?w.orValue([]).size()";
        let v = |value| Stored::new(ty.clone(), value);
        cases.stored(source, v(map(&[])), Got::Value(I(0)));
        cases.stored(source, v(map(&[("w", items)])), Got::Value(I(1)));
    });
}
