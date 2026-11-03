//! The forms an expression takes beyond a call: logic, a ternary, and a presence test.

use super::*;

impl<'a> Lower<'a> {
    /// The call of the overload the checker resolved the operator or index `e` to.
    pub(super) fn operated(&mut self, e: &'a SpannedExpr) -> Result<Node, Error> {
        match self.overload(e)? {
            Some(node) => Ok(node),
            None => unsupported(self.unresolved(e)),
        }
    }

    /// `&&`, `||`, a comparison to null, and membership in a column of messages.
    pub(super) fn logic(&mut self, e: &'a SpannedExpr) -> Result<Option<Node>, Error> {
        Ok(Some(match &e.node {
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
                op: op @ (BinaryOp::Eq | BinaryOp::Ne),
                left,
                right,
            } if matches!(right.node, Expr::Null) && self.holds_null(left)? => {
                let has = Typed::call("optional_has_value", vec![self.expr(left)?], Type::Bool);
                match op {
                    BinaryOp::Ne => has.node,
                    _ => Node::Call("logical_not".into(), vec![has]),
                }
            }
            Expr::Binary {
                op: BinaryOp::In,
                left,
                right,
            } if self.messages(right).is_some() => {
                let root = self.messages(right).unwrap_or_default();
                let column = self.column(&root, &self.loop_column(&root)?)?;
                Node::Call("in_map".into(), vec![self.expr(left)?, column])
            }
            _ => return Ok(None),
        }))
    }

    /// The ternary `e` of type `ty`, each branch held as it.
    pub(super) fn ternary(&mut self, e: &'a SpannedExpr, ty: &Type) -> Result<Node, Error> {
        let Expr::Ternary {
            cond,
            then_expr,
            else_expr,
        } = &e.node
        else {
            return invalid("a ternary that isn't one");
        };
        let (then, other) = self.pair_as(then_expr, else_expr)?;
        let branch = |t: Typed| match t.node {
            Node::Null if matches!(ty, Type::Nullable(_)) => Typed::new(ty.clone(), Node::None),
            _ => super::message::held(t, ty),
        };
        Ok(Node::Cond(
            self.sub_bool(cond)?,
            Box::new(branch(then)),
            Box::new(branch(other)),
        ))
    }

    /// `has(expr.field)`: a message column's key, a constant or variable, or a field.
    pub(super) fn test_only(
        &mut self,
        e: &'a SpannedExpr,
        expr: &'a SpannedExpr,
        field: &str,
    ) -> Result<Node, Error> {
        if let Some((map, key)) = self.message_field(e)? {
            return Ok(Node::Call("in_map".into(), vec![key, map]));
        }
        match self.path(e).and_then(|p| self.found(&p)) {
            Some(At::Constant(_)) => Ok(Node::Bool(true)),
            Some(At::Var(v)) => Ok(Node::Has(v)),
            None => self.has_field(expr, field),
        }
    }
}
