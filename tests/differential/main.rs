//! Differential test: each expression of `cases.rs` is evaluated by cel-core's interpreter, and
//! lowered, emitted, built natively, and run; the two must agree on the value or on being an
//! error. All cases build as one program against the prebuilt cel-emit-runtime rlib (found beside
//! this test's binary), each run under `catch_unwind`; a build that fails is retried case by case.
//! Cases in `KNOWN_FAILING` may disagree; a new disagreement fails the test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod cases;
#[path = "../common/mod.rs"]
mod common;
mod declared;
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
    fn has(&self, _: usize) -> Result<String, Error> {
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
fn function(n: usize, source: &str, emitter: &mut Emitter) -> Result<String, String> {
    let tree = cel_emit::lower(source, &declared::declared()).map_err(|e| e.to_string())?;
    // A list or map of dyn is shown as the dyn it makes.
    let tree = match &tree.ty {
        Type::List(t) | Type::Map(_, t) if **t == Type::Dyn => {
            Typed::new(Type::Dyn, Node::ToDyn(Box::new(tree)))
        }
        _ => tree,
    };
    let expr = emitter.expr(&tree, &NoVars).map_err(|e| e.to_string())?;
    let show = match tree.ty {
        Type::Int | Type::Bool => "format!(\"{}\", v)",
        Type::Uint => "format!(\"{}u\", v)",
        Type::Double => "format!(\"{:?}\", v)",
        Type::String => "format!(\"{:?}\", v)",
        Type::Dyn => "show(v)",
        ref other => return Err(format!("result of type {other:?}")),
    };
    let rust = emitter.rust(&tree.ty).map_err(|e| e.to_string())?;
    Ok(format!(
        "fn c{n}() -> String {{ let r: s::Result<{rust}> = (|| s::ok({expr}))(); \
         match r {{ Ok(v) => {show}, Err(_) => \"err\".into() }} }}\n"
    ))
}

/// How the emitted program writes a dyn, as [`show`] writes the interpreter's value.
const SHOW: &str = r#"
fn show(v: s::Dyn) -> String {
    match v {
        s::Dyn::Null => "null".into(),
        s::Dyn::Bool(b) => format!("{b}"),
        s::Dyn::Int(i) => format!("{i}"),
        s::Dyn::UInt(u) => format!("{u}u"),
        s::Dyn::Double(d) => format!("{d:?}"),
        s::Dyn::Float32(f) => format!("{:?}", f64::from(f)),
        s::Dyn::Rest(t) => format!("rest {t:?}"),
        s::Dyn::Str(t) => format!("{t:?}"),
        s::Dyn::Timestamp(t) => format!("{t:?}"),
        s::Dyn::Duration(d) => format!("{d:?}"),
        s::Dyn::List(xs) => {
            let xs: Vec<String> = xs.into_iter().map(show).collect();
            format!("[{}]", xs.join(","))
        }
        s::Dyn::Map(m) => {
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

#[test]
fn emitted_code_agrees_with_the_interpreter() {
    let scratch = std::env::temp_dir().join(format!("cel-emit-diff-{}", std::process::id()));
    let mut emitter = Emitter::new(&[], Box::new(common::Positional));
    let mut wrong: BTreeMap<&str, String> = BTreeMap::new();
    let mut fns = Vec::new();
    let all: Vec<&str> = cases::CASES
        .iter()
        .chain(messages::CASES)
        .chain(shaped::CASES)
        .copied()
        .collect();
    for (n, source) in all.iter().enumerate() {
        match function(n, source, &mut emitter) {
            Ok(f) => fns.push((n, f)),
            // A refusal is by design (README: refused, never stubbed): a gap, not a mismatch.
            Err(e) if e.starts_with("unsupported") => eprintln!("refused: {source}: {e}"),
            Err(e) => {
                wrong.insert(source, format!("not emitted: {e}"));
            }
        }
    }
    let items = common::items(&mut emitter).unwrap();
    let mut answers: BTreeMap<usize, String> = BTreeMap::new();
    let mut collect = |lines: Vec<String>| {
        for l in lines {
            let (n, v) = l.split_once('\t').unwrap();
            answers.insert(n.parse().unwrap(), v.to_string());
        }
    };
    match common::build_run(&scratch.join("all"), &common::program(&fns, SHOW, &items)) {
        Ok(lines) => collect(lines),
        Err(_) => {
            for f in &fns {
                match common::build_run(
                    &scratch.join(f.0.to_string()),
                    &common::program(std::slice::from_ref(f), SHOW, &items),
                ) {
                    Ok(lines) => collect(lines),
                    Err(e) => {
                        wrong.insert(
                            all[f.0],
                            format!("rustc: {}", e.lines().next().unwrap_or("")),
                        );
                    }
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
    for (n, got) in &answers {
        let source = all[*n];
        let want = reference(source);
        if *got != want {
            wrong.insert(source, format!("emitted {got}, interpreter {want}"));
        }
    }
    for source in cases::KNOWN_FAILING {
        if !wrong.contains_key(source) {
            eprintln!("agrees now; remove from KNOWN_FAILING: {source}");
        }
    }
    let new: Vec<String> = wrong
        .iter()
        .filter(|(s, _)| !cases::KNOWN_FAILING.contains(s))
        .map(|(s, why)| format!("{s}\n    {why}"))
        .collect();
    for (s, why) in &wrong {
        eprintln!("disagrees: {s}: {why}");
    }
    assert!(new.is_empty(), "new disagreements:\n{}", new.join("\n"));
}
