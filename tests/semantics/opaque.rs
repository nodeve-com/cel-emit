//! An opaque value, a Rust type the embedder names: an expression moves it whole, each move a
//! clone in the emitted Rust, and every read into it is refused. JSON takes it only through the
//! functions the embedder names.

use cel_emit::emit::{Emitter, Messages};
use cel_emit::{lower, lower_as, Decls, Error, Field, Message, Name, Type};

use crate::common::bind::{answers, Bound};
use crate::common::{build_run, program};

/// The opaque type, only `Clone` (and `Debug`, to print an answer); the message holding it,
/// whose default is its own; and the two functions the embedder names.
const PRELUDE: &str = r#"
#[derive(Clone, Debug)] pub struct Pair(pub u8, pub u8);
#[derive(Clone, Debug)] pub struct M { pub p: Pair, pub o: Option<Pair>, pub n: i64 }
impl Default for M { fn default() -> M { M { p: Pair(0, 0), o: None, n: 0 } } }
fn write_pair(p: &Pair) -> s::Result<s::Value> {
    Ok(s::Value::List(vec![s::Value::from(i64::from(p.0)), s::Value::from(i64::from(p.1))]))
}
fn read_pair(v: &s::Value) -> s::Result<Pair> {
    match v {
        s::Value::List(l) if l.len() == 2 => {
            Ok(Pair(s::dynamic::to_int(&l[0])? as u8, s::dynamic::to_int(&l[1])? as u8))
        }
        _ => Err(s::Error::Cast),
    }
}
"#;

/// What each case binds, in the order [`decls`] declares.
const LOCALS: &str = "let p = Pair(1, 2); let q = Pair(3, 4); \
    let m = M { p: Pair(5, 6), o: Some(Pair(7, 8)), n: 9 }; \
    let ps = vec![Pair(1, 2), Pair(3, 4)]; let by = vec![(String::from(\"a\"), Pair(9, 9))]; \
    let o: Option<Pair> = None; let c = true; let text = String::from(\"[4,5]\");";

/// Names `M` and its fields as the prelude writes them, and `Pair`'s two functions where asked.
struct Own {
    hooks: bool,
}

impl Messages for Own {
    fn path(&mut self, _: usize, _: &Message) -> String {
        "M".into()
    }

    fn fields(&self, message: &Message) -> Vec<String> {
        message.fields.iter().map(|f| f.name.clone()).collect()
    }

    fn writes(&self, rust: &str) -> Option<String> {
        (self.hooks && rust == "Pair").then(|| "write_pair".into())
    }

    fn reads(&self, rust: &str) -> Option<String> {
        (self.hooks && rust == "Pair").then(|| "read_pair".into())
    }
}

fn pair() -> Type {
    Type::Opaque("Pair".into())
}

fn message() -> Type {
    let field = |name: &str, ty| Field {
        name: name.into(),
        ty,
        required: false,
    };
    Type::named(
        "M",
        vec![
            field("p", pair()),
            field("o", Type::Optional(Box::new(pair()))),
            field("n", Type::Int),
        ],
    )
}

const NAMES: [&str; 8] = ["p", "q", "m", "ps", "by", "o", "c", "text"];

fn decls() -> Decls {
    let types = [
        pair(),
        pair(),
        message(),
        Type::list(pair()),
        Type::map(pair()),
        Type::Optional(Box::new(pair())),
        Type::Bool,
        Type::String,
    ];
    let mut decls = Decls::default();
    decls.messages.insert("M".into(), message());
    for (i, (name, ty)) in NAMES.into_iter().zip(types).enumerate() {
        decls.names.insert(name.into(), Name::var(ty, i));
    }
    decls
}

/// `source` as Rust, returning `result` where one is given.
fn code(emitter: &mut Emitter, source: &str, result: Option<&Type>) -> Result<String, Error> {
    let tree = match result {
        Some(ty) => lower_as(source, &decls(), ty)?,
        None => lower(source, &decls())?,
    };
    emitter.expr(&tree, &Bound(NAMES.map(String::from).to_vec()))
}

/// A source, the type it returns where it is given one, and its answer as `Debug` writes it.
type Case = (&'static str, Option<Type>, &'static str);

/// Each of `cases` built and run as one program: its source, the type it returns where it is
/// given one, and its answer as `Debug` writes it.
fn run(cases: &[Case]) {
    let mut emitter = Emitter::new(&[], Box::new(Own { hooks: true }));
    let fns: Vec<(usize, String)> = cases
        .iter()
        .enumerate()
        .map(|(n, (source, result, _))| {
            let code = code(&mut emitter, source, result.as_ref())
                .unwrap_or_else(|e| panic!("{source}: {e}"));
            let body = format!(
                "{LOCALS} let r = (|| -> s::Result<_> {{ Ok({code}) }})(); \
                 match r {{ Ok(v) => format!(\"{{v:?}}\"), Err(e) => format!(\"err {{e:?}}\") }}"
            );
            (n, format!("fn c{n}() -> String {{ {body} }}\n"))
        })
        .collect();
    let items = emitter.items().unwrap().code;
    let dir = std::env::temp_dir().join(format!("cel-emit-opaque-{}", std::process::id()));
    let lines = build_run(&dir, &program(&fns, PRELUDE, &items));
    let _ = std::fs::remove_dir_all(&dir);
    let got = answers(lines.unwrap_or_else(|e| panic!("rustc: {e}")));
    for ((n, got), (source, _, want)) in got.iter().zip(cases) {
        assert_eq!(got, want, "case {n}: {source}");
    }
    assert_eq!(got.len(), cases.len());
}

