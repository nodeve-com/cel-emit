//! Open literals: a list or map the checker types with `dyn` because it has no elements to say
//! otherwise, `[]` and `{}`, and lists of them; and `optional.none()`, which holds no value. Such
//! a literal holds no value, so its type is what its use asks: the other operand's, the parameter
//! a function declares, or any, where nothing is read from it. Literals of truly mixed elements
//! are lists and maps of dyn ([`super::dynamic`]).

use super::*;

impl<'a> Lower<'a> {
    /// Whether `e` is a list or map literal the checker types with `dyn`, or `optional.none()`
    /// typed so.
    pub(crate) fn open(&self, e: &SpannedExpr) -> bool {
        (matches!(e.node, Expr::List(_) | Expr::Map(_)) || self.open_none(e) || self.empty_loop(e))
            && self.settled(e).is_none()
    }

    /// Whether `e` is `optional.none()`, which the checker types `optional<dyn>` where nothing
    /// else gives its type.
    pub(crate) fn open_none(&self, e: &SpannedExpr) -> bool {
        match &e.node {
            Expr::Call { expr, args } => {
                args.is_empty()
                    && self.named(e, expr) == Some("optional.none")
                    && self.settled(e).is_none()
            }
            _ => false,
        }
    }

    /// `e` lowered as `expected`, when it is an open literal: an empty one takes that type's
    /// elements, and with none given, `int` and `string`. Anything else lowers as it is.
    pub(crate) fn expr_as(
        &mut self,
        e: &'a SpannedExpr,
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        if !self.open(e) {
            return self.expr(e);
        }
        if self.open_none(e) {
            let of = match expected {
                None => Type::Int,
                Some(Type::Optional(t)) => (**t).clone(),
                Some(_) => return unsupported("an optional where another type is wanted"),
            };
            return Ok(Typed::new(Type::Optional(Box::new(of)), Node::None));
        }
        // A dyn hint says nothing of an empty literal's elements.
        let expected = expected.filter(|t| !t.has_dyn());
        match &e.node {
            Expr::List(items) => match self.open_list(items, expected)? {
                Some(list) => Ok(list),
                None => self.expr(e),
            },
            Expr::Map(entries) if entries.is_empty() => self.open_map(expected),
            Expr::Comprehension(c) => self.empty_loop_result(c, expected),
            _ => self.expr(e),
        }
    }

    /// Whether `e` is a comprehension over `[]` or `{}`.
    pub(crate) fn empty_loop(&self, e: &SpannedExpr) -> bool {
        match &e.node {
            Expr::Comprehension(c) => match &c.iter_range.node {
                Expr::List(items) => items.is_empty(),
                Expr::Map(entries) => entries.is_empty(),
                _ => false,
            },
            _ => false,
        }
    }

    /// An empty map literal taking the expected type's keys and values, or `string` to `int`.
    fn open_map(&mut self, expected: Option<&Type>) -> Result<Typed, Error> {
        let (key, value) = match expected {
            None => (Type::String, Type::Int),
            Some(Type::Map(k, v)) => ((**k).clone(), (**v).clone()),
            Some(_) => return unsupported("a map where another type is wanted"),
        };
        let ty = Type::Map(Box::new(key), Box::new(value));
        if !crate::stdlib::literal(&ty) {
            return unsupported("a map whose keys don't compare");
        }
        Ok(Typed::new(ty, Node::MapOf(Vec::new())))
    }

    /// A comprehension over `[]` or `{}`, which never runs its loop: its result, over the
    /// accumulator's start. Its body is never lowered, so its loop variables need no type.
    pub(crate) fn empty_loop_result(
        &mut self,
        c: &'a ComprehensionData,
        expected: Option<&Type>,
    ) -> Result<Typed, Error> {
        if matches!(&c.result.node, Expr::Ident(n) if *n == c.accu_var) {
            return match &c.accu_init.node {
                Expr::List(items) if items.is_empty() => self
                    .open_list(&[], expected)?
                    .ok_or_else(|| Error::Invalid("an empty list of mixed types".into())),
                Expr::Map(entries) if entries.is_empty() => self.open_map(expected),
                _ => self.expr(c.accu_init.as_ref()),
            };
        }
        let (lowered, lets) = self.binding(&c.accu_var, c.accu_init.as_ref(), |s| {
            s.expr(c.result.as_ref())
        })?;
        Ok(super::scope::wrap(lowered, &lets))
    }

