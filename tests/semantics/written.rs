//! A function written in CEL: typed against its parameters, run as one Rust function, and an error
//! inside it the call's error.

use cel_core::Value;
use cel_emit::emit::Emitter;
use cel_emit::written::check as checked;
use cel_emit::{lower, Decls, Error, Name, Type, Written};
use cel_emit_runtime::Error as Fail;

use crate::cases::{check, list, s, Got};
use crate::common::bind::Bound;
use crate::common::Positional;

pub(crate) fn written(name: &str, params: &[(&str, Type)], cel: &str) -> Written {
    Written {
        name: name.into(),
        params: params
            .iter()
            .map(|(n, t)| (n.to_string(), t.clone()))
            .collect(),
        cel: cel.into(),
        path: name.into(),
    }
}

fn ints() -> Type {
    Type::List(Box::new(Type::Int))
}

/// Only here: a call is typed by the body and runs it, over copied, text, and list parameters; a
/// body calls another.
#[test]
fn a_call_runs_its_body() {
    check(|cases| {
        cases.function(written("twice", &[("x", Type::Int)], "x * 2"));
        cases.function(written("shout", &[("t", Type::String)], "t + '!'"));
        cases.function(written(
            "big",
            &[("xs", ints())],
            "xs.filter(x, x > 1).size()",
        ));
        cases.function(written("quad", &[("x", Type::Int)], "twice(twice(x))"));
        let xs = list(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let inputs = [("n", Value::Int(5)), ("t", s("hi")), ("xs", xs)];
        for (source, want) in [
            ("twice(n) + twice(4)", Value::Int(18)),
            ("shout(t)", s("hi!")),
            ("big(xs)", Value::Int(2)),
            ("quad(n)", Value::Int(20)),
        ] {
            cases.gives(source, &inputs, Got::Value(want));
        }
    });
}

/// Only here: an error in the body is the call's error.
#[test]
fn an_error_in_the_body_fails_the_call() {
    check(|cases| {
        let params = [("a", Type::Int), ("b", Type::Int)];
        cases.function(written("share", &params, "a / b"));
        let inputs = [("n", Value::Int(5)), ("z", Value::Int(0))];
        cases.gives("share(n, z)", &inputs, Got::Status(Fail::Division));
        cases.gives("share(n, 5)", &inputs, Got::Value(Value::Int(1)));
    });
}

/// `decls` reading `n`, with `functions` checked and declared, or why not.
fn declared(functions: &[Written]) -> Result<Decls, Error> {
    let mut decls = Decls::default();
    decls.names.insert("n".into(), Name::var(Type::Int, 0));
    decls.define(&checked(functions, &decls)?);
    Ok(decls)
}

/// Only here: a call with arguments of other types, or another count, is refused.
#[test]
fn a_wrong_argument_is_refused() {
    let decls = declared(&[written("twice", &[("x", Type::Int)], "x * 2")]).unwrap();
    for source in ["twice('a')", "twice(n, n)", "twice()"] {
        assert!(
            matches!(lower(source, &decls), Err(Error::Invalid(_))),
            "{source}"
        );
    }
    assert_eq!(lower("twice(n)", &decls).unwrap().ty, Type::Int);
}

/// Only here: a body sees its parameters and nothing else, and a name is written once.
#[test]
fn a_body_reads_only_its_parameters() {
    let leaky = written("leaky", &[("x", Type::Int)], "x + n");
    assert!(matches!(declared(&[leaky]), Err(Error::Invalid(_))));
    let twice = written("twice", &[("x", Type::Int)], "x");
    assert!(matches!(
        declared(&[twice.clone(), twice]),
        Err(Error::Invalid(_))
    ));
}

/// Only here: functions that call each other, in a loop or by themselves, are refused, naming them.
#[test]
fn a_cycle_is_refused() {
    let a = written("a", &[("x", Type::Int)], "b(x)");
    let b = written("b", &[("x", Type::Int)], "a(x)");
    let own = written("own", &[("x", Type::Int)], "own(x)");
    for (functions, named) in [(vec![a, b], "a -> b -> a"), (vec![own], "own -> own")] {
        match declared(&functions) {
            Err(Error::Invalid(m)) => assert!(m.contains(named), "{m}"),
            other => panic!("{:?}", other.err()),
        }
    }
}

/// The Rust of each `source` over `inputs`, in an emitter holding `functions`, then its items.
pub(crate) fn emitted(
    functions: &[Written],
    inputs: &[(&str, Type)],
    sources: &[&str],
) -> (Vec<String>, String) {
    let mut decls = Decls::default();
    for (i, (name, ty)) in inputs.iter().enumerate() {
        decls
            .names
            .insert(name.to_string(), Name::var(ty.clone(), i));
    }
    let bodies = checked(functions, &decls).unwrap();
    decls.define(&bodies);
    let mut emitter = Emitter::new(&[], Box::new(Positional));
    emitter.define(&bodies);
    let bound = Bound(inputs.iter().map(|(name, _)| name.to_string()).collect());
    let calls = sources.iter().map(|source| {
        let tree = lower(source, &decls).unwrap();
        emitter.expr(&tree, &bound).unwrap()
    });
    let calls: Vec<String> = calls.collect();
    (calls, emitter.items().unwrap().code)
}

/// Only here: a function many expressions call is one Rust function.
#[test]
fn a_function_called_twice_is_emitted_once() {
    let twice = written("twice", &[("x", Type::Int)], "x * 2");
    let (calls, items) = emitted(
        &[twice],
        &[("n", Type::Int)],
        &["twice(n) + twice(n)", "twice(3)"],
    );
    assert!(calls.iter().all(|c| c.contains("twice(")), "{calls:?}");
    assert_eq!(items.matches("fn twice(").count(), 1, "{items}");
}
