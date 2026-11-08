//! Lowering: cel-core's checked tree walked into the typed tree, one `Lower` per expression.

use std::collections::BTreeMap;

use cel_core::types::{BinaryOp, ComprehensionData, Expr, SpannedExpr};
use cel_core::{CelType, CheckResult};

use crate::decls::{At, Decls, Name};
use crate::env::{environment, register};
use crate::tree::{Node, Part, Typed};
use crate::types::{of_cel, Type};
use crate::Error;

mod calls;
mod digits;
mod dynamic;
mod empty;
mod encode;
mod enums;
mod expr;
mod extreme;
mod fold;
mod forms;
mod has;
mod literal;
mod message;
mod names;
mod nullable;
mod number;
mod optional;
mod parts;
mod pattern;
mod result;
mod scope;
mod select;
mod share;
mod special;
mod value;

pub use calls::{literal, ms};
pub use result::lower_as;

/// `source`, checked against `decls` and lowered: one value, which a message is too.
pub fn lower(source: &str, decls: &Decls) -> Result<Typed, Error> {
    lowered(source, decls).map(|l| l.tree)
}

/// How the root of an expression lowers.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Root {
    /// A map literal of string keys is a message of its entries.
    Value,
    /// A map literal is a map.
    Map,
}

/// An expression lowered, and what its tree no longer says.
#[derive(Clone, Debug, PartialEq)]
pub struct Lowered {
    pub tree: Typed,
    /// Whether it yields a map literal: its result, through `cel.bind` and either branch of a
    /// ternary. One lowers to a message, as a message read whole does.
    pub map: bool,
}

/// [`lower`], keeping what the tree drops.
pub fn lowered(source: &str, decls: &Decls) -> Result<Lowered, Error> {
    lowered_at(source, decls, Root::Value)
}

pub(crate) fn lowered_at(source: &str, decls: &Decls, at: Root) -> Result<Lowered, Error> {
    if !decls.written.is_empty() {
        return lowered_at(source, &crate::written::resolved(decls)?, at);
    }
    let env = environment(decls);
    let params = crate::functions::params(env.functions().values());
    let ast = crate::macros::compile(&env, source).map_err(|e| Error::Invalid(describe(e)))?;
    let info = ast
        .type_info()
        .ok_or_else(|| Error::Invalid("not checked".into()))?;
    let root = ast.expr();
    let mut messages = BTreeMap::new();
    for n in decls.names.values() {
        register(&n.ty, &mut messages);
    }
    for ty in decls.messages.values() {
        register(ty, &mut messages);
    }
    let mut lower = Lower {
        info,
        decls,
        loops: Vec::new(),
        binds: Vec::new(),
        next: 0,
        messages,
        paths: crate::functions::Index::new(&decls.functions),
        params,
    };
    let tree = match at {
        Root::Value => lower.value(root)?,
        Root::Map => lower.expr(root)?,
    };
    let tree = lower.share(tree)?;
    let map = yields_map(root);
    Ok(Lowered { tree, map })
}

/// Whether `e`'s result is a map literal.
fn yields_map(e: &SpannedExpr) -> bool {
    match &e.node {
        Expr::Map(_) => true,
        Expr::Bind { body, .. } => yields_map(body),
        Expr::Ternary {
            then_expr,
            else_expr,
            ..
        } => yields_map(then_expr) || yields_map(else_expr),
        _ => false,
    }
}

/// A CEL compile error's messages, joined.
pub(crate) fn describe(e: cel_core::CompileError) -> String {
    match e {
        cel_core::CompileError::Parse(errs) => errs
            .iter()
            .map(|e| e.message.clone())
            .collect::<Vec<_>>()
            .join("; "),
        cel_core::CompileError::Check(errs) => errs
            .iter()
            .map(|e| e.message())
            .collect::<Vec<_>>()
            .join("; "),
        other => format!("{other:?}"),
    }
}

pub(crate) struct Lower<'a> {
    pub(crate) info: &'a CheckResult,
    pub(crate) decls: &'a Decls,
    /// Each comprehension around what is lowering, outermost first.
    pub(crate) loops: Vec<Loop>,
    /// Each `cel.bind` around what is lowering, outermost first.
    pub(crate) binds: Vec<scope::Bind<'a>>,
    /// The next local's number.
    pub(crate) next: usize,
    /// Each message type by its message name.
    pub(crate) messages: BTreeMap<String, Type>,
    /// The Rust running each overload, and the parameters each declares.
    pub(crate) paths: crate::functions::Index,
    pub(crate) params: std::collections::HashMap<String, Vec<CelType>>,
}

/// A comprehension: what it binds, and what it loops over.
pub(crate) struct Loop {
    /// A list's index, a map's key.
    pub(crate) index: Option<String>,
    /// A list's element, a map's value.
    pub(crate) item: Option<String>,
    pub(crate) over: Over,
}

pub(crate) enum Over {
    /// A list of elements of this type.
    List(Type),
    /// A map of keys and values of these types, as stored.
    Map(Type, Type),
}

pub(crate) fn unsupported<T>(what: impl Into<String>) -> Result<T, Error> {
    Err(Error::Unsupported(what.into()))
}

pub(crate) fn invalid<T>(what: impl Into<String>) -> Result<T, Error> {
    Err(Error::Invalid(what.into()))
}

pub(crate) fn kind(e: &Expr) -> &'static str {
    match e {
        Expr::Null => "null",
        Expr::String(_) => "strings",
        Expr::Bytes(_) => "bytes",
        Expr::List(_) => "lists",
        Expr::Map(_) => "maps",
        Expr::Index { .. } => "indexing",
        Expr::Struct { .. } => "a message where a value is wanted",
        Expr::Comprehension(_) => "comprehensions",
        Expr::MemberTestOnly { .. } => "has()",
        _ => "this expression",
    }
}

impl<'a> Lower<'a> {
    pub(crate) fn ty(&self, e: &SpannedExpr) -> Result<Type, Error> {
        match self.info.get_type(e.id) {
            Some(cel) => match of_cel(cel, &self.messages) {
                Some(ty) => Ok(ty),
                None => unsupported(format!("type {}", cel.display_name())),
            },
            None => invalid("an expression without a type"),
        }
    }

    pub(crate) fn sub(&mut self, e: &'a SpannedExpr) -> Result<Box<Typed>, Error> {
        self.expr(e).map(Box::new)
    }
}
