//! What cel-emit's test binaries share: building the Rust cel-emit writes natively, against the
//! prebuilt cel-emit-runtime rlib found beside the running test.
#![allow(dead_code, clippy::arithmetic_side_effects)]

use std::path::{Path, PathBuf};
use std::process::Command;

pub mod bind;
pub mod values;

/// Messages named by position, as these tests declare them.
pub struct Positional;

impl cel_emit::emit::Messages for Positional {
    fn path(&mut self, i: usize, _: &[cel_emit::Field]) -> String {
        format!("Declared{i}")
    }

    fn fields(&self, fields: &[cel_emit::Field]) -> Vec<String> {
        (0..fields.len()).map(|n| format!("f{n}")).collect()
    }
}

/// `emitter`'s items, with the struct of each message it declares.
pub fn items(emitter: &mut cel_emit::emit::Emitter) -> Result<String, cel_emit::Error> {
    let items = emitter.items()?;
    let mut out = items.code;
    for r in items.messages {
        let body: String = r
            .fields
            .iter()
            .map(|(n, t)| format!("pub {n}: {t}, "))
            .collect();
        out.push_str(&format!(
            "#[derive(Clone, Debug, Default)] pub struct {} {{ {body}}}\n",
            r.rust
        ));
    }
    Ok(out)
}

/// A program printing `c{n}`'s answer for each of `fns`, one line each. `prelude` and `items`
/// come first: what the functions use, and the items the emitter wrote.
pub fn program(fns: &[(usize, String)], prelude: &str, items: &str) -> String {
    let mut out = format!("#![allow(warnings)]\nuse ::cel_emit_runtime as s;\n{prelude}{items}");
    for (_, f) in fns {
        out.push_str(f);
    }
    out.push_str("fn main() {\n");
    for (n, _) in fns {
        out.push_str(&format!(
            "    println!(\"{n}\\t{{}}\", std::panic::catch_unwind(c{n}).unwrap_or_else(|_| \"panic\".into()));\n"
        ));
    }
    out + "}\n"
}

/// The newest rlib of crate `name` beside the running test, and the dir that holds it.
pub fn rlib_of(name: &str) -> (PathBuf, PathBuf) {
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let rlib = std::fs::read_dir(&deps)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            let file = p.file_name().unwrap().to_string_lossy().into_owned();
            file.starts_with(&format!("lib{name}-")) && file.ends_with(".rlib")
        })
        .max_by_key(|p| p.metadata().unwrap().modified().unwrap())
        .unwrap_or_else(|| panic!("no {name} rlib beside the test"));
    (rlib, deps)
}

/// `source` built natively and run: each line it prints, or rustc's errors.
pub fn build_run(dir: &Path, source: &str) -> Result<Vec<String>, String> {
    let (std, deps) = rlib_of("cel_emit_runtime");
    std::fs::create_dir_all(dir).unwrap();
    let (root, exe) = (dir.join("main.rs"), dir.join("main"));
    std::fs::write(&root, source).unwrap();
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let built = Command::new(rustc)
        .args(["--edition", "2021", "-Cdebuginfo=0", "--extern"])
        .arg(format!("cel_emit_runtime={}", std.display()))
        .arg("-L")
        .arg(format!("dependency={}", deps.display()))
        .arg("-o")
        .arg(&exe)
        .arg(&root)
        .output()
        .unwrap();
    if !built.status.success() {
        return Err(String::from_utf8_lossy(&built.stderr).into_owned());
    }
    let run = Command::new(&exe).output().unwrap();
    Ok(String::from_utf8_lossy(&run.stdout)
        .lines()
        .map(Into::into)
        .collect())
}
