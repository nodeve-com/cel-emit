//! The forms lowering shapes itself: folds over a `map`, sizes, optionals, and comprehensions.

use super::*;

impl<'a> Lower<'a> {
    /// `e` as one of the forms lowering shapes itself, if it is one: a fold over a `map`, a
    /// map of messages' size, or an optional's call ([`Lower::optional_form`]).
    pub(crate) fn special(&mut self, e: &'a SpannedExpr) -> Result<Option<Node>, Error> {
        let Expr::Call { expr: callee, args } = &e.node else {
            return Ok(None);
        };
        let method = match &callee.node {
            Expr::Member { expr, field, .. } if !self.namespace(e, callee) => Some((expr, field)),
            _ => None,
        };
        let name = match (&callee.node, method) {
            (_, Some((_, field))) => field.clone(),
            (Expr::Ident(name), _) => name.clone(),
            (Expr::Member { expr, field, .. }, _) => match &expr.node {
                Expr::Ident(ns) => format!("{ns}.{field}"),
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };
        let receiver = method.map(|(r, _)| r.as_ref());
        if let ("json.decode", None, [text, _]) = (name.as_str(), receiver, args.as_slice()) {
            return self.shaped(e, text).map(Some);
        }
        if let ("json.encode", None, [arg]) = (name.as_str(), receiver, args.as_slice()) {
            return self.encoded(arg).map(Some);
        }
        if let Some(node) = self.dyn_form(&name, receiver, args)? {
            return Ok(Some(node));
        }
        let mapped = |l: &SpannedExpr| matches!(l.node, Expr::Comprehension(_));
        let node = match (name.as_str(), receiver, args.as_slice()) {
            ("math.greatest" | "math.least", None, [list]) if mapped(list) => {
                match self.mapped(list)? {
                    Some(list) => Node::Extreme(name == "math.greatest", list),
                    None => return Ok(None),
                }
            }
            ("math.sum", None, [list]) if mapped(list) => match self.mapped(list)? {
                Some(list) => Node::Sum(list),
                None => return Ok(None),
            },
            ("size", Some(list), []) | ("size", None, [list]) if self.messages(list).is_some() => {
                let root = self.messages(list).unwrap_or_default();
                let column = self.column(&root, &self.loop_column(&root)?)?;
                Node::Call("size_map".into(), vec![column])
            }
            ("size", Some(list), []) | ("size", None, [list]) if mapped(list) => {
                match self.mapped(list)? {
                    Some(list) => Node::Count(list),
                    None => return Ok(None),
                }
            }
            _ => match self.optional_form(&name, receiver, args)? {
                Some(node) => node,
                None => return Ok(None),
            },
        };
        Ok(Some(node))
    }

    /// `list`, a comprehension, where it lowers to a [`Node::Map`] or [`Node::Keep`]: a fold folds
    /// it as it goes.
    pub(crate) fn mapped(&mut self, list: &'a SpannedExpr) -> Result<Option<Box<Typed>>, Error> {
        let list = self.sub(list)?;
        Ok(matches!(list.node, Node::Map(..) | Node::Keep(..)).then_some(list))
    }

    /// A comprehension cel-core expands `map`, `transformList`, `all`, or `exists` to.
    /// A map's binds its key first, then its value; a map of messages' loops over its loop column.
    pub(crate) fn comprehension(&mut self, c: &'a ComprehensionData) -> Result<Node, Error> {
        let (list, over) = match self.messages(&c.iter_range) {
            Some(root) => (
                Box::new(self.column(&root, &self.loop_column(&root)?)?),
                Over::Messages(root),
            ),
            None => {
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
                (list, over)
            }
        };
        let accu = |x: &SpannedExpr| matches!(&x.node, Expr::Ident(n) if *n == c.accu_var);
        // `accu + [one]`: the element `one` added.
        let added = |x: &'a SpannedExpr| match &x.node {
            Expr::Binary { op, left, right } if *op == BinaryOp::Add && accu(left) => {
                match &right.node {
                    Expr::List(items) => match items.as_slice() {
                        [one] if !one.optional => Some(&one.expr),
                        _ => None,
                    },
                    Expr::Int(1) => Some(right.as_ref()),
                    _ => None,
                }
            }
            _ => None,
        };
        // `accu == 1`: exists_one's result.
        let one = |x: &SpannedExpr| match &x.node {
            Expr::Binary { op, left, right } => {
                *op == BinaryOp::Eq && accu(left) && matches!(right.node, Expr::Int(1))
            }
            _ => false,
        };
        let (kind, body) = match (&c.accu_init.node, &c.loop_step.node) {
            (Expr::List(init), _) if init.is_empty() && added(&c.loop_step).is_some() => {
                (Fold::Map(None), added(&c.loop_step))
            }
            (
                Expr::List(init),
                Expr::Ternary {
                    cond,
                    then_expr,
                    else_expr,
                },
            ) if init.is_empty() && accu(else_expr) && added(then_expr).is_some() => {
                (Fold::Map(Some(cond.as_ref())), added(then_expr))
            }
            (
                Expr::Int(0),
                Expr::Ternary {
                    cond,
                    then_expr,
                    else_expr,
                },
            ) if accu(else_expr) && added(then_expr).is_some() && one(&c.result) => {
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
        let second = (!c.iter_var2.is_empty()).then(|| c.iter_var2.clone());
        let (index, item) = match (&over, second) {
            (Over::List(_), None) => (None, Some(c.iter_var.clone())),
            (_, second) => (Some(c.iter_var.clone()), second),
        };
        self.loops.push(Loop { index, item, over });
        let lowered = self.folded(kind, body);
        self.loops.pop();
        let (kind, body, pred) = lowered?;
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

    /// A comprehension's body, and its predicate, lowered inside it: exists_one's body only
    /// counts.
    #[allow(clippy::type_complexity)]
    fn folded(
        &mut self,
        kind: Fold<'a>,
        body: &'a SpannedExpr,
    ) -> Result<(Fold<'a>, Box<Typed>, Option<Box<Typed>>), Error> {
        Ok(match kind {
            Fold::Map(Some(pred)) => {
                let pred = self.sub(pred)?;
                (kind, self.sub(body)?, Some(pred))
            }
            Fold::One => (kind, Box::new(Typed::bool(true)), Some(self.sub(body)?)),
            _ => (kind, self.sub(body)?, None),
        })
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

/// What a comprehension folds to: a list of its bodies, kept where a predicate holds; whether
/// exactly one element meets one; or whether every or some element does.
#[derive(Clone, Copy)]
pub(crate) enum Fold<'a> {
    Map(Option<&'a SpannedExpr>),
    One,
    All(bool),
}
