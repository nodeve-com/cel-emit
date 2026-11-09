//! A number's stored width, wherever it sits: a write outside it is a Range status, an element
//! of a list or map included; a read of a stored collection widens each element it reads.

use cel_core::Value::{self, Bool as B, Double as D, Int as I, UInt as U};
use cel_emit::{Field, Type, Width};
use cel_emit_runtime::Error;

use crate::cases::{check, list, Cases, Got, Stored};

fn field(name: &str, ty: Type) -> Field {
    Field {
        name: name.into(),
        ty,
        required: false,
    }
}

fn narrow(w: Width) -> Type {
    Type::Narrow(w)
}

/// A map of int32 to uint8.
fn bytes_by_int() -> Type {
    Type::Map(Box::new(narrow(Width::I32)), Box::new(narrow(Width::U8)))
}

/// Message `M`: a pair of int32s, a float32, and a list of int32s, of float32s, and a map of int32
/// to uint8.
fn message(cases: &mut Cases) {
    let fields = vec![
        field("a", narrow(Width::I32)),
        field("b", narrow(Width::I32)),
        field("f", narrow(Width::F32)),
        field("xs", Type::list(narrow(Width::I32))),
        field("fs", Type::list(narrow(Width::F32))),
        field("m", bytes_by_int()),
    ];
    cases.message("M", Type::Message(fields));
}

fn value(v: Value) -> Got {
    Got::Value(v)
}

fn range() -> Got {
    Got::Status(Error::Range)
}

/// Only here: a write outside its field's width is a Range status, selected or not, and for
/// each element of a list or map; a float32 list and field round each value; a whole message
/// chosen by a ternary or bound by `cel.bind` holds each field at its width.
#[test]
fn a_write_outside_its_width_is_a_range_status() {
    let rounded = || value(D(f64::from(0.1f32)));
    check(|cases| {
        message(cases);
        for (source, want) in [
            ("M{a: 7, b: 2147483647}.a", value(I(7))),
            ("M{a: 7, b: 2147483648}.a", range()),
            ("M{a: 2147483648}.a", range()),
            (
                "M{xs: [1, -2147483648]}.xs",
                value(list(vec![I(1), I(-2147483648)])),
            ),
            ("M{xs: [1, 3000000000]}.xs", range()),
            ("M{a: 1, xs: [1, 3000000000]}.a", range()),
            (
                "M{xs: [1, 2]}.xs.map(x, x + 3000000000)[1]",
                value(I(3000000002)),
            ),
            ("M{fs: [0.1, 2.5]}.fs[0]", rounded()),
            ("M{fs: [0.1]}.fs[0] == 0.1", value(B(false))),
            ("(1 < 2 ? M{f: 0.1} : M{f: 0.2}).f", rounded()),
            ("cel.bind(m, M{a: 7, f: 0.1}, m.f)", rounded()),
            ("(1 < 2 ? M{a: 7} : M{a: 8}).a", value(I(7))),
            ("(1 > 2 ? M{a: 7} : M{a: 2147483648}).a", range()),
            ("(1 < 2 ? M{fs: [0.1]} : M{}).fs[0]", rounded()),
            ("(1 < 2 ? M{m: {1: 200u}} : M{}).m[1]", value(U(200))),
            ("M{m: {1: 200u}}.m[1]", value(U(200))),
            ("M{m: {1: 256u}}.m[1]", range()),
            ("M{m: {3000000000: 1u}}.m[1]", range()),
        ] {
            cases.gives(source, &[], want);
        }
    });
}

/// Only here: a stored value passed on at its width, through a ternary, a comparison, a list, an
/// optional, `orValue`, a field, or a message's `==`, gives CEL's answer.
#[test]
fn a_value_passed_on_at_its_width_gives_cels_answer() {
    let rounded = || value(D(f64::from(0.1f32)));
    check(|cases| {
        message(cases);
        for (source, want) in [
            ("1 < 2 ? M{f: 0.1}.f : M{f: 0.2}.f", rounded()),
            ("M{a: 1 < 2 ? M{a: 7}.a : M{a: 8}.a}.a", value(I(7))),
            ("M{f: 0.1}.f == M{f: 0.1}.f", value(B(true))),
            ("M{f: 0.1}.f == 0.1", value(B(false))),
            ("M{a: 1}.a < M{a: 2}.a", value(B(true))),
            ("M{a: 1}.a != M{a: 1}.a", value(B(false))),
            ("M{f: 0.2}.f >= M{f: 0.1}.f", value(B(true))),
            ("[M{a: 1}.a, M{a: 2}.a][1]", value(I(2))),
            (
                "M{xs: [M{a: 1}.a, M{b: 5}.b]}.xs",
                value(list(vec![I(1), I(5)])),
            ),
            ("optional.of(M{f: 0.1}.f).value()", rounded()),
            ("optional.of(M{a: 1}.a).orValue(M{a: 2}.a)", value(I(1))),
            ("M{a: 7} == M{a: 7}", value(B(true))),
            ("M{f: 0.1} == M{f: 0.2}", value(B(false))),
        ] {
            cases.gives(source, &[], want);
        }
    });
}

