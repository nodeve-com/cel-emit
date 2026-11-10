#![doc = include_str!("../README.md")]

use cel_emit::emit::{Emitter, Messages, Vars};
use cel_emit::functions::Function;
use cel_emit::{lower, Body, Decl, Decls, Error, Message, Name, Type, Written};
use cel_emit_schema_bridge::{types, Models};
use schema_emit::emit::std_form::{names, StdForm};
use schema_emit::ir::Schema;

mod computed;
mod functions;

pub use computed::computed;

/// No message reaches here: every value read is dynamic.
struct NoMessages;

impl Messages for NoMessages {
    fn path(&mut self, i: usize, _: &Message) -> String {
        format!("Message{i}")
    }

    fn fields(&self, message: &Message) -> Vec<String> {
        message.fields.iter().map(|f| f.name.clone()).collect()
    }
}

/// The one input, a parameter.
struct Input(&'static str);

impl Vars for Input {
    fn read(&self, _: usize) -> Result<String, Error> {
        Ok(self.0.into())
    }
}

fn function(name: &str, param: &[Type], result: Type) -> Function {
    Function {
        decl: Decl::new(name).with_overload(name, param.to_vec(), result),
        paths: vec![(name.into(), format!("s::time::civil::{name}"))],
    }
}

/// What a program's expression checks against, and the functions written in CEL it calls,
/// checked once.
pub struct Declared {
    pub decls: Decls,
    pub bodies: Vec<Body>,
}

/// What every program declares: the functions, those `written` in CEL, and the one input `name`,
/// of type `ty`.
fn declarations(name: &str, ty: Type, written: &[Written]) -> Result<Declared, String> {
    let mut decls = Decls::default();
    decls.names.insert(name.into(), Name::var(ty, 0));
    declared_over(decls, written)
}

/// `decls` with the functions every program may call, and those `written` in CEL.
fn declared_over(mut decls: Decls, written: &[Written]) -> Result<Declared, String> {
    decls.functions = vec![
        function("to_utc", &[Type::String, Type::String], Type::Timestamp),
        function("iso_duration", &[Type::String], Type::Duration),
    ];
    let bodies = cel_emit::written::check(written, &decls).map_err(|e| e.to_string())?;
    decls.define(&bodies);
    Ok(Declared { decls, bodies })
}

/// `source`, formatted.
fn formatted(source: &str) -> Result<String, String> {
    let file = syn::parse_file(source).map_err(|e| e.to_string())?;
    Ok(prettyplease::unparse(&file))
}

/// The Rust program computing `expression`, a CEL expression reading `input`: dynamic JSON, or
/// with `schema`, a JSON Schema document, the types it describes. `functions` is a JSON file of
/// functions the expression may call, written in CEL.
pub fn program(
    expression: &str,
    schema: Option<&str>,
    functions: Option<&str>,
) -> Result<String, String> {
    match schema {
        Some(schema) => typed(expression, schema, functions),
        None => dynamic(expression, functions),
    }
}

/// `source`, CEL reading the one input `input`, as Rust: its expression and the items it uses.
fn emitted(
    source: &str,
    declared: &Declared,
    input: &'static str,
    messages: Box<dyn Messages>,
) -> Result<(String, String), String> {
    let tree = lower(source, &declared.decls).map_err(|e| e.to_string())?;
    let mut emitter = Emitter::new(&declared.decls.functions, messages);
    emitter.define(&declared.bodies);
    let output = emitter
        .expr(&tree, &Input(input))
        .map_err(|e| e.to_string())?;
    let items = emitter.items().map_err(|e| e.to_string())?;
    Ok((output, items.code))
}

/// How a program reads its text: `decode`'s body, from `text` to a `Result` of the input, and
/// the input's type.
struct Entry<'a> {
    decode: &'a str,
    input: &'a str,
}

/// A program's first statement: its input, all of stdin, as `text`.
const STDIN: &str =
    "let text = std::io::read_to_string(std::io::stdin()).map_err(|e| e.to_string())?;";

