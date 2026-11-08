//! The forms lowering shapes itself: folds over a `map`, sizes, optionals, and comprehensions.

use super::*;

impl<'a> Lower<'a> {
    /// `e` as one of the forms lowering shapes itself, if it is one: a fold over a `map`, a
    /// map of messages' size, or an optional's call ([`Lower::optional_form`]).
    pub(crate) fn special(&mut self, e: &'a SpannedExpr) -> Result<Option<Node>, Error> {
        let Expr::Call { expr: callee, args } = &e.node else {
            return Ok(None);
        };
        let Some((name, receiver)) = self.callee_name(e, callee) else {
            return Ok(None);
        };
        if let ("json.encode", None, [arg]) = (name.as_str(), receiver, args.as_slice()) {
            return self.encoded(arg).map(Some);
        }
        if let Some(node) = self.dyn_form(&name, receiver, args)? {
            return Ok(Some(node));
        }
        self.folding_call(&name, receiver, args)
    }

    /// The name `callee` calls in `e`, a method's or a function's, and a method's receiver.
    fn callee_name(
        &self,
        e: &SpannedExpr,
        callee: &'a SpannedExpr,
    ) -> Option<(String, Option<&'a SpannedExpr>)> {
        match &callee.node {
            Expr::Member { expr, field, .. } if !self.namespace(e, callee) => {
                Some((field.clone(), Some(expr.as_ref())))
            }
            Expr::Ident(name) => Some((name.clone(), None)),
            Expr::Member { expr, field, .. } => match &expr.node {
                Expr::Ident(ns) => Some((format!("{ns}.{field}"), None)),
                _ => None,
            },
            _ => None,
        }
    }

    /// A fold over a `map`, a map of messages' size, or an optional's call.
    fn folding_call(
        &mut self,
        name: &str,
        receiver: Option<&'a SpannedExpr>,
        args: &'a [SpannedExpr],
    ) -> Result<Option<Node>, Error> {
        let mapped = |l: &SpannedExpr| matches!(l.node, Expr::Comprehension(_));
        let list = match (receiver, args) {
            (Some(list), []) | (None, [list]) if mapped(list) => Some(list),
            _ => None,
        };
        match (list, Reduce::named(name)) {
            (Some(list), Some(reduce)) => Ok(self.mapped(list)?.map(|l| Node::Reduce(reduce, l))),
            _ => self.optional_form(name, receiver, args),
        }
    }

    /// `list`, a comprehension, where it lowers to a [`Node::Map`] or [`Node::Keep`]: a fold folds
    /// it as it goes.
    pub(crate) fn mapped(&mut self, list: &'a SpannedExpr) -> Result<Option<Box<Typed>>, Error> {
        let list = self.sub(list)?;
        Ok(matches!(list.node, Node::Map(..) | Node::Keep(..)).then_some(list))
    }
}

/// `t`, where it is `list.filter(x, pred)[0]`, as the first match: `depth` comprehensions around it.
/// A list that widens each element, or a `pred` reading the index, stays as it is.
pub(crate) fn first_match(t: Typed, depth: usize) -> Typed {
    let Node::Call(id, args) = &t.node else {
        return t;
    };
    let ([list, index], "index_list") = (args.as_slice(), id.as_str()) else {
        return t;
    };
    let Node::Keep(over, pred, body) = &list.node else {
        return t;
    };
    let whole = body.node == Node::Item(depth, Part::Whole);
    let indexed = crate::walk::any(pred, |n| *n == Node::Position(depth));
    if index.node != Node::Int(0) || !whole || indexed {
        return t;
    }
    let node = Node::First(over.clone(), pred.clone());
    Typed { node, ..t }
}
