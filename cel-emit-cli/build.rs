//! Hands the library the rev its manifest pins `schema-emit` to, as `SCHEMA_EMIT_REV`, so a
//! program's manifest links the decode that wrote its types.

fn main() {
    println!("cargo::rerun-if-changed=Cargo.toml");
    let manifest = std::fs::read_to_string("Cargo.toml").unwrap_or_else(|e| panic!("{e}"));
    let line = manifest.lines().find(|l| l.starts_with("schema-emit = "));
    let rev = line.and_then(|l| l.split("rev = \"").nth(1));
    let rev = rev.and_then(|r| r.split('"').next());
    let rev = rev.unwrap_or_else(|| panic!("Cargo.toml pins no schema-emit rev"));
    println!("cargo::rustc-env=SCHEMA_EMIT_REV={rev}");
}
