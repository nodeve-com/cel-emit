//! The messages the message and decode cases build: `M` (messages.rs), and `N`, `P`, `Q`, and `R` (shaped.rs).

use cel_emit::{Decls, Field, Type};

/// Every message, declared.
pub fn declared() -> Decls {
    let field = |name: &str, ty| Field {
        name: name.into(),
        ty,
        required: false,
    };
    let m = Type::Message(vec![
        field("f", Type::Nullable(Box::new(Type::Int))),
        field("n", Type::Int),
        field("s", Type::String),
    ]);
    let n = Type::Message(vec![field("k", Type::Int), field("t", Type::String)]);
    let p = Type::Message(vec![
        field("i", Type::Int),
        field("u", Type::Uint),
        field("d", Type::Double),
        field("b", Type::Bool),
        field("s", Type::String),
        field("f", Type::Nullable(Box::new(Type::Int))),
        field("n", n.clone()),
    ]);
    let required = Field {
        required: true,
        ..field("q", Type::Int)
    };
    let q = Type::Message(vec![
        required,
        field("r", Type::Nullable(Box::new(Type::Int))),
    ]);
    // `R` keeps the keys a decode does not declare, and a message writes, under `rest`.
    let r = Type::Message(vec![field("n", Type::Int), field("rest", Type::String)]);
    let mut decls = Decls {
        rest: Some("rest".into()),
        ..Decls::default()
    };
    for (name, ty) in [("M", m), ("N", n), ("P", p), ("Q", q), ("R", r)] {
        decls.messages.insert(name.into(), ty);
    }
    decls
}
