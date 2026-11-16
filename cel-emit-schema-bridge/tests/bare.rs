//! A record filling a dynamic reference with a type that is no object: the property holds that
//! type bare, and the record reads and writes by functions of its own.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use cel_emit::{Type, Width};
use cel_emit_schema_bridge::Bridged;
use serde_json::{json, Value};

mod asked;
#[path = "filler/program.rs"]
mod program;

/// The generic model: a required string, and a required dynamic reference of any type.
fn generic() -> Value {
    json!({
        "$id": "reading",
        "type": "object",
        "required": ["id", "value"],
        "properties": {"id": {"type": "string"}, "value": {"$dynamicRef": "#value"}},
        "$defs": {"value": {"$dynamicAnchor": "value"}},
    })
}

/// A record filling the generic model's dynamic reference with `value`.
fn record(name: &str, value: Value) -> Value {
    let mut value = value;
    value["$dynamicAnchor"] = json!("value");
    json!({
        "$id": format!("https://example.test/{name}"),
        "$ref": "reading",
        "$defs": {"Reading": generic(), "value": value},
    })
}

fn ask<'a>(name: &'a str, module: &'a str) -> asked::Ask<'a> {
    asked::Ask {
        name,
        into: None,
        computed: &["value"],
        module,
        external: &[],
        later: true,
    }
}

fn bridged(name: &str, module: &str, value: Value) -> Result<Bridged, String> {
    asked::bridged(&record(name, value), &ask(name, module))
}

/// A kind: its schema, its type, and the Rust filling the type parameter.
type Kind = (Value, Type, &'static str);

/// Each kind of text that is no plain string.
fn texts() -> Vec<Kind> {
    let text = |key: &str, value: &str| json!({"type": "string", key: value});
    let stamp = "::chrono::DateTime<::chrono::Utc>";
    vec![
        (text("format", "date-time"), Type::Timestamp, stamp),
        (
            text("format", "human-duration"),
            Type::Duration,
            "::chrono::TimeDelta",
        ),
        (
            text("contentEncoding", "base64"),
            Type::Bytes,
            "::std::vec::Vec<u8>",
        ),
    ]
}

/// Each kind that is no object.
fn kinds() -> Vec<Kind> {
    let of = |kind: &str| json!({"type": kind});
    let list = json!({"type": "array", "items": {"type": "number"}});
    let map = json!({"type": "object", "additionalProperties": {"type": "integer"}});
    let narrow = json!({"type": "integer", "format": "int32"});
    let pairs = "::std::vec::Vec<(::std::string::String, i64)>";
    let mut kinds = vec![
        (of("boolean"), Type::Bool, "bool"),
        (of("string"), Type::String, "::std::string::String"),
        (json!({"type": "number", "minimum": 0}), Type::Double, "f64"),
        (of("integer"), Type::Int, "i64"),
        (narrow, Type::Narrow(Width::I32), "i32"),
        (list, Type::list(Type::Double), "::std::vec::Vec<f64>"),
        (map, Type::map(Type::Int), pairs),
    ];
    kinds.extend(texts());
    kinds
}

#[test]
fn each_kind_is_a_property_of_its_own_type_and_rust() {
    for (schema, ty, rust) in kinds() {
        let bridged = bridged("Level", "b", schema.clone()).unwrap();
        let value = &bridged.record.properties[1];
        assert_eq!((value.key.as_str(), &value.ty), ("value", &ty), "{schema}");
        assert_eq!((value.place.as_str(), value.empty), ("value", true));
        assert!(value.check.is_some(), "{schema}");
        assert_eq!(bridged.required, ["value"]);
        let alias = format!("pub type Level = Reading<{rust}>;");
        assert!(bridged.items.contains(&alias), "{alias}");
    }
}

#[test]
fn a_bare_record_reads_and_writes_by_its_own_functions() {
    let bridged = bridged("Level", "b", json!({"type": "boolean"})).unwrap();
    assert_eq!(bridged.record.rust, "b::Level");
    assert_eq!(bridged.decode, "b::decode_level");
    assert_eq!(bridged.encode.as_deref(), Some("b::encode_level"));
}

#[test]
fn a_bare_record_another_expression_reads_holds_the_type() {
    let schema = record("Level", json!({"type": "boolean"}));
    let read = asked::read(&schema, &ask("Level", "b")).unwrap();
    let fields: Vec<(&str, &Type)> = (read.fields().iter())
        .map(|f| (f.name.as_str(), &f.ty))
        .collect();
    let later = Type::Optional(Box::new(Type::Bool));
    assert_eq!(fields, [("id", &Type::String), ("value", &later)]);
}

#[test]
fn a_nullable_kind_is_a_nullable_property() {
    let schema = record("Level", json!({"type": ["string", "null"]}));
    let mut ask = ask("Level", "b");
    ask.computed = &[];
    let bridged = asked::bridged(&schema, &ask).unwrap();
    let value = &bridged.record.properties[1];
    assert_eq!(value.ty, Type::Nullable(Box::new(Type::String)));
    assert!(bridged
        .items
        .contains("pub type Level = Reading<::core::option::Option<::std::string::String>>;"));
}

/// What the program `name` prints: `fill` filling the `value` of `record`, a record of `kind`,
/// reading the record `x` of `beside`, decoded from `held`.
fn run(name: &str, (kind, fill): (Value, &str), (beside, held): (Value, &str)) -> Value {
    let own = bridged("Level", "b", kind).unwrap();
    let other = bridged("Source", "c", beside).unwrap();
    let program = program::Program {
        name,
        bridged: &own,
        held: &[],
        fill: ("value", fill),
        beside: program::Beside::record(&own, ("x", "c"), &other, held),
        encode: own.encode.as_deref().unwrap(),
    };
    program::run(program, r#"{"id":"a"}"#).unwrap()
}

#[test]
fn one_expression_fills_a_bare_property_from_another_records() {
    let out = run(
        "bare_bool",
        (json!({"type": "boolean"}), "x.value == 'on'"),
        (json!({"type": "string"}), r#"{"id":"s","value":"on"}"#),
    );
    assert_eq!(out, json!({"id": "a", "value": true}));
}

#[test]
fn a_bare_list_fills_and_writes_as_its_array() {
    let list = json!({"type": "array", "items": {"type": "number"}});
    let map = json!({"type": "object", "additionalProperties": {"type": "number"}});
    let out = run(
        "bare_list",
        (list, "[x.value['low'], x.value['high']]"),
        (map, r#"{"id":"s","value":{"low":1.5,"high":4}}"#),
    );
    assert_eq!(out, json!({"id": "a", "value": [1.5, 4.0]}));
}
