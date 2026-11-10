//! `cel-emit <expression.cel>... <dir> [--schema <file>] [--defs <Name>=<file>] [--functions
//! <file.json>] [--into <property>]`: writes the program as a Cargo package in `dir`. One
//! expression is the program's value, with `--schema` over that JSON Schema's types. Two or more
//! are computed properties, each filling the property its file's name keys, in the record
//! `--schema` types, under `--into` where that names their object. `--defs` joins a second
//! schema file's definitions to `--schema`'s, its root as `Name`. `--functions` gives either
//! functions written in CEL.

use std::path::Path;

const USAGE: &str = "usage: cel-emit <expression.cel>... <dir> [--schema <file>] \
                     [--defs <Name>=<file>] [--functions <file.json>] [--into <property>]";

/// `schema` with the definitions of `defs`, a `<Name>=<file>`, joined to its own.
fn joined(schema: Option<String>, defs: Option<String>) -> Result<Option<String>, String> {
    let Some(defs) = defs else {
        return Ok(schema);
    };
    let schema = schema.ok_or("--defs takes a --schema")?;
    let (name, file) = defs.split_once('=').ok_or(USAGE)?;
    let text = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
    cel_emit_cli::with_defs(&schema, name, &text).map(Some)
}

/// Each expression file's stem and text.
fn expressions(files: &[String]) -> Result<Vec<(String, String)>, String> {
    let read = |file: &String| {
        let stem = Path::new(file).file_stem().and_then(|s| s.to_str());
        let text = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
        Ok((stem.ok_or(USAGE)?.to_string(), text))
    };
    files.iter().map(read).collect()
}

/// The program of `sources`, each a key and its CEL: one expression's value, or with two or
/// more, or an `into`, computed properties. `texts` holds the schema, the functions, and `into`.
fn program(sources: &[(String, String)], texts: [Option<&str>; 3]) -> Result<String, String> {
    let [schema, functions, into] = texts;
    match (sources, schema) {
        ([], _) => Err(USAGE.into()),
        ([(_, source)], _) if into.is_none() => cel_emit_cli::program(source, schema, functions),
        (_, None) => Err("computed properties take a --schema".into()),
        (all, Some(schema)) => {
            let all: Vec<_> = all.iter().map(|(k, c)| (k.as_str(), c.as_str())).collect();
            cel_emit_cli::computed(&all, schema, into, functions)
        }
    }
}

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1).peekable();
    let mut files = Vec::new();
    while let Some(file) = args.next_if(|a| !a.starts_with("--")) {
        files.push(file);
    }
    let dir = files.pop().ok_or(USAGE)?;
    let (mut schema, mut functions, mut into, mut defs) = (None, None, None, None);
    while let Some(flag) = args.next() {
        let slot = match flag.as_str() {
            "--schema" => &mut schema,
            "--functions" => &mut functions,
            "--into" => &mut into,
            "--defs" => &mut defs,
            _ => return Err(USAGE.into()),
        };
        *slot = Some(args.next().ok_or(USAGE)?);
    }
    let write = |e: std::io::Error| e.to_string();
    let read = |file: Option<String>| file.map(std::fs::read_to_string).transpose().map_err(write);
    let (schema, functions) = (joined(read(schema)?, defs)?, read(functions)?);
    let texts = [schema.as_deref(), functions.as_deref(), into.as_deref()];
    let program = program(&expressions(&files)?, texts)?;
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
