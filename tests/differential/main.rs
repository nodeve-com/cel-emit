//! Differential test: each expression of `cases.rs` is evaluated by cel-core's interpreter, and
//! lowered, emitted, built natively, and run; the two must agree on the value or on being an
//! error. All cases build as one program against the prebuilt cel-emit-runtime rlib (found beside
//! this test's binary), each run under `catch_unwind`; a build that fails is retried case by case.
//! Cases in `KNOWN_FAILING` or `INTENDED_DEPARTURES` may disagree, and those in `REFUSED` are
//! refused; any other disagreement or refusal fails the test, as does an entry in `KNOWN_FAILING`
//! that agrees or in `REFUSED` that emits.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod cases;
#[path = "../common/mod.rs"]
mod common;
mod declared;
mod exceptions;
mod messages;
mod shaped;

use std::collections::BTreeMap;

use cel_core::Value;
use cel_emit::emit::{Emitter, Vars};
use cel_emit::{Decls, Error, Node, Type, Typed};

struct NoVars;

impl Vars for NoVars {
    fn read(&self, _: usize) -> Result<String, Error> {
        Err(Error::Invalid("no inputs".into()))
    }
}

/// What the interpreter says: `err`, or the value as the emitted program writes it. A list is
/// `[a,b]` and a map `{"k":v}` by key; null is `null`.
fn reference(source: &str) -> String {
    let source = messages::REFERENCES
        .iter()
        .chain(shaped::REFERENCES)
        .find(|(s, _)| *s == source)
        .map_or(source, |(_, r)| *r);
    let env = cel_emit::env::environment(&Decls::default());
    let ast = cel_emit::macros::compile(&env, source).unwrap_or_else(|e| panic!("{source}: {e:?}"));
    show(&env.program(&ast).unwrap().eval(&NoNames)).unwrap_or_else(|| "err".into())
}

fn show(v: &Value) -> Option<String> {
    Some(match v {
        Value::Null => "null".into(),
        Value::Int(v) => format!("{v}"),
        Value::UInt(v) => format!("{v}u"),
        Value::Double(v) => format!("{v:?}"),
        Value::Bool(v) => format!("{v}"),
        Value::String(v) => format!("{:?}", &**v),
        Value::List(xs) => {
            let xs: Option<Vec<String>> = xs.iter().map(show).collect();
            format!("[{}]", xs?.join(","))
        }
        Value::Map(m) => {
            let mut entries = Vec::new();
            for (k, v) in m.iter() {
                let cel_core::MapKey::String(k) = k else {
                    panic!("a map keyed by other than strings");
                };
                entries.push(format!("{:?}:{}", &**k, show(v)?));
            }
            entries.sort();
            format!("{{{}}}", entries.join(","))
        }
        Value::Error(_) => return None,
        other => panic!("not a plain value: {other:?}"),
    })
}

struct NoNames;

impl cel_core::Activation for NoNames {
    fn resolve(&self, _: &str) -> Option<Value> {
        None
    }
}

/// The Rust of one case, a function `c{n}` returning its `show`n result, or why it has none.
fn function(n: usize, source: &str, emitter: &mut Emitter) -> Result<String, Error> {
    let tree = cel_emit::lower(source, &declared::declared())?;
    // A list or map of dyn is shown as the dyn it makes.
    let tree = match &tree.ty {
        Type::List(t) | Type::Map(_, t) if **t == Type::Dyn => {
            Typed::new(Type::Dyn, Node::ToDyn(Box::new(tree)))
        }
        _ => tree,
    };
    let expr = emitter.expr(&tree, &NoVars)?;
    let show = match tree.ty {
        Type::Int | Type::Bool => "format!(\"{}\", v)",
        Type::Uint => "format!(\"{}u\", v)",
        Type::Double => "format!(\"{:?}\", v)",
        Type::String => "format!(\"{:?}\", v)",
        Type::Dyn => "show(v)",
        ref other => return Err(Error::Invalid(format!("result of type {other:?}"))),
    };
    let rust = emitter.rust(&tree.ty)?;
    Ok(format!(
        "fn c{n}() -> String {{ let r: s::Result<{rust}> = (|| s::ok({expr}))(); \
         match r {{ Ok(v) => {show}, Err(_) => \"err\".into() }} }}\n"
    ))
}