/// The whole program: `prelude`, `items`, `decode` from input text to the input, the CEL as
/// `expression` from the input to output text, `output` calling the two, and a `main` reading
/// stdin.
fn source(prelude: &str, items: &str, expression: &str, entry: Entry) -> Result<String, String> {
    let Entry { decode, input } = entry;
    formatted(&format!(
        "use ::cel_emit_runtime as s;
        {prelude}
        {items}
        pub fn decode(text: &str) -> Result<{input}, String> {{
            {decode}
        }}
        pub fn expression(input: &{input}) -> s::Result<String> {{
            Ok({expression})
        }}
        pub fn output(text: &str) -> Result<String, String> {{
            expression(&decode(text)?).map_err(|e| format!(\"{{e:?}}\"))
        }}
        fn main() -> Result<(), String> {{
            {STDIN}
            println!(\"{{}}\", output(&text)?);
            Ok(())
        }}",
    ))
}

/// `expression` as a program computes it: its value as JSON text.
pub fn encoded(expression: &str) -> String {
    format!("json.encode({expression}\n)")
}

/// What a program's expression checks against: the functions, and `input`, dynamic JSON or with
/// `schema` the type it describes.
pub fn declared(schema: Option<&str>, functions: Option<&str>) -> Result<Declared, String> {
    let ty = match schema {
        Some(schema) => {
            let (ir, root) = lowered(schema)?;
            types::record(&ir, root).map_err(|e| e.to_string())?
        }
        None => Type::Dyn,
    };
    declarations("input", ty, &written(functions, schema)?)
}

/// The functions a `--functions` file holds, none without one.
fn written(functions: Option<&str>, schema: Option<&str>) -> Result<Vec<Written>, String> {
    functions.map_or_else(|| Ok(Vec::new()), |f| functions::written(f, schema))
}

fn dynamic(expression: &str, functions: Option<&str>) -> Result<String, String> {
    let declared = declared(None, functions)?;
    let messages = Box::new(NoMessages);
    let (output, items) = emitted(&encoded(expression), &declared, "input", messages)?;
    let entry = Entry {
        decode: "s::json::decode(text).map_err(|e| format!(\"{e:?}\"))",
        input: "s::Value",
    };
    source("", &items, &output, entry)
}

/// `schema`, a JSON Schema document, as schema-emit's models, and the root's index.
fn lowered(schema: &str) -> Result<(Schema, usize), String> {
    let document: serde_json::Value = serde_json::from_str(schema).map_err(|e| e.to_string())?;
    let shared = serde_json::Map::new();
    schema_emit::json_schema::lower("Input", &document, &shared, &[]).map_err(|e| e.to_string())
}

fn typed(expression: &str, schema: &str, functions: Option<&str>) -> Result<String, String> {
    let (ir, root) = lowered(schema)?;
    let ty = types::record(&ir, root).map_err(|e| e.to_string())?;
    let declared = declarations("input", ty, &written(functions, Some(schema))?)?;
    let named = names(&ir);
    let name = |i: usize| named.get(i).cloned().unwrap_or_default();
    let types = StdForm::new(&ir, &name)
        .items()
        .map_err(|e| e.to_string())?;
    let messages = Models::new(&ir, "types");
    let (output, items) = emitted(&encoded(expression), &declared, "input", Box::new(messages))?;
    let root = name(root);
    let prelude = format!("mod types {{ use ::schema_emit_runtime as __s; {types} }}");
    let entry = Entry {
        decode: "::schema_emit_runtime::decode::json(text, |d| types::decode_input(d))\n            .map(|(input, _)| input)\n            .map_err(|e| format!(\"{e:?}\"))",
        input: &format!("types::{root}"),
    };
    source(&prelude, &items, &output, entry)
}

/// What a program over a schema's types links beside the runtime: schema-emit-runtime at the rev
/// `schema-emit` is pinned to here, so its decode matches what wrote it.
const TYPED: &str = concat!(
    "serde_json = \"1\"\n",
    "schema-emit-runtime = { git = \"https://github.com/nodeve-com/schema-emit\", rev = \"",
    env!("SCHEMA_EMIT_REV"),
    "\", features = [\"decode\"] }\n",
);

/// A Cargo manifest for `program`, built against this repo's cel-emit-runtime, with the `civil`
/// feature only if it calls those functions, what a schema's types need only if it has them, and
/// `chrono` only if one of them holds a time.
pub fn manifest(name: &str, program: &str) -> String {
    let runtime = concat!(env!("CARGO_MANIFEST_DIR"), "/../cel-emit-runtime");
    let features = match program.contains("s::time::civil::") {
        true => ", features = [\"civil\"]",
        false => "",
    };
    let typed = match program.contains("schema_emit_runtime") {
        true => TYPED,
        false => "",
    };
    let time = match program.contains("::chrono::") {
        true => "chrono = { version = \"0.4\", default-features = false }\n",
        false => "",
    };
    format!(
        "[package]\nname = {name:?}\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[workspace]\n\n\
         [dependencies]\ncel-emit-runtime = {{ path = {runtime:?}{features} }}\n{typed}{time}"
    )
}
