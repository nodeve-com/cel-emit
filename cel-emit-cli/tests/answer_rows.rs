//! The example expression's program, built as its own package against this repository's
//! cel-emit-runtime, and run on the example answer.

use std::path::{Path, PathBuf};

mod programs;

const EXPRESSION: &str = include_str!("../../examples/answer_rows/rows.cel");
const ANSWER: &str = include_str!("../../examples/answer_rows/answer.json");
const EXPECT: &str = include_str!("../../examples/answer_rows/expect.json");
const SCHEMA: &str = include_str!("../../examples/answer_rows/answer.schema.json");

/// The program, as its own package `name`, built.
fn built(name: &str, schema: Option<&str>) -> PathBuf {
    let program = cel_emit_cli::program(EXPRESSION, schema).unwrap();
    let manifest = cel_emit_cli::manifest(name, &program);
    programs::build(
        name,
        &manifest,
        &[("main.rs", program.as_bytes())],
        &["-Dwarnings"],
    )
}

/// What the program prints for `answer`, and whether it succeeded.
fn run(exe: &Path, answer: &str) -> (String, bool) {
    let (out, _, ok) = programs::run(exe, answer);
    (out, ok)
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
    let exe = built("answer_rows_schema", Some(SCHEMA));
    let broken = ANSWER.replace("\"total\": 13000", "\"total\": \"none\"");
    let (rows, said, ok) = programs::run(&exe, &broken);
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
