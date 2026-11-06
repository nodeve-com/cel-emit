//! An expression given a result type returns it, holding its width as a message field does: a
//! value past the width is a Range status, element by element in a list or map.

use cel_core::Value::{Double as D, Int as I, UInt as U};
use cel_emit::{Type, Width};
use cel_emit_runtime::Error;

use crate::cases::{check, list, Got};

fn narrow(w: Width) -> Type {
    Type::Narrow(w)
}

fn value(v: cel_core::Value) -> Got {
    Got::Value(v)
}

fn range() -> Got {
    Got::Status(Error::Range)
}

/// Only here: a number result holds its width, and a float32 rounds to it.
#[test]
fn a_number_result_holds_its_width() {
    check(|cases| {
        let int8 = || narrow(Width::I8);
        let uint8 = || narrow(Width::U8);
        for (source, ty, want) in [
            ("127", int8(), value(I(127))),
            ("128", int8(), range()),
            ("-129", int8(), range()),
            ("x + 1", int8(), range()),
            ("255u", uint8(), value(U(255))),
            ("256u", uint8(), range()),
            ("0.1", narrow(Width::F32), value(D(f64::from(0.1f32)))),
            ("7", Type::Int, value(I(7))),
            ("7", Type::Double, value(D(7.0))),
        ] {
            cases.gives_as(source, ty, &[("x", I(127))], want);
        }
    });
}

/// Only here: a list or map result holds each element's width, as a message field does.
#[test]
fn a_collection_result_holds_each_elements_width() {
    check(|cases| {
        let bytes = Type::list(narrow(Width::U8));
        let by_name = Type::Map(Box::new(Type::String), Box::new(narrow(Width::I8)));
        cases.gives_as(
            "[1u, 255u]",
            bytes.clone(),
            &[],
            value(list(vec![U(1), U(255)])),
        );
        cases.gives_as("[1u, 256u]", bytes, &[], range());
        cases.gives_as(
            "{'a': 127}",
            by_name.clone(),
            &[],
            value(cel_core::Value::map([("a", I(127))])),
        );
        cases.gives_as("{'a': 1, 'b': 128}", by_name, &[], range());
    });
}

/// Only here: a dyn from `json.decode` fills a result as it fills a field, and a value past the
/// width is a Range status.
#[test]
fn a_dyn_result_fills_the_type_it_is_given() {
    check(|cases| {
        for (text, want) in [("12", value(I(12))), ("300", range())] {
            cases.gives_as(
                "json.decode(t)",
                narrow(Width::I8),
                &[("t", crate::cases::s(text))],
                want,
            );
        }
    });
}
