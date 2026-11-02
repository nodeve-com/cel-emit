//! Parsing with the macros cel-core lacks. `xs.sortBy(x, key)` is cel-go's lists extension
//! macro; it expands to a call of a hidden function over `xs` and `xs.map(x, key)`, with `xs`
//! bound so it evaluates once. Every place that parses CEL goes through here.

use cel_core::parser::{
    ArgCount, Macro, MacroContext, MacroExpansion, MacroRegistry, MacroStyle, ParseOptions,
    ParseResult,
};
use cel_core::types::{Expr, Span, Spanned, SpannedExpr};
use cel_core::{Ast, CompileError, Env};

/// The hidden function `sortBy` expands to: a list and one key per element.
pub(crate) const SORT_BY_KEYS: &str = "@sortByAssociatedKeys";

/// What the receiver is bound to while the expansion runs.
const HELD: &str = "@sortByInput";

/// The standard macros and `sortBy`.
fn registry() -> MacroRegistry {
    let mut macros = MacroRegistry::standard();
    macros.register(Macro::new(
        "sortBy",
        MacroStyle::Receiver,
        ArgCount::Exact(2),
        expand_sort_by,
    ));
    macros
}

/// `source` parsed, its macros expanded.
fn parse(source: &str) -> ParseResult {
    cel_core::parser::parse_with_options(source, ParseOptions::with_macros(registry()))
}

/// `source` parsed here and checked against `env`: what [`Env::compile`] does with `sortBy`.
pub fn compile(env: &Env, source: &str) -> Result<Ast, CompileError> {
    let parsed = parse(source);
    if !parsed.errors.is_empty() {
        return Err(CompileError::Parse(parsed.errors));
    }
    let expr = parsed.ast.ok_or(CompileError::NoAst)?;
    let checked = env.check(&expr);
    if !checked.errors.is_empty() {
        return Err(CompileError::Check(checked.errors));
    }
    Ok(Ast::new_checked(expr, source, checked))
}

fn node(ctx: &mut MacroContext, expr: Expr, span: &Span) -> SpannedExpr {
    Spanned::new(ctx.next_id(), expr, span.clone())
}

fn ident(ctx: &mut MacroContext, name: &str, span: &Span) -> SpannedExpr {
    node(ctx, Expr::Ident(name.into()), span)
}

/// `xs.sortBy(x, key)` to `cel.bind(held, xs, sort(held, held.map(x, key)))`.
fn expand_sort_by(
    ctx: &mut MacroContext,
    span: Span,
    receiver: Option<SpannedExpr>,
    args: Vec<SpannedExpr>,
) -> MacroExpansion {
    let (Some(xs), [var, _]) = (receiver, args.as_slice()) else {
        return MacroExpansion::Error("sortBy() takes a list, a name, and a key".into());
    };
    if !matches!(var.node, Expr::Ident(_)) {
        return MacroExpansion::Error("sortBy() first argument must be an identifier".into());
    }
    let Some(map) = MacroRegistry::standard()
        .lookup("map", 2, true)
        .map(|m| m.expander)
    else {
        return MacroExpansion::Error("sortBy() needs the map macro".into());
    };
    let held = ident(ctx, HELD, &span);
    let keys = match map(ctx, span.clone(), Some(held), args) {
        MacroExpansion::Expanded(keys) => keys,
        error => return error,
    };
    let held = ident(ctx, HELD, &span);
    let callee = ident(ctx, SORT_BY_KEYS, &span);
    let call = node(
        ctx,
        Expr::Call {
            expr: Box::new(callee),
            args: vec![held, keys],
        },
        &span,
    );
    MacroExpansion::Expanded(node(
        ctx,
        Expr::Bind {
            var_name: HELD.into(),
            init: Box::new(xs),
            body: Box::new(call),
        },
        &span,
    ))
}
