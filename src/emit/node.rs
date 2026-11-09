//! The typed tree's nodes as Rust, by the kind of node: reads, control, conversions,
//! comprehensions, and collections.

use super::{narrow, place, Emitter, Vars};
use crate::functions::STD;
use crate::tree::{Node, Typed};
use crate::types::Type;
use crate::{walk, Error};

impl Emitter {
    /// A variable, local, element, field, call, or test that reads a value.
    pub(super) fn access(&mut self, t: &Typed, vars: &dyn Vars) -> Result<String, Error> {
        Ok(match &t.node {
            // A field reads as a value, so one of a copied type needs no clone.
            Node::Field(..) if t.ty.copied() => place::receiver(&self.place(t, vars)?),
            Node::Call(..) => self.call_node(t, vars)?,
            Node::Matches(subject, table, source) => {
                let n = self.pattern(table, source);
                format!(
                    "{STD}::strings::matches_table(&P{n}, {})?",
                    self.reference(subject, true, vars)?
                )
            }
            _ if t.ty.copied() && self.by_reference(t) => format!("*{}", self.place(t, vars)?),
            _ => format!("{}.clone()", place::receiver(&self.place(t, vars)?)),
        })
    }

    /// The number of the table for a constant `matches` pattern, adding it once.
    fn pattern(&mut self, table: &[u8], source: &str) -> usize {
        if let Some(n) = self.patterns.iter().position(|(_, p)| p == table) {
            return n;
        }
        self.patterns.push((source.into(), table.to_vec()));
        self.patterns.len().saturating_sub(1)
    }

    /// The call of overload `id` that `t` is: through a stored collection's widening, a place it
    /// lends, or on its arguments.
    fn call_node(&mut self, t: &Typed, vars: &dyn Vars) -> Result<String, Error> {
        let Node::Call(id, args) = &t.node else {
            return not_a("call", t);
        };
        if narrow::widens_each(id) && args.iter().any(|a| a.ty.narrow()) {
            return self.stored_call(id, args, vars);
        }
        if self.by_reference(t) {
            return Ok(format!("{}.clone()", self.place(t, vars)?));
        }
        if place::lends_optional(id) {
            return Ok(format!("{}.cloned()", self.optional_reference(t, vars)?));
        }
        self.call(id, args, vars)
    }

