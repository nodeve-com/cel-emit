//! Reads with no copy: a place to read a value through, a reference to it, and whether a local can
//! hold that reference.

use super::{Emitter, Vars};
use crate::functions::STD;
use crate::tree::{Node, Part, Typed};
use crate::types::Type;
use crate::Error;

/// Whether overload `id` gives a reference into its first argument.
fn lends(id: &str) -> bool {
    matches!(id, "dyn_get" | "dyn_at")
}

/// Whether overload `id` gives an optional reference into its first argument.
pub(super) fn lends_optional(id: &str) -> bool {
    matches!(id, "dyn_optional_get" | "dyn_chain")
}

/// Whether `t` emits as a reference already, so a read through it needs no `&`.
pub(super) fn by_reference(t: &Typed) -> bool {
    match &t.node {
        Node::Local(_) | Node::Item(_, Part::Whole) | Node::First(..) => true,
        Node::Call(id, _) => lends(id),
        Node::FromDyn(_, ty) => dyn_list(ty),
        _ => false,
    }
}

fn dyn_list(ty: &Type) -> bool {
    matches!(ty, Type::List(of) if **of == Type::Dyn)
}

/// Whether a reference to `t` outlives the statement reading it: what it reads is no temporary.
fn rooted(t: &Typed) -> bool {
    match &t.node {
        Node::Var(_) | Node::Local(_) | Node::Item(..) => true,
        Node::Field(x, _) | Node::First(x, _) => rooted(x),
        Node::FromDyn(x, ty) => dyn_list(ty) && rooted(x),
        Node::Call(id, args) => lends(id) && args.first().is_some_and(rooted),
        _ => false,
    }
}

/// `code` as the receiver of a method or field: in parentheses where it would read otherwise.
pub(super) fn receiver(code: &str) -> String {
    let bare = code.starts_with(|c: char| c.is_alphanumeric() || c == '_' || c == '(')
        && !code.starts_with("if ")
        && !code.starts_with("match ");
    match bare {
        true => code.into(),
        false => format!("({code})"),
    }
}

impl Emitter {
    /// `t` by reference, with no copy; where `plain`, text as `&str`, which a literal is already.
    pub(super) fn reference(
        &mut self,
        t: &Typed,
        plain: bool,
        vars: &dyn Vars,
    ) -> Result<String, Error> {
        Ok(match (&t.node, &t.ty) {
            (Node::Str(v), _) if plain => format!("{v:?}"),
            (Node::FromDyn(x, _), Type::String) if plain => {
                format!(
                    "{STD}::dynamic::str_of({})?",
                    self.reference(x, false, vars)?
                )
            }
            (_, Type::String) if plain => format!("{}.as_str()", receiver(&self.place(t, vars)?)),
            _ if by_reference(t) => self.place(t, vars)?,
            _ => format!("&{}", self.place(t, vars)?),
        })
    }

    /// `t` as a Rust place or reference to read it through, no copy made; what is neither, as its
    /// value.
    pub(super) fn place(&mut self, t: &Typed, vars: &dyn Vars) -> Result<String, Error> {
        Ok(match &t.node {
            Node::Var(v) => vars.read(*v)?,
            Node::Local(n) => format!("l{n}"),
            Node::Item(n, part) => match part {
                Part::Whole => format!("x{n}"),
                Part::Value => format!("x{n}.1"),
                Part::Key => format!("x{n}.0"),
            },
            Node::Field(x, i) => {
                let names = self.declare.fields(x.ty.fields());
                let name = names
                    .get(*i)
                    .ok_or_else(|| Error::Invalid("a field past its message".into()))?;
                format!("{}.{name}", receiver(&self.place(x, vars)?))
            }
            Node::Call(id, args) if lends(id) => self.call(id, args, vars)?,
            Node::FromDyn(x, ty) if dyn_list(ty) => {
                let x = self.reference(x, false, vars)?;
                format!("{STD}::dynamic::list_of({x})?")
            }
            Node::First(list, pred) => self.first(list, pred, vars)?,
            _ => self.expr(t, vars)?,
        })
    }

    /// `t`, an optional, as an optional reference, no copy made.
    pub(super) fn optional_reference(
        &mut self,
        t: &Typed,
        vars: &dyn Vars,
    ) -> Result<String, Error> {
        Ok(match &t.node {
            Node::Call(id, args) if lends_optional(id) => self.lent_optional(id, args, vars)?,
            _ => format!("{}.as_ref()", receiver(&self.place(t, vars)?)),
        })
    }

    /// The call of `id`, which [`lends_optional`]: its optional receiver stays a reference.
    fn lent_optional(
        &mut self,
        id: &str,
        args: &[Typed],
        vars: &dyn Vars,
    ) -> Result<String, Error> {
        match (id, args) {
            ("dyn_chain", [o, key]) => {
                let path = self.path(id, args)?;
                let o = self.optional_reference(o, vars)?;
                let key = self.reference(key, true, vars)?;
                Ok(format!("{path}({o}, {key})?"))
            }
            _ => self.call(id, args, vars),
        }
    }

    /// What `let` binds a local to, a reference to `value`: the value it reads where that
    /// outlives the statement, else a temporary the `let` keeps.
    pub(super) fn bound(&mut self, value: &Typed, vars: &dyn Vars) -> Result<String, Error> {
        match rooted(value) {
            true => self.reference(value, false, vars),
            false => Ok(format!("&{}", self.expr(value, vars)?)),
        }
    }
}