/// How the emitted program writes a dyn, as [`show`] writes the interpreter's value.
const SHOW: &str = r#"
fn show(v: s::Value) -> String {
    match v {
        s::Value::Null => "null".into(),
        s::Value::Bool(b) => format!("{b}"),
        s::Value::Int(i) => format!("{i}"),
        s::Value::UInt(u) => format!("{u}u"),
        s::Value::Double(d) => format!("{d:?}"),
        s::Value::Float32(f) => format!("{:?}", f64::from(f)),
        s::Value::Str(t) => format!("{t:?}"),
        s::Value::Timestamp(t) => format!("{t:?}"),
        s::Value::Duration(d) => format!("{d:?}"),
        s::Value::List(xs) => {
            let xs: Vec<String> = xs.into_iter().map(show).collect();
            format!("[{}]", xs.join(","))
        }
        s::Value::Map(m) => {
            let mut e: Vec<String> = m
                .into_iter()
                .map(|(k, v)| format!("{k:?}:{}", show(v)))
                .collect();
            e.sort();
            format!("{{{}}}", e.join(","))
        }
    }
}
"#;

type Wrong<'a> = BTreeMap<&'a str, String>;

#[test]
fn emitted_code_agrees_with_the_interpreter() {
    let scratch = std::env::temp_dir().join(format!("cel-emit-diff-{}", std::process::id()));
    let mut emitter = Emitter::new(&[], Box::new(common::Positional));
    let mut wrong = Wrong::new();
    let all: Vec<&str> = cases::CASES
        .iter()
        .chain(messages::CASES)
        .chain(shaped::CASES)
        .copied()
        .collect();
    let (fns, stale) = emit_all(&all, &mut emitter, &mut wrong);
    let items = common::items(&mut emitter).unwrap();
    let (answers, failed) = run_all(&fns, &items, &scratch);
    for (n, why) in failed {
        wrong.insert(all[n], why);
    }
    let _ = std::fs::remove_dir_all(&scratch);
    for (n, got) in &answers {
        let source = all[*n];
        let want = reference(source);
        if *got != want {
            wrong.insert(source, format!("emitted {got}, interpreter {want}"));
        }
    }
    report(&wrong, stale);
}

/// Each of `all` emitted as a function, and each case of `REFUSED` that emits; a failure other than
/// a listed refusal is `wrong`.
fn emit_all<'a>(
    all: &[&'a str],
    emitter: &mut Emitter,
    wrong: &mut Wrong<'a>,
) -> (Vec<(usize, String)>, Vec<String>) {
    let mut fns = Vec::new();
    let mut stale = Vec::new();
    for (n, source) in all.iter().enumerate() {
        let refused = exceptions::REFUSED.contains(source);
        match function(n, source, emitter) {
            Ok(f) => {
                if refused {
                    stale.push(format!("emits now; remove from REFUSED: {source}"));
                }
                fns.push((n, f));
            }
            Err(Error::Unsupported(_)) if refused => {}
            Err(e) => {
                wrong.insert(source, format!("not emitted: {e}"));
            }
        }
    }
    (fns, stale)
}

/// The answer of each function in `fns`, built together, else one at a time; a program that
/// fails to build gives its case's number and why.
fn run_all(
    fns: &[(usize, String)],
    items: &str,
    scratch: &std::path::Path,
) -> (BTreeMap<usize, String>, Vec<(usize, String)>) {
    let mut answers = BTreeMap::new();
    let mut failed = Vec::new();
    let mut collect = |lines: Vec<String>| {
        for l in lines {
            let (n, v) = l.split_once('\t').unwrap();
            answers.insert(n.parse().unwrap(), v.to_string());
        }
    };
    if let Ok(lines) = common::build_run(&scratch.join("all"), &common::program(fns, SHOW, items)) {
        collect(lines);
        return (answers, failed);
    }
    for f in fns {
        let program = common::program(std::slice::from_ref(f), SHOW, items);
        match common::build_run(&scratch.join(f.0.to_string()), &program) {
            Ok(lines) => collect(lines),
            Err(e) => failed.push((f.0, format!("rustc: {}", e.lines().next().unwrap_or("")))),
        }
    }
    (answers, failed)
}

/// Prints each disagreement, and fails on those not known and on `stale` entries of the lists.
fn report(wrong: &Wrong, mut stale: Vec<String>) {
    for source in exceptions::KNOWN_FAILING {
        if !wrong.contains_key(source) {
            stale.push(format!("agrees now; remove from KNOWN_FAILING: {source}"));
        }
    }
    let new: Vec<String> = wrong
        .iter()
        .filter(|(s, _)| {
            !exceptions::KNOWN_FAILING.contains(s) && !exceptions::INTENDED_DEPARTURES.contains(s)
        })
        .map(|(s, why)| format!("{s}\n    {why}"))
        .collect();
    for (s, why) in wrong {
        eprintln!("disagrees: {s}: {why}");
    }
    assert!(new.is_empty(), "new disagreements:\n{}", new.join("\n"));
    assert!(stale.is_empty(), "{}", stale.join("\n"));
}
