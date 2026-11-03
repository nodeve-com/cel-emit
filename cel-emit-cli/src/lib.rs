#![doc = include_str!("../README.md")]

use cel_emit::emit::{Emitter, Messages, Vars};
use cel_emit::functions::Function;
use cel_emit::{lower, Decl, Decls, Error, Field, Name, Type};
use schema_emit::emit::std_form::{self, names, StdForm};
use schema_emit::ir::Schema;

pub mod schema_types;

/// No message reaches here: every value read is dynamic.
struct NoMessages;

impl Messages for NoMessages {
    fn path(&mut self, i: usize, _: &[Field]) -> String {
        format!("Message{i}")
    }

    fn fields(&self, fields: &[Field]) -> Vec<String> {
        fields.iter().map(|f| f.name.clone()).collect()
    }
}

/// Each message is the struct schema-emit writes for its model.
struct Models {
    records: Vec<(Vec<Field>, String)>,
}

impl Messages for Models {
    fn path(&mut self, _: usize, fields: &[Field]) -> String {
        let found = self.records.iter().find(|(held, _)| held == fields);
        found.map_or_else(String::new, |(_, name)| format!("types::{name}"))
    }

    fn fields(&self, fields: &[Field]) -> Vec<String> {
        std_form::fields(fields.iter().map(|f| f.name.as_str()))
    }
}

/// The one input, a parameter.
struct Input(&'static str);

impl Vars for Input {
    fn read(&self, _: usize) -> Result<String, Error> {
        Ok(self.0.into())
    }

    fn has(&self, _: usize) -> Result<String, Error> {
        Ok("true".into())
    }
}

fn function(name: &str, param: &[Type], result: Type) -> Function {
    Function {
        decl: Decl::new(name).with_overload(name, param.to_vec(), result),
        paths: vec![(name.into(), format!("s::time::civil::{name}"))],
    }
}

/// What every program declares: the functions, and the one input `name`, of type `ty`.
fn declarations(name: &str, ty: Type) -> Decls {
    let mut decls = Decls {
        functions: vec![
            function("to_utc", &[Type::String, Type::String], Type::Timestamp),
            function("iso_duration", &[Type::String], Type::Duration),
        ],
        ..Decls::default()
    };
    decls.names.insert(name.into(), Name::var(ty, 0));
    decls
}

/// `source`, formatted.
fn formatted(source: &str) -> Result<String, String> {
    let file = syn::parse_file(source).map_err(|e| e.to_string())?;
    Ok(prettyplease::unparse(&file))
}

/// The Rust program computing `expression`, a CEL expression reading `input`: dynamic JSON, or
/// with `schema`, a JSON Schema document, the types it describes.
pub fn program(expression: &str, schema: Option<&str>) -> Result<String, String> {
    match schema {
        Some(schema) => typed(expression, schema),
        None => dynamic(expression),
    }
}

/// `source`, CEL reading the one input `input`, as Rust: its expression and the items it uses.
fn emitted(
    source: &str,
    decls: &Decls,
    input: &'static str,
    messages: Box<dyn Messages>,
) -> Result<(String, String), String> {
    let tree = lower(source, decls).map_err(|e| e.to_string())?;
    let mut emitter = Emitter::new(&decls.functions, messages);
    let output = emitter
        .expr(&tree, &Input(input))
        .map_err(|e| e.to_string())?;
    let items = emitter.items().map_err(|e| e.to_string())?;
    Ok((output, items.code))
}

/// How a program's `output` reads its text: the statements binding `input` from `text`, and
/// `expression`'s parameter.
struct Entry<'a> {
    decode: &'a str,
    param: &'a str,
}

/// The whole program: `prelude`, `items`, the CEL as `expression`, `output` from input text to
/// output text, and a `main` reading stdin.
fn source(prelude: &str, items: &str, expression: &str, entry: Entry) -> Result<String, String> {
    let Entry { decode, param } = entry;
    formatted(&format!(
        "use ::cel_emit_runtime as s;
        {prelude}
        {items}
        fn expression({param}) -> s::Result<String> {{
            Ok({expression})
        }}
        pub fn output(text: &str) -> Result<String, String> {{
            {decode}
            expression(input).map_err(|e| format!(\"{{e:?}}\"))
        }}
        fn main() -> Result<(), String> {{
            let text = std::io::read_to_string(std::io::stdin()).map_err(|e| e.to_string())?;
            println!(\"{{}}\", output(&text)?);
            Ok(())
        }}",
    ))
}

fn dynamic(expression: &str) -> Result<String, String> {
    let whole = format!("json.encode({expression}\n)");
    let decls = declarations("input", Type::Dyn);
    let (output, items) = emitted(&whole, &decls, "input", Box::new(NoMessages))?;
    let entry = Entry {
        decode: "let input = &s::json::decode(text).map_err(|e| format!(\"{e:?}\"))?;",
        param: "input: &s::dynamic::Dyn",
    };
    source("", &items, &output, entry)
}

/// Each model of `schema` a record names, as its fields and struct.
fn records(schema: &Schema, named: &[String]) -> Vec<(Vec<Field>, String)> {
    let each = (0..schema.models.len()).zip(named);
    let found = each.filter_map(|(i, name)| match schema_types::record(schema, i) {
        Ok(Type::Message(fields)) => Some((fields, name.clone())),
        _ => None,
    });
    found.collect()
}

fn typed(expression: &str, schema: &str) -> Result<String, String> {
    let document: serde_json::Value = serde_json::from_str(schema).map_err(|e| e.to_string())?;
    let shared = serde_json::Map::new();
    let (ir, root) = schema_emit::json_schema::lower("Input", &document, &shared, &[])
        .map_err(|e| e.to_string())?;
    let ty = schema_types::record(&ir, root).map_err(|e| e.to_string())?;
    let decls = declarations("input", ty);
    let named = names(&ir);
    let name = |i: usize| named.get(i).cloned().unwrap_or_default();
    let types = StdForm::new(&ir, &name)
        .items()
        .map_err(|e| e.to_string())?;
    let messages = Models {
        records: records(&ir, &named),
    };
    let whole = format!("json.encode({expression}\n)");
    let (output, items) = emitted(&whole, &decls, "input", Box::new(messages))?;
    let root = name(root);
    let prelude = format!("mod types {{ use ::schema_emit_runtime as s; {types} }}");
    let entry = Entry {
        decode:
            "let value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
            let input = &types::decode_input(&value).map_err(|e| format!(\"{e:?}\"))?;",
        param: &format!("input: &types::{root}"),
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
/// feature only if it calls those functions, and what a schema's types need only if it has them.
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
    format!(
        "[package]\nname = {name:?}\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[workspace]\n\n\
         [dependencies]\ncel-emit-runtime = {{ path = {runtime:?}{features} }}\n{typed}"
    )
}
