//! Functions an embedder writes in CEL ([`Written`]): each body checked once, over its parameters
//! alone, then declared to the checker as a [`Function`] of one overload, so every call is typed
//! against it, and emitted once ([`crate::emit::Emitter::define`]).

use std::collections::BTreeMap;

use crate::decls::{Decl, Decls, Name};
use crate::functions::Function;
use crate::tree::Typed;
use crate::types::Type;
use crate::Error;

mod order;

pub use crate::decls::Written;

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
    pub(crate) fn function(&self) -> Function {
        let w = &self.written;
        let params = w.params.iter().map(|(_, ty)| ty.clone()).collect();
        let decl = Decl::new(w.name.clone()).with_overload(&self.id, params, self.tree.ty.clone());
        Function {
            decl,
            paths: vec![(self.id.clone(), w.path.clone())],
        }
    }
}

/// Every function of `decls.written` checked, a callee before its callers.
pub fn bodies(decls: &Decls) -> Result<Vec<Body>, Error> {
    let mut seen = BTreeMap::new();
    for w in &decls.written {
        if seen.insert(w.name.as_str(), w).is_some() {
            return Err(Error::Invalid(format!("{}: written twice", w.name)));
        }
        names(w)?;
    }
    let mut done: Vec<Body> = Vec::new();
    for w in order::callees_first(&decls.written)? {
        let mut within = Decls {
            written: Vec::new(),
            names: BTreeMap::new(),
            ..decls.clone()
        };
        within.functions.extend(done.iter().map(Body::function));
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

/// `decls` with each written function declared as a [`Function`], the form lowering reads.
pub(crate) fn resolved(decls: &Decls) -> Result<Decls, Error> {
    let mut all = Decls {
        written: Vec::new(),
        ..decls.clone()
    };
    all.functions
        .extend(bodies(decls)?.iter().map(Body::function));
    Ok(all)
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