/// Only here: a stored map of int32 to uint8 widens each key and value it reads, and its
/// size, membership, and comprehensions see the widened keys.
#[test]
fn a_stored_map_widens_what_is_read_from_it() {
    check(|cases| {
        let stored = Value::map([(1i64, U(7)), (-3i64, U(255))]);
        for (source, want) in [
            ("v[1]", value(U(7))),
            ("v[-3] + 1u", value(U(256))),
            ("v[2]", Got::Status(Error::NoSuchKey)),
            ("size(v)", value(I(2))),
            ("-3 in v", value(B(true))),
            ("2 in v", value(B(false))),
            ("v.exists(k, k < 0)", value(B(true))),
            ("v.all(k, k > 1)", value(B(false))),
            ("size(v.map(k, k * 2147483648))", value(I(2))),
        ] {
            cases.stored(source, Stored::new(bytes_by_int(), stored.clone()), want);
        }
    });
}

/// Only here: a stored int32 list widens each element it reads, or the whole list where it
/// must, and an index past its end fails.
#[test]
fn a_stored_list_widens_each_element_it_reads_or_the_whole_where_it_must() {
    check(|cases| {
        let stored = list(vec![I(5), I(-7), I(2147483647)]);
        for (source, want) in [
            ("v[2] + 1", value(I(2147483648))),
            ("v[3]", Got::Status(Error::Function)),
            ("-7 in v", value(B(true))),
            ("size(v)", value(I(3))),
            ("v.map(x, x * 2)[2]", value(I(4294967294))),
            ("(v + [1])[3]", value(I(1))),
            ("v == [5, -7, 2147483647]", value(B(true))),
        ] {
            cases.stored(
                source,
                Stored::new(Type::list(narrow(Width::I32)), stored.clone()),
                want,
            );
        }
    });
}

/// Only here: a read filling a field of its own stored type holds what it held.
#[test]
fn a_read_of_a_fields_own_type_fills_it_whole() {
    check(|cases| {
        message(cases);
        let float32s = Type::list(narrow(Width::F32));
        let stored = list(vec![D(f64::from(0.1f32)), D(2.5)]);
        cases.stored(
            "M{fs: v}.fs[0] == v[0]",
            Stored::new(float32s.clone(), stored.clone()),
            value(B(true)),
        );
        cases.stored(
            "size(M{fs: v}.fs)",
            Stored::new(float32s, stored),
            value(I(2)),
        );
        cases.stored(
            "M{a: 1, xs: v}.xs[1]",
            Stored::new(Type::list(narrow(Width::I32)), list(vec![I(5), I(-7)])),
            value(I(-7)),
        );
    });
}

/// Only here: a stored float32 becomes text in its own shortest digits, where its double would
/// write `9.300000190734863`.
#[test]
fn a_stored_float32_writes_its_own_digits() {
    check(|cases| {
        message(cases);
        scalar_digits(cases);
        list_digits(cases);
    });
}

fn text(t: &str) -> Got {
    value(Value::String(t.to_string().into()))
}

fn scalar_digits(cases: &mut Cases) {
    let one = narrow(Width::F32);
    let held = D(f64::from(9.3f32));
    for (source, want) in [
        ("string(v)", "9.3"),
        ("json.encode(v)", "9.3"),
        ("'%s'.format([v])", "9.3"),
        ("'%s %s'.format([v, v])", "9.3 9.3"),
        ("'%.1f'.format([v])", "9.3"),
        ("string(M{f: v}.f)", "9.3"),
    ] {
        cases.stored(source, Stored::new(one.clone(), held.clone()), text(want));
    }
    cases.stored("string(v)", Stored::new(one.clone(), D(21.0)), text("21.0"));
    cases.stored(
        "string(v + 1.0)",
        Stored::new(one, held),
        text("10.300000190734863"),
    );
}

fn list_digits(cases: &mut Cases) {
    let many = Type::list(narrow(Width::F32));
    let stored = list(vec![D(f64::from(9.3f32)), D(0.5)]);
    for (source, want) in [
        ("'%s'.format(v)", "9.3"),
        ("json.encode(v)", "[9.3,0.5]"),
        ("'%s'.format([v])", "[9.3, 0.5]"),
        ("v.map(x, string(x))[0]", "9.3"),
    ] {
        cases.stored(
            source,
            Stored::new(many.clone(), stored.clone()),
            text(want),
        );
    }
}
