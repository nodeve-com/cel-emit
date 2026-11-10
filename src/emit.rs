//! The typed tree as Rust: each node an expression of its type's Rust ([`Emitter::rust`]), a CEL
//! error raised by `?`, so a tree emits into a closure or function returning
//! `cel_emit_runtime::Result`. Every name it writes is generated: cel-emit-runtime as `s`, locals `l0`, a comprehension's element and index `x0` and `n0`, a struct literal's `rec` and
//! `val`, a computed property's `at`; a string is a literal's data, never a name; a message and its fields are named by
//! [`Messages`], as a CEL environment declares a message type. The embedder writes the crate around
//! it: `use ::cel_emit_runtime as s;`, the variables, each struct [`Items::messages`] declares, and
//! [`Items::code`]. A value is owned: what an expression reads, a variable, a
//! local, a field, an element, or what a dyn holds, a call that only reads takes by reference, and
//! any other use clones.

mod comprehension;
mod dynamic;
mod encode;
mod encode_parts;
mod messages;
mod narrow;
mod node;
mod optional;
mod place;
mod tables;
mod written;

use crate::functions::{Function, Index, Pass, STD};
use crate::tree::{Node, Typed};
use crate::types::{Message, Type};
use crate::{walk, Error};

pub use messages::{Declared, Items, Messages};

/// How the embedder reads each variable: as a Rust place of its type, which may raise an error
/// by `?`.
pub trait Vars {
    fn read(&self, var: usize) -> Result<String, Error>;
}

/// Emits the trees of one crate: each message type once.
pub struct Emitter {
    paths: Index,
    declare: Box<dyn Messages>,
    /// Each message type met, with its Rust.
    messages: Vec<(Message, String)>,
    /// Each constant `matches` pattern, once, with its table; `static P{n}`.
    patterns: Vec<(String, Vec<u8>)>,
    /// Comprehensions around what is emitting.
    depth: usize,
    /// Each function written in CEL, emitted by [`Emitter::items`].
    defined: Vec<crate::written::Body>,
    /// The ids of those returning a reference to a part of their arguments.
    lent: Vec<String>,
    /// Each message type an expression compares, and those nested in it: each gets `==`.
    compared: Vec<Message>,
    /// The record place the value about to emit fills, where a computed property's failure names
    /// one: a struct literal there notes each field's place in `at` before computing it.
    pub(crate) at: Option<String>,
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
            defined: Vec::new(),
            lent: Vec::new(),
            compared: Vec::new(),
            at: None,
        }
    }

    /// What the expressions need: each message's `==`, each pattern's table as a `static`, and
    /// `load_patterns`, which checks them all (only if there are any); and each message to declare.
    pub fn items(&mut self) -> Result<Items, Error> {
        let functions = self.written_items()?;
        let mut code = self.pattern_items();
        code.push_str(&functions);
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
            Type::Dyn => format!("{STD}::Value"),
            Type::Number => format!("{STD}::number::Number"),
            Type::List(t) => format!("Vec<{}>", self.rust(t)?),
            Type::Map(k, v) => format!("Vec<({}, {})>", self.rust(k)?, self.rust(v)?),
            Type::Message(m) => match self.messages.iter().find(|(f, _)| f == m) {
                Some((_, rust)) => rust.clone(),
                None => {
                    let rust = self.declare.path(self.messages.len(), m);
                    self.messages.push((m.clone(), rust.clone()));
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
        // Only a value that is the place's own keeps it: a conditional's branch, a binding's
        // body, a struct literal.
        if !matches!(t.node, Node::Cond(..) | Node::Let(..) | Node::Message(..)) {
            self.at = None;
        }
        match &t.node {
            walk::literal!() => node::literal(t),
            walk::access!() => self.access(t, vars),
            walk::control!() => self.control(t, vars),
            walk::converting!() => self.converted(t, vars),
            walk::iterating!() => self.comprehension(t, vars),
            walk::collecting!() => self.collection(t, vars),
        }
    }

    /// The call of overload `id`.
    fn call(&mut self, id: &str, args: &[Typed], vars: &dyn Vars) -> Result<String, Error> {
        if crate::stdlib::uses_equality(id) {
            args.iter().for_each(|a| self.compares(&a.ty));
        }
        if let Some(x) = self.own_text(id, args) {
            return Ok(format!("{STD}::json::own(&({}))?", self.expr(x, vars)?));
        }
        let path = self.path(id, args)?;
        let types: Vec<Type> = args.iter().map(|a| a.ty.clone()).collect();
        let pass = self.paths.pass(id, &types);
        let args = self.args(id, args, vars)?.join(", ");
        Ok(match pass {
            Pass::Each => format!("{path}({args})?"),
            Pass::Array => format!("{path}([{args}])?"),
        })
    }

    /// The arguments of overload `id`, by reference where it reads them so.
    fn args(&mut self, id: &str, args: &[Typed], vars: &dyn Vars) -> Result<Vec<String>, Error> {
        let types: Vec<Type> = args.iter().map(|a| a.ty.clone()).collect();
        let borrows = self.paths.borrows(id, &types);
        // A list's or map's element type is the function's generic, so text there stays `String`.
        let plain = !self.defines(id)
            && !args
                .iter()
                .any(|a| matches!(a.ty, Type::List(_) | Type::Map(..)));
        args.iter()
            .map(|a| match (borrows, a.ty.copied(), self.by_reference(a)) {
                (_, true, true) => Ok(format!("*{}", self.place(a, vars)?)),
                (true, false, _) => self.reference(a, plain, vars),
                _ => self.expr(a, vars),
            })
            .collect()
    }
}
