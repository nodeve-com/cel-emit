//! A call's type argument names any type the embedder declares under a name: a message, a list,
//! a map, or a scalar. It picks the overload declared for that type and passes nothing.

use cel_emit::emit::Emitter;
use cel_emit::functions::Function;
use cel_emit::{lower, Decl, Decls, Error, Field, Name, Type, Width};

use crate::common::bind::{answers, show, Bound};
use crate::common::{build_run, items, program, Positional};

fn point() -> Type {
    Type::message(vec![Field {
        name: "x".into(),
        ty: Type::Int,
        required: true,
    }])
}

/// Each name `of(text, Name)` takes, its type, and the Rust body running its overload.
fn named() -> Vec<(&'static str, Type, &'static str)> {
    vec![
        ("Point", point(), "Ok(Declared0 { f0: 7 })"),
        (
            "Points",
            Type::list(point()),
            "Ok(vec![Declared0 { f0: t.len() as i64 }])",
        ),
        (
            "Names",
            Type::list(Type::String),
            "Ok(t.split(',').map(String::from).collect())",
        ),
        (
            "Counts",
            Type::map(Type::Int),
            "Ok(vec![(t.to_string(), 2)])",
        ),
        ("Score", Type::Int, "Ok(t.len() as i64)"),
        (
            "Small",
            Type::list(Type::Narrow(Width::U8)),
            "Ok(t.bytes().collect())",
        ),
    ]
}

/// `of(text, Name)` declared for each of [`named`], over the text `v`.
fn declared() -> (Decls, Function) {
    let mut decls = Decls::default();
    let mut decl = Decl::new("of");
    let mut paths = Vec::new();
    for (name, ty, _) in named() {
        let id = format!("of_{}", name.to_lowercase());
        decl = decl
            .with_overload(id.clone(), vec![Type::String], ty.clone())
            .naming(ty.clone());
        paths.push((id.clone(), id));
        match ty {
            Type::Message(_) => decls.messages.insert(name.into(), ty),
            _ => decls.types.insert(name.into(), ty),
        };
    }
    let function = Function { decl, paths };
    decls.functions.push(function.clone());
    decls.names.insert("v".into(), Name::var(Type::String, 0));
    (decls, function)
}

/// What `source` returns, and its Rust.
fn rust(source: &str) -> Result<(Type, String), Error> {
    let (decls, function) = declared();
    let tree = lower(source, &decls)?;
    let mut emitter = Emitter::new(&[function], Box::new(Positional));
    let code = emitter.expr(&tree, &Bound(vec!["v".into()]))?;
    Ok((tree.ty, code))
}

/// Only here: a name of a list, a map, or a scalar picks its overload as a message's does, the
/// call typed as the type it names.
#[test]
fn a_type_argument_names_any_declared_type() {
    for (name, ty, _) in named() {
        let (got, code) = rust(&format!("of(v, {name})")).unwrap();
        assert_eq!(got, ty.cel(), "{name}");
        let path = format!("of_{}(", name.to_lowercase());
        assert!(code.contains(&path), "{name}: {code}");
        assert!(!code.contains(name), "{name}: {code}");
    }
}

/// Only here: a name no one declared, a CEL type's own name, and a variable are no type
/// argument.
#[test]
fn a_type_argument_naming_no_declared_type_is_an_error() {
    let Err(Error::Invalid(why)) = rust("of(v, Missing)") else {
        panic!("Missing lowered");
    };
    assert!(why.contains("undeclared reference to 'Missing'"), "{why}");
    for source in ["of(v, int)", "of(v, v)", "of(v)"] {
        assert!(rust(source).is_err(), "{source}");
    }
}

/// Only here: a name in scope keeps its meaning over a declared type of that name.
#[test]
fn a_name_in_scope_is_no_type_argument() {
    let (mut decls, _) = declared();
    decls
        .names
        .insert("Names".into(), Name::var(Type::String, 1));
    assert!(lower("of(v, Names)", &decls).is_err());
    assert_eq!(lower("Names + v", &decls).unwrap().ty, Type::String);
}

/// Only here: names of equal types are one type, so either picks the overload declared for it,
/// and a second overload declared for that type leaves the call none to pick.
#[test]
fn names_of_equal_types_pick_one_overload() {
    let (mut decls, _) = declared();
    decls.types.insert("Tags".into(), Type::list(Type::String));
    let tree = lower("of(v, Tags)", &decls).unwrap();
    assert!(
        matches!(&tree.node, cel_emit::Node::Call(id, _) if id == "of_names"),
        "{tree:?}"
    );
    let again = |f: &mut Function| {
        let decl = f.decl.clone();
        f.decl = decl
            .with_overload("of_tags", vec![Type::String], Type::list(Type::String))
            .naming(Type::list(Type::String));
    };
    decls.functions.iter_mut().for_each(again);
    assert!(lower("of(v, Tags)", &decls).is_err());
}

/// The Rust of each overload of [`named`]: `of_<name>`, returning its type as `emitter` writes it.
fn overloads(emitter: &mut Emitter) -> String {
    let each = named().into_iter().map(|(name, ty, body)| {
        let returns = emitter.rust(&ty).unwrap();
        let id = format!("of_{}", name.to_lowercase());
        format!("fn {id}(t: &str) -> s::Result<{returns}> {{ {body} }}\n")
    });
    each.collect()
}

/// Only here: each overload a type argument picks runs, its result read as the type named.
#[test]
fn the_overload_a_type_argument_picks_runs() {
    let cases = [
        ("of(v, Point).x", "7"),
        ("of(v, Points)[0].x + of(v, Score)", "6"),
        ("of(v, Names)[1]", "\"b\""),
        ("of(v, Names).size()", "2"),
        ("of(v, Counts)['a,b']", "2"),
        ("of(v, Small)[0] + 1u", "98u"),
        ("of(v, Small).map(b, b * 2u)[2]", "196u"),
    ];
    let (decls, function) = declared();
    let mut emitter = Emitter::new(&[function], Box::new(Positional));
    let prelude = overloads(&mut emitter);
    let bound = Bound(vec!["String::from(\"a,b\")".into()]);
    let fns: Vec<(usize, String)> = (cases.iter().enumerate())
        .map(|(n, (source, _))| {
            let tree = lower(source, &decls).unwrap_or_else(|e| panic!("{source}: {e}"));
            let expr = emitter.expr(&tree, &bound).unwrap();
            let shown = show(&tree.ty, "v", 0);
            let run = format!("(|| -> s::Result<String> {{ let v = {expr}; s::ok({shown}) }})()");
            (
                n,
                format!("fn c{n}() -> String {{ {run}.unwrap_or_else(|_| \"err\".into()) }}\n"),
            )
        })
        .collect();
    let dir = std::env::temp_dir().join(format!("cel-emit-type-argument-{}", std::process::id()));
    let lines = build_run(
        &dir,
        &program(&fns, &prelude, &items(&mut emitter).unwrap()),
    );
    let _ = std::fs::remove_dir_all(&dir);
    let got = answers(lines.unwrap_or_else(|e| panic!("rustc: {e}")));
    let want: Vec<_> = (cases.iter().enumerate())
        .map(|(n, (_, want))| (n, want.to_string()))
        .collect();
    assert_eq!(got, want);
}
