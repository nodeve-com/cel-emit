//! A computed property may fill a place inside a struct the record also holds: its key is dotted,
//! and an expression reads it by that dotted name.

use cel_emit::computed::{Compute, Computed, Params, Property, Record};
use cel_emit::emit::Emitter;
use cel_emit::{Decls, Type};

use super::{compute, of};
use crate::common::bind::Bound;
use crate::common::{build_run, items, Positional};

fn property(key: &str, place: &str, empty: bool) -> Property {
    Property {
        key: key.into(),
        ty: Type::String,
        place: place.into(),
        empty,
        check: None,
    }
}

const RECORDS: &str = "
#[derive(Clone, Debug, Default)]
pub struct Place { pub id: String, pub key: Option<String> }
#[derive(Clone, Debug, Default)]
pub struct Data { pub label: Option<String>, pub tag: Option<String> }
#[derive(Clone, Debug, Default)]
pub struct Parcel { pub place: Place, pub data: Data }
pub struct Params;
fn main() {
    for entry in INDEX.iter() {
        println!(\"{} {:?} {}\", entry.key, entry.reads, entry.required);
    }
    for (n, label) in [Some(\"a\"), None].into_iter().enumerate() {
        let data = Data { label: label.map(String::from), tag: None };
        let mut record = Parcel { place: Place { id: \"p\".into(), key: None }, data };
        for entry in INDEX.iter() {
            println!(\"{n} {} {:?}\", entry.key, (entry.fill)(&mut record, &Params));
        }
        println!(\"{n} {:?}\", record);
    }
}
";

/// The lines the walk prints over `record`, its index built from two computed properties.
fn built(record: &Record) -> Vec<String> {
    let decls = Decls::default();
    let computed = Computed {
        record,
        decls: &decls,
        params: Params::Rust("Params"),
        vars: &Bound(Vec::new()),
    };
    let computes: Vec<Compute> = vec![
        compute("tag", "place.id + '/' + place.key"),
        compute("place.key", "label + '-k'"),
    ];
    walk(&computed, &computes, RECORDS, "nested")
}

/// The lines a program prints: the index of `computes` as `INDEX`, then `records`, the Rust
/// around it, built in a folder named after `name`.
pub(super) fn walk(
    computed: &Computed,
    computes: &[Compute],
    records: &str,
    name: &str,
) -> Vec<String> {
    let mut emitter = Emitter::new(&[], Box::new(Positional));
    let index = computed
        .index("INDEX", computes, &mut emitter)
        .unwrap_or_else(|e| panic!("{e}"));
    let items = items(&mut emitter).unwrap();
    let source =
        format!("#![allow(warnings)]\nuse ::cel_emit_runtime as s;\n{items}{index}{records}");
    let dir = std::env::temp_dir().join(format!("cel-emit-{name}-{}", std::process::id()));
    let lines = build_run(&dir, &source);
    let _ = std::fs::remove_dir_all(&dir);
    lines.unwrap_or_else(|e| panic!("rustc: {e}"))
}

/// Only here: a dotted key names a place inside another struct; readers of it, and of its
/// neighbour `place.id`, fill after it with no `after`, even listed first.
#[test]
fn a_computed_property_fills_a_place_inside_a_struct() {
    let record = Record {
        rust: "Parcel".into(),
        properties: vec![
            property("place.id", "place.id", false),
            property("place.key", "place.key", true),
            property("label", "data.label", true),
            property("tag", "data.tag", true),
        ],
    };
    let lines = built(&record);
    assert_eq!(
        &lines[..2],
        [
            r#"place.key ["label"] false"#,
            r#"tag ["place.id", "place.key"] false"#
        ]
    );
    assert_eq!(of(&lines, 0)[..2], ["place.key Filled", "tag Filled"]);
    assert!(of(&lines, 0)[2].contains(r#"key: Some("a-k")"#));
    assert!(of(&lines, 0)[2].contains(r#"tag: Some("p/a-k")"#));
    assert_eq!(of(&lines, 1)[..2], ["place.key Skipped", "tag Skipped"]);
}
