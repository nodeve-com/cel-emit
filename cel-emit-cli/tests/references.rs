//! Each example holding a reference, `<name>.rs` beside its `<name>.cel`, built beside its
//! generated program and cel-core's interpreter running its `.cel`: each must give the example's
//! `expect.json`, and a table per example prints what each costs. Only allocation calls gate, at
//! [`ALLOCATIONS`]; every other measure only shows. `programs.tsv` in the scratch
//! directory lists each executable, its phase, and its input.

use std::fmt::Write as _;
use std::path::Path;

use prost::Message as _;

#[path = "references/cost.rs"]
mod cost;
#[path = "references/examples.rs"]
mod examples;
#[path = "references/functions.rs"]
mod functions;
mod programs;
#[path = "references/source.rs"]
mod source;

use cost::{ir, llvm_ir, table, Cost, Counts};
use examples::{examples_in, Example};

const EXAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples");
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");

/// Each measured program's allocator, counting.
const COUNT: &str = include_str!("references/count.rs");

/// The interpreter program's `main`.
const INTERPRETER: &str = include_str!("references/interpreter.rs");

/// A generated program's and a reference's `main`.
const COMPILED: &str = include_str!("references/compiled.rs");

/// How far a generated program's allocation calls for a record, decode and expression, may run
/// past its reference's, as numerator and denominator: a goal, never a measured figure, and only
/// ever lowered.
const ALLOCATIONS: [usize; 2] = [5, 4];

/// The executable's `main`: `program`'s `decode` and `expression` functions, by the names given,
/// counted apart.
fn main_rs([decode, expression]: [&str; 2]) -> String {
    format!(
        "{COMPILED}\n#[allow(dead_code)]\nmod program;\n\
        use program::{{{decode} as decode, {expression} as expression}};\n"
    )
}

/// What a reference may link: what ordinary Rust reaches for to read and write JSON and time.
const REFERENCE: &str = "\n[dependencies]\n\
    serde = { version = \"1\", features = [\"derive\"] }\n\
    serde_json = \"1\"\n\
    chrono = { version = \"0.4\", default-features = false, features = [\"std\"] }\n\
    chrono-tz = \"0.10\"\n\
    jiff = { version = \"0.2\", default-features = false, features = [\"std\"] }\n";

/// A reference's two public functions, in order: its decode and its expression.
fn public(reference: &str) -> [&str; 2] {
    let mut names = reference.split("pub fn ").skip(1);
    let mut next = || names.next().expect("a reference has two public functions");
    let mut name = || next().split('(').next().unwrap();
    [name(), name()]
}

/// What the interpreter program links: cel-core and its protobuf form, cel-emit's functions as
/// the interpreter runs them, and cel-emit-runtime's civil time for the embedder's.
fn interpreter_manifest(name: &str) -> String {
    let core = "{ git = \"https://github.com/nodeve-com/cel-core\", branch = \"loom\" }";
    format!(
        "[package]\nname = {name:?}\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[workspace]\n\n\
         [dependencies]\ncel-core = {core}\ncel-core-proto = {core}\nprost = \"0.14\"\n\
         cel-emit = {{ path = {ROOT:?} }}\n\
         cel-emit-runtime = {{ path = \"{ROOT}/cel-emit-runtime\", features = [\"civil\"] }}\n"
    )
}

/// What `example`'s expression checks against.
fn declared(example: &Example) -> cel_emit_cli::Declared {
    let (schema, functions) = (example.schema.as_deref(), example.functions.as_deref());
    cel_emit_cli::declared(schema, functions).unwrap()
}

/// `example`'s expression, as its generated program computes it, parsed and checked against the
/// same declarations, as a `CheckedExpr` protobuf.
fn checked(example: &Example) -> Vec<u8> {
    let env = cel_emit::env::environment(&functions::plain(&declared(example)));
    let source = cel_emit_cli::encoded(&example.cel);
    let ast = cel_emit::macros::compile(&env, &source).unwrap();
    let info = ast.type_info().unwrap();
    cel_core_proto::to_checked_expr_from_ast(info, ast.expr(), ast.source()).encode_to_vec()
}

/// One of an example's programs: its kind, its manifest, its files under `src/`, and its own
/// source, if measured.
struct Program<'a> {
    kind: &'static str,
    manifest: String,
    files: Vec<(&'static str, Vec<u8>)>,
    source: Option<&'a str>,
}

/// A generated program or a reference: `source`, its decode and expression `functions` called by
/// `main_rs`.
fn compiled<'a>(
    kind: &'static str,
    manifest: String,
    source: &'a str,
    functions: [&str; 2],
) -> Program<'a> {
    let files = vec![
        ("main.rs", main_rs(functions).into_bytes()),
        ("program.rs", source.as_bytes().to_vec()),
        ("count.rs", COUNT.as_bytes().to_vec()),
    ];
    Program {
        kind,
        manifest,
        files,
        source: Some(source),
    }
}

