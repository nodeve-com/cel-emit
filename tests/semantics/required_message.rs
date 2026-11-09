//! A message field its message must hold: `has()` and `.?` see it present, and read through it.

use cel_core::Value::{Bool as B, Int as I};
use cel_emit::{Field, Type};

use crate::cases::{check, map, Got, Stored};

/// The type of `M`: a required message field `o` of an int `f` its message may leave out.
fn outer() -> Type {
    let inner = Type::Message(vec![Field {
        name: "f".into(),
        ty: Type::Int,
        required: false,
    }]);
    Type::Message(vec![Field {
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
