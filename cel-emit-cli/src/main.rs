//! `cel-emit <expression.cel> <dir> [--schema <file>] [--functions <file.json>]`: writes the
//! expression's program as a Cargo package in `dir`, with `--schema` over that JSON Schema's
//! types, and `--functions` giving it functions written in CEL.

use std::path::Path;

const USAGE: &str =
    "usage: cel-emit <expression.cel> <dir> [--schema <file>] [--functions <file.json>]";

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let (Some(expression), Some(dir)) = (args.next(), args.next()) else {
        return Err(USAGE.into());
    };
    let (mut schema, mut functions) = (None, None);
    while let Some(flag) = args.next() {
        let slot = match flag.as_str() {
            "--schema" => &mut schema,
            "--functions" => &mut functions,
            _ => return Err(USAGE.into()),
        };
        *slot = Some(args.next().ok_or(USAGE)?);
    }
    let write = |e: std::io::Error| e.to_string();
    let read = |file: Option<String>| file.map(std::fs::read_to_string).transpose().map_err(write);
    let source = std::fs::read_to_string(&expression).map_err(write)?;
    let (schema, functions) = (read(schema)?, read(functions)?);
    let program = cel_emit_cli::program(&source, schema.as_deref(), functions.as_deref())?;
    let dir = Path::new(&dir);
    let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("rows");
    std::fs::create_dir_all(dir.join("src")).map_err(write)?;
    std::fs::write(
        dir.join("Cargo.toml"),
        cel_emit_cli::manifest(name, &program),
    )
    .map_err(write)?;
    std::fs::write(dir.join("src/main.rs"), program).map_err(write)
}
