//! What translating computed properties refuses, each naming the property.

use cel_emit::computed::{Compute, Expression};
use cel_emit::{Error, Name, Type, Typed};

use super::{compute, decls, record, refusal};

fn refused(computes: &[Compute]) -> String {
    match refusal(&record(), &decls(), computes) {
        Error::Invalid(message) => message,
        e => panic!("{e}"),
    }
}

/// Only here: an expression gives its property's type, reads only names there are, and fills a
/// property the record has, once.
#[test]
fn a_computed_property_fills_one_property_of_its_type() {
    assert!(refused(&[compute("total", "'six'")]).starts_with("total: "));
    assert!(refused(&[compute("total", "nothing + 1")]).starts_with("total: "));
    assert_eq!(refused(&[compute("none", "1")]), "none: no such property");
    let twice = [compute("total", "1"), compute("total", "2")];
    assert_eq!(refused(&twice), "total: computed twice");
    let tree = Compute {
        expression: Expression::Tree(Typed::string("six")),
        ..compute("total", "")
    };
    assert_eq!(refused(&[tree]), "total: a tree of another type");
}

/// Only here: a cycle through reads or `after` fails, naming its properties, as does an `after`
/// naming no property.
#[test]
fn a_cycle_or_an_unknown_after_fails() {
    let after = |key: &str, cel: &str, after: &str| Compute {
        after: vec![after.into()],
        ..compute(key, cel)
    };
    let cycle = [compute("total", "early"), after("early", "1", "total")];
    assert_eq!(
        refused(&cycle),
        "computed properties depend on each other: early -> total -> early"
    );
    assert_eq!(
        refused(&[after("early", "1", "none")]),
        "early: after none, no property"
    );
}

/// Only here: a name two properties share, or a property and one of the caller's names, fails.
#[test]
fn a_shared_name_fails() {
    let mut twice = record();
    let again = twice.properties[0].clone();
    twice.properties.push(again);
    let error = refusal(&twice, &decls(), &[]);
    assert_eq!(error.to_string(), "invalid: text: a name two things share");
    let mut own = decls();
    own.names.insert("text".into(), Name::var(Type::Int, 0));
    let error = refusal(&record(), &own, &[]);
    assert_eq!(error.to_string(), "invalid: text: a name two things share");
}
