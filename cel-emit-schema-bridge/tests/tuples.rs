//! A tuple, an array typed by position: an opaque type wherever it sits, moved whole, and read
//! and written as the array it is.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use cel_emit::Type;
use cel_emit_schema_bridge::Bridged;
use serde_json::{json, Value};

mod asked;
#[path = "filler/program.rs"]
mod program;

/// Each place a tuple sits: four shapes as a property, one as a list's item and a map's value.
fn places() -> Value {
    let closed = json!({"type": "array", "items": false,
        "prefixItems": [{"type": "integer"}, {"type": "string"}]});
    json!({
        "closed": closed,
        "typed": {"type": "array", "items": {"type": "number"},
            "prefixItems": [{"type": "string"}, {"type": "boolean"}]},
        "free": {"type": "array", "prefixItems": [{"type": "boolean"}, {"type": "integer"}]},
        "same": {"type": "array", "items": false,
            "prefixItems": [{"type": "number"}, {"type": "number"}]},
        "listed": {"type": "array", "items": closed},
        "keyed": {"type": "object", "additionalProperties": closed},
    })
}

const PLACES: [&str; 6] = ["closed", "typed", "free", "same", "listed", "keyed"];

/// A record holding each place, a model `Moved` holding each again, and text.
fn schema() -> Value {
    let mut properties = places();
    properties["moved"] = json!({"$ref": "#/$defs/Moved"});
    properties["out"] = json!({"type": "string"});
    json!({
        "$id": "https://example.test/tuples",
        "type": "object",
        "required": PLACES,
        "properties": properties,
        "$defs": {"Moved": {"type": "object", "required": PLACES, "properties": places()}},
    })
}

fn bridged() -> Bridged {
    let ask = asked::Ask {
        name: "tuples",
        into: None,
        computed: &["moved", "out"],
        module: "b",
        external: &[],
        later: true,
    };
    asked::bridged(&schema(), &ask).unwrap()
}

const INPUT: &str = r#"{"closed":[1,"a"],"typed":["k",true,1.5,2.5],"free":[true,7,"x",{"n":1}],"same":[1.5,2.5],"listed":[[1,"a"],[2,"b"]],"keyed":{"p":[3,"c"]}}"#;

fn program<'a>(
    name: &'a str,
    bridged: &'a Bridged,
    fill: (&'a str, &'a str),
) -> program::Program<'a> {
    program::Program {
        name,
        bridged,
        held: &[],
        fill,
        beside: program::Beside::nothing(bridged),
        encode: program::SERDE,
    }
}

fn ty<'a>(bridged: &'a Bridged, key: &str) -> &'a Type {
    let found = bridged.record.properties.iter().find(|p| p.key == key);
    &found.unwrap().ty
}

#[test]
fn a_tuple_is_opaque_wherever_it_sits() {
    let bridged = bridged();
    let closed = ty(&bridged, "closed").clone();
    for key in ["closed", "typed", "free", "same"] {
        let ty = ty(&bridged, key);
        assert!(
            matches!(ty, Type::Opaque(rust) if rust.starts_with('(')),
            "{key}: {ty:?}"
        );
    }
    assert_eq!(ty(&bridged, "listed"), &Type::list(closed.clone()));
    assert_eq!(ty(&bridged, "keyed"), &Type::map(closed.clone()));
    let moved = &bridged.messages["Moved"];
    assert_eq!(moved.fields()[0].ty, closed);
    // One pair of functions for each distinct type, however many places hold it.
    let held: Vec<Type> = (bridged.tuples.iter())
        .map(|t| Type::Opaque(t.rust.clone()))
        .collect();
    assert_eq!(held.len(), 4, "{held:?}");
    assert!(held.contains(&closed));
    for tuple in &bridged.tuples {
        let (write, read) = (&tuple.writes[3..], &tuple.reads[3..]);
        assert!(
            bridged.items.contains(&format!("pub fn {write}(")),
            "{write}"
        );
        assert!(bridged.items.contains(&format!("pub fn {read}(")), "{read}");
    }
}

/// Each place moved into `Moved`'s, one of them through `json.encode` and `json.decode`: the
/// record writes each array as it read it.
#[test]
fn a_tuple_moves_into_another_models_field() {
    let bridged = bridged();
    let cel = "Moved{closed: closed, typed: typed, free: json.decode(json.encode(free)), \
               same: same, listed: listed, keyed: keyed}";
    let out = program::run(program("tuples_moved", &bridged, ("moved", cel)), INPUT).unwrap();
    let read: Value = serde_json::from_str(INPUT).unwrap();
    assert_eq!(out["moved"], read);
    for place in PLACES {
        assert_eq!(out[place], read[place], "{place}");
    }
}

/// `json.encode` of a model holding each place, and of each place alone.
#[test]
fn json_encode_writes_the_array_read() {
    let bridged = bridged();
    let moved = "json.encode(Moved{closed: closed, typed: typed, free: free, same: same, \
                 listed: listed, keyed: keyed})";
    let alone: Vec<String> = PLACES.iter().map(|p| format!("json.encode({p})")).collect();
    let cel = format!("{moved} + ' ' + {}", alone.join(" + ' ' + "));
    let out = program::run(program("tuples_encoded", &bridged, ("out", &cel)), INPUT).unwrap();
    let read: Value = serde_json::from_str(INPUT).unwrap();
    let texts: Vec<Value> = (out["out"].as_str().unwrap().split(' '))
        .map(|text| serde_json::from_str(text).unwrap())
        .collect();
    assert_eq!(texts[0], read);
    for (place, text) in PLACES.iter().zip(&texts[1..]) {
        assert_eq!(text, &read[*place], "{place}");
    }
}

#[test]
fn a_read_into_a_tuple_is_refused() {
    let bridged = bridged();
    for cel in [
        "string(closed[0])",
        "string(listed[0][1])",
        "string(size(same))",
    ] {
        let refused = program::index_of(program("tuples_refused", &bridged, ("out", cel)));
        let why = format!("{:?}", refused.unwrap_err());
        assert!(why.contains("opaque"), "{cel}: {why}");
    }
}