/// cel-core's interpreter running `example`'s checked expression.
fn interpreted(example: &Example) -> Program<'static> {
    let files = vec![
        ("main.rs", INTERPRETER.as_bytes().to_vec()),
        ("checked.pb", checked(example)),
        (
            "functions.rs",
            functions::table(&declared(example)).into_bytes(),
        ),
        ("count.rs", COUNT.as_bytes().to_vec()),
    ];
    Program {
        kind: "interpreter",
        manifest: interpreter_manifest(&format!("{}_interpreter", example.name)),
        files,
        source: None,
    }
}

/// What a program's output gives, checked against `example`'s expectation, and its stderr: a line
/// per phase, its name and counts.
fn phases(example: &Example, package: &str, exe: &Path) -> Vec<(String, Counts)> {
    let (out, said, ok) = programs::run(exe, &example.input);
    assert!(ok, "{package}: {said}");
    let json = |t: &str| serde_json::from_str::<serde_json::Value>(t).unwrap();
    assert_eq!(json(&out), json(&example.expect), "{package}");
    let phase = |line: &str| {
        let mut words = line.split(' ');
        let name = words.next().unwrap().to_string();
        let counts: Vec<usize> = words.map(|n| n.parse().unwrap()).collect();
        (name, [counts[0], counts[1], counts[2]])
    };
    said.lines().map(phase).collect()
}

/// `program`, as the package `<example>_<kind>`, built, run on `example`'s input, checked against
/// its expectation, and measured; each phase listed with its input in `listed`.
fn measured(example: &Example, program: &Program, listed: &mut String) -> Cost {
    let package = format!("{}_{}", example.name, program.kind);
    let files: Vec<(&str, &[u8])> = program
        .files
        .iter()
        .map(|(f, b)| (*f, b.as_slice()))
        .collect();
    let rustc: &[&str] = match program.source {
        Some(_) => &["--emit=llvm-ir,link"],
        None => &[],
    };
    let exe = programs::build(&package, &program.manifest, &files, rustc);
    let phases = phases(example, &package, &exe);
    let (exe_path, input) = (exe.display(), example.input_path.display());
    for (phase, _) in &phases {
        writeln!(
            listed,
            "{}\t{}\t{phase}\t{exe_path}\t{input}",
            example.name, program.kind
        )
        .unwrap();
    }
    Cost {
        source: program.source.map(|s| match program.kind {
            "generated" => source::split(s),
            _ => source::whole(s),
        }),
        ir: program.source.map(|_| ir(&llvm_ir(&exe))),
        binary: usize::try_from(exe.metadata().unwrap().len()).unwrap(),
        phases,
    }
}

/// Every example holding a reference, and each variant of one, by name.
fn examples() -> Vec<Example> {
    let dirs = std::fs::read_dir(EXAMPLES).unwrap();
    let mut examples: Vec<Example> = dirs.flat_map(|e| examples_in(&e.unwrap().path())).collect();
    examples.sort_by(|a, b| a.name.cmp(&b.name));
    assert!(!examples.is_empty(), "no example holds a reference");
    examples
}

#[test]
fn each_reference_its_generated_program_and_the_interpreter_give_the_example_output() {
    let (mut tables, mut listed, mut over) = (String::new(), String::new(), Vec::new());
    for example in &examples() {
        let (schema, functions) = (example.schema.as_deref(), example.functions.as_deref());
        let program = cel_emit_cli::program(&example.cel, schema, functions).unwrap();
        let name = format!("{}_generated", example.name);
        let manifest = cel_emit_cli::manifest(&name, &program);
        let generated = compiled("generated", manifest, &program, ["decode", "expression"]);
        let name = format!("{}_reference", example.name);
        let manifest = format!("[package]\nname = {name:?}\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[workspace]\n{REFERENCE}");
        let reference = compiled(
            "reference",
            manifest,
            &example.reference,
            public(&example.reference),
        );
        let costs: Vec<(&str, Cost)> = [generated, reference, interpreted(example)]
            .iter()
            .map(|p| (p.kind, measured(example, p, &mut listed)))
            .collect();
        tables.push_str(&table(&example.name, &costs));
        over.extend(past(&example.name, &costs));
    }
    let list = Path::new(env!("CARGO_TARGET_TMPDIR")).join("programs/programs.tsv");
    std::fs::write(list, listed).unwrap();
    println!("{tables}");
    assert!(
        over.is_empty(),
        "allocations past the reference's: {over:?}"
    );
}

/// `name`, if its generated program's allocation calls for a record run past its reference's by
/// more than [`ALLOCATIONS`].
fn past(name: &str, costs: &[(&str, Cost)]) -> Option<String> {
    let record = |kind: &str| {
        let cost = &costs.iter().find(|(k, _)| *k == kind).unwrap().1;
        cost::record_calls(cost)
    };
    let (generated, reference) = (record("generated"), record("reference"));
    let [above, below] = ALLOCATIONS;
    let over = generated * below > reference * above;
    over.then(|| format!("{name}: {generated} against {reference}"))
}
