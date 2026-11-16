//! A program of computed properties: each expression fills the property its key names, in a
//! record the schema types.

use cel_emit::computed::{Compute, Computed, Expression, Params};
use cel_emit::emit::{Emitter, Vars};
use cel_emit::{Decls, Error};
use cel_emit_schema_bridge::{bridge, Asked, Bridged};
use schema_emit::emit::std_form::{names, StdForm};

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
    println!(\"{}\", encode(&record)?);
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
    // A struct writes by its serde impl, unless the bridge names a function of its own.
    let encode = bridged.encode.as_deref();
    let encode = encode.unwrap_or("::schema_emit_runtime::decode::Serialize::serialize");
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
        pub fn encode(record: &{record}) -> Result<String, String> {{
            let mut out = Vec::new();
            {encode}(record, &mut serde_json::Serializer::new(&mut out))
                .map_err(|e| e.to_string())?;
            String::from_utf8(out).map_err(|e| e.to_string())
        }}
        fn main() -> Result<(), String> {{
            {STDIN}
            let mut record = decode(&text)?;
            {loads}
            {WALK}
        }}",
    ))
}

/// `schema`, the record's JSON Schema, lowered as `Record` and bridged: each of `keys` a field
/// filled later, in `into` where it is set.
fn bridged(schema: &str, into: Option<&str>, keys: &[&str]) -> Result<Bridged, String> {
    let document: serde_json::Value = serde_json::from_str(schema).map_err(|e| e.to_string())?;
    let shared = serde_json::Map::new();
    let lowered = schema_emit::json_schema::lower("Record", &document, &shared, &[]);
    let (lowered, root) = lowered.map_err(|e| e.to_string())?;
    let named = names(&lowered);
    let name = |i: usize| named.get(i).cloned().unwrap_or_default();
    let paths: Vec<String> = match into {
        Some(into) => keys.iter().map(|key| format!("{into}.{key}")).collect(),
        None => keys.iter().map(|key| (*key).to_string()).collect(),
    };
    let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
    let std = StdForm::new(&lowered, &name).with_empty(root, &paths);
    let asked = Asked {
        std: &std.map_err(|e| e.to_string())?,
        root,
        into,
        computed: keys,
        module: "types",
    };
    bridge(&asked).map_err(|e| e.to_string())
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
    let keys: Vec<&str> = expressions.iter().map(|(key, _)| *key).collect();
    let bridged = bridged(schema, into, &keys).map_err(|e| e.to_string())?;
    let mut decls = Decls::default();
    decls.messages.clone_from(&bridged.messages);
    let declared = crate::declared_over(decls, &crate::written(functions, Some(schema))?)?;
    let computes = computes(expressions, &bridged);
    let computed = Computed {
        record: &bridged.record,
        decls: &declared.decls,
        params: Params::Rust("()"),
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
