//! A schema bridged: each property's type, place, and check, the types an expression builds, and
//! what the bridge refuses.

use cel_emit::Type;
use cel_emit_schema_bridge::Bridged;
use serde_json::{json, Value};

mod asked;

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

/// `schema` bridged as `Record`, each computed property a field filled later where `later`.
fn asked(
    schema: &Value,
    into: Option<&str>,
    computed: &[&str],
    later: bool,
) -> Result<Bridged, String> {
    let ask = asked::Ask {
        name: "Record",
        into,
        computed,
        module: "types",
        external: &[],
        later,
    };
    asked::bridged(schema, &ask)
}

fn bridged(schema: &Value, into: Option<&str>, computed: &[&str]) -> Result<Bridged, String> {
    asked(schema, into, computed, true)
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

/// A property neither required nor defaulted reads as an optional; any other as its value.
#[test]
fn an_optional_property_reads_as_one_and_a_named_type_builds() {
    let bridged = bridged(&schema(), Some("data"), &["label"]).unwrap();
    let label = bridged.messages.get("Label").cloned().unwrap();
    assert!(matches!(label, Type::Message(_)));
    let optional = |ty: &Type| Type::Optional(Box::new(ty.clone()));
    let types: Vec<&Type> = bridged.record.properties.iter().map(|p| &p.ty).collect();
    assert_eq!(types[0], &Type::Int);
    assert_eq!(types[1], &optional(&Type::String));
    assert_eq!(types[2], &optional(&label));
    assert_eq!(types[3], &Type::Int);
}

/// A definition the record reaches that is no record is offered by its name too, apart from
/// the records.
#[test]
fn a_named_type_that_is_no_record_is_offered_by_its_name() {
    let mut schema = schema();
    let label = json!({"$ref": "#/$defs/Label"});
    schema["$defs"]["Labels"] = json!({"type": "array", "items": label});
    schema["$defs"]["Tags"] = json!({"type": "array", "items": {"type": "string"}});
    schema["$defs"]["Count"] = json!({"type": "integer", "format": "int32"});
    for name in ["Labels", "Tags", "Count"] {
        schema["properties"][name] = json!({"$ref": format!("#/$defs/{name}")});
    }
    let bridged = bridged(&schema, Some("data"), &["label"]).unwrap();
    let label = bridged.messages.get("Label").cloned().unwrap();
    assert_eq!(bridged.messages.len(), 1);
    let want = [
        ("Count", Type::Narrow(cel_emit::Width::I32)),
        ("Labels", Type::list(label)),
        ("Tags", Type::list(Type::String)),
    ];
    let got: Vec<(&str, Type)> = bridged
        .types
        .iter()
        .map(|(name, ty)| (name.as_str(), ty.clone()))
        .collect();
    assert_eq!(got, want);
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
    let refused = |into, computed: &[&str]| asked(&schema(), into, computed, false).err().unwrap();
    assert!(refused(Some("data"), &["none"]).contains("the schema doesn't declare"));
    assert!(refused(Some("none"), &[]).contains("`into` names no property"));
    assert!(refused(Some("price"), &[]).contains("`into` names no required object"));
    assert!(refused(Some("data"), &["maybe"]).contains("a nullable computed property"));
}

#[test]
fn a_record_another_module_holds_is_named_by_that_modules_path() {
    let theirs = [("Record".to_string(), "crate::theirs")];
    let ask = asked::Ask {
        name: "Record",
        into: None,
        computed: &[],
        module: "types",
        external: &theirs,
        later: true,
    };
    let bridged = asked::bridged(&schema(), &ask).unwrap();
    assert_eq!(bridged.record.rust, "crate::theirs::Record");
    assert_eq!(bridged.decode, "crate::theirs::decode_record");
    assert!(!bridged.items.contains("struct Record "), "its own struct");
    // A model the form leaves here stays in the bridge's module.
    let label = &bridged.messages["Label"];
    let cel_emit::Type::Message(label) = label else {
        panic!("not a message");
    };
    let path = cel_emit::emit::Messages::path(&mut bridged.models.clone(), 0, label);
    assert_eq!(path, "types::Label");
}
