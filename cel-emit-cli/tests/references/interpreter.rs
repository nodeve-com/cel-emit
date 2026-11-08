//! cel-core's interpreter running the checked expression saved beside it, `checked.pb`, in three
//! counted phases: `load`, the saved expression to a program, once; then on stdin's JSON,
//! `decode`, the text to cel-core values, and `expression`, evaluation to the output text.

mod count;

use cel_core::{
    CelType, Env, FunctionDecl, MapActivation, OverloadDecl, Program, Timestamp, Value,
};
use cel_core_proto::{check_result_from_proto, from_parsed_expr, CheckedExpr, ParsedExpr};
use cel_emit_runtime::time::civil;
use prost::Message;
use std::hint::black_box;
use std::sync::Arc;

#[global_allocator]
static COUNTING: count::Counting = count::Counting;

const CHECKED: &[u8] = include_bytes!("checked.pb");

/// Each function written in CEL: its name, its params, and its checked body, each after those it
/// calls.
const FUNCTIONS: &[(&str, &[&str], &[u8])] = include!("functions.rs");

/// The two text arguments of a call.
fn texts(args: &[Value]) -> Option<(String, String)> {
    match args {
        [Value::String(a), Value::String(b)] => Some((a.to_string(), b.to_string())),
        _ => None,
    }
}

/// The functions an embedder declares, running as cel-emit-runtime's do.
fn civil_time() -> [FunctionDecl; 2] {
    let to_utc = OverloadDecl::function(
        "to_utc",
        vec![CelType::String, CelType::String],
        CelType::Timestamp,
    )
    .with_impl(
        |args| match texts(args).map(|(l, z)| civil::to_utc(&l, &z)) {
            Some(Ok(t)) => Value::Timestamp(Timestamp::from_datetime(&t)),
            Some(Err(e)) => Value::error(format!("{e:?}")),
            None => Value::error("to_utc takes two strings"),
        },
    );
    let iso_duration =
        OverloadDecl::function("iso_duration", vec![CelType::String], CelType::Duration).with_impl(
            |args| match args {
                [Value::String(text)] => match civil::iso_duration(text) {
                    Ok(d) => Value::Duration(cel_core::Duration::from_chrono(d)),
                    Err(e) => Value::error(format!("{e:?}")),
                },
                _ => Value::error("iso_duration takes a string"),
            },
        );
    [
        FunctionDecl::new("to_utc").with_overload(to_utc),
        FunctionDecl::new("iso_duration").with_overload(iso_duration),
    ]
}

/// A saved checked expression, decoded.
fn saved(bytes: &[u8]) -> Result<cel_core::Ast, String> {
    let checked = CheckedExpr::decode(bytes).map_err(|e| e.to_string())?;
    let info = check_result_from_proto(&checked).map_err(|e| e.to_string())?;
    let parsed = ParsedExpr {
        expr: checked.expr,
        source_info: checked.source_info,
    };
    let expr = from_parsed_expr(&parsed).map_err(|e| e.to_string())?;
    Ok(cel_core::Ast::new_checked(expr, "", info))
}

/// `env` with each written function: a call evaluates the function's body over the arguments.
fn with_written(mut env: Env) -> Result<Env, String> {
    for (name, params, body) in FUNCTIONS {
        let program = Arc::new(env.program(&saved(body)?).map_err(|e| e.to_string())?);
        let overload =
            OverloadDecl::function(*name, vec![CelType::Dyn; params.len()], CelType::Dyn)
                .with_impl(move |args| {
                    let mut activation = MapActivation::new();
                    for (param, arg) in params.iter().zip(args) {
                        activation.insert(*param, arg.clone());
                    }
                    program.eval(&activation)
                });
        env = env.with_extension([FunctionDecl::new(*name).with_overload(overload)]);
    }
    Ok(env)
}

/// The saved expression, decoded, and the program running it.
#[inline(never)]
fn measured_load() -> Result<Program, String> {
    let ast = saved(CHECKED)?;
    let env = cel_emit::env::environment(&cel_emit::Decls::default()).with_extension(civil_time());
    with_written(env)?.program(&ast).map_err(|e| e.to_string())
}

/// `text` as cel-core values.
#[inline(never)]
fn measured_decode(text: &str) -> Value {
    cel_emit::functions::json_value(text)
}

/// The expression's output text, `input` bound.
#[inline(never)]
fn measured_expression(program: &Program, input: &Value) -> Result<Arc<str>, String> {
    let mut activation = MapActivation::new();
    activation.insert("input", input.clone());
    match program.eval(&activation) {
        Value::String(out) => Ok(out),
        other => Err(format!("{other:?}")),
    }
}

/// `text` decoded, then the expression's output text: each phase counted.
fn record(program: &Program, text: &str) -> Result<Arc<str>, String> {
    let input = count::counted("decode", || measured_decode(black_box(text)));
    count::counted("expression", || {
        measured_expression(program, black_box(&input))
    })
}

fn main() {
    let text = std::io::read_to_string(std::io::stdin()).unwrap();
    let out = count::counted("load", measured_load).and_then(|program| record(&program, &text));
    match out {
        Ok(out) => println!("{out}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
