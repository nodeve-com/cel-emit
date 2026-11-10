//! A program of computed properties: each expression fills the property its key names, in a
//! record the schema types.

use cel_emit::computed::{Compute, Computed, Expression};
use cel_emit::emit::{Emitter, Vars};
use cel_emit::{Decls, Error};
use cel_emit_schema_bridge::{bridge, Asked, Bridged};

use crate::STDIN;

/// A program's expressions read nothing but the record.
struct NoVars;

impl Vars for NoVars {
    fn read(&self, _: usize) -> Result<String, Error> {
        Err(Error::Invalid("no such input".into()))
    }
}

/// What `main` does with the record: walks the index, prints each failure, prints the record,
/// and fails where a required property is left empty.
const WALK: &str = "
    let mut empty = Vec::new();
    for entry in INDEX.iter() {
        let outcome = (entry.fill)(&mut record, &());
        if let s::computed::Outcome::Failed(failure) = &outcome {
            eprintln!(\"{}: {:?}\", failure.place, failure.reason);
        }
        if entry.required && outcome != s::computed::Outcome::Filled {
            empty.push(entry.key);
        }
    }
    println!(\"{}\", serde_json::to_string(&record).map_err(|e| e.to_string())?);
    match empty.is_empty() {
        true => Ok(()),
        false => Err(format!(\"required and empty: {}\", empty.join(\", \"))),
    }";

/// Each of `expressions` as a computed property, required where the schema says so and its
/// property can be empty.
fn computes(expressions: &[(&str, &str)], bridged: &Bridged) -> Vec<Compute> {
    let empty = |key: &str| {
        let found = bridged.record.properties.iter().find(|p| p.key == key);
        found.is_some_and(|p| p.empty)
    };
    let each = expressions.iter().map(|(key, cel)| Compute {
        key: (*key).into(),
        expression: Expression::Cel((*cel).into()),
        after: Vec::new(),
        required: bridged.required.iter().any(|k| k == key) && empty(key),
    });
    each.collect()
}

/// The whole program: the schema's types and checks, the index, and a `main` reading the record
/// from stdin and walking it.
fn source(bridged: &Bridged, items: &str, index: &str) -> Result<String, String> {
    let (types, record, decode) = (&bridged.items, &bridged.record.rust, &bridged.decode);
    let loads: String = bridged
        .loads
        .iter()
        .map(|load| format!("{load}().map_err(|e| format!(\"{{e:?}}\"))?;"))
        .collect();
    crate::formatted(&format!(
        "use ::cel_emit_runtime as s;
        mod types {{ use ::schema_emit_runtime as __s; {types} }}
        {items}
        {index}
        pub fn decode(text: &str) -> Result<{record}, String> {{
            ::schema_emit_runtime::decode::json(text, |d| {decode}(d))
                .map(|(record, _)| record)
                .map_err(|e| format!(\"{{e:?}}\"))
        }}
        fn main() -> Result<(), String> {{
            {STDIN}
            let mut record = decode(&text)?;
            {loads}
            {WALK}
        }}",
    ))
}

/// The Rust program filling a record: each of `expressions` a property's key and the CEL filling
/// it, over `schema`, the record's JSON Schema. `into` names the one object property they fill,
/// where they don't fill the record's own.
pub fn computed(
    expressions: &[(&str, &str)],
    schema: &str,
    into: Option<&str>,
    functions: Option<&str>,
) -> Result<String, String> {
    let document: serde_json::Value = serde_json::from_str(schema).map_err(|e| e.to_string())?;
    let keys: Vec<&str> = expressions.iter().map(|(key, _)| *key).collect();
    let asked = Asked {
        schema: &document,
        name: "Record",
        into,
        computed: &keys,
        module: "types",
    };
    let bridged = bridge(&asked).map_err(|e| e.to_string())?;
    let mut decls = Decls::default();
    decls.messages.clone_from(&bridged.messages);
    let declared = crate::declared_over(decls, &crate::written(functions, Some(schema))?)?;
    let computes = computes(expressions, &bridged);
    let computed = Computed {
        record: &bridged.record,
        decls: &declared.decls,
        params: "()",
        vars: &NoVars,
    };
    let models = Box::new(bridged.models.clone());
    let mut emitter = Emitter::new(&declared.decls.functions, models);
    emitter.define(&declared.bodies);
    let index = computed.index("INDEX", &computes, &mut emitter);
    let index = index.map_err(|e| e.to_string())?;
    let items = emitter.items().map_err(|e| e.to_string())?;
    source(&bridged, &items.code, &index)
}
