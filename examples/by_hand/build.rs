//! Translates one CEL expression into a Rust function, `status`, for `src/main.rs` to include.

use cel_emit::emit::{Emitter, Messages, Vars};
use cel_emit::{lower, Decls, Error, Field, Name, Type};

const EXPRESSION: &str = r#"sensor.temp > sensor.limit ? "hot" : "ok""#;

/// Tells cel-emit which Rust struct a message is, and its field names.
struct OurStructs;

impl Messages for OurStructs {
    fn path(&mut self, _: usize, _: &[Field]) -> String {
        "crate::Sensor".into()
    }

    fn fields(&self, fields: &[Field]) -> Vec<String> {
        fields.iter().map(|f| f.name.clone()).collect()
    }
}

/// Tells cel-emit how the Rust it writes reads each input: here, a function parameter.
struct OurInputs;

impl Vars for OurInputs {
    fn read(&self, _: usize) -> Result<String, Error> {
        Ok("sensor".into())
    }

    fn has(&self, _: usize) -> Result<String, Error> {
        Ok("true".into())
    }
}

fn number(name: &str) -> Field {
    Field {
        name: name.into(),
        ty: Type::Double,
        required: true,
    }
}

fn translate() -> Result<String, Error> {
    // What the expression may read, matching `struct Sensor` in `src/main.rs`.
    let sensor = Type::Message(vec![number("temp"), number("limit")]);
    let decls = Decls::new([("sensor".to_string(), Name::var(sensor, 0))].into());

    // Check the CEL, then write it as Rust.
    let tree = lower(EXPRESSION, &decls)?;
    let mut emitter = Emitter::new(&decls.functions, Box::new(OurStructs));
    let rust = emitter.expr(&tree, &OurInputs)?;
    let result = emitter.rust(&tree.ty)?;

    Ok(format!(
        "use ::cel_emit_runtime as s;
        pub fn status(sensor: &crate::Sensor) -> s::Result<{result}> {{ Ok({rust}) }}"
    ))
}

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    let rust = translate().unwrap_or_else(|e| panic!("{EXPRESSION}: {e}"));
    let out = std::env::var("OUT_DIR").unwrap_or_else(|e| panic!("OUT_DIR: {e}"));
    std::fs::write(format!("{out}/status.rs"), rust).unwrap_or_else(|e| panic!("{e}"));
}