/// Only here: every move of an opaque value, and JSON through the embedder's two functions.
#[test]
fn an_opaque_value_moves_whole() {
    run(&[moves(), through_json()].concat());
}

fn moves() -> Vec<Case> {
    let both = "[Pair(1, 2), Pair(3, 4)]";
    vec![
        ("p", None, "Pair(1, 2)"),
        ("p", Some(pair()), "Pair(1, 2)"),
        ("m.p", Some(pair()), "Pair(5, 6)"),
        ("M{p: q, n: 1}", None, "M { p: Pair(3, 4), o: None, n: 1 }"),
        (
            "M{p: m.p, o: q}",
            None,
            "M { p: Pair(5, 6), o: Some(Pair(3, 4)), n: 0 }",
        ),
        ("c ? p : q", None, "Pair(1, 2)"),
        ("cel.bind(x, m.p, x)", None, "Pair(5, 6)"),
        ("ps", None, both),
        ("[p, q]", None, both),
        ("ps + ps.filter(x, !c)", None, both),
        ("ps.map(x, x)", None, both),
        ("ps[1]", None, "Pair(3, 4)"),
        ("size(ps)", None, "2"),
        ("by", None, "[(\"a\", Pair(9, 9))]"),
        ("by['a']", None, "Pair(9, 9)"),
        ("has(m.o)", None, "true"),
        ("m.o", None, "Pair(7, 8)"),
        ("m.?o", None, "Some(Pair(7, 8))"),
        ("m.?o.orValue(q)", None, "Pair(7, 8)"),
        ("o.orValue(q)", None, "Pair(3, 4)"),
        ("optional.of(p)", None, "Some(Pair(1, 2))"),
    ]
}

fn through_json() -> Vec<Case> {
    vec![
        ("json.encode(m.p)", None, "\"[5,6]\""),
        ("json.encode(ps)", None, "\"[[1,2],[3,4]]\""),
        (
            "json.encode(m)",
            None,
            "\"{\\\"p\\\":[5,6],\\\"o\\\":[7,8],\\\"n\\\":9}\"",
        ),
        ("json.decode(text)", Some(pair()), "Pair(4, 5)"),
        (
            "M{p: json.decode(text)}",
            None,
            "M { p: Pair(4, 5), o: None, n: 0 }",
        ),
    ]
}

/// Only here: a move is a clone of the place, with no function between.
#[test]
fn a_move_is_a_clone() {
    let mut emitter = Emitter::new(&[], Box::new(Own { hooks: false }));
    assert_eq!(code(&mut emitter, "m.p", None).unwrap(), "m.p.clone()");
    assert_eq!(emitter.rust(&pair()).unwrap(), "Pair");
}

/// Only here: each read into an opaque value is refused as the expression is checked, the
/// refusal naming the type.
#[test]
fn a_read_into_an_opaque_value_is_refused() {
    let reads = [
        "p[0]",
        "p.x",
        "has(p.x)",
        "p + q",
        "-p",
        "p < q",
        "p == q",
        "p != q",
        "p == null",
        "m.p != null",
        "ps == ps",
        "m == m",
        "p in ps",
        "size(p)",
        "string(p)",
        "int(p)",
        "dyn(p)",
        "p.contains('a')",
        "ps.exists(x, x == p)",
        "ps.distinct()",
        "M{}.p",
    ];
    for source in reads {
        let refused = lower(source, &decls()).map(|_| ()).unwrap_err().to_string();
        assert!(refused.contains("Pair"), "{source}: {refused}");
    }
    // Refused by a rule of its own, which names no type: a map literal takes only text as a
    // key, no function orders a list of them, and a macro loops over a list or a map.
    for source in ["{p: 1}", "ps.sort()", "p.map(x, x)"] {
        assert!(lower(source, &decls()).is_err(), "{source}");
    }
}

/// Only here: two Rust types are two CEL types, so one never fills the other's place.
#[test]
fn opaque_types_differ_by_their_rust() {
    let mut decls = decls();
    let other = Name::var(Type::Opaque("Other".into()), NAMES.len());
    decls.names.insert("other".into(), other);
    for source in ["M{p: other}", "c ? p : other", "[p, other]"] {
        assert!(lower(source, &decls).is_err(), "{source}");
    }
}

/// Only here: with no function named, JSON refuses an opaque value where the Rust is written.
#[test]
fn json_needs_the_embedders_functions() {
    let mut emitter = Emitter::new(&[], Box::new(Own { hooks: false }));
    for (source, result) in [
        ("json.encode(m.p)", None),
        ("json.encode(m)", None),
        ("json.decode(text)", Some(pair())),
    ] {
        let refused = code(&mut emitter, source, result.as_ref()).unwrap_err();
        assert!(
            refused.to_string().contains("names no function"),
            "{source}"
        );
    }
}