    /// An open list literal: its elements that have a type settle the element type, an optional
    /// (`?`) one by the type it holds, the empty ones take it, and two types that differ leave a
    /// list of dyn (none).
    fn open_list(
        &mut self,
        items: &'a [cel_core::types::ListElement],
        expected: Option<&Type>,
    ) -> Result<Option<Typed>, Error> {
        let mut elem = match expected {
            None => None,
            Some(Type::List(t)) => Some((**t).clone()),
            Some(_) => return Ok(None),
        };
        let mut slots = Vec::with_capacity(items.len());
        for item in items {
            match self.open_slot(item, &mut elem)? {
                Slot::Mixed => return Ok(None),
                Slot::Open => slots.push(None),
                Slot::Typed(typed, optional) => slots.push(Some((typed, optional))),
            }
        }
        let elem = elem.unwrap_or(Type::Int);
        let mut made = Vec::with_capacity(items.len());
        for (item, slot) in items.iter().zip(slots) {
            made.push(match slot {
                Some(typed) => typed,
                None => (self.expr_as(&item.expr, Some(&elem))?, false),
            });
        }
        let ty = Type::list(elem);
        if !made.is_empty() && !crate::stdlib::literal(&ty) {
            return unsupported("a list of messages");
        }
        let node = match made.iter().any(|(_, optional)| *optional) {
            true => Node::ListOpt(made),
            false => super::literal::stored_list(made.into_iter().map(|(x, _)| x).collect()),
        };
        Ok(Some(Typed::new(ty, node)))
    }

    /// Element `item` of an open list literal, settling the element type `elem` where it has none.
    fn open_slot(
        &mut self,
        item: &'a cel_core::types::ListElement,
        elem: &mut Option<Type>,
    ) -> Result<Slot, Error> {
        if item.optional {
            let want = elem.clone().map(|t| Type::Optional(Box::new(t)));
            let typed = self.expr_as(&item.expr, want.as_ref())?;
            let Type::Optional(held) = &typed.ty else {
                return invalid("an optional list element of other than an optional");
            };
            let held = (**held).clone();
            return Ok(settle_elem(elem, held, typed, true));
        }
        let empty = matches!(&item.expr.node, Expr::List(v) if v.is_empty())
            || matches!(&item.expr.node, Expr::Map(v) if v.is_empty());
        if empty && self.open(&item.expr) {
            return Ok(Slot::Open);
        }
        let typed = self.expr_as(&item.expr, elem.as_ref())?;
        let held = typed.ty.clone();
        Ok(settle_elem(elem, held, typed, false))
    }

    /// Two operands of one type, `a` then `b`, where either may be open: an open one takes the
    /// other's type, so the other is lowered first.
    pub(crate) fn pair_as(
        &mut self,
        a: &'a SpannedExpr,
        b: &'a SpannedExpr,
    ) -> Result<(Typed, Typed), Error> {
        if self.open(a) && !self.open(b) {
            let b = self.expr(b)?;
            return Ok((self.expr_as(a, Some(&b.ty))?, b));
        }
        let a = self.expr_as(a, None)?;
        let b = self.expr_as(b, Some(&a.ty))?;
        Ok((a, b))
    }

    /// A binary operator's operands. An open literal takes the other operand's type; `in`'s
    /// container takes the left operand's type for its elements or keys.
    pub(crate) fn binary(
        &mut self,
        op: &BinaryOp,
        left: &'a SpannedExpr,
        right: &'a SpannedExpr,
    ) -> Result<Vec<Typed>, Error> {
        if *op == BinaryOp::In {
            let l = self.expr_as(left, None)?;
            let hint = match &right.node {
                Expr::Map(_) => Type::Map(Box::new(l.ty.clone()), Box::new(Type::Int)),
                _ => Type::list(l.ty.clone()),
            };
            let r = self.expr_as(right, Some(&hint))?;
            return Ok(vec![l, r]);
        }
        let (l, r) = match (self.open(left), self.open(right)) {
            (true, false) => {
                let r = self.expr(right)?;
                (self.expr_as(left, Some(&r.ty))?, r)
            }
            (false, true) => {
                let l = self.expr(left)?;
                let r = self.expr_as(right, Some(&l.ty))?;
                (l, r)
            }
            _ => (self.expr_as(left, None)?, self.expr_as(right, None)?),
        };
        Ok(vec![l, r])
    }
}

/// An element of an open list literal.
enum Slot {
    /// Its type differs from an earlier element's.
    Mixed,
    /// An empty list or map, typed once the others settle the element type.
    Open,
    /// A value, and whether it is optional (`?`).
    Typed(Typed, bool),
}

/// `typed`, whose element type is `held`, as a slot of a list of `elem`, which it sets if unset.
fn settle_elem(elem: &mut Option<Type>, held: Type, typed: Typed, optional: bool) -> Slot {
    match elem {
        Some(t) if *t != held => Slot::Mixed,
        Some(_) => Slot::Typed(typed, optional),
        None => {
            *elem = Some(held);
            Slot::Typed(typed, optional)
        }
    }
}
