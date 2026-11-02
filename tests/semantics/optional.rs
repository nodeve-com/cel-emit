//! A message field typed optional: a literal sets it, leaves it out, or sets it with a `?` entry,
//! and select and `has()` read back what the literal did.

use cel_core::Value::{Bool as B, Double as D, Int as I};
use cel_emit::{Field, Type, Width};
use cel_emit_runtime::Error;

use crate::cases::{check, Cases, Got};

fn field(name: &str, ty: Type) -> Field {
    Field {
        name: name.into(),
        ty,
        required: false,
    }
}

/// Message `O`: an int, and optional fields of an int, an int32, and a float32.
fn message(cases: &mut Cases) {
    let optional = |ty| Type::Optional(Box::new(ty));
    let fields = vec![
        field("n", Type::Int),
        field("o", optional(Type::Int)),
        field("q", optional(Type::Narrow(Width::I32))),
        field("w", optional(Type::Narrow(Width::F32))),
    ];
    cases.message("O", Type::Message(fields));
}

/// Only here: a value fills an optional field as set, even a zero, and a field the literal
/// leaves out or sets with an empty `?` entry reads absent and is not present, as a stored one.
#[test]
fn a_literal_sets_or_leaves_out_an_optional_field() {
    check(|cases| {
        message(cases);
        for (source, want) in [
            ("O{o: 5}.o", Got::Value(I(5))),
            ("has(O{o: 5}.o)", Got::Value(B(true))),
            ("has(O{o: 0}.o)", Got::Value(B(true))),
            ("has(O{n: 1}.o)", Got::Value(B(false))),
            ("O{?o: optional.of(5)}.o", Got::Value(I(5))),
            ("has(O{?o: optional.of(0)}.o)", Got::Value(B(true))),
            ("has(O{?o: optional.none()}.o)", Got::Value(B(false))),
            ("(1 < 2 ? O{o: 5} : O{}).o", Got::Value(I(5))),
            ("has((1 < 2 ? O{o: 5} : O{}).o)", Got::Value(B(true))),
            ("has((1 > 2 ? O{o: 5} : O{}).o)", Got::Value(B(false))),
            ("cel.bind(m, O{o: 5}, m.o + 1)", Got::Value(I(6))),
            ("cel.bind(m, O{o: 5}, has(m.o))", Got::Value(B(true))),
            ("cel.bind(m, O{n: 5}, has(m.o))", Got::Value(B(false))),
        ] {
            cases.gives(source, &[], want);
        }
        cases.absent("O{n: 1}.o", &[]);
        cases.absent("O{?o: optional.none()}.o", &[]);
    });
}

/// Only here: an optional field of a stored width holds the value at that width, and a value
/// past it fails `range` at the literal, a `?` entry's too; a float32 rounds, to infinity past its range.
#[test]
fn an_optional_field_of_a_stored_width_is_held_to_it() {
    let rounded = || Got::Value(D(f64::from(0.1f32)));
    let range = || Got::Status(Error::Range);
    check(|cases| {
        message(cases);
        for (source, want) in [
            ("O{q: 7}.q", Got::Value(I(7))),
            ("has(O{q: 7}.q)", Got::Value(B(true))),
            ("O{q: 2147483648}.q", range()),
            ("O{n: 1, q: 2147483648}.n", range()),
            ("O{?q: optional.of(2147483648)}.q", range()),
            ("O{?q: optional.of(7)}.q", Got::Value(I(7))),
            ("has(O{?q: optional.none()}.q)", Got::Value(B(false))),
            ("O{w: 0.1}.w", rounded()),
            ("O{?w: optional.of(0.1)}.w", rounded()),
            ("O{w: 1e300}.w", Got::Value(D(f64::INFINITY))),
            ("O{?w: optional.of(1e300)}.w", Got::Value(D(f64::INFINITY))),
            ("has(O{w: 0.1}.w)", Got::Value(B(true))),
            ("O{q: 5}.q + 3000000000", Got::Value(I(3000000005))),
        ] {
            cases.gives(source, &[], want);
        }
    });
}
