//! The example expression's program, built by rustc against the cel-emit-runtime this test links,
//! and run on the example answer.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const EXPRESSION: &str = include_str!("../../examples/answer_rows/rows.cel");
const ANSWER: &str = include_str!("../../examples/answer_rows/answer.json");
const EXPECT: &str = include_str!("../../examples/answer_rows/expect.json");

/// Each cel-emit-runtime rlib beside this test, newest first.
fn runtimes(deps: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(deps)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name.starts_with("libcel_emit_runtime-") && name.ends_with(".rlib")
        })
        .collect();
    found.sort_by_key(|p| std::cmp::Reverse(p.metadata().unwrap().modified().unwrap()));
    found
}

/// The program, built once: other builds leave rlibs without the `civil` feature, which fail.
fn built() -> PathBuf {
    let program = cel_emit_cli::program(EXPRESSION).unwrap();
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("answer_rows");
    std::fs::create_dir_all(&dir).unwrap();
    let (source, exe) = (dir.join("main.rs"), dir.join("main"));
    std::fs::write(&source, program).unwrap();
    let deps = std::env::current_exe().unwrap();
    let deps = deps.parent().unwrap();
    let mut errors = String::new();
    for runtime in runtimes(deps) {
        let out = Command::new(std::env::var("RUSTC").unwrap_or("rustc".into()))
            .args([
                "--edition",
                "2021",
                "-Cdebuginfo=0",
                "-Dwarnings",
                "--extern",
            ])
            .arg(format!("cel_emit_runtime={}", runtime.display()))
            .arg("-L")
            .arg(format!("dependency={}", deps.display()))
            .arg("-o")
            .arg(&exe)
            .arg(&source)
            .output()
            .unwrap();
        if out.status.success() {
            return exe;
        }
        errors.push_str(&String::from_utf8_lossy(&out.stderr));
    }
    panic!("no runtime built it: {errors}");
}

/// What the program prints for `answer`, and whether it succeeded.
fn run(exe: &Path, answer: &str) -> (String, bool) {
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(answer.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (String::from_utf8(out.stdout).unwrap(), out.status.success())
}

#[test]
fn only_a_program_calling_them_links_the_civil_functions() {
    let plain = "input.a";
    let manifest = |d| cel_emit_cli::manifest("p", &cel_emit_cli::program(d).unwrap());
    assert!(!manifest(plain).contains("civil"));
    assert!(manifest(EXPRESSION).contains("civil"));
}

#[test]
fn an_expression_alone_gives_the_rows() {
    let exe = built();
    assert_eq!(run(&exe, ANSWER), (EXPECT.to_string(), true));

    let zero = ANSWER.replace("\"total\": 13000", "\"total\": 0");
    let (rows, ok) = run(&exe, &zero);
    assert!(ok && rows.contains("\"best_total_minor\":0"), "{rows}");

    let unnamed = ANSWER.replace(
        "\"product_id\": \"route-standard\"",
        "\"product_id\": \"none\"",
    );
    assert!(!run(&exe, &unnamed).1, "a code naming nothing is an error");
}
