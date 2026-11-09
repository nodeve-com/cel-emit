//! Cases of CEL semantics: an expression over bindings, and what it must give. [`check`] lowers
//! every case, emits it, builds all of them as one native program against the prebuilt
//! cel-emit-runtime rlib, runs it, and compares each answer with what the case expects.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use cel_core::{Activation, Value};
use cel_emit::emit::Emitter;
use cel_emit::{Body, Decls, Error, Name, Type, Written};
use cel_emit_runtime as s;

mod build;

use crate::common::bind::{answers, show, Bound};
use crate::common::values::{fits, infer, rust, show_with};
use crate::common::{build_run, program};
use build::Build;

/// What a case gives: a value, or an error.
pub enum Got {
    Value(Value),
    Error,
    /// An error of this kind.
    Status(s::Error),
}

enum Want {
    /// What cel-core gives.
    Reference,
    /// This, which cel-core does not give.
    Exactly(Got),
    /// No value: a fold of nothing.
    Absent,
}

struct Case {
    source: String,
    /// The bindings' types, where their values don't say.
    types: Option<Vec<(String, Type)>>,
    /// The type the expression returns, where the case gives one.
    result: Option<Type>,
    /// One program run takes these in order, sharing what the emitted code keeps between calls.
    messages: Vec<Vec<(String, Value)>>,
    want: Want,
    /// Lowered with [`Decls::skip_failed`].
    skip_failed: bool,
}

/// A value of a stored type.
pub struct Stored {
    ty: Type,
    value: Value,
}

impl Stored {
    pub fn new(ty: Type, value: Value) -> Stored {
        Stored { ty, value }
    }
}

/// Cases to check together, and the messages they build; [`check`] builds and compares them.
#[derive(Default)]
pub struct Cases(Vec<Case>, BTreeMap<String, Type>, Vec<Written>);

impl Cases {
    fn push(&mut self, source: &str, messages: &[Vec<(&str, Value)>], want: Want) {
        let messages = messages
            .iter()
            .map(|r| r.iter().map(|(n, v)| (n.to_string(), v.clone())).collect())
            .collect();
        self.0.push(Case {
            source: source.into(),
            types: None,
            result: None,
            messages,
            want,
            skip_failed: false,
        });
    }

    /// `source` over `inputs` gives what cel-core does.
    pub fn agrees(&mut self, source: &str, inputs: &[(&str, Value)]) {
        self.push(source, &[inputs.to_vec()], Want::Reference);
    }

    /// `source` over `inputs` gives `want`, where cel-emit follows cel-go over cel-core.
    pub fn gives(&mut self, source: &str, inputs: &[(&str, Value)], want: Got) {
        self.push(source, &[inputs.to_vec()], Want::Exactly(want));
    }

    /// `source` over `inputs`, returning `ty`, gives `want`.
    pub fn gives_as(&mut self, (source, ty): (&str, Type), inputs: &[(&str, Value)], want: Got) {
        self.gives(source, inputs, want);
        if let Some(case) = self.0.last_mut() {
            case.result = Some(ty);
        }
    }

    /// `source` over `inputs`, lowered with [`Decls::skip_failed`], gives `want`.
    pub fn gives_skipping(&mut self, source: &str, inputs: &[(&str, Value)], want: Got) {
        self.gives(source, inputs, want);
        if let Some(case) = self.0.last_mut() {
            case.skip_failed = true;
        }
    }

    /// `source` over `inputs` holds no value.
    pub fn absent(&mut self, source: &str, inputs: &[(&str, Value)]) {
        self.push(source, &[inputs.to_vec()], Want::Absent);
    }

    /// A message the cases may build or read by `name`.
    pub fn message(&mut self, name: &str, ty: Type) {
        self.1.insert(name.into(), ty);
    }

    /// A function the cases may call, written in CEL.
    pub fn function(&mut self, w: Written) {
        self.2.push(w);
    }

    /// `source` over `v`, a stored value, gives `want`.
    pub fn stored(&mut self, source: &str, v: Stored, want: Got) {
        self.push(source, &[vec![("v", v.value)]], Want::Exactly(want));
        if let Some(case) = self.0.last_mut() {
            case.types = Some(vec![("v".into(), v.ty)]);
        }
    }

    /// `source` over `v`, a stored value of `ty`, holds no value.
    pub fn stored_absent(&mut self, source: &str, ty: Type, v: Value) {
        self.push(source, &[vec![("v", v)]], Want::Absent);
        if let Some(case) = self.0.last_mut() {
            case.types = Some(vec![("v".into(), ty)]);
        }
    }

    /// `source` over each of `each` in order, in one run, each giving what cel-core does.
    pub fn messages(&mut self, source: &str, each: &[Vec<(&str, Value)>]) {
        self.push(source, each, Want::Reference);
    }
}

pub fn s(v: &str) -> Value {
    Value::String(v.into())
}

