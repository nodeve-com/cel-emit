//! A program built from a bridged filler and run: the module holding its generic model, the
//! bridge's items, the index filling `title`, and a `main` reading a record from stdin.

use cel_emit::computed::{Compute, Computed, Expression, Params};
use cel_emit::emit::{Emitter, Vars};
use cel_emit::{Decls, Error};
use cel_emit_schema_bridge::Bridged;
use schema_emit::emit::std_form::source;
use schema_emit::json_schema::lower_defs;
use serde_json::{Map, Value};

#[path = "../../../cel-emit-cli/tests/programs/mod.rs"]
mod programs;

const PRELUDE: &str =
    "#![allow(unused_imports, dead_code, clippy::all)] use ::schema_emit_runtime as __s;";

/// The expression filling `title`: a property outside the slot, and one inside.
const TITLE: &str = "id + ':' + string(pieces)";

/// An expression reads nothing but the record.
struct NoVars;

impl Vars for NoVars {
    fn read(&self, _: usize) -> Result<String, Error> {
        Err(Error::Invalid("no such input".into()))
    }
}

/// A Cargo manifest for the program.
fn manifest(name: &str) -> String {
    let runtime = concat!(env!("CARGO_MANIFEST_DIR"), "/../cel-emit-runtime");
    format!(
        "[package]\nname = {name:?}\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[workspace]\n\n\
         [dependencies]\ncel-emit-runtime = {{ path = {runtime:?} }}\nserde_json = \"1\"\n\
         schema-emit-runtime = {{ git = \"https://github.com/nodeve-com/schema-emit\", \
         rev = \"c909bbc3a47cfccbfb9495f9956f2ea9ff2f5bab\", features = [\"decode\"] }}\n"
    )
}

/// The module `a`, written from each of `held`.
fn held_modules(held: &[&Value]) -> String {
    let mut out = String::new();
    for document in held {
        let (schema, _) = lower_defs(document, &Map::new(), &[]).unwrap();
        let items = source(&schema, &|_| None, &[]).unwrap();
        out.push_str(&format!("mod a {{ {PRELUDE} {items} }}\n"));
    }
    out
}

/// The functions and the index filling `title`, as the Rust the program holds.
fn index_of(bridged: &Bridged) -> String {
    let mut decls = Decls::default();
    decls.messages.clone_from(&bridged.messages);
    let compute = Compute {
        key: "title".into(),
        expression: Expression::Cel(TITLE.into()),
        after: Vec::new(),
        required: true,
    };
    let computed = Computed {
        record: &bridged.record,
        decls: &decls,
        params: Params::Rust("()"),
        vars: &NoVars,
    };
    let mut emitter = Emitter::new(&decls.functions, Box::new(bridged.models.clone()));
    let index = computed.index("INDEX", &[compute], &mut emitter).unwrap();
    format!("{}{index}", emitter.items().unwrap().code)
}

/// The program's source: `held` as module `a`, the bridge's items as `b`, and the index.
fn source_of(bridged: &Bridged, held: &[&Value]) -> String {
    let (record, decode) = (&bridged.record.rust, &bridged.decode);
    format!(
        "use ::cel_emit_runtime as s;
        {held}
        mod b {{ {PRELUDE} {types} }}
        {index}
        fn main() -> Result<(), String> {{
            let text = std::io::read_to_string(std::io::stdin()).map_err(|e| e.to_string())?;
            let mut record: {record} = ::schema_emit_runtime::decode::json(&text, |d| {decode}(d))
                .map(|(record, _)| record)
                .map_err(|e| format!(\"{{e:?}}\"))?;
            for entry in INDEX.iter() {{
                if (entry.fill)(&mut record, &()) != s::computed::Outcome::Filled {{
                    return Err(format!(\"not filled: {{}}\", entry.key));
                }}
            }}
            println!(\"{{}}\", serde_json::to_string(&record).map_err(|e| e.to_string())?);
            Ok(())
        }}",
        held = held_modules(held),
        types = bridged.items,
        index = index_of(bridged),
    )
}

/// What the program named `name` prints for a record holding `id` and `pieces`.
pub fn run(name: &str, bridged: &Bridged, held: &[&Value]) -> Value {
    let source = source_of(bridged, held);
    let files = [("main.rs", source.as_bytes())];
    let exe = programs::build(name, &manifest(name), &files, &[]);
    let (out, err, ok) = programs::run(&exe, r#"{"id":"a","contents":{"pieces":2}}"#);
    assert!(ok, "{err}");
    serde_json::from_str(&out).unwrap()
}
