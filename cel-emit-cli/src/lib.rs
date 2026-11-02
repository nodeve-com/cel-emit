#![doc = include_str!("../README.md")]

use cel_emit::emit::{Emitter, Messages, Vars};
use cel_emit::functions::Function;
use cel_emit::{lower, Decl, Decls, Error, Field, Name, Type};

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

/// The one input, `text`, is a parameter.
struct Text;

impl Vars for Text {
    fn read(&self, _: usize) -> Result<String, Error> {
        Ok("text".into())
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

/// The Rust program computing `expression`, a CEL expression reading `input`.
pub fn program(expression: &str) -> Result<String, String> {
    let mut decls = Decls {
        functions: vec![
            function("to_utc", &[Type::String, Type::String], Type::Timestamp),
            function("iso_duration", &[Type::String], Type::Duration),
        ],
        ..Decls::default()
    };
    decls
        .names
        .insert("text".into(), Name::var(Type::String, 0));
    let whole = format!("cel.bind(input, json.decode(text), json.encode({expression}\n))");
    let tree = lower(&whole, &decls).map_err(|e| e.to_string())?;
    let mut emitter = Emitter::new(&decls.functions, Box::new(NoMessages));
    let output = emitter.expr(&tree, &Text).map_err(|e| e.to_string())?;
    let items = emitter.items().map_err(|e| e.to_string())?;
    let source = format!(
        "use ::cel_emit_runtime as s;
        {items}
        fn output(text: String) -> s::Result<String> {{
            Ok({output})
        }}
        fn main() -> Result<(), String> {{
            let text = std::io::read_to_string(std::io::stdin()).map_err(|e| e.to_string())?;
            println!(\"{{}}\", output(text).map_err(|e| format!(\"{{e:?}}\"))?);
            Ok(())
        }}",
        items = items.code,
    );
    let file = syn::parse_file(&source).map_err(|e| e.to_string())?;
    Ok(prettyplease::unparse(&file))
}

/// A Cargo manifest for `program`, built against this repo's cel-emit-runtime, with the `civil`
/// feature only if it calls those functions.
pub fn manifest(name: &str, program: &str) -> String {
    let runtime = concat!(env!("CARGO_MANIFEST_DIR"), "/../cel-emit-runtime");
    let features = match program.contains("s::time::civil::") {
        true => ", features = [\"civil\"]",
        false => "",
    };
    format!(
        "[package]\nname = {name:?}\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[workspace]\n\n\
         [dependencies]\ncel-emit-runtime = {{ path = {runtime:?}{features} }}\n"
    )
}
