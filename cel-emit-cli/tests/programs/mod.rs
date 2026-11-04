//! Programs built as Cargo packages, each its own workspace, in release, against this
//! repository's lock and offline, into one target directory under the test's scratch space.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The package directory and target directory every program here shares.
fn dirs(name: &str) -> (PathBuf, PathBuf) {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("programs");
    (root.join(name), root.join("target"))
}

/// What every package's release profile adds: one codegen unit, so its IR is one file, and no
/// symbols, so its size is the program's. `CARGO_PROFILE_RELEASE_STRIP` overrides the stripping.
const PROFILE: &str = "\n[profile.release]\ncodegen-units = 1\nstrip = true\n";

/// `text` at `path`, unless it already holds it: an unchanged file keeps its mtime, so Cargo keeps
/// its build. Written beside and renamed over, so a test building the same package at once never
/// reads it half written.
fn write(path: &Path, text: &[u8]) {
    if std::fs::read(path).is_ok_and(|held| held == text) {
        return;
    }
    let file = path.file_name().unwrap().to_string_lossy();
    let part = path.with_file_name(format!(".{file}.{}", std::process::id()));
    std::fs::write(&part, text).unwrap();
    std::fs::rename(part, path).unwrap();
}

/// The package `name`, `manifest` with each of `files` under `src/`, built; its executable.
/// `rustc` passes those arguments to the package's own compile.
pub fn build(name: &str, manifest: &str, files: &[(&str, &[u8])], rustc: &[&str]) -> PathBuf {
    let (dir, target) = dirs(name);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    write(
        &dir.join("Cargo.toml"),
        format!("{manifest}{PROFILE}").as_bytes(),
    );
    let lock = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.lock"));
    write(&dir.join("Cargo.lock"), lock);
    for (file, text) in files {
        write(&dir.join("src").join(file), text);
    }
    let cargo = std::env::var("CARGO").unwrap_or("cargo".into());
    let out = Command::new(cargo)
        .args([
            "rustc",
            "--release",
            "--offline",
            "--quiet",
            "--manifest-path",
        ])
        .arg(dir.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(&target)
        .arg("--")
        .args(rustc)
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{name}: {said}");
    target.join("release").join(name)
}

/// What `exe` prints for `input`, what it says on stderr, and whether it succeeded.
pub fn run(exe: &Path, input: &str) -> (String, String, bool) {
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input.as_bytes()).unwrap();
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    (
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
        out.status.success(),
    )
}
