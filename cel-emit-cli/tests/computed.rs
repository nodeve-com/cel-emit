//! Programs of computed properties, each built as its own package and run: the example's, and a
//! small one filling an `into` object.

use std::path::PathBuf;

use serde_json::{json, Value};

mod programs;

const ROWS: &str = include_str!("../../examples/answer_rows/computed/rows.cel");
const LOCATION: &str = include_str!("../../examples/answer_rows/computed/location.cel");
const RECORD: &str = include_str!("../../examples/answer_rows/computed/record.schema.json");
const ANSWER: &str = include_str!("../../examples/answer_rows/answer.json");
const EXPECT: &str = include_str!("../../examples/answer_rows/expect.json");

/// `expressions` over `schema`, filling `into`, as its own package `name`, built.
fn built(name: &str, expressions: &[(&str, &str)], schema: &str, into: Option<&str>) -> PathBuf {
    let program = cel_emit_cli::computed(expressions, schema, into, None).unwrap();
    let manifest = cel_emit_cli::manifest(name, &program);
    let files = [("main.rs", program.as_bytes())];
    programs::build(name, &manifest, &files, &["-Dwarnings"])
}

fn parsed(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

/// The example's record: its `input` the answer.
fn record(answer: &Value) -> String {
    json!({ "input": answer }).to_string()
}

#[test]
fn the_example_fills_rows_then_location() {
    // Listed in key order, `location` first: it reads `rows`, so it fills after.
    let expressions = [("location", LOCATION), ("rows", ROWS)];
    let exe = built("computed_rows", &expressions, RECORD, None);
    let (out, err, ok) = programs::run(&exe, &record(&parsed(ANSWER)));
    assert!(ok && err.is_empty(), "{err}");
    let filled = parsed(&out);
    assert_eq!(filled["rows"], parsed(EXPECT));
    assert_eq!(filled["location"]["count"], 3);
    assert_eq!(filled["location"]["to"]["locode"], "XBBBC");
}

#[test]
fn a_failed_required_property_prints_its_failure_and_exits_non_zero() {
    let expressions = [("location", LOCATION), ("rows", ROWS)];
    let exe = built("computed_rows", &expressions, RECORD, None);
    let mut answer = parsed(ANSWER);
    answer["from"]["id"] = json!("nowhere");
    let (out, err, ok) = programs::run(&exe, &record(&answer));
    assert!(!ok);
    assert!(err.starts_with("location.from: Expression("), "{err}");
    assert!(err.contains("required and empty: location"), "{err}");
    let filled = parsed(&out);
    assert_eq!(filled["rows"], parsed(EXPECT));
    assert_eq!(filled.get("location"), None);
}

const INTO: &str = r##"{
  "$defs": {
    "Label": {
      "type": "object",
      "required": ["text"],
      "properties": {"text": {"type": "string", "maxLength": 3}},
      "additionalProperties": false
    }
  },
  "type": "object",
  "required": ["qty", "price", "data"],
  "properties": {
    "qty": {"type": "integer"},
    "price": {"type": "integer"},
    "data": {
      "type": "object",
      "required": ["total"],
      "properties": {
        "label": {"$ref": "#/$defs/Label"},
        "total": {"type": "integer", "maximum": 1000}
      },
      "additionalProperties": false
    }
  },
  "additionalProperties": false
}"##;

/// What the `into` program prints for `qty` and `price`: the record, its failures, its success.
fn into(qty: i64, price: i64) -> (Value, String, bool) {
    let expressions = [
        ("label", "Label{text: string(total)}"),
        ("total", "qty * price"),
    ];
    let exe = built("computed_into", &expressions, INTO, Some("data"));
    let input = json!({"qty": qty, "price": price, "data": {}}).to_string();
    let (out, err, ok) = programs::run(&exe, &input);
    (parsed(&out), err, ok)
}

#[test]
fn into_names_the_object_the_properties_fill() {
    let (record, err, ok) = into(2, 3);
    assert!(ok && err.is_empty(), "{err}");
    assert_eq!(record["data"], json!({"label": {"text": "6"}, "total": 6}));
}

#[test]
fn a_check_refuses_a_value_naming_its_place() {
    // `label` is not required: the record goes on without it.
    let (record, err, ok) = into(10, 100);
    assert!(ok);
    assert_eq!(err, "label.text: Check(\"range\")\n");
    assert_eq!(record["data"], json!({"total": 1000}));
    // `total` is, and what reads it doesn't run.
    let (record, err, ok) = into(11, 100);
    assert!(!ok);
    assert!(err.starts_with("total: Check(\"range\")\n"), "{err}");
    assert!(err.contains("required and empty: total"), "{err}");
    assert_eq!(record["data"], json!({}));
}
