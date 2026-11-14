//! Hands the library the version its manifest takes `schema-emit` at, as `SCHEMA_EMIT_VERSION`, so
//! a program's manifest links the decode that wrote its types.

fn main() {
    println!("cargo::rerun-if-changed=Cargo.toml");
    let manifest = std::fs::read_to_string("Cargo.toml").unwrap_or_else(|e| panic!("{e}"));
    let line = manifest.lines().find(|l| l.starts_with("schema-emit = "));
    let version = line.and_then(|l| l.split('"').nth(1));
    let version = version.unwrap_or_else(|| panic!("Cargo.toml names no schema-emit version"));
    println!("cargo::rustc-env=SCHEMA_EMIT_VERSION={version}");
}
