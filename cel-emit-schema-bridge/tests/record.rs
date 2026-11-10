//! A schema bridged: each property's type, place, and check, the types an expression builds, and
//! what the bridge refuses.

use cel_emit::Type;
use cel_emit_schema_bridge::{bridge, Asked, Bridged};
use serde_json::{json, Value};

fn schema() -> Value {
    json!({
        "$defs": {
            "Label": {
                "type": "object",
                "required": ["text"],
                "properties": {"text": {"type": "string"}},
            }
        },
        "type": "object",
        "required": ["price", "data"],
        "properties": {
            "price": {"type": "integer"},
            "note": {"type": "string"},
            "data": {
                "type": "object",
                "required": ["total"],
                "properties": {
                    "label": {"$ref": "#/$defs/Label"},
                    "limit": {"type": "integer", "default": 7},
                    "maybe": {"type": ["integer", "null"]},
                    "total": {"type": "integer", "format": "int32"},
                },
            },
        },
    })
}

fn bridged(schema: &Value, into: Option<&str>, computed: &[&str]) -> Result<Bridged, String> {
    let asked = Asked {
        schema,
        name: "Record",
        into,
        computed,
        module: "types",
    };
    bridge(&asked).map_err(|e| e.to_string())
}

/// Each property as `key place empty checked`.
fn properties(bridged: &Bridged) -> Vec<String> {
    let each = bridged.record.properties.iter().map(|p| {
        let checked = p.check.is_some();
        format!("{} {} {} {checked}", p.key, p.place, p.empty)
    });
    each.collect()
}

#[test]
fn into_gives_its_properties_their_own_names_and_places() {
    let bridged = bridged(&schema(), Some("data"), &["label", "limit", "total"]).unwrap();
    assert_eq!(bridged.record.rust, "types::Record");
    assert_eq!(bridged.decode, "types::decode_record");
    let want = [
        "price price false false",
        "note note true false",
        "label data.label true true",
        "limit data.limit false true",
        "maybe data.maybe true false",
        "total data.total true true",
    ];
    assert_eq!(properties(&bridged), want);
}

#[test]
fn a_required_computed_property_may_be_empty_and_is_named() {
    let bridged = bridged(&schema(), Some("data"), &["total"]).unwrap();
    assert_eq!(bridged.required, ["total"]);
    assert!(bridged
        .items
        .contains("pub total: ::core::option::Option<i32>"));
    assert!(bridged.items.contains("pub price: i64"));
    let check = bridged.record.properties[5].check.as_deref();
    assert_eq!(
        check,
        Some("(|value| types::checked(value, types::check_0))")
    );
    assert!(bridged.items.contains("pub fn check_0<"));
}

#[test]
fn a_property_reads_as_its_value_and_a_named_type_builds() {
    let bridged = bridged(&schema(), Some("data"), &["label"]).unwrap();
    let label = bridged.messages.get("Label").cloned().unwrap();
    assert!(matches!(label, Type::Message(_)));
    let types: Vec<&Type> = bridged.record.properties.iter().map(|p| &p.ty).collect();
    assert_eq!(types[1], &Type::String);
    assert_eq!(types[2], &label);
}

#[test]
fn without_into_the_records_own_properties_fill() {
    let bridged = bridged(&schema(), None, &["price"]).unwrap();
    assert_eq!(bridged.required, ["price"]);
    let keys: Vec<&str> = bridged
        .record
        .properties
        .iter()
        .map(|p| p.key.as_str())
        .collect();
    assert_eq!(keys, ["price", "note", "data"]);
}

#[test]
fn what_the_bridge_refuses() {
    let refused = |into, computed: &[&str]| bridged(&schema(), into, computed).err().unwrap();
    assert!(refused(Some("data"), &["none"]).contains("the schema doesn't declare"));
    assert!(refused(Some("none"), &[]).contains("`into` names no property"));
    assert!(refused(Some("price"), &[]).contains("`into` names no required object"));
    assert!(refused(Some("data"), &["maybe"]).contains("a nullable computed property"));
}
