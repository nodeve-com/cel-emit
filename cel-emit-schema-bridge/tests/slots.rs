//! A record whose schema fills another model's slots: its properties, places, and Rust, with the
//! generic model written beside it or in a module of its own, and the programs built from both.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use cel_emit::Type;
use cel_emit_schema_bridge::{bridge, Asked, Bridged, Held};
use serde_json::{json, Value};

#[path = "slots/program.rs"]
mod program;

/// The generic model: a required string, and a required slot for an object.
fn generic() -> Value {
    json!({
        "$id": "parcel",
        "type": "object",
        "required": ["id", "contents"],
        "properties": {
            "id": {"type": "string"},
            "contents": {"$dynamicRef": "#contents"},
        },
        "$defs": {"contents": {"$dynamicAnchor": "contents", "type": "object"}},
    })
}

/// The document the generic model's module is written from.
fn held() -> Value {
    json!({"$id": "https://example.test/root", "$defs": {"Parcel": generic()}})
}

/// A record filling the generic model's slot, holding the generic model's schema too.
fn record() -> Value {
    json!({
        "$id": "https://example.test/book",
        "$ref": "parcel",
        "$defs": {
            "Parcel": generic(),
            "contents": {
                "$dynamicAnchor": "contents",
                "type": "object",
                "required": ["title", "pieces"],
                "properties": {"title": {"type": "string"}, "pieces": {"type": "integer"}},
            },
        },
    })
}

fn bridged(into: Option<&str>, held: &[Held]) -> Result<Bridged, String> {
    let schema = record();
    // `title` is the slot's: without `into` nothing is computed.
    let computed: &[&str] = if into.is_some() { &["title"] } else { &[] };
    let asked = Asked {
        schema: &schema,
        name: "Book",
        into,
        computed,
        module: "b",
        held,
    };
    bridge(&asked).map_err(|e| e.to_string())
}

/// Each property as `key place empty checked`.
fn properties(bridged: &Bridged) -> Vec<String> {
    let each = bridged
        .record
        .properties
        .iter()
        .map(|p| format!("{} {} {} {}", p.key, p.place, p.empty, p.check.is_some()));
    each.collect()
}

#[test]
fn into_names_a_slot_and_its_properties_are_the_filling_types() {
    let bridged = bridged(Some("contents"), &[]).unwrap();
    assert_eq!(bridged.record.rust, "b::Book");
    assert_eq!(bridged.decode, "b::decode_parcel");
    let want = [
        "id id false false",
        "title contents.title true true",
        "pieces contents.pieces false false",
    ];
    assert_eq!(properties(&bridged), want);
    assert_eq!(bridged.required, ["title"]);
    let types: Vec<&Type> = bridged.record.properties.iter().map(|p| &p.ty).collect();
    assert_eq!(types, [&Type::String, &Type::String, &Type::Int]);
    for part in [
        "pub type Book = Parcel<Contents>;",
        "pub struct Parcel<Contents_ = ParcelContents>",
        "pub title: ::core::option::Option<::std::string::String>",
        "pub pieces: i64",
    ] {
        assert!(bridged.items.contains(part), "{part}");
    }
}

#[test]
fn a_slot_not_named_is_a_property_holding_its_filling_type() {
    let bridged = bridged(None, &[]).unwrap();
    assert_eq!(
        properties(&bridged),
        ["id id false false", "contents contents false false"]
    );
    let fields = bridged.record.properties[1].ty.fields();
    let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["title", "pieces"]);
}

#[test]
fn a_computed_property_fills_in_the_slot() {
    let out = program::run(
        "slots_beside",
        &bridged(Some("contents"), &[]).unwrap(),
        &[],
    );
    assert_eq!(
        out,
        json!({"id": "a", "contents": {"title": "a:2", "pieces": 2}})
    );
}

#[test]
fn a_generic_model_another_module_holds_is_read_by_its_path() {
    let doc = held();
    let theirs = [Held {
        schema: &doc,
        module: "crate::a",
    }];
    let bridged = bridged(Some("contents"), &theirs).unwrap();
    assert_eq!(bridged.record.rust, "b::Book");
    assert_eq!(bridged.decode, "crate::a::decode_parcel");
    for absent in [
        "struct Parcel",
        "for Parcel",
        "decode_parcel",
        "struct ParcelContents",
    ] {
        assert!(!bridged.items.contains(absent), "{absent}");
    }
    assert!(bridged
        .items
        .contains("pub type Book = crate::a::Parcel<Contents>;"));
    let out = program::run("slots_apart", &bridged, &[&doc]);
    assert_eq!(
        out,
        json!({"id": "a", "contents": {"title": "a:2", "pieces": 2}})
    );
}

#[test]
fn what_is_no_slot_of_a_filler_is_refused() {
    let refused = bridged(Some("id"), &[]).err().unwrap();
    assert!(refused.contains("`into` names no property"), "{refused}");
}
