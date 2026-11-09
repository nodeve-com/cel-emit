//! A message's name is part of its type: messages of equal fields and different names are asked of
//! `Messages::path` apart, and an expression over each emits that message's Rust.

use std::cell::RefCell;
use std::rc::Rc;

use cel_emit::emit::{Emitter, Messages};
use cel_emit::{lower, Decls, Field, Message, Name, Type};

use crate::common::bind::Bound;

fn fields() -> Vec<Field> {
    vec![Field {
        name: "n".into(),
        ty: Type::Int,
        required: false,
    }]
}

/// Names each message's Rust by its name, `crate::{name}`, or `crate::Plain` for none, and records
/// each message it is asked.
struct ByName(Rc<RefCell<Vec<Option<String>>>>);

impl Messages for ByName {
    fn path(&mut self, _: usize, message: &Message) -> String {
        self.0.borrow_mut().push(message.name.clone());
        format!("crate::{}", message.name.as_deref().unwrap_or("Plain"))
    }

    fn fields(&self, message: &Message) -> Vec<String> {
        message.fields.iter().map(|f| f.name.clone()).collect()
    }
}

/// Only here: a name is part of a message's identity.
#[test]
fn messages_differing_only_in_name_are_different_types() {
    let named = Type::named("A", fields());
    assert_ne!(named, Type::named("B", fields()));
    assert_ne!(named, Type::message(fields()));
    assert_eq!(named, Type::named("A", fields()));
}

/// Only here: each message of equal fields and a different name is asked once, and an expression
/// building each writes its own Rust.
#[test]
fn an_expression_emits_the_rust_of_its_messages_name() {
    let (a, b, plain) = (
        Type::named("A", fields()),
        Type::named("B", fields()),
        Type::message(fields()),
    );
    let mut decls = Decls::default();
    decls.messages.insert("A".into(), a.clone());
    decls.messages.insert("B".into(), b.clone());
    for (i, (name, ty)) in [("a", &a), ("b", &b), ("p", &plain)]
        .into_iter()
        .enumerate()
    {
        decls.names.insert(name.into(), Name::var(ty.clone(), i));
    }
    let asked = Rc::new(RefCell::new(Vec::new()));
    let mut emitter = Emitter::new(&[], Box::new(ByName(asked.clone())));
    let paths = [&a, &b, &plain].map(|t| emitter.rust(t).unwrap());
    assert_eq!(paths, ["crate::A", "crate::B", "crate::Plain"]);
    let vars = Bound(vec!["a".into(), "b".into(), "p".into()]);
    let mut code = |source: &str| {
        let tree = lower(source, &decls).unwrap();
        emitter.expr(&tree, &vars).unwrap()
    };
    let from_a = code("A{n: 1}");
    let from_b = code("B{n: 2}");
    assert!(from_a.contains("crate::A::default()"), "{from_a}");
    assert!(from_b.contains("crate::B::default()"), "{from_b}");
    let selected = code("a.n + b.n + p.n");
    assert!(selected.contains(".n"), "{selected}");
    assert_eq!(*asked.borrow(), [Some("A".into()), Some("B".into()), None]);
}
