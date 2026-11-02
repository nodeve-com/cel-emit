//! A chunk of cases as one native program: each case a function writing its result as text,
//! built against the prebuilt cel-emit-runtime rlib and run. A chunk that doesn't build is retried a
//! case at a time, to find which.

use std::path::Path;

use cel_emit::emit::Emitter;
use cel_emit::Error;

use super::case::{Case, Prepared};
use super::common::bind::{answers, show, Bound};
use super::common::values;
use super::{common, Verdict};

/// What a program wrote for one case, or why there is none.
pub type Answer = Result<String, Verdict>;

/// Separates a message's leaves in an answer.
pub const LEAF: char = '\u{1f}';

/// The Rust of case `n`, a function `c{n}` returning its answer, or why it has none.
fn function(n: usize, case: &Case, em: &mut Emitter) -> Result<String, Verdict> {
    let wrong = |e: Error| match e {
        Error::Unsupported(m) => Verdict::Unsupported(m),
        Error::Invalid(m) => Verdict::Wrong(format!("emit: {m}")),
    };
    let bound = Bound(
        case.inputs
            .iter()
            .map(|(_, ty, v)| values::rust(v, ty, em))
            .collect(),
    );
    let mut body = String::new();
    for (path, tree) in &case.leaves {
        let expr = em.expr(tree, &bound).map_err(wrong)?;
        let rust = em.rust(&tree.ty).map_err(wrong)?;
        let shown = show(&tree.ty, "v", 0);
        let name = path.join(".");
        let push = if case.message {
            format!("out.push(format!(\"{{}}={{}}\", {name:?}, t))")
        } else {
            "out.push(t)".into()
        };
        body.push_str(&format!(
            "    match (|| -> s::Result<String> {{ let v: {rust} = {expr}; s::ok({shown}) }})() \
             {{ Ok(t) => {push}, Err(_) => return \"err\".into() }}\n"
        ));
    }
    Ok(format!(
        "fn c{n}() -> String {{\n    let mut out: Vec<String> = Vec::new();\n{body}    out.join(\"{LEAF}\")\n}}\n"
    ))
}

const PRELUDE: &str = r#"
fn dbl(d: f64) -> String {
    if d == 0.0 { "0.0".into() } else { format!("{d:?}") }
}
"#;

/// `chunk`'s cases built and run: one answer each.
pub fn build(scratch: &Path, chunk: &[Prepared]) -> Vec<Answer> {
    let mut em = Emitter::new(&[], Box::new(common::Positional));
    let mut done: Vec<Option<Answer>> = chunk.iter().map(|_| None).collect();
    let mut fns = Vec::new();
    for (n, (_, _, case)) in chunk.iter().enumerate() {
        match function(n, case, &mut em) {
            Ok(f) => fns.push((n, f)),
            Err(v) => done[n] = Some(Err(v)),
        }
    }
    let items = match common::items(&mut em) {
        Ok(items) => items,
        Err(e) => {
            return chunk
                .iter()
                .map(|_| Err(Verdict::Wrong(e.to_string())))
                .collect()
        }
    };
    let run =
        |tag: &str, fns: &[(usize, String)], done: &mut [Option<Answer>]| match common::build_run(
            &scratch.join(tag),
            &common::program(fns, PRELUDE, &items),
        ) {
            Ok(lines) => {
                for (n, a) in answers(lines) {
                    done[n] = Some(Ok(a));
                }
            }
            Err(e) if fns.len() == 1 => {
                let why = e.lines().find(|l| l.starts_with("error")).unwrap_or("");
                done[fns[0].0] = Some(Err(Verdict::Wrong(format!("rustc: {why}"))));
            }
            Err(_) => {}
        };
    run("all", &fns, &mut done);
    // A build that failed, or a run that stopped early, is found out a case at a time.
    for f in fns {
        if done[f.0].is_none() {
            run(&f.0.to_string(), std::slice::from_ref(&f), &mut done);
        }
    }
    done.into_iter()
        .map(|d| d.unwrap_or_else(|| Err(Verdict::Wrong("the program stopped".into()))))
        .collect()
}
