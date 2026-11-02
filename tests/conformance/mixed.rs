//! A case whose result is a `math.greatest` or `math.least` call over literal int, uint, and
//! double arguments of more than one kind, not wrapped in a conversion. Typed CEL gives such a
//! call a number no declaration holds, so cel-emit takes it only under a cast. Found in the case's
//! expression alone: its call name, and its arguments' literal kinds.

use cel_core::types::UnaryOp;
use cel_core::{Expr, SpannedExpr};
use std::collections::BTreeSet;

use super::scope::{dotted, MIXED_EXTREME};

/// Why the case's result is a bare mixed-kind extreme, if it is.
pub fn bare(ast: &SpannedExpr) -> Option<&'static str> {
    let Expr::Call { expr, args } = &ast.node else {
        return None;
    };
    if !matches!(
        dotted(expr).as_deref(),
        Some("math.greatest" | "math.least")
    ) {
        return None;
    }
    let kinds: Option<BTreeSet<&str>> = args.iter().map(literal_kind).collect();
    (kinds?.len() > 1).then_some(MIXED_EXTREME)
}

/// The kind of a numeric literal, a negated one included: `int`, `uint`, or `double`.
fn literal_kind(e: &SpannedExpr) -> Option<&'static str> {
    match &e.node {
        Expr::Int(_) => Some("int"),
        Expr::UInt(_) => Some("uint"),
        Expr::Float(_) => Some("double"),
        Expr::Unary {
            op: UnaryOp::Neg,
            expr,
        } => match &expr.node {
            Expr::Int(_) => Some("int"),
            Expr::Float(_) => Some("double"),
            _ => None,
        },
        _ => None,
    }
}
