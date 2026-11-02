//! The typed tree as Rust: each node an expression of its type's Rust ([`Emitter::rust`]), a CEL
//! error raised by `?`, so a tree emits into a closure or function returning
//! `cel_emit_runtime::Result`. Every name it writes is generated: cel-emit-runtime as `s`, locals `l0`, a comprehension's element and index `x0` and `n0`, a struct literal's `rec` and
//! `val`; a string is a literal's data, never a name; a message and its fields are named by
//! [`Messages`], as a CEL environment declares a message type. The embedder writes the crate around
//! it: `use ::cel_emit_runtime as s;`, the variables, each struct [`Items::messages`] declares, and
//! [`Items::code`]. A value is owned: what an expression reads, a variable, a
//! local, a field, an element, or what a dyn holds, a call that only reads takes by reference, and
//! any other use clones.

mod comprehension;
mod dynamic;
mod encode;
mod messages;
mod narrow;
mod optional;
mod place;
mod tables;

use crate::functions::{Function, Index, STD};
use crate::tree::{Node, Typed};
use crate::types::{Field, Type};
use crate::{walk, Error};

pub use messages::{Declared, Items, Messages};

/// How the embedder reads each variable: as a Rust place of its type, which may raise an error
/// by `?`, and whether it holds a value, a `bool` expression.
pub trait Vars {
    fn read(&self, var: usize) -> Result<String, Error>;
    fn has(&self, var: usize) -> Result<String, Error>;
}

/// Emits the trees of one crate: each message type once.
pub struct Emitter {
    paths: Index,
    declare: Box<dyn Messages>,
    /// Each message type met, with its Rust.
    messages: Vec<(Vec<Field>, String)>,
    /// Each constant `matches` pattern, once, with its table; `static P{n}`.
    patterns: Vec<(String, Vec<u8>)>,
    /// Comprehensions around what is emitting.
    depth: usize,
}

impl Emitter {
    /// The Rust path running overload `id` on `args`.
    pub(crate) fn path(&self, id: &str, args: &[Typed]) -> Result<String, Error> {
        let types: Vec<Type> = args.iter().map(|a| a.ty.clone()).collect();
        self.paths
            .path(id, &types)
            .ok_or_else(|| Error::Unsupported(format!("no Rust runs {id}")))
    }

    /// Calling `functions` beside cel-emit-runtime's, its messages named by `messages`.
    pub fn new(functions: &[Function], messages: Box<dyn Messages>) -> Emitter {
        Emitter {
            paths: Index::new(functions),
            declare: messages,
            messages: Vec::new(),
            patterns: Vec::new(),
            depth: 0,
        }
    }

    /// What the expressions need: each message's `==`, each pattern's table as a `static`, and
    /// `load_patterns`, which checks them all (only if there are any); and each message to declare.
    pub fn items(&mut self) -> Result<Items, Error> {
        let mut code = self.pattern_items();
        let messages = self.declared(&mut code)?;
        Ok(Items { code, messages })
    }

    /// `ty`'s Rust.
    pub fn rust(&mut self, ty: &Type) -> Result<String, Error> {
        Ok(match ty {
            Type::Int => "i64".into(),
            Type::Uint => "u64".into(),
            Type::Double => "f64".into(),
            Type::Bool => "bool".into(),
            Type::String => "String".into(),
            Type::Bytes => "Vec<u8>".into(),
            Type::Timestamp => format!("{STD}::DateTime<{STD}::Utc>"),
            Type::Duration => format!("{STD}::TimeDelta"),
            Type::Dyn => format!("{STD}::Dyn"),
            Type::Number => format!("{STD}::number::Number"),
            Type::List(t) => format!("Vec<{}>", self.rust(t)?),
            Type::Map(k, v) => format!("Vec<({}, {})>", self.rust(k)?, self.rust(v)?),
            Type::Message(fields) => match self.messages.iter().find(|(f, _)| f == fields) {
                Some((_, rust)) => rust.clone(),
                None => {
                    let rust = self.declare.path(self.messages.len(), fields);
                    self.messages.push((fields.clone(), rust.clone()));
                    rust
                }
            },
            Type::Null => format!("{STD}::Null"),
            Type::Optional(t) | Type::Nullable(t) => format!("Option<{}>", self.rust(t)?),
            Type::Narrow(w) => w.rust().into(),
        })
    }

