//! `cel-emit <expression.cel> <dir> [--schema <file>]`: writes the expression's program as a
//! Cargo package in `dir`, with `--schema` over that JSON Schema's types.

use std::path::Path;

const USAGE: &str = "usage: cel-emit <expression.cel> <dir> [--schema <file>]";

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let (Some(expression), Some(dir)) = (args.next(), args.next()) else {
        return Err(USAGE.into());
    };
    let schema = match (args.next().as_deref(), args.next()) {
        (None, _) => None,
        (Some("--schema"), Some(file)) => Some(file),
        _ => return Err(USAGE.into()),
    };
    let write = |e: std::io::Error| e.to_string();
    let source = std::fs::read_to_string(&expression).map_err(write)?;
    let schema = schema
        .map(std::fs::read_to_string)
        .transpose()
        .map_err(write)?;
    let program = cel_emit_cli::program(&source, schema.as_deref())?;
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
