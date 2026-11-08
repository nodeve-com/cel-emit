//! The order written functions check in: a callee before its callers.

use std::collections::BTreeMap;

use cel_core::types::{Expr, SpannedExpr};

use super::Written;
use crate::Error;

/// `all`, each function after those its body calls; a cycle is an error naming it.
pub(super) fn callees_first(all: &[Written]) -> Result<Vec<&Written>, Error> {
    let by_name: BTreeMap<&str, &Written> = all.iter().map(|w| (w.name.as_str(), w)).collect();
    let mut calls: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for w in all {
        let ast = crate::macros::parse(&w.cel);
        if !ast.errors.is_empty() {
            let e = cel_core::CompileError::Parse(ast.errors);
            return Err(Error::Invalid(crate::lower::describe(e)).at(&w.name));
        }
        let mut found = Vec::new();
        if let Some(root) = &ast.ast {
            called(root, &mut found);
        }
        let known = found
            .iter()
            .filter_map(|n| by_name.get_key_value(n.as_str()));
        calls.insert(&w.name, known.map(|(name, _)| *name).collect());
    }
    let mut state = Walk {
        by_name,
        calls,
        done: Vec::new(),
        path: Vec::new(),
    };
    for w in all {
        state.visit(&w.name)?;
    }
    Ok(state.done)
}

struct Walk<'a> {
    by_name: BTreeMap<&'a str, &'a Written>,
    calls: BTreeMap<&'a str, Vec<&'a str>>,
    done: Vec<&'a Written>,
    /// The functions being visited, each called by the one before.
    path: Vec<&'a str>,
}

impl<'a> Walk<'a> {
    fn visit(&mut self, name: &'a str) -> Result<(), Error> {
        if self.done.iter().any(|w| w.name == name) {
            return Ok(());
        }
        if let Some(at) = self.path.iter().position(|n| *n == name) {
            let mut cycle: Vec<&str> = self.path.iter().skip(at).copied().collect();
            cycle.push(name);
            return Err(Error::Invalid(format!(
                "functions call each other: {}",
                cycle.join(" -> ")
            )));
        }
        self.path.push(name);
        for callee in self.calls.get(name).cloned().unwrap_or_default() {
            self.visit(callee)?;
        }
        self.path.pop();
        if let Some(w) = self.by_name.get(name) {
            self.done.push(w);
        }
        Ok(())
    }
}

/// Each function name a call in `e` names, `ns.f` or `f`.
fn called(e: &SpannedExpr, out: &mut Vec<String>) {
    if let Expr::Call { expr, args } = &e.node {
        if let Some(name) = dotted(expr) {
            out.push(name);
        }
        args.iter().for_each(|a| called(a, out));
    }
    each_child(&e.node, &mut |c| called(c, out));
}

/// `e` as `a.b.c`, where it is only names.
fn dotted(e: &SpannedExpr) -> Option<String> {
    match &e.node {
        Expr::Ident(n) => Some(n.clone()),
        Expr::Member { expr, field, .. } => Some(format!("{}.{field}", dotted(expr)?)),
        _ => None,
    }
}

/// Calls `f` on each sub-expression of `e`, except a call's arguments.
fn each_child<'a>(e: &'a Expr, f: &mut dyn FnMut(&'a SpannedExpr)) {
    match e {
        Expr::List(items) => items.iter().for_each(|i| f(&i.expr)),
        Expr::Map(entries) => entries.iter().for_each(|m| {
            f(&m.key);
            f(&m.value);
        }),
        Expr::Unary { expr, .. }
        | Expr::Member { expr, .. }
        | Expr::MemberTestOnly { expr, .. }
        | Expr::Call { expr, .. } => f(expr),
        Expr::Binary { left, right, .. } => {
            f(left);
            f(right);
        }
        Expr::Index { expr, index, .. } => {
            f(expr);
            f(index);
        }
        Expr::Struct { fields, .. } => fields.iter().for_each(|s| f(&s.value)),
        _ => scoped(e, f),
    }
}

/// [`each_child`] for a ternary, a comprehension, and a binding.
fn scoped<'a>(e: &'a Expr, f: &mut dyn FnMut(&'a SpannedExpr)) {
    match e {
        Expr::Ternary {
            cond,
            then_expr,
            else_expr,
        } => [cond, then_expr, else_expr].into_iter().for_each(|x| f(x)),
        Expr::Comprehension(c) => [
            &c.iter_range,
            &c.accu_init,
            &c.loop_condition,
            &c.loop_step,
            &c.result,
        ]
        .into_iter()
        .for_each(|x| f(x)),
        Expr::Bind { init, body, .. } => {
            f(init);
            f(body);
        }
        _ => {}
    }
}
