//! Functions an embedder writes in CEL ([`Written`]): [`check`] checks each body once, over its
//! parameters alone, and each [`Body`] then declares to lowering as a [`Function`] of one overload
//! ([`Decls::define`]), so every call is typed against it, and to the emitter, which writes it once
//! ([`crate::emit::Emitter::define`]).

use std::collections::BTreeMap;

use crate::decls::{Decl, Decls, Name};
use crate::functions::Function;
use crate::tree::Typed;
use crate::types::Type;
use crate::Error;

mod order;

/// A function written in CEL: its name, its parameters in order, each a name and a type, and a
/// body that reads only them. Its result is its body's type. One Rust function emits for it,
/// taking each parameter not `Copy` by reference, and returning `cel_emit_runtime::Result` of its
/// result.
#[derive(Clone, Debug)]
pub struct Written {
    /// The name an expression calls it by.
    pub name: String,
    pub params: Vec<(String, Type)>,
    /// The body, in CEL.
    pub cel: String,
    /// The Rust path of the function: a call writes it, and the function's name is its last part.
    pub path: String,
}

/// A written function checked and lowered: its signature and body, as the emitter writes them.
#[derive(Clone, Debug)]
pub struct Body {
    pub written: Written,
    /// The overload id calls of it resolve to.
    pub id: String,
    /// The body, over each parameter as a variable numbered by its position.
    pub tree: Typed,
}

impl Body {
    /// The function every call of it resolves through: one overload, the declared parameters, the
    /// body's type.
    pub fn function(&self) -> Function {
        let w = &self.written;
        let params = w.params.iter().map(|(_, ty)| ty.clone()).collect();
        let decl = Decl::new(w.name.clone()).with_overload(&self.id, params, self.tree.ty.clone());
        Function {
            decl,
            paths: vec![(self.id.clone(), w.path.clone())],
        }
    }
}

/// Every function of `all` checked against `decls`, a callee before its callers. `decls` holds
/// what a body may call beyond the written functions; a body reads no name of it.
pub fn check(all: &[Written], decls: &Decls) -> Result<Vec<Body>, Error> {
    let mut seen = BTreeMap::new();
    for w in all {
        if seen.insert(w.name.as_str(), w).is_some() {
            return Err(Error::Invalid(format!("{}: written twice", w.name)));
        }
        names(w)?;
    }
    let mut done: Vec<Body> = Vec::new();
    for w in order::callees_first(all)? {
        let mut within = Decls {
            names: BTreeMap::new(),
            ..decls.clone()
        };
        within.define(&done);
        for (i, (name, ty)) in w.params.iter().enumerate() {
            within.names.insert(name.clone(), Name::var(ty.clone(), i));
        }
        let tree = crate::lower(&w.cel, &within).map_err(|e| e.at(&w.name))?;
        if tree.ty == Type::Number {
            return Err(Error::Unsupported(format!(
                "{}: a mixed number result",
                w.name
            )));
        }
        done.push(Body {
            id: format!("written_{}", w.name),
            written: w.clone(),
            tree,
        });
    }
    Ok(done)
}

/// Whether `w`'s parameter names are distinct.
fn names(w: &Written) -> Result<(), Error> {
    let mut seen = std::collections::BTreeSet::new();
    match w.params.iter().find(|(name, _)| !seen.insert(name)) {
        Some((name, _)) => Err(Error::Invalid(format!(
            "{}: parameter {name} twice",
            w.name
        ))),
        None => Ok(()),
    }
}
