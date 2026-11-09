//! A nullable message field: compared to null, read in a ternary beside null, and selected
//! through, which stops where it holds null.

use cel_core::Value::{Bool as B, Int as I};
use cel_emit::{Field, Type};

use crate::cases::{check, map, Got, Stored};

fn field(name: &str, ty: Type, required: bool) -> Field {
    Field {
        name: name.into(),
        ty,
        required,
    }
}

/// The type of `M`: a nullable message field `o` of an int `f`, always present.
fn outer() -> Type {
    let inner = Type::message(vec![field("f", Type::Int, true)]);
    Type::message(vec![field("o", Type::Nullable(Box::new(inner)), true)])
}

#[test]
fn a_null_message_field_compares_and_selects() {
    let value = |v| Got::Value(v);
    check(|cases| {
        let held = map(&[("o", map(&[("f", I(5))]))]);
        // A field the value leaves out holds its default: null.
        let null = map(&[]);
        cases.stored(
            "v.o != null",
            Stored::new(outer(), held.clone()),
            value(B(true)),
        );
        cases.stored(
            "v.o == null",
            Stored::new(outer(), held.clone()),
            value(B(false)),
        );
        cases.stored(
            "v.o != null",
            Stored::new(outer(), null.clone()),
            value(B(false)),
        );
        cases.stored("v.o.f", Stored::new(outer(), held.clone()), value(I(5)));
        cases.stored_absent("v.o.f", outer(), null.clone());
        let guarded = "dyn(v.o != null ? v.o.f : null) == 5";
        cases.stored(guarded, Stored::new(outer(), held), value(B(true)));
        let guarded = "dyn(v.o != null ? v.o.f : null) == null";
        cases.stored(guarded, Stored::new(outer(), null), value(B(true)));
    });
}
