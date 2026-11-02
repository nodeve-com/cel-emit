//! A name holding a message and a message of the same name live apart, as CEL keeps variables and
//! types apart: the value is read and tested by field, and `M{…}` builds one.
//! Messages of equal fields are one type, however each was reached: a variable's, a literal's, a
//! field's, or a name's, even with several declared alike.

use cel_emit::{lower, At, Decls, Field, Name, Type};

fn field(name: &str, ty: Type) -> Field {
    Field {
        name: name.into(),
        ty,
        required: false,
    }
}

/// Only here: one identifier, a variable of message `M` and also message `M`.
#[test]
fn a_value_and_a_literal_share_a_name() {
    let ty = Type::Message(vec![
        field("n", Type::Int),
        field("o", Type::Optional(Box::new(Type::Int))),
    ]);
    let mut decls = Decls::default();
    decls.messages.insert("M".into(), ty.clone());
    decls.names.insert("M".into(), Name { ty, at: At::Var(0) });
    for (source, want) in [
        ("M.n + 1", Type::Int),
        ("M.o + 1", Type::Int),
        ("has(M.o)", Type::Bool),
        ("M{n: 1}.n", Type::Int),
        ("M{n: M.n}.n", Type::Int),
    ] {
        let got = lower(source, &decls).unwrap_or_else(|e| panic!("{source}: {e}"));
        assert_eq!(got.ty, want, "{source}");
    }
}

/// Only here: a variable of a message with an optional message field reads through it.
#[test]
fn a_value_reads_through_an_optional_message_field() {
    let inner = Type::Message(vec![field("f", Type::Int)]);
    let ty = Type::Message(vec![field("o", Type::Optional(Box::new(inner)))]);
    let mut decls = Decls::default();
    decls.names.insert("M".into(), Name { ty, at: At::Var(0) });
    for (source, want) in [("M.o.f + 1", Type::Int), ("has(M.o.f)", Type::Bool)] {
        let got = lower(source, &decls).unwrap_or_else(|e| panic!("{source}: {e}"));
        assert_eq!(got.ty, want, "{source}");
    }
}

fn var(ty: Type, v: usize) -> Name {
    Name { ty, at: At::Var(v) }
}

/// Only here: a variable's message, unnamed, and two declared messages of the same fields, one
/// also a field of another.
fn decls() -> (Decls, Type) {
    let m = Type::Message(vec![field("n", Type::Int)]);
    let outer = Type::Message(vec![field("inner", m.clone())]);
    let mut decls = Decls::default();
    decls.messages.insert("M".into(), m.clone());
    decls.messages.insert("N".into(), m.clone());
    decls.messages.insert("Outer".into(), outer.clone());
    decls
        .names
        .insert("xs".into(), var(Type::list(m.clone()), 0));
    decls
        .names
        .insert("ys".into(), var(Type::list(m.clone()), 1));
    decls.names.insert("outer".into(), var(outer, 2));
    (decls, m)
}

/// Only here: a list of each spelling of one message joins as that message's list.
#[test]
fn equal_messages_join_as_one_type() {
    let (decls, m) = decls();
    for source in [
        "xs + ys",
        "xs + ys.map(y, M{n: y.n})",
        "ys.map(y, N{n: y.n}) + xs.map(x, M{n: x.n})",
        "xs.map(x, M{n: x.n}) + ys.map(y, N{n: y.n})",
        "xs.map(x, outer.inner) + ys",
        "xs.map(x, Outer{inner: N{n: x.n}}.inner) + ys",
    ] {
        let got = lower(source, &decls).unwrap_or_else(|e| panic!("{source}: {e}"));
        assert_eq!(got.ty, Type::list(m.clone()), "{source}");
    }
}

/// Only here: a read through a joined list stays typed.
#[test]
fn a_joined_list_reads_by_field() {
    let (decls, _) = decls();
    let got =
        lower("(xs + ys.map(y, M{n: y.n}))[0].n + 1", &decls).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(got.ty, Type::Int);
}
