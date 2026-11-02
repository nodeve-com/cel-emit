//! What lowering refuses of time: a zone that is no literal or no zone, a literal that does not
//! parse, and one past CEL's range.

use cel_emit::{Decls, Name, Type};

/// Lowers `source` with a timestamp `ts` and a string `topic` in scope.
fn lower(source: &str) -> Result<(), cel_emit::Error> {
    let names = [("ts", Type::Timestamp), ("topic", Type::String)]
        .into_iter()
        .enumerate()
        .map(|(i, (n, t))| (n.to_string(), Name::var(t, i)))
        .collect();
    cel_emit::lower(source, &Decls::new(names)).map(|_| ())
}

/// Only here: lowering refuses a zone that is no string literal or no zone, and a duration or
/// timestamp literal that does not parse.
#[test]
fn lowering_refuses_what_time_cant_be() {
    for (cel, why) in [
        ("ts.getHours(topic)", "zone other than a string literal"),
        ("ts.getHours('Middle/Earth')", "Middle/Earth"),
        (
            "ts > ts + duration('1 fortnight')",
            "duration('1 fortnight')",
        ),
        ("ts > timestamp('noon')", "timestamp('noon')"),
    ] {
        let e = lower(cel).unwrap_err();
        assert!(e.to_string().contains(why), "{cel}: {e}");
    }
}

/// Only here: a literal reads as the same text computed.
#[test]
fn a_literal_reads_as_the_same_text_computed() {
    use crate::cases::{check, s, Got};
    use cel_core::Value;
    check(|cases| {
        for (function, text) in [
            ("duration", "1h"),
            ("duration", "1500ms"),
            ("duration", "-1.5s"),
            ("duration", "+90m"),
            ("timestamp", "2016-12-31T23:59:60Z"),
            ("timestamp", "2026-01-01T00:00:00.5+02:00"),
        ] {
            let source = format!("{function}('{text}') == {function}(x)");
            cases.gives(&source, &[("x", s(text))], Got::Value(Value::Bool(true)));
        }
    });
}

/// Only here: lowering refuses a time literal past CEL's range and accepts the range's ends.
#[test]
fn lowering_refuses_a_time_literal_past_cel_range() {
    for (cel, holds) in [
        ("timestamp('0001-01-01T00:00:00Z')", true),
        ("timestamp('0000-12-31T23:59:59.999Z')", false),
        ("timestamp('9999-12-31T23:59:59.999Z')", true),
        ("duration('315537897598s 999ms')", true),
        ("duration('315537897599s')", false),
    ] {
        match lower(cel) {
            Ok(()) => assert!(holds, "{cel} lowered"),
            Err(e) => assert!(
                !holds && e.to_string().contains("outside CEL's range"),
                "{cel}: {e}"
            ),
        }
    }
}