    /// `t` as a Rust expression of its type.
    pub fn expr(&mut self, t: &Typed, vars: &dyn Vars) -> Result<String, Error> {
        let e = |x: &Typed, me: &mut Emitter| me.expr(x, vars);
        Ok(match &t.node {
            Node::Int(v) => format!("{v}i64"),
            Node::Uint(v) => format!("{v}u64"),
            Node::Double(v) => format!("f64::from_bits({:#x})", v.to_bits()),
            Node::Bool(v) => v.to_string(),
            Node::Str(v) => format!("String::from({v:?})"),
            Node::Bytes(v) => format!("Vec::<u8>::from(&{v:?}[..])"),
            Node::Null => format!("{STD}::Null"),
            Node::Some(x) => format!("Some({})", e(x, self)?),
            Node::None => match &t.ty {
                Type::Optional(of) | Type::Nullable(of) => {
                    format!("None::<{}>", self.rust(of)?)
                }
                _ => return Err(Error::Invalid("a none of other than an optional".into())),
            },
            Node::ListOpt(items) => self.list_opt(items, vars)?,
            Node::MapOpt(entries) => self.map_opt(entries, vars)?,
            Node::Timestamp(s, n) => format!("{STD}::time::timestamp_at({s}i64, {n}u32)?"),
            Node::Duration(s, n) => format!("{STD}::time::duration_at({s}i64, {n}i32)?"),
            // A field reads as a value, so one of a copied type needs no clone.
            Node::Field(..) if t.ty.copied() => place::receiver(&self.place(t, vars)?),
            Node::Var(_) | Node::Local(_) | Node::Item(..) | Node::Field(..) | Node::First(..) => {
                format!("{}.clone()", place::receiver(&self.place(t, vars)?))
            }
            Node::Has(v) => vars.has(*v)?,
            Node::Call(id, args)
                if args.iter().any(|a| a.ty.narrow()) && !narrow::writes_stored(id) =>
            {
                self.stored_call(id, args, vars)?
            }
            Node::Call(..) if place::by_reference(t) => {
                format!("{}.clone()", self.place(t, vars)?)
            }
            Node::Call(id, _) if place::lends_optional(id) => {
                format!("{}.cloned()", self.optional_reference(t, vars)?)
            }
            Node::Call(id, args) => self.call(id, args, vars)?,
            Node::Matches(subject, table, source) => {
                let n = match self.patterns.iter().position(|(_, p)| p == table) {
                    Some(n) => n,
                    None => {
                        self.patterns.push((source.clone(), table.clone()));
                        self.patterns.len().saturating_sub(1)
                    }
                };
                format!(
                    "{STD}::strings::matches_table(&P{n}, {})?",
                    self.reference(subject, true, vars)?
                )
            }
            Node::And(a, b) | Node::Or(a, b) => {
                let f = if matches!(t.node, Node::And(..)) {
                    "and"
                } else {
                    "or"
                };
                let (a, b) = (e(a, self)?, e(b, self)?);
                format!("{STD}::cmp::{f}(|| {STD}::ok({a}), || {STD}::ok({b}))?")
            }
            Node::Cond(c, a, b) => {
                let (c, a, b) = (e(c, self)?, e(a, self)?, e(b, self)?);
                format!("if {c} {{ {a} }} else {{ {b} }}")
            }
            Node::Message(entries) => self.message(t, entries, vars)?,
            Node::Within(x, ty) => {
                let x = e(x, self)?;
                narrow::convert(&place::receiver(&x), ty, narrow::To::Within, 0)
                    .map_or(x, |c| format!("{c}?"))
            }
            Node::Store(x) => {
                let x = e(x, self)?;
                narrow::convert(&place::receiver(&x), &t.ty, narrow::To::Stored, 0)
                    .map_or(x, |c| format!("{c}?"))
            }
            Node::FromDyn(x, ty) => self.read_dyn(x, ty, vars)?,
            Node::Shaped(text, message, rest) => {
                self.shaped(text, message, rest.as_deref(), vars)?
            }
            Node::ToDyn(x) => self.enter_dyn(x, &t.ty, vars)?,
            Node::Written(x, rest) => self.written(x, rest.as_deref(), vars)?,
            Node::Widen(x) => {
                let inner = e(x, self)?;
                narrow::convert(&place::receiver(&inner), &x.ty, narrow::To::Cel, 0)
                    .map_or(inner, |c| format!("{c}?"))
            }
            Node::Position(n) => format!("n{n}"),
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
            Node::Extreme(greatest, map) => {
                let f = if *greatest { "greatest_of" } else { "least_of" };
                format!("{STD}::math::{f}({})?", self.mapped(map, vars)?)
            }
            Node::Sum(map) => format!("{STD}::math::sum_of({})?", self.mapped(map, vars)?),
            Node::Count(map) => {
                format!("{STD}::collections::count({})?", self.mapped(map, vars)?)
            }
            Node::List(items) => {
                let Type::List(of) = &t.ty else {
                    return Err(Error::Invalid("a list literal of other than a list".into()));
                };
                if items.is_empty() {
                    format!("Vec::<{}>::new()", self.rust(of)?)
                } else {
                    let items = items
                        .iter()
                        .map(|a| e(a, self))
                        .collect::<Result<Vec<_>, _>>()?;
                    format!("Vec::from([{}])", items.join(", "))
                }
            }
            Node::MapOf(entries) => {
                if entries.is_empty() {
                    format!("{}::new()", self.rust(&t.ty)?.replacen("Vec<", "Vec::<", 1))
                } else {
                    let mut made = Vec::with_capacity(entries.len());
                    for (k, v) in entries {
                        made.push(format!("({}, {})", e(k, self)?, e(v, self)?));
                    }
                    format!("{STD}::collections::map_of([{}])?", made.join(", "))
                }
            }
            Node::Encode(object) => self.encode(object, vars)?,
            Node::Let(..) => {
                let mut rest = t;
                let mut lets = String::new();
                while let Node::Let(n, value, body) = &rest.node {
                    let read = walk::any(body, |x| *x == Node::Local(*n));
                    let name = if read { format!("l{n}") } else { "_".into() };
                    lets.push_str(&format!("let {name} = {}; ", self.bound(value, vars)?));
                    rest = body;
                }
                format!("{{ {lets}{} }}", e(rest, self)?)
            }
            Node::Fail(error) => {
                let ty = self.rust(&t.ty)?;
                format!("{STD}::fail::<{ty}>({STD}::Error::{error:?})?")
            }
        })
    }

    /// The call of overload `id`.
    fn call(&mut self, id: &str, args: &[Typed], vars: &dyn Vars) -> Result<String, Error> {
        let path = self.path(id, args)?;
        let args = self.args(id, args, vars)?;
        Ok(format!("{path}({})?", args.join(", ")))
    }

    /// The arguments of overload `id`, by reference where it reads them so.
    fn args(&mut self, id: &str, args: &[Typed], vars: &dyn Vars) -> Result<Vec<String>, Error> {
        let types: Vec<Type> = args.iter().map(|a| a.ty.clone()).collect();
        let borrows = self.paths.borrows(id, &types);
        // A list's or map's element type is the function's generic, so text there stays `String`.
        let plain = !args
            .iter()
            .any(|a| matches!(a.ty, Type::List(_) | Type::Map(..)));
        args.iter()
            .map(|a| match borrows && !a.ty.copied() {
                true => self.reference(a, plain, vars),
                false => self.expr(a, vars),
            })
            .collect()
    }
}
