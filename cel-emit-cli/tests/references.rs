//! Each example holding a reference, `<name>.rs` beside its `<name>.cel`, built beside its
//! generated program: both must give the example's `expect.json`, and a table prints what each
//! costs. `programs.tsv` in the scratch directory lists each executable and its input.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

mod programs;

const EXAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples");

/// Each measured program's allocator, counting.
const COUNT: &str = include_str!("references/count.rs");

/// The executable's `main`: `program::{function}` once on stdin, counted, its allocations on
/// stderr's first line.
fn main_rs(function: &str) -> String {
    format!(
        r#"mod count;
#[allow(dead_code)]
mod program;

#[global_allocator]
static COUNTING: count::Counting = count::Counting;

#[inline(never)]
fn measured(text: &str) -> Result<String, String> {{
    program::{function}(text).map_err(|e| format!("{{e:?}}"))
}}

fn main() {{
    let text = std::io::read_to_string(std::io::stdin()).unwrap();
    count::start();
    let out = measured(&text);
    let (calls, bytes, peak) = count::stop();
    eprintln!("{{calls}} {{bytes}} {{peak}}");
    match out {{
        Ok(out) => println!("{{out}}"),
        Err(e) => {{
            eprintln!("{{e}}");
            std::process::exit(1);
        }}
    }}
}}
"#
    )
}

/// What a reference may link: what ordinary Rust reaches for to read and write JSON and time.
const REFERENCE: &str = "\n[dependencies]\n\
    serde = { version = \"1\", features = [\"derive\"] }\n\
    serde_json = \"1\"\n\
    chrono = \"0.4\"\n\
    chrono-tz = \"0.10\"\n\
    jiff = \"0.2\"\n";

/// An example holding a reference.
struct Example {
    name: String,
    cel: String,
    reference: String,
    schema: Option<String>,
    input: String,
    input_path: PathBuf,
    expect: String,
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The example in `dir`, if it holds a reference.
fn example(dir: &Path) -> Option<Example> {
    let files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .map(|e| e.unwrap().path())
        .collect();
    let named = |p: &&PathBuf| p.file_name().unwrap().to_string_lossy().into_owned();
    let cel = files
        .iter()
        .find(|p| p.extension().is_some_and(|e| e == "cel"))?;
    let reference = cel.with_extension("rs");
    let schema = files.iter().find(|p| named(p).ends_with(".schema.json"));
    let input = files.iter().find(|p| {
        let file = named(p);
        file.ends_with(".json") && file != "expect.json" && !file.ends_with(".schema.json")
    })?;
    Some(Example {
        name: named(&&dir.to_path_buf()),
        cel: read(cel),
        reference: std::fs::read_to_string(reference).ok()?,
        schema: schema.map(|p| read(p)),
        input: read(input),
        input_path: input.clone(),
        expect: read(&dir.join("expect.json")),
    })
}

/// The one public function of a reference.
fn public(reference: &str) -> &str {
    let after = reference
        .split("pub fn ")
        .nth(1)
        .expect("no public function");
    after.split('(').next().unwrap()
}

/// What one program costs.
struct Cost {
    lines: usize,
    bytes: usize,
    ir: Ir,
    binary: u64,
    calls: usize,
    allocated: usize,
    peak: usize,
}

/// LLVM IR lines and defined functions, for the crate and for its own: `measured`, where the
/// expression inlines, and the `program` module.
#[derive(Default)]
struct Ir {
    lines: usize,
    defines: usize,
    own_lines: usize,
    own_defines: usize,
}

fn ir(text: &str) -> Ir {
    let mut ir = Ir::default();
    let mut own = false;
    for line in text.lines() {
        ir.lines += 1;
        if line.starts_with("define ") {
            ir.defines += 1;
            own = line.contains("program") || line.contains("measured");
            ir.own_defines += usize::from(own);
        }
        ir.own_lines += usize::from(own);
        own &= line != "}";
    }
    ir
}

/// The LLVM IR the build of `exe` wrote beside it, asked for by `--emit=llvm-ir,link`.
fn llvm_ir(exe: &Path) -> String {
    let deps = exe.parent().unwrap().join("deps");
    let prefix = format!("{}-", exe.file_name().unwrap().to_string_lossy());
    let newest = std::fs::read_dir(deps)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            let file = p.file_name().unwrap().to_string_lossy();
            file.starts_with(&prefix) && file.ends_with(".ll")
        })
        .max_by_key(|p| p.metadata().unwrap().modified().unwrap())
        .unwrap();
    std::fs::read_to_string(newest).unwrap()
}

