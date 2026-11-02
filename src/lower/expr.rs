//! One value: literals, names, operators and calls by the overload the checker resolved, and
//! list and map literals.

use super::dynamic::{fits, retype};
use super::special::first_match;
use super::*;

impl<'a> Lower<'a> {
    pub(crate) fn expr(&mut self, e: &'a SpannedExpr) -> Result<Typed, Error> {
        if let Some(t) = self.bound_value(e)? {
            return Ok(t);
        }
        if let Expr::Struct { .. } = &e.node {
            return self.message_value(e);
        }
        if let Expr::Bind {
            var_name,
            init,
            body,
        } = &e.node
        {
            let (v, lets) = self.binding(var_name, init, |s| s.expr(body))?;
            return Ok(super::scope::wrap(v, &lets));
        }
        if let Some(t) = self.number_form(e)? {
            return self.as_value(e, t);
        }
        if self.open_none(e) {
            return self.expr_as(e, None);
        }
        if let Expr::Comprehension(c) = &e.node {
            if self.empty_loop(e) {
                let want = self.settled(e);
                return self.empty_loop_result(c, want.as_ref());
            }
        }
        // The checker joins a ternary's branches to `dyn` where one is an optional holding
        // nothing: the other branch's type is the ternary's.
        if let Expr::Ternary {
            cond,
            then_expr,
            else_expr,
        } = &e.node
        {
            if self.settled(e).is_none() {
                let (then, other) = self.pair_as(then_expr, else_expr)?;
                let (then, other) = joined(then, other)?;
                return Ok(Typed::cond(*self.sub_bool(cond)?, then, other));
            }
        }
        let ty = self.ty(e)?;
        if let Some(node) = self.special(e)? {
            return Ok(retype(Typed { ty, node }));
        }
        let node = match &e.node {
            Expr::Int(v) => Node::Int(*v),
            Expr::UInt(v) => Node::Uint(*v),
            Expr::Float(v) => Node::Double(*v),
            Expr::Bool(v) => Node::Bool(*v),
            Expr::String(v) => Node::Str(v.clone()),
            Expr::Bytes(v) => Node::Bytes(v.clone()),
            Expr::Null => Node::Null,
            Expr::Member {
                expr,
                field,
                optional: true,
            } => self.optional_field(e, expr, field)?,
            Expr::Index {
                expr,
                index,
                optional: true,
            } => self.optional_index(expr, index)?,
            // A selection after a `.?` is optional too: `a.?b.c` is present where `a.b` is.
            Expr::Member { .. } if matches!(ty, Type::Optional(_)) => self.optional_chain(e)?,
            Expr::Ident(_) | Expr::Member { .. } => self.name(e)?,
            Expr::Binary {
                op: BinaryOp::And,
                left,
                right,
            } => Node::And(self.sub_bool(left)?, self.sub_bool(right)?),
            Expr::Binary {
                op: BinaryOp::Or,
                left,
                right,
            } => Node::Or(self.sub_bool(left)?, self.sub_bool(right)?),
            Expr::Binary {
                op: BinaryOp::In,
                left,
                right,
            } if self.messages(right).is_some() => {
                let root = self.messages(right).unwrap_or_default();
                let column = self.column(&root, &self.loop_column(&root)?)?;
                Node::Call("in_map".into(), vec![self.expr(left)?, column])
            }
            Expr::Ternary {
                cond,
                then_expr,
                else_expr,
            } => {
                let (then, other) = self.pair_as(then_expr, else_expr)?;
                Node::Cond(self.sub_bool(cond)?, Box::new(then), Box::new(other))
            }
            Expr::Call { expr, args } => self.call(e, expr, args)?,
            Expr::Comprehension(c) => self.comprehension(c)?,
            Expr::MemberTestOnly { expr, field } => match self.message_field(e)? {
                Some((map, key)) => Node::Call("in_map".into(), vec![key, map]),
                None => match self.path(e).and_then(|p| self.found(&p)) {
                    Some(At::Constant(_)) => Node::Bool(true),
                    Some(At::Var(v)) => Node::Has(v),
                    None => self.has_field(expr, field)?,
                },
            },
            Expr::List(items) if crate::stdlib::literal(&ty) => {
                let Type::List(elem) = &ty else {
                    return invalid("a list literal of other than a list");
                };
                let mut made = Vec::with_capacity(items.len());
                for item in items {
                    let x = self.expr(&item.expr)?;
                    made.push((x, item.optional));
                }
                if made.iter().any(|(_, optional)| *optional) {
                    return super::optional::list(made, elem);
                }
                let made = made.into_iter().map(|(x, _)| fits(x, elem));
                Node::List(made.collect::<Result<_, _>>()?)
            }
            Expr::Map(entries) if crate::stdlib::literal(&ty) => {
                let Type::Map(key, value) = &ty else {
                    return invalid("a map literal of other than a map");
                };
                let mut made = Vec::with_capacity(entries.len());
                for entry in entries {
                    let (k, v) = (self.expr(&entry.key)?, self.expr(&entry.value)?);
                    let v = if entry.optional { v } else { fits(v, value)? };
                    made.push((fits(k, key)?, v, entry.optional));
                }
                if made.iter().any(|(_, _, optional)| *optional) {
                    Node::MapOpt(made)
                } else {
                    Node::MapOf(made.into_iter().map(|(k, v, _)| (k, v)).collect())
                }
            }
            Expr::Unary { .. }
            | Expr::Binary { .. }
            | Expr::Index {
                optional: false, ..
            } => match self.overload(e)? {
                Some(node) => node,
                None => return unsupported(self.unresolved(e)),
            },
            other => return unsupported(kind(other)),
        };
        Ok(first_match(retype(Typed { ty, node }), self.loops.len()))
    }

