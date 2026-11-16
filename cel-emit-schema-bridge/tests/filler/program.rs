//! A program built from a bridged filler and run: the module holding its generic model, the
//! bridge's items, the index filling one property, and a `main` reading a record from stdin.

use cel_emit::computed::{Compute, Computed, Expression, Params};
use cel_emit::emit::{Emitter, Messages, Vars};
use cel_emit::{Decls, Error};
use cel_emit_schema_bridge::Bridged;
use schema_emit::emit::std_form::source;
use schema_emit::json_schema::lower_defs;
use serde_json::{Map, Value};

#[path = "../../../cel-emit-cli/tests/programs/mod.rs"]
mod programs;

pub const PRELUDE: &str =
    "#![allow(unused_imports, dead_code, clippy::all)] use ::schema_emit_runtime as __s;";

/// A program: the record it fills, and what its one expression reads beside it.
pub struct Program<'a> {
    pub name: &'a str,
    pub bridged: &'a Bridged,
    /// Each document written as module `a`.
    pub held: &'a [&'a Value],
    /// The computed property's key and the expression filling it.
    pub fill: (&'a str, &'a str),
    pub beside: Beside<'a>,
    /// The Rust path of the function writing the record to a serializer.
    pub encode: &'a str,
}

/// What an expression reads beside the record, and the Rust holding it.
pub struct Beside<'a> {
    pub decls: Decls,
    pub params: Params<'a>,
    pub vars: &'a dyn Vars,
    /// Each message's Rust, the record's included.
    pub messages: Box<dyn Messages>,
    /// Rust items the params need, beside the record's module.
    pub items: String,
    /// The Rust giving the params' value.
    pub value: String,
}

impl<'a> Beside<'a> {
    /// Nothing beside `bridged`'s record.
    pub fn nothing(bridged: &Bridged) -> Beside<'a> {
        let mut decls = Decls::default();
        decls.messages.clone_from(&bridged.messages);
        Beside {
            decls,
            params: Params::Rust("()"),
            vars: &NoVars,
            messages: Box::new(bridged.models.clone()),
            items: String::new(),
            value: "()".into(),
        }
    }

    /// `other`, a record written as module `module`, read as `name`: its value decoded from the
    /// JSON text `held`.
    #[allow(dead_code)]
    pub fn record(
        bridged: &Bridged,
        (name, module): (&'a str, &str),
        other: &'a Bridged,
        held: &str,
    ) -> Beside<'a> {
        let (items, decode) = (&other.items, &other.decode);
        Beside {
            params: Params::Record {
                name,
                record: &other.record,
            },
            items: format!("mod {module} {{ {PRELUDE} {items} }}"),
            value: format!(
                "::schema_emit_runtime::decode::json({held:?}, |d| {decode}(d))
                    .map(|(record, _)| record)
                    .map_err(|e| format!(\"{{e:?}}\"))?"
            ),
            ..Beside::nothing(bridged)
        }
    }
}

/// How a struct with a serde impl of its own writes.
#[allow(dead_code)]
pub const SERDE: &str = "::schema_emit_runtime::decode::Serialize::serialize";

/// An expression reads nothing but the record.
struct NoVars;

impl Vars for NoVars {
    fn read(&self, _: usize) -> Result<String, Error> {
        Err(Error::Invalid("no such input".into()))
    }
}

/// A Cargo manifest for the program: schema-emit-runtime at the version this repo's lock holds.
fn manifest(name: &str) -> String {
    let runtime = concat!(env!("CARGO_MANIFEST_DIR"), "/../cel-emit-runtime");
    format!(
        "[package]\nname = {name:?}\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[workspace]\n\n\
         [dependencies]\ncel-emit-runtime = {{ path = {runtime:?} }}\nserde_json = \"1\"\nany-value = \"*\"\n\
         schema-emit-runtime = {{ version = \"*\", features = [\"decode\"] }}\n"
    )
}

/// The module `a`, written from each of `held`.
fn held_modules(held: &[&Value]) -> String {
    let mut out = String::new();
    for document in held {
        let schema = lower_defs(document, &Map::new(), &[]).unwrap();
        let items = source(&schema, &|_| None, &[]).unwrap();
        out.push_str(&format!("mod a {{ {PRELUDE} {items} }}\n"));
    }
    out
}

/// The functions and the index filling the program's property, as the Rust it holds, or why
/// its expression is refused.
#[allow(dead_code)]
pub fn index_of(program: Program) -> Result<String, Error> {
    let (key, cel) = program.fill;
    let compute = Compute {
        key: key.into(),
        expression: Expression::Cel(cel.into()),
        after: Vec::new(),
        required: true,
    };
    let beside = program.beside;
    let computed = Computed {
        record: &program.bridged.record,
        decls: &beside.decls,
        params: beside.params,
        vars: beside.vars,
    };
    let mut emitter = Emitter::new(&beside.decls.functions, beside.messages);
    let index = computed.index("INDEX", &[compute], &mut emitter)?;
    Ok(format!("{}{index}", emitter.items()?.code))
}

/// The program's source: `held` as module `a`, the bridge's items as `b`, and the index.
fn source_of(program: Program) -> String {
    let bridged = program.bridged;
    let (record, decode, encode) = (&bridged.record.rust, &bridged.decode, program.encode);
    let held = held_modules(program.held);
    let (beside, params) = (program.beside.items.clone(), program.beside.value.clone());
    let index = index_of(program).unwrap();
    format!(
        "use ::cel_emit_runtime as s;
        {held}
        mod b {{ {PRELUDE} {types} }}
        {beside}
        {index}
        fn main() -> Result<(), String> {{
            let text = std::io::read_to_string(std::io::stdin()).map_err(|e| e.to_string())?;
            let mut record: {record} = ::schema_emit_runtime::decode::json(&text, |d| {decode}(d))
                .map(|(record, _)| record)
                .map_err(|e| format!(\"{{e:?}}\"))?;
            let params = {params};
            for entry in INDEX.iter() {{
                let outcome = (entry.fill)(&mut record, &params);
                if outcome != s::computed::Outcome::Filled {{
                    return Err(format!(\"not filled: {{}}: {{outcome:?}}\", entry.key));
                }}
            }}
            let mut out = Vec::new();
            {encode}(&record, &mut serde_json::Serializer::new(&mut out))
                .map_err(|e| e.to_string())?;
            println!(\"{{}}\", String::from_utf8(out).map_err(|e| e.to_string())?);
            Ok(())
        }}",
        types = bridged.items,
    )
}

/// What the program prints for the record `input`, or why it stopped.
pub fn run(program: Program, input: &str) -> Result<Value, String> {
    let name = program.name;
    let source = source_of(program);
    let files = [("main.rs", source.as_bytes())];
    let exe = programs::build(name, &manifest(name), &files, &[]);
    let (out, err, ok) = programs::run(&exe, input);
    match ok {
        true => Ok(serde_json::from_str(&out).unwrap()),
        false => Err(err),
    }
}
