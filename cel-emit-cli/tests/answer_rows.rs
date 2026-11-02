//! The example expression's program, built by rustc against the cel-emit-runtime this test links,
//! and run on the example answer.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const EXPRESSION: &str = include_str!("../../examples/answer_rows/rows.cel");
const ANSWER: &str = include_str!("../../examples/answer_rows/answer.json");
const EXPECT: &str = include_str!("../../examples/answer_rows/expect.json");
const SCHEMA: &str = include_str!("../../examples/answer_rows/answer.schema.json");

/// Each `name` rlib beside this test, newest first.
fn rlibs(deps: &Path, name: &str) -> Vec<PathBuf> {
    let prefix = format!("lib{name}-");
    let mut found: Vec<PathBuf> = std::fs::read_dir(deps)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            let name = p.file_name().unwrap().to_string_lossy();
            name.starts_with(&prefix) && name.ends_with(".rlib")
        })
        .collect();
    found.sort_by_key(|p| std::cmp::Reverse(p.metadata().unwrap().modified().unwrap()));
    found
}

/// The program, built once as `name`: other builds leave rlibs without the `civil` feature, and
/// a `serde_json` other than the decode's, which fail.
fn built(name: &str, schema: Option<&str>) -> PathBuf {
    let program = cel_emit_cli::program(EXPRESSION, schema).unwrap();
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let (source, exe) = (dir.join("main.rs"), dir.join("main"));
    std::fs::write(&source, program).unwrap();
    let deps = std::env::current_exe().unwrap();
    let deps = deps.parent().unwrap();
    let schema_runtime = rlibs(deps, "schema_emit_runtime").remove(0);
    let mut errors = String::new();
    for runtime in rlibs(deps, "cel_emit_runtime") {
        for json in rlibs(deps, "serde_json") {
            let linked = [
                ("cel_emit_runtime", &runtime),
                ("schema_emit_runtime", &schema_runtime),
                ("serde_json", &json),
            ];
            let mut rustc = Command::new(std::env::var("RUSTC").unwrap_or("rustc".into()));
            rustc.args(["--edition", "2021", "-Cdebuginfo=0", "-Dwarnings"]);
            for (name, rlib) in linked {
                rustc
                    .arg("--extern")
                    .arg(format!("{name}={}", rlib.display()));
            }
            let out = rustc
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
    }
    panic!("no runtime built it: {errors}");
}

/// What the program prints for `answer`, and whether it succeeded.
fn run(exe: &Path, answer: &str) -> (String, bool) {
    let (out, _, ok) = ran(exe, answer);
    (out, ok)
}

/// What the program prints for `answer`, what it says on failing, and whether it succeeded.
fn ran(exe: &Path, answer: &str) -> (String, String, bool) {
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(answer.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
        out.status.success(),
    )
}

#[test]
fn only_a_program_calling_them_links_the_civil_functions() {
    let plain = "input.a";
    let manifest = |d| cel_emit_cli::manifest("p", &cel_emit_cli::program(d, None).unwrap());
    assert!(!manifest(plain).contains("civil"));
    assert!(manifest(EXPRESSION).contains("civil"));
}

/// The rows, and the failures of what the expression reads, the same through `exe`.
fn gives_the_rows(exe: &Path) {
    assert_eq!(run(exe, ANSWER), (EXPECT.to_string(), true));

    let zero = ANSWER.replace("\"total\": 13000", "\"total\": 0");
    let (rows, ok) = run(exe, &zero);
    assert!(ok && rows.contains("\"best_total_minor\":0"), "{rows}");

    let unnamed = ANSWER.replace(
        "\"product_id\": \"route-standard\"",
        "\"product_id\": \"none\"",
    );
    assert!(!run(exe, &unnamed).1, "a code naming nothing is an error");
}

#[test]
fn an_expression_alone_gives_the_rows() {
    gives_the_rows(&built("answer_rows", None));
}

#[test]
fn a_schema_gives_the_same_rows() {
    gives_the_rows(&built("answer_rows_schema", Some(SCHEMA)));
}

#[test]
fn a_program_over_a_schema_names_no_dyn_read() {
    let program = cel_emit_cli::program(EXPRESSION, Some(SCHEMA)).unwrap();
    assert!(!program.contains("dynamic::get"), "{program}");
    assert!(!program.contains("json::decode"), "{program}");
}

#[test]
fn an_answer_breaking_the_schema_fails_by_name() {
    let exe = built("answer_rows_broken", Some(SCHEMA));
    let broken = ANSWER.replace("\"total\": 13000", "\"total\": \"none\"");
    let (rows, said, ok) = ran(&exe, &broken);
    assert!(!ok && rows.is_empty(), "{rows}");
    let place = "options[0].pricing_potential_best.total";
    assert!(said.contains(place), "{said}");
}

#[test]
fn a_program_over_a_schema_links_its_decode() {
    let program = cel_emit_cli::program(EXPRESSION, Some(SCHEMA)).unwrap();
    let manifest = cel_emit_cli::manifest("p", &program);
    assert!(manifest.contains("schema-emit-runtime"), "{manifest}");
    let plain = cel_emit_cli::program(EXPRESSION, None).unwrap();
    let manifest = cel_emit_cli::manifest("p", &plain);
    assert!(!manifest.contains("schema-emit-runtime"), "{manifest}");
}