pub fn list(items: Vec<Value>) -> Value {
    Value::list(items)
}

pub fn map(entries: &[(&str, Value)]) -> Value {
    Value::map(entries.to_vec())
}

/// Separates a case's messages in an answer.
const SEP: char = '\u{1f}';

/// Doubles to the bit, as `{:?}` writes them: a negative zero stays one.
const PRELUDE: &str = "fn dbl(d: f64) -> String { format!(\"{d:?}\") }\n";

fn exact(d: f64) -> String {
    format!("{d:?}")
}

struct Names(HashMap<String, Value>);

impl Activation for Names {
    fn resolve(&self, name: &str) -> Option<Value> {
        self.0.get(name).cloned()
    }
}

/// The types of a case's bindings, an empty list's or map's elements doubles.
fn declared(case: &Case) -> Vec<(String, Type)> {
    if let Some(types) = &case.types {
        return types.clone();
    }
    let first = case.messages.first().unwrap();
    first
        .iter()
        .map(|(n, v)| (n.clone(), infer(v, Some(&Type::Double)).unwrap()))
        .collect()
}

/// What cel-core gives for `source` over `message`, as a native program writes it.
fn reference(source: &str, types: &[(String, Type)], message: &[(String, Value)]) -> String {
    let names: BTreeMap<String, Name> = types
        .iter()
        .enumerate()
        .map(|(i, (n, t))| (n.clone(), Name::var(t.clone(), i)))
        .collect();
    let env = cel_emit::env::environment(&Decls::new(names));
    let ast = cel_emit::macros::compile(&env, source).unwrap();
    let program = env.program(&ast).unwrap();
    let names = Names(message.iter().cloned().collect());
    match program.eval(&names) {
        Value::Error(_) => "err".into(),
        v => show_with(&v, exact),
    }
}

/// What case `case` must answer.
fn expected(case: &Case, types: &[(String, Type)]) -> String {
    let each: Vec<String> = case
        .messages
        .iter()
        .map(|message| match &case.want {
            Want::Reference => reference(&case.source, types, message),
            Want::Exactly(Got::Error) => "err".into(),
            Want::Exactly(Got::Status(e)) => format!("err {e:?}"),
            Want::Exactly(Got::Value(v)) => show_with(v, exact),
            Want::Absent => "absent".into(),
        })
        .collect();
    each.join(&SEP.to_string())
}

/// The cases `fill` adds, built as one native program and checked: every case that differs is
/// named, with what it should give and what it gave.
pub fn check(fill: impl FnOnce(&mut Cases)) {
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    let mut cases = Cases::default();
    fill(&mut cases);
    let types: Vec<_> = cases.0.iter().map(declared).collect();
    let mut base = Decls::default();
    base.messages.clone_from(&cases.1);
    let bodies = cel_emit::written::check(&cases.2, &base).unwrap_or_else(|e| panic!("{e}"));
    let mut build = Build {
        messages: cases.1.clone(),
        bodies,
        em: Emitter::new(&[], Box::new(crate::common::Positional)),
    };
    build.define();
    let fns: Vec<(usize, String)> = cases
        .0
        .iter()
        .zip(&types)
        .enumerate()
        .map(|(n, (case, types))| (n, build.function(n, case, types)))
        .collect();
    let items = crate::common::items(&mut build.em).unwrap_or_else(|e| panic!("{e}"));
    let dir: PathBuf = std::env::temp_dir().join(format!(
        "cel-emit-semantics-{}-{}",
        std::process::id(),
        RUNS.fetch_add(1, Ordering::Relaxed)
    ));
    let lines = build_run(&dir, &program(&fns, PRELUDE, &items));
    let _ = std::fs::remove_dir_all(&dir);
    let got: BTreeMap<usize, String> = answers(lines.unwrap_or_else(|e| panic!("rustc: {e}")))
        .into_iter()
        .collect();
    let wrong = mismatches(&cases.0, &types, &got);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// A line for each case whose answer in `got` differs from what it expects.
fn mismatches(
    cases: &[Case],
    types: &[Vec<(String, Type)>],
    got: &BTreeMap<usize, String>,
) -> Vec<String> {
    let mut wrong = Vec::new();
    for (n, (case, types)) in cases.iter().zip(types).enumerate() {
        let want = expected(case, types);
        match got.get(&n) {
            Some(g) if *g == want => {}
            other => wrong.push(format!(
                "{} {:?}\n    want {want:?}\n    got  {other:?}",
                case.source, case.messages
            )),
        }
    }
    wrong
}

/// Why `source` over `names` does not lower, or `None` where it does.
pub fn refusal(source: &str, names: &[(&str, Type)]) -> Option<Error> {
    let names = names
        .iter()
        .enumerate()
        .map(|(i, (n, t))| (n.to_string(), Name::var(t.clone(), i)))
        .collect();
    cel_emit::lower(source, &Decls::new(names)).err()
}
