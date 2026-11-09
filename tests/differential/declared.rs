//! The messages the message and shaped cases build: `M` (messages.rs), and `N`, `P`, and `Q` (shaped.rs).

use cel_emit::{Decls, Field, Type};

/// `Q`: a required field and a nullable one.
fn required_q(field: impl Fn(&str, Type) -> Field) -> Type {
    let required = Field {
        required: true,
        ..field("q", Type::Int)
    };
    Type::message(vec![
        required,
        field("r", Type::Nullable(Box::new(Type::Int))),
    ])
}

/// Every message, declared.
pub fn declared() -> Decls {
    let field = |name: &str, ty| Field {
        name: name.into(),
        ty,
        required: false,
    };
    let m = Type::message(vec![
        field("f", Type::Nullable(Box::new(Type::Int))),
        field("n", Type::Int),
        field("s", Type::String),
    ]);
    let n = Type::message(vec![field("k", Type::Int), field("t", Type::String)]);
    let p = Type::message(vec![
        field("i", Type::Int),
        field("u", Type::Uint),
        field("d", Type::Double),
        field("b", Type::Bool),
        field("s", Type::String),
        field("f", Type::Nullable(Box::new(Type::Int))),
        field("n", n.clone()),
    ]);
    let q = required_q(field);
    let mut decls = Decls::default();
    for (name, ty) in [("M", m), ("N", n), ("P", p), ("Q", q)] {
        decls.messages.insert(name.into(), ty);
    }
    decls
}
