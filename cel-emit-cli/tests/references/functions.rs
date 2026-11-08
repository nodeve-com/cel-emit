//! A written function as the interpreter runs it: a function of dyn arguments, whose body is its
//! own checked expression, evaluated on each call.

use cel_emit::functions::Function;
use cel_emit::{Decl, Decls, Name, Type};
use cel_emit_cli::Declared;
use prost::Message as _;

/// `declared`'s declarations, each written function declared as a function of dyn arguments and
/// result, in place of the checked one.
pub fn plain(declared: &Declared) -> Decls {
    let mut all = declared.decls.clone();
    let written: Vec<&str> = declared.bodies.iter().map(|b| b.id.as_str()).collect();
    all.functions
        .retain(|f| !f.paths.iter().any(|(id, _)| written.contains(&id.as_str())));
    for body in &declared.bodies {
        let w = &body.written;
        let params = vec![Type::Dyn; w.params.len()];
        all.functions.push(Function {
            decl: Decl::new(w.name.clone()).with_overload(w.name.clone(), params, Type::Dyn),
            paths: Vec::new(),
        });
    }
    all
}

/// The interpreter's table, as a Rust expression: each written function's name, its params, and
/// its checked body, each after those it calls.
pub fn table(declared: &Declared) -> String {
    let plain = plain(declared);
    let mut out = String::from("&[");
    for body in &declared.bodies {
        let w = &body.written;
        let mut within = plain.clone();
        within.names.clear();
        for (i, (name, _)) in w.params.iter().enumerate() {
            within.names.insert(name.clone(), Name::var(Type::Dyn, i));
        }
        let env = cel_emit::env::environment(&within);
        let ast = cel_emit::macros::compile(&env, &w.cel).unwrap();
        let info = ast.type_info().unwrap();
        let bytes = cel_core_proto::to_checked_expr_from_ast(info, ast.expr(), ast.source())
            .encode_to_vec();
        let params: Vec<String> = w.params.iter().map(|(n, _)| format!("{n:?}")).collect();
        out += &format!("({:?}, &[{}], &{bytes:?}),", w.name, params.join(", "));
    }
    out + "]"
}