    /// The message `json.encode` writes, where it serializes itself: its text is its
    /// `Serialize`'s, with no dyn between.
    pub(super) fn own_text<'t>(&self, id: &str, args: &'t [Typed]) -> Option<&'t Typed> {
        let [Typed {
            node: Node::Written(x),
            ..
        }] = args
        else {
            return None;
        };
        let Type::Message(message) = &x.ty else {
            return None;
        };
        (id == "json_encode" && self.declare.serializes(message)).then_some(&**x)
    }

    /// `&&`, `||`, a conditional, a binding, and a failure.
    pub(super) fn control(&mut self, t: &Typed, vars: &dyn Vars) -> Result<String, Error> {
        Ok(match &t.node {
            Node::And(a, b) | Node::Or(a, b) => {
                let f = if matches!(t.node, Node::And(..)) {
                    "and"
                } else {
                    "or"
                };
                let (a, b) = (self.expr(a, vars)?, self.expr(b, vars)?);
                format!("{STD}::cmp::{f}(|| {STD}::ok({a}), || {STD}::ok({b}))?")
            }
            Node::Cond(c, a, b) => {
                let (c, a, b) = (
                    self.expr(c, vars)?,
                    self.expr(a, vars)?,
                    self.expr(b, vars)?,
                );
                format!("if {c} {{ {a} }} else {{ {b} }}")
            }
            Node::Let(..) => self.lets(t, vars)?,
            Node::Fail(error) => {
                let ty = self.rust(&t.ty)?;
                format!("{STD}::fail::<{ty}>({STD}::Error::{error:?})?")
            }
            _ => return not_a("control", t),
        })
    }

    /// The bindings `t` starts with, around the body they end in.
    fn lets(&mut self, t: &Typed, vars: &dyn Vars) -> Result<String, Error> {
        let mut rest = t;
        let mut lets = String::new();
        while let Node::Let(n, value, body) = &rest.node {
            let read = walk::any(body, |x| *x == Node::Local(*n));
            let name = if read { format!("l{n}") } else { "_".into() };
            lets.push_str(&format!("let {name} = {}; ", self.bound(value, vars)?));
            rest = body;
        }
        Ok(format!("{{ {lets}{} }}", self.expr(rest, vars)?))
    }

    /// A value changing between stored, CEL, and dyn forms, or written as JSON.
    pub(super) fn converted(&mut self, t: &Typed, vars: &dyn Vars) -> Result<String, Error> {
        Ok(match &t.node {
            Node::Within(x, ty) => cast(self.expr(x, vars)?, ty, narrow::To::Within),
            Node::Store(x) => cast(self.expr(x, vars)?, &t.ty, narrow::To::Stored),
            Node::Widen(x) => cast(self.expr(x, vars)?, &x.ty, narrow::To::Cel),
            Node::FromDyn(x, ty) => self.read_dyn(x, ty, vars)?,
            Node::ToDyn(x) => self.enter_dyn(x, &t.ty, vars)?,
            Node::Written(x) => self.written(x, vars)?,
            Node::Encode(object) => self.encode(object, vars)?,
            Node::EncodeEach(..) => self.encode_each(t, vars)?,
            _ => return not_a("conversion", t),
        })
    }

    /// A comprehension, or a fold over one.
    pub(super) fn comprehension(&mut self, t: &Typed, vars: &dyn Vars) -> Result<String, Error> {
        Ok(match &t.node {
            Node::Map(..) | Node::Keep(..) => {
                format!(
                    "{}.collect::<{STD}::Result<Vec<_>>>()?",
                    self.mapped(t, vars)?
                )
            }
            Node::All(every, list, body) => {
                let (iter, bind) = self.over(list, &[body], vars)?;
                let body = self.inside(body, vars)?;
                format!("{STD}::collections::all({iter}, {every}, |{bind}| {STD}::ok({body}))?")
            }
            Node::Reduce(reduce, map) => {
                format!("{STD}::{}({})?", reduce.path(), self.mapped(map, vars)?)
            }
            _ => return not_a("comprehension", t),
        })
    }

    /// An optional, or a list, map, or message literal.
    pub(super) fn collection(&mut self, t: &Typed, vars: &dyn Vars) -> Result<String, Error> {
        Ok(match &t.node {
            Node::Some(x) => format!("Some({})", self.expr(x, vars)?),
            Node::None => match &t.ty {
                Type::Optional(of) | Type::Nullable(of) => {
                    format!("None::<{}>", self.rust(of)?)
                }
                _ => return Err(Error::Invalid("a none of other than an optional".into())),
            },
            Node::ListOpt(items) => self.list_opt(items, vars)?,
            Node::MapOpt(entries) => self.map_opt(entries, vars)?,
            Node::Message(entries) => self.message(t, entries, vars)?,
            Node::List(items) => self.list_literal(t, items, vars)?,
            Node::MapOf(entries) => self.map_literal(t, entries, vars)?,
            _ => return not_a("collection", t),
        })
    }

    /// The list literal `t` of `items`.
    fn list_literal(
        &mut self,
        t: &Typed,
        items: &[Typed],
        vars: &dyn Vars,
    ) -> Result<String, Error> {
        let Type::List(of) = &t.ty else {
            return Err(Error::Invalid("a list literal of other than a list".into()));
        };
        if items.is_empty() {
            return Ok(format!("Vec::<{}>::new()", self.rust(of)?));
        }
        let items = items
            .iter()
            .map(|a| self.expr(a, vars))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(format!("Vec::from([{}])", items.join(", ")))
    }

    /// The map literal `t` of `entries`.
    fn map_literal(
        &mut self,
        t: &Typed,
        entries: &[(Typed, Typed)],
        vars: &dyn Vars,
    ) -> Result<String, Error> {
        if entries.is_empty() {
            return Ok(format!(
                "{}::new()",
                self.rust(&t.ty)?.replacen("Vec<", "Vec::<", 1)
            ));
        }
        let mut made = Vec::with_capacity(entries.len());
        for (k, v) in entries {
            made.push(format!(
                "({}, {})",
                self.expr(k, vars)?,
                self.expr(v, vars)?
            ));
        }
        Ok(format!(
            "{STD}::collections::map_of([{}])?",
            made.join(", ")
        ))
    }
}

/// A node with no child, as Rust.
pub(super) fn literal(t: &Typed) -> Result<String, Error> {
    Ok(match &t.node {
        Node::Int(v) => format!("{v}i64"),
        Node::Uint(v) => format!("{v}u64"),
        Node::Double(v) => format!("f64::from_bits({:#x})", v.to_bits()),
        Node::Bool(v) => v.to_string(),
        Node::Str(v) => format!("String::from({v:?})"),
        Node::Bytes(v) => format!("Vec::<u8>::from(&{v:?}[..])"),
        Node::Null => format!("{STD}::Null"),
        Node::Timestamp(s, n) => format!("{STD}::time::timestamp_at({s}i64, {n}u32)?"),
        Node::Duration(s, n) => format!("{STD}::time::duration_at({s}i64, {n}i32)?"),
        Node::Position(n) => format!("n{n}"),
        _ => return not_a("literal", t),
    })
}

/// The error for a node the group of its emitting does not hold.
fn not_a(group: &str, t: &Typed) -> Result<String, Error> {
    Err(Error::Invalid(format!(
        "a {} node as a {group}",
        t.ty.display()
    )))
}

/// The Rust `x`, converted as `to` says for type `ty`, if it needs it.
fn cast(x: String, ty: &Type, to: narrow::To) -> String {
    narrow::convert(&place::receiver(&x), ty, to, 0).map_or(x, |c| format!("{c}?"))
}