    /// The call of the one overload the checker resolved `e` to, on its operands, if cel-emit
    /// runs it on their types.
    pub(crate) fn overload(&mut self, e: &'a SpannedExpr) -> Result<Option<Node>, Error> {
        let Some(reference) = self.info.get_reference(e.id) else {
            return Ok(None);
        };
        if let Some(node) = self.dyn_overload(e)? {
            return Ok(Some(node));
        }
        let [id] = reference.overload_ids.as_slice() else {
            return Ok(None);
        };
        let mut operands = self.operands(e)?;
        if matches!(id.as_str(), "equals" | "not_equals") {
            super::nullable::compare(&mut operands);
        }
        self.settle(id, &mut operands)?;
        widen_lazily(id, &mut operands);
        let id = super::digits::own_digits(id, &mut operands);
        let types: Vec<Type> = operands.iter().map(|o| o.ty.clone()).collect();
        if self.paths.path(id, &types).is_none() {
            return Ok(None);
        }
        if id.ends_with("_with_tz") {
            match operands.get_mut(1).map(|z| &mut z.node) {
                Some(Node::Str(zone)) => *zone = crate::zone::table(zone)?,
                _ => return unsupported("a zone other than a string literal"),
            }
        }
        super::pattern::call(id, operands).map(Some)
    }

    /// Why `e` has no overload cel-emit runs.
    pub(crate) fn unresolved(&self, e: &SpannedExpr) -> String {
        let ids = self
            .info
            .get_reference(e.id)
            .map(|r| r.overload_ids.join(", "))
            .unwrap_or_default();
        match &e.node {
            Expr::Call { expr, .. } => match &expr.node {
                Expr::Member { field, .. } => format!("the method {field} ({ids})"),
                Expr::Ident(name) => format!("{name} ({ids})"),
                _ => format!("this call ({ids})"),
            },
            other => format!("{} ({ids})", kind(other)),
        }
    }

    /// An operator's, index's, or call's operands, a method's receiver first.
    pub(crate) fn operands(&mut self, e: &'a SpannedExpr) -> Result<Vec<Typed>, Error> {
        let operands: Vec<&'a SpannedExpr> = match &e.node {
            Expr::Binary { op, left, right } => return self.binary(op, left, right),
            Expr::Unary { expr, .. } => vec![expr],
            Expr::Index { expr, index, .. } => vec![expr, index],
            Expr::Call { expr, args } => match &expr.node {
                Expr::Member { expr: receiver, .. } if !self.namespace(e, expr) => {
                    std::iter::once(receiver.as_ref()).chain(args).collect()
                }
                _ => args.iter().collect(),
            },
            other => return unsupported(kind(other)),
        };
        if !operands.iter().any(|o| self.open(o)) {
            return operands.into_iter().map(|o| self.expr(o)).collect();
        }
        let hints = self.hints(e, operands.len());
        operands
            .into_iter()
            .zip(hints)
            .map(|(o, hint)| self.expr_as(o, hint.as_ref()))
            .collect()
    }

    /// Each operand's type as the overload the checker resolved `e` to declares it, where it is
    /// declared concretely: a method's receiver first, as the call's operands are.
    fn hints(&self, e: &SpannedExpr, n: usize) -> Vec<Option<Type>> {
        let declared = match self.info.get_reference(e.id) {
            Some(reference) => match reference.overload_ids.as_slice() {
                [id] => self.params.get(id),
                _ => None,
            },
            None => None,
        };
        match declared {
            Some(params) if params.len() == n => {
                params.iter().map(|p| of_cel(p, &self.messages)).collect()
            }
            _ => vec![None; n],
        }
    }

    /// Whether `callee` names a namespaced function, as the checker resolved `e`'s call.
    pub(crate) fn namespace(&self, e: &SpannedExpr, callee: &SpannedExpr) -> bool {
        self.named(e, callee).is_some_and(|n| n.contains('.'))
    }

    /// The function `e`'s call names, `ns.f` or `f`, as the checker resolved it.
    pub(crate) fn named(&self, e: &SpannedExpr, callee: &SpannedExpr) -> Option<&str> {
        let r = self.info.get_reference(e.id)?;
        match &callee.node {
            Expr::Ident(name) if *name == r.name => Some(r.name.as_str()),
            Expr::Member { expr, field, .. } => match &expr.node {
                Expr::Ident(ns) if r.name == format!("{ns}.{field}") => Some(r.name.as_str()),
                _ => None,
            },
            _ => None,
        }
    }
}

/// A ternary's branches, made one type where one is a dyn.
fn joined(a: Typed, b: Typed) -> Result<(Typed, Typed), Error> {
    if a.ty != b.ty && (a.ty == Type::Dyn || b.ty == Type::Dyn) {
        return Ok((fits(a, &Type::Dyn)?, fits(b, &Type::Dyn)?));
    }
    Ok((a, b))
}

/// A stored list or map `id` reads one element of, or the size of, left as stored: the call
/// widens what it reads ([`crate::emit`]) rather than a copy of the whole collection.
fn widen_lazily(id: &str, operands: &mut [Typed]) {
    let at = match id {
        "size_list" | "list_size" | "size_map" | "map_size" | "index_list" | "index_map" => 0,
        "in_list" | "in_map" => 1,
        _ => return,
    };
    if let Some(operand) = operands.get_mut(at) {
        if let Node::Widen(stored) = &mut operand.node {
            let stored = std::mem::replace(&mut **stored, Typed::bool(false));
            *operand = stored;
        }
    }
}
