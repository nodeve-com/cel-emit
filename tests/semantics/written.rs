//! A function written in CEL: typed against its parameters, run as one Rust function, and an error
//! inside it the call's error.

use cel_core::Value;
use cel_emit::emit::Emitter;
use cel_emit::{lower, Decls, Error, Name, Type, Written};
use cel_emit_runtime::Error as Fail;

use crate::cases::{check, list, s, Got};
use crate::common::bind::Bound;
use crate::common::Positional;

fn written(name: &str, params: &[(&str, Type)], cel: &str) -> Written {
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

fn decls(functions: Vec<Written>) -> Decls {
    let mut decls = Decls::default();
    decls.names.insert("n".into(), Name::var(Type::Int, 0));
    decls.written = functions;
    decls
}

/// Only here: a call with arguments of other types, or another count, is refused.
#[test]
fn a_wrong_argument_is_refused() {
    let decls = decls(vec![written("twice", &[("x", Type::Int)], "x * 2")]);
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
    let decls = decls(vec![written("leaky", &[("x", Type::Int)], "x + n")]);
    assert!(matches!(lower("leaky(1)", &decls), Err(Error::Invalid(_))));
    let twice = written("twice", &[("x", Type::Int)], "x");
    let decls = self::decls(vec![twice.clone(), twice]);
    assert!(matches!(lower("n", &decls), Err(Error::Invalid(_))));
}

/// Only here: functions that call each other, in a loop or by themselves, are refused, naming them.
#[test]
fn a_cycle_is_refused() {
    let a = written("a", &[("x", Type::Int)], "b(x)");
    let b = written("b", &[("x", Type::Int)], "a(x)");
    let own = written("own", &[("x", Type::Int)], "own(x)");
    for (functions, named) in [(vec![a, b], "a -> b -> a"), (vec![own], "own -> own")] {
        match lower("n", &decls(functions)) {
            Err(Error::Invalid(m)) => assert!(m.contains(named), "{m}"),
            other => panic!("{other:?}"),
        }
    }
}

/// Only here: a function many expressions call is one Rust function.
#[test]
fn a_function_called_twice_is_emitted_once() {
    let decls = decls(vec![written("twice", &[("x", Type::Int)], "x * 2")]);
    let mut emitter = Emitter::new(&[], Box::new(Positional));
    emitter.define(&decls).unwrap();
    for source in ["twice(n) + twice(n)", "twice(3)"] {
        let tree = lower(source, &decls).unwrap();
        let code = emitter.expr(&tree, &Bound(vec!["n".into()])).unwrap();
        assert!(code.contains("twice("), "{code}");
    }
    let items = emitter.items().unwrap().code;
    assert_eq!(items.matches("fn twice(").count(), 1, "{items}");
}
