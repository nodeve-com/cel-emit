//! A variable typed optional: a selection or index on it is optional too, as `.?` and `[?]`
//! are, holding nothing where the variable holds nothing or the field or key is missing.

use cel_core::OptionalValue;
use cel_core::Value::{self, Bool as B, Int as I, Optional};
use cel_emit::{Field, Type};

use crate::cases::{check, map, Got, Stored};

fn field(name: &str, ty: Type) -> Field {
    Field {
        name: name.into(),
        ty,
        required: false,
    }
}

/// `optional(M)`: `M` an int `n`, a bool `b`, and a map `m` of string to int.
fn ty() -> Type {
    let m = Type::Map(Box::new(Type::String), Box::new(Type::Int));
    Type::Optional(Box::new(Type::Message(vec![
        field("n", Type::Int),
        field("b", Type::Bool),
        field("m", m),
    ])))
}

fn some(v: Value) -> Stored {
    Stored::new(ty(), Optional(OptionalValue::some(v)))
}

fn none() -> Stored {
    Stored::new(ty(), Optional(OptionalValue::none()))
}

/// Only here: on an optional variable, `.f` and `[k]` read as `.?f` and `[?k]` do, through any
/// chain after them; a missing key holds nothing, and `has()` is false where the variable is.
#[test]
fn an_optional_variable_selects_and_indexes_optionally() {
    let full = || map(&[("n", I(5)), ("b", B(true)), ("m", map(&[("k", I(3))]))]);
    check(|cases| {
        for (source, held, empty) in [
            ("v.?n.orValue(-1)", I(5), I(-1)),
            ("v.?b.orValue(false)", B(true), B(false)),
            ("v.n.orValue(-1)", I(5), I(-1)),
            ("v.n.hasValue()", B(true), B(false)),
            ("has(v.n)", B(true), B(false)),
            ("v.m.k.orValue(0)", I(3), I(0)),
            ("v.?m.k.orValue(0)", I(3), I(0)),
            ("v.m['k'].orValue(0)", I(3), I(0)),
            ("v.m[?'k'].orValue(0)", I(3), I(0)),
            ("v.?m[?'k'].orValue(0)", I(3), I(0)),
            ("v.m['z'].orValue(0)", I(0), I(0)),
            ("v.m.z.hasValue()", B(false), B(false)),
        ] {
            cases.stored(source, some(full()), Got::Value(held));
            cases.stored(source, none(), Got::Value(empty));
        }
    });
}
