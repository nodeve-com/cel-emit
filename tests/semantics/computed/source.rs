//! The params may be a record: an expression reads each of its properties by the name the
//! caller gives it and the property's key, each read as the record's own would be.

use cel_emit::computed::{Compute, Computed, Params, Property, Record};
use cel_emit::emit::Emitter;
use cel_emit::{Decls, Error, Name, Type};

use super::nested::walk;
use super::{compute, label, of};
use crate::common::bind::Bound;
use crate::common::Positional;

fn property(key: &str, ty: Type, empty: bool) -> Property {
    Property {
        key: key.into(),
        ty,
        place: key.into(),
        empty,
        check: None,
    }
}

/// What the params hold: `reading` perhaps empty, `unit` always held, a message, and a place
/// inside a struct.
fn source() -> Record {
    Record {
        rust: "Source".into(),
        properties: vec![
            property("reading", Type::Int, true),
            property("unit", Type::String, false),
            property("label", label(), false),
            property("place.id", Type::String, false),
        ],
    }
}

/// The record filled: `reading`, a key the params share.
fn target(extra: &str) -> Record {
    let mut properties = vec![
        property("reading", Type::Int, true),
        property("text", Type::String, true),
    ];
    if !extra.is_empty() {
        properties.push(property(extra, Type::Int, true));
    }
    Record {
        rust: "Target".into(),
        properties,
    }
}

/// The caller's own param `scale`, beside the record's.
fn decls() -> Decls {
    let mut decls = Decls::default();
    decls.names.insert("scale".into(), Name::var(Type::Int, 0));
    decls
}

const RECORDS: &str = "
#[derive(Clone, Debug, Default)]
pub struct Target { pub reading: Option<i64>, pub text: Option<String> }
pub struct Spot { pub id: String }
pub struct Label { pub f0: String }
pub struct Source {
    pub reading: Option<i64>, pub unit: String, pub label: Label, pub place: Spot,
    pub scale: i64,
}
fn main() {
    for entry in INDEX.iter() {
        println!(\"{} {:?}\", entry.key, entry.reads);
    }
    for (n, reading) in [Some(3), None].into_iter().enumerate() {
        let label = Label { f0: \"a\".into() };
        let place = Spot { id: \"p\".into() };
        let source = Source { reading, unit: \"kg\".into(), label, place, scale: 10 };
        let mut record = Target::default();
        for entry in INDEX.iter() {
            println!(\"{n} {} {:?}\", entry.key, (entry.fill)(&mut record, &source));
        }
        println!(\"{n} {:?}\", record);
    }
}
";

fn computes() -> Vec<Compute> {
    vec![
        compute("reading", "source.reading * scale"),
        compute(
            "text",
            "source.unit + ':' + source.label.text + '@' + source.place.id",
        ),
    ]
}

/// `then` over `record`, its params the record `source()` named `source`, `scale` a field of it.
fn over<T>(record: &Record, decls: &Decls, then: impl FnOnce(&Computed) -> T) -> T {
    let params = source();
    let computed = Computed {
        record,
        decls,
        params: Params::Record {
            name: "source",
            record: &params,
        },
        vars: &Bound(vec!["p.scale".into()]),
    };
    then(&computed)
}

/// What translating over `record` refuses.
fn refusal(record: &Record, decls: &Decls) -> Error {
    let mut emitter = Emitter::new(&[], Box::new(Positional));
    over(record, decls, |c| {
        c.index("INDEX", &computes(), &mut emitter)
    })
    .unwrap_err()
}

/// Only here: a record fills from a params record, a key both share, a message read by
/// reference, a dotted key, and the caller's own param beside them; an empty one skips.
#[test]
fn a_record_fills_from_a_params_record() {
    let lines = over(&target(""), &decls(), |c| {
        walk(c, &computes(), RECORDS, "source")
    });
    assert_eq!(&lines[..2], ["reading []", "text []"]);
    assert_eq!(of(&lines, 0)[..2], ["reading Filled", "text Filled"]);
    assert_eq!(
        of(&lines, 0)[2],
        r#"Target { reading: Some(30), text: Some("kg:a@p") }"#
    );
    assert_eq!(of(&lines, 1)[..2], ["reading Skipped", "text Filled"]);
    assert!(of(&lines, 1)[2].contains("reading: None"));
}

/// Only here: a record's property, or a caller's name, that is the params' name or starts with
/// it and a dot fails translation.
#[test]
fn the_params_name_stands_apart() {
    let mut taken = decls();
    taken.names.insert("source".into(), Name::var(Type::Int, 1));
    let refused = [
        refusal(&target("source"), &decls()),
        refusal(&target("source.unit"), &decls()),
        refusal(&target(""), &taken),
    ];
    for error in refused {
        assert!(
            error.to_string().contains("a name two things share"),
            "{error}"
        );
    }
}
