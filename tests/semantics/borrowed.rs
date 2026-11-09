//! A written function whose body is a part of its parameters returns it by reference.

use cel_emit::{Field, Type};

use crate::cases::{check, list, map, s, Got, Stored};
use crate::written::{emitted, written};

fn place() -> Type {
    let field = |name: &str| Field {
        name: name.into(),
        ty: Type::String,
        required: true,
    };
    Type::message(vec![field("id"), field("name")])
}

fn places() -> Type {
    Type::List(Box::new(place()))
}

/// The functions: a lookup, a read through it, a choice between two parts, and a new value.
fn functions() -> Vec<cel_emit::Written> {
    let lookup = [("ps", places()), ("id", Type::String)];
    vec![
        written("pick", &lookup, "ps.filter(p, p.id == id)[0]"),
        written("name_of", &lookup, "pick(ps, id).name"),
        written(
            "either",
            &[("a", place()), ("b", place()), ("first", Type::Bool)],
            "first ? a.name : b.name",
        ),
        written("greet", &[("p", place())], "p.name + '!'"),
    ]
}

fn inputs() -> [(&'static str, Type); 3] {
    [("ps", places()), ("id", Type::String), ("p", place())]
}

/// Only here: a body that is a part of its parameters returns it by one lifetime shared with
/// them, through the functions it calls; one that builds a value returns it owned.
#[test]
fn a_part_of_a_parameter_is_returned_by_reference() {
    let (_, items) = emitted(&functions(), &inputs(), &[]);
    let signature = |name: &str| {
        let at = items.find(&format!("fn {name}")).unwrap();
        items[at..].split(" { ").next().unwrap().to_string()
    };
    for name in ["pick", "name_of", "either"] {
        let sig = signature(name);
        assert!(sig.contains("<'a>") && sig.contains("Result<&'a "), "{sig}");
    }
    let sig = signature("greet");
    assert!(
        !sig.contains("'a") && sig.contains("Result<String>"),
        "{sig}"
    );
}

/// Only here: a read through a borrowed result copies nothing; a use needing an owned value
/// clones there.
#[test]
fn a_caller_reads_a_borrowed_result_as_a_place() {
    let sources = [
        "pick(ps, id).name == 'a'",
        "size(name_of(ps, id))",
        "pick(ps, id)",
        "[name_of(ps, id)]",
    ];
    let (calls, _) = emitted(&functions(), &inputs(), &sources);
    for through in &calls[..2] {
        assert!(!through.contains("clone()"), "{through}");
    }
    for owned in &calls[2..] {
        assert!(owned.contains("clone()"), "{owned}");
    }
}

/// Only here: the borrowed results run: a lookup, a nested read, a choice, and a computed value.
#[test]
fn borrowed_and_computed_results_give_the_same_values() {
    check(|cases| {
        for f in functions() {
            cases.function(f);
        }
        cases.message("P", place());
        let row = |id: &str, name: &str| map(&[("id", s(id)), ("name", s(name))]);
        let ps = list(vec![row("a", "one"), row("b", "two")]);
        let with = |body: &str| format!("cel.bind(p, P{{id: 'c', name: 'three'}}, {body})");
        let value = |v: &str| Got::Value(s(v));
        for (source, want) in [
            ("name_of(v, 'b')", value("two")),
            ("pick(v, 'a').name + pick(v, 'b').id", value("oneb")),
            ("either(pick(v, 'a'), p, true)", value("one")),
            ("either(pick(v, 'a'), p, false)", value("three")),
            ("greet(p)", value("three!")),
        ] {
            cases.stored(&with(source), Stored::new(places(), ps.clone()), want);
        }
    });
}
