//! What a program costs, and the table of an example's costs.

use std::fmt::Write as _;
use std::path::Path;

use super::source::Source;

/// One phase's allocation calls, bytes, and peak bytes live.
pub(super) type Counts = [usize; 3];

/// What one program costs. Source and IR are only a generated program's and a reference's; a
/// source's split is a generated program's.
pub(super) struct Cost {
    pub(super) source: Option<Source>,
    pub(super) ir: Option<Ir>,
    pub(super) binary: usize,
    pub(super) phases: Vec<(String, Counts)>,
}

/// LLVM IR lines and defined functions, for the crate and for its own: the `measured_` functions,
/// where the program's functions may inline, and the `program` module.
#[derive(Default)]
pub(super) struct Ir {
    lines: usize,
    defines: usize,
    own_lines: usize,
    own_defines: usize,
}

pub(super) fn ir(text: &str) -> Ir {
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
pub(super) fn llvm_ir(exe: &Path) -> String {
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

/// What a program's cost gives for a measure.
type Of = Box<dyn Fn(&Cost) -> Option<usize>>;

/// Each measure: its name and what a program's cost gives for it.
type Measure = (String, Of);

/// What a program's cost gives for a measure of the whole program.
type Whole = fn(&Cost) -> Option<usize>;

/// The measures of the program as a whole.
const WHOLE: [(&str, Whole); 11] = [
    ("Lines, decode", |c| split(c, |s| s.decode, 0)),
    ("Lines, expression", |c| split(c, |s| s.expression, 0)),
    ("Lines, whole", |c| c.source.as_ref().map(|s| s.whole[0])),
    ("Bytes, decode", |c| split(c, |s| s.decode, 1)),
    ("Bytes, expression", |c| split(c, |s| s.expression, 1)),
    ("Bytes, whole", |c| c.source.as_ref().map(|s| s.whole[1])),
    ("IR lines", |c| c.ir.as_ref().map(|i| i.lines)),
    ("IR functions", |c| c.ir.as_ref().map(|i| i.defines)),
    ("Own IR lines", |c| c.ir.as_ref().map(|i| i.own_lines)),
    ("Own IR functions", |c| c.ir.as_ref().map(|i| i.own_defines)),
    ("Binary", |c| Some(c.binary)),
];

/// `cost`'s source `part`, lines (0) or bytes (1), where it is split.
fn split(cost: &Cost, part: fn(&Source) -> Option<[usize; 2]>, at: usize) -> Option<usize> {
    cost.source.as_ref().and_then(part).map(|s| s[at])
}

/// Each phase a program may count, in the order it runs.
const PHASES: [&str; 3] = ["load", "decode", "expression"];

/// The phases a record passes through, summed in a measure's total.
const RECORD: [&str; 2] = ["decode", "expression"];

/// Each count of a phase, and whether its phases sum: a peak doesn't.
const COUNTS: [(&str, bool); 3] = [
    ("Allocations", true),
    ("Bytes allocated", true),
    ("Peak bytes", false),
];

/// `cost`'s `count` in `phase`, if it has that phase.
fn counted(cost: &Cost, phase: &str, count: usize) -> Option<usize> {
    let found = cost.phases.iter().find(|(p, _)| p == phase);
    found.map(|(_, c)| c[count])
}

/// `cost`'s allocation calls for a record: decode and expression.
pub(super) fn record_calls(cost: &Cost) -> usize {
    ["decode", "expression"]
        .iter()
        .filter_map(|p| counted(cost, p, 0))
        .sum()
}

/// Every measure: the program's whole, then each count by phase and, where phases sum, a record's
/// total.
fn measures() -> Vec<Measure> {
    let mut measures: Vec<Measure> = WHOLE
        .iter()
        .map(|&(name, of)| (name.to_string(), Box::new(of) as Of))
        .collect();
    for (count, (name, sums)) in COUNTS.into_iter().enumerate() {
        for phase in PHASES {
            let of = move |c: &Cost| counted(c, phase, count);
            measures.push((format!("{name}, {phase}"), Box::new(of)));
        }
        if sums {
            let total = move |c: &Cost| RECORD.iter().map(|p| counted(c, p, count)).sum();
            measures.push((format!("{name}, decode + expression"), Box::new(total)));
        }
    }
    measures
}

/// `costs`, by program, as a table with a column per program and a row per measure.
pub(super) fn table(name: &str, costs: &[(&str, Cost)]) -> String {
    let kinds: Vec<&str> = costs.iter().map(|(k, _)| *k).collect();
    let mut out = format!(
        "## {name}\n\n| Measure | {} |\n|{}\n",
        kinds.join(" | "),
        " - |".repeat(kinds.len() + 1)
    );
    for (measure, of) in measures() {
        let cells: Vec<String> = costs
            .iter()
            .map(|(_, c)| of(c).map_or("-".into(), |n| n.to_string()))
            .collect();
        writeln!(out, "| {measure} | {} |", cells.join(" | ")).unwrap();
    }
    out
}
