//! A field its message must hold: `has()` and `.?` see it present, a message read through, a
//! scalar at its zero value too.

use cel_core::Value::{Bool as B, Int as I};
use cel_emit::{Field, Type};

use crate::cases::{check, map, Got, Stored};

/// The type of `M`: a required message field `o` of an int `f` its message may leave out.
fn outer() -> Type {
    let inner = Type::message(vec![Field {
        name: "f".into(),
        ty: Type::Int,
        required: false,
    }]);
    Type::message(vec![Field {
        name: "o".into(),
        ty: inner,
        required: true,
    }])
}

/// Only here: `has(M.o)` and `M.?o` are present for a required message field, and `.?` reads
/// on through it.
#[test]
fn a_required_message_field_is_present() {
    check(|cases| {
        let v = map(&[("o", map(&[("f", I(5))]))]);
        let sources = [
            ("has(v.o)", B(true)),
            ("v.?o.hasValue()", B(true)),
            ("v.?o.?f.orValue(7)", I(5)),
            ("has(v.o.f)", B(true)),
        ];
        for (source, want) in sources {
            cases.stored(source, Stored::new(outer(), v.clone()), Got::Value(want));
        }
    });
}

/// The type of `M`: a required bool `on` and a required int `n`.
fn scalars() -> Type {
    Type::message(vec![
        Field {
            name: "on".into(),
            ty: Type::Bool,
            required: true,
        },
        Field {
            name: "n".into(),
            ty: Type::Int,
            required: true,
        },
    ])
}

/// A required scalar at its zero value is present: `has()` is true and `.?` yields it.
#[test]
fn a_required_scalar_at_zero_is_present() {
    check(|cases| {
        let v = map(&[("on", B(false)), ("n", I(0))]);
        let sources = [
            ("has(v.on)", B(true)),
            ("v.?on.orValue(true)", B(false)),
            ("has(v.n)", B(true)),
            ("v.?n.orValue(7)", I(0)),
        ];
        for (source, want) in sources {
            cases.stored(source, Stored::new(scalars(), v.clone()), Got::Value(want));
        }
    });
}
