//! A message's `==`: written only for the messages an expression compares, and those nested in
//! them.

use cel_emit::emit::Emitter;
use cel_emit::{lower, Decls, Field, Name, Type};

use crate::common::bind::Bound;
use crate::common::Positional;

fn field(name: &str, ty: Type) -> Field {
    Field {
        name: name.into(),
        ty,
        required: false,
    }
}

/// The items of an emitter that declares `m`, a message holding an int and a message of one
/// string, and emits `source` over it.
fn items(source: &str) -> String {
    let inner = Type::message(vec![field("s", Type::String)]);
    let outer = Type::message(vec![field("n", Type::Int), field("in", inner)]);
    let mut decls = Decls::default();
    decls.names.insert("m".into(), Name::var(outer.clone(), 0));
    let mut emitter = Emitter::new(&[], Box::new(Positional));
    emitter.rust(&outer).unwrap();
    let tree = lower(source, &decls).unwrap();
    emitter.expr(&tree, &Bound(vec!["m".into()])).unwrap();
    emitter.items().unwrap().code
}

/// Only here: a message no expression compares gets no `==`; one compared gets it, with the
/// message nested in it.
#[test]
fn a_message_has_eq_only_where_compared() {
    let unread = items("m.n + 1");
    assert!(!unread.contains("cmp::Equal for"), "{unread}");
    let compared = items("m == m");
    assert_eq!(compared.matches("cmp::Equal for").count(), 2, "{compared}");
}