/// One of an example's programs: its kind, its manifest, and the public function of its source.
struct Program<'a> {
    kind: &'static str,
    manifest: String,
    function: &'a str,
    source: &'a str,
}

/// What a program's output and stderr's first line, its allocations, give.
fn checked(example: &Example, package: &str, exe: &Path) -> [usize; 3] {
    let (out, said, ok) = programs::run(exe, &example.input);
    assert!(ok, "{package}: {said}");
    let json = |t: &str| serde_json::from_str::<serde_json::Value>(t).unwrap();
    assert_eq!(json(&out), json(&example.expect), "{package}");
    let first = said.lines().next().unwrap_or_default();
    let counts: Vec<usize> = first.split(' ').map(|n| n.parse().unwrap()).collect();
    [counts[0], counts[1], counts[2]]
}

/// `program`, as the package `<example>_<kind>`, built, run on `example`'s input, checked against
/// its expectation, measured into `table`, and listed with its input in `listed`.
fn measured(example: &Example, program: &Program, table: &mut String, listed: &mut String) {
    let package = format!("{}_{}", example.name, program.kind);
    let main = main_rs(program.function);
    let files = [
        ("main.rs", main.as_str()),
        ("program.rs", program.source),
        ("count.rs", COUNT),
    ];
    let exe = programs::build(
        &package,
        &program.manifest,
        &files,
        &["--emit=llvm-ir,link"],
    );
    let [calls, allocated, peak] = checked(example, &package, &exe);
    let cost = Cost {
        lines: program.source.lines().count(),
        bytes: program.source.len(),
        ir: ir(&llvm_ir(&exe)),
        binary: exe.metadata().unwrap().len(),
        calls,
        allocated,
        peak,
    };
    row(table, &package, &cost);
    let (exe, input) = (exe.display(), example.input_path.display());
    writeln!(listed, "{}\t{}\t{exe}\t{input}", example.name, program.kind).unwrap();
}

fn row(table: &mut String, program: &str, c: &Cost) {
    let Ir {
        lines,
        defines,
        own_lines,
        own_defines,
    } = c.ir;
    writeln!(
        table,
        "| {program} | {} | {} | {lines} | {defines} | {own_lines} | {own_defines} | {} | {} | {} | {} |",
        c.lines, c.bytes, c.binary, c.calls, c.allocated, c.peak
    )
    .unwrap();
}

const HEAD: &str = "| Program | Lines | Bytes | IR lines | IR functions | Own IR lines \
    | Own IR functions | Binary | Allocations | Bytes allocated | Peak bytes |\n\
    | - | - | - | - | - | - | - | - | - | - | - |\n";

/// Every example holding a reference, by name.
fn examples() -> Vec<Example> {
    let mut examples: Vec<Example> = std::fs::read_dir(EXAMPLES)
        .unwrap()
        .filter_map(|e| example(&e.unwrap().path()))
        .collect();
    examples.sort_by(|a, b| a.name.cmp(&b.name));
    assert!(!examples.is_empty(), "no example holds a reference");
    examples
}

#[test]
fn each_reference_and_its_generated_program_give_the_example_output() {
    let (mut table, mut listed) = (String::from(HEAD), String::new());
    for example in &examples() {
        let program = cel_emit_cli::program(&example.cel, example.schema.as_deref()).unwrap();
        let generated = Program {
            kind: "generated",
            manifest: cel_emit_cli::manifest(&format!("{}_generated", example.name), &program),
            function: "output",
            source: &program,
        };
        let name = format!("{}_reference", example.name);
        let reference = Program {
            kind: "reference",
            manifest: format!("[package]\nname = {name:?}\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[workspace]\n{REFERENCE}"),
            function: public(&example.reference),
            source: &example.reference,
        };
        for program in [generated, reference] {
            measured(example, &program, &mut table, &mut listed);
        }
    }
    let list = Path::new(env!("CARGO_TARGET_TMPDIR")).join("programs/programs.tsv");
    std::fs::write(list, listed).unwrap();
    println!("{table}");
}
