//! A comprehension cel-core expands `map`, `transformList`, `all`, `exists`, or `exists_one` to,
//! lowered as the fold it is.

use super::*;

impl<'a> Lower<'a> {
    /// The comprehension `c`. A map's binds its key first, then its value; a map of messages'
    /// loops over its loop column.
    pub(crate) fn comprehension(&mut self, c: &'a ComprehensionData) -> Result<Node, Error> {
        self.fold(c, false)
    }

    /// The comprehension `c`; with `valued`, a map's body is lowered as what it fills.
    pub(super) fn fold(&mut self, c: &'a ComprehensionData, valued: bool) -> Result<Node, Error> {
        let (list, over) = self.loop_range(c)?;
        let (kind, body) = fold_of(c)?;
        let second = (!c.iter_var2.is_empty()).then(|| c.iter_var2.clone());
        let (index, item) = match (&over, second) {
            (Over::List(_), None) => (None, Some(c.iter_var.clone())),
            (_, second) => (Some(c.iter_var.clone()), second),
        };
        self.loops.push(Loop { index, item, over });
        let lowered = self.folded(kind, body, valued);
        self.loops.pop();
        fold_node(list, lowered?)
    }

    /// What `c` loops over, and the shape of what it finds there.
    fn loop_range(&mut self, c: &'a ComprehensionData) -> Result<(Box<Typed>, Over), Error> {
        if let Some(root) = self.messages(&c.iter_range) {
            let column = self.column(&root, &self.loop_column(&root)?)?;
            return Ok((Box::new(column), Over::Messages(root)));
        }
        let list = self.expr_as(&c.iter_range, None)?;
        // A stored list is looped over as stored, each element widened as it is read.
        let list = match list.node {
            Node::Widen(stored) => stored,
            _ if list.ty == Type::Dyn => {
                let to = Type::list(Type::Dyn);
                Box::new(Typed::new(to.clone(), Node::FromDyn(Box::new(list), to)))
            }
            _ => Box::new(list),
        };
        let over = match &list.ty {
            Type::List(elem) => Over::List((**elem).clone()),
            Type::Map(key, value) => Over::Map((**key).clone(), (**value).clone()),
            _ => return unsupported("a comprehension over other than a list or map"),
        };
        Ok((list, over))
    }

    /// A comprehension's body, and its predicate, lowered inside it: exists_one's body only
    /// counts.
    fn folded(
        &mut self,
        kind: Fold<'a>,
        body: &'a SpannedExpr,
        valued: bool,
    ) -> Result<Lowered<'a>, Error> {
        let element = |s: &mut Self| match valued {
            true => s.value(body).map(Box::new),
            false => s.sub(body),
        };
        Ok(match kind {
            Fold::Map(Some(pred)) => {
                let pred = self.sub(pred)?;
                Lowered {
                    kind,
                    body: element(self)?,
                    pred: Some(pred),
                }
            }
            Fold::One => Lowered {
                kind,
                body: Box::new(Typed::bool(true)),
                pred: Some(self.sub(body)?),
            },
            _ => Lowered {
                kind,
                body: element(self)?,
                pred: None,
            },
        })
    }
}

/// A fold's body and predicate, lowered.
struct Lowered<'a> {
    kind: Fold<'a>,
    body: Box<Typed>,
    pred: Option<Box<Typed>>,
}

/// The node for `list` folded as `lowered` says.
fn fold_node(list: Box<Typed>, lowered: Lowered) -> Result<Node, Error> {
    let Lowered { kind, body, pred } = lowered;
    Ok(match (kind, pred) {
        (Fold::Map(None), _) => Node::Map(list, body),
        (Fold::Map(Some(_)), Some(pred)) => Node::Keep(list, pred, body),
        (Fold::One, Some(pred)) => {
            let kept = Typed::new(Type::list(Type::Bool), Node::Keep(list, pred, body));
            let count = Typed::new(Type::Int, Node::Count(Box::new(kept)));
            Typed::binary("==", count, Typed::int(1))
                .ok_or_else(|| Error::Invalid("exists_one's count".into()))?
                .node
        }
        (Fold::All(all), _) => Node::All(all, list, body),
        _ => return invalid("a comprehension's predicate"),
    })
}

/// How the macro `c` folds, and the expression it folds each element with.
pub(super) fn fold_of(c: &ComprehensionData) -> Result<(Fold<'_>, &SpannedExpr), Error> {
    let accu = |x: &SpannedExpr| matches!(&x.node, Expr::Ident(n) if *n == c.accu_var);
    let (kind, body) = match (&c.accu_init.node, &c.loop_step.node) {
        (Expr::List(init), _) if init.is_empty() && added(&c.loop_step, accu).is_some() => {
            (Fold::Map(None), added(&c.loop_step, accu))
        }
        (
            Expr::List(init),
            Expr::Ternary {
                cond,
                then_expr,
                else_expr,
            },
        ) if init.is_empty() && accu(else_expr) && added(then_expr, accu).is_some() => {
            (Fold::Map(Some(cond.as_ref())), added(then_expr, accu))
        }
        (
            Expr::Int(0),
            Expr::Ternary {
                cond,
                then_expr,
                else_expr,
            },
        ) if accu(else_expr) && added(then_expr, accu).is_some() && is_one(&c.result, accu) => {
            (Fold::One, Some(cond.as_ref()))
        }
        (Expr::Bool(all), Expr::Binary { op, left, right })
            if accu(right) && (*op == BinaryOp::And) == *all =>
        {
            (Fold::All(*all), Some(left.as_ref()))
        }
        _ => return unsupported("a comprehension but a macro's"),
    };
    let body = body.ok_or_else(|| Error::Invalid("a comprehension without a body".into()))?;
    Ok((kind, body))
}

/// `accu + [one]`: the element `one` added.
fn added(x: &SpannedExpr, accu: impl Fn(&SpannedExpr) -> bool) -> Option<&SpannedExpr> {
    let Expr::Binary { op, left, right } = &x.node else {
        return None;
    };
    if *op != BinaryOp::Add || !accu(left) {
        return None;
    }
    match &right.node {
        Expr::List(items) => match items.as_slice() {
            [one] if !one.optional => Some(&one.expr),
            _ => None,
        },
        Expr::Int(1) => Some(right.as_ref()),
        _ => None,
    }
}

/// `accu == 1`: exists_one's result.
fn is_one(x: &SpannedExpr, accu: impl Fn(&SpannedExpr) -> bool) -> bool {
    match &x.node {
        Expr::Binary { op, left, right } => {
            *op == BinaryOp::Eq && accu(left) && matches!(right.node, Expr::Int(1))
        }
        _ => false,
    }
}

/// What a comprehension folds to: a list of its bodies, kept where a predicate holds; whether
/// exactly one element meets one; or whether every or some element does.
#[derive(Clone, Copy)]
pub(crate) enum Fold<'a> {
    Map(Option<&'a SpannedExpr>),
    One,
    All(bool),
}
