//! Numbers the checker types `dyn`: `math.greatest` and `math.least` over operands of mixed int,
//! uint, and double. Such a call is a number ([`Type::Number`]) that only `int()`, `uint()`,
//! `double()`, or a comparison takes; anywhere else it is refused. A call whose operands agree
//! keeps its typed overload, which the checker joins to `dyn` too.

use cel_core::types::{BinaryOp, SpannedExpr, UnaryOp};
use cel_core::CelType;

use super::*;

impl<'a> Lower<'a> {
    /// `e` as a number, where it is one of the forms here: a `math.greatest` or `math.least` the
    /// checker typed `dyn`, a cast of one to `int`, `uint`, or `double`, a comparison, arithmetic,
    /// or negation over one. A [`Type::Number`] or uint comes back only as an operand of what
    /// takes it ([`Self::as_value`]).
    pub(crate) fn number_form(&mut self, e: &'a SpannedExpr) -> Result<Option<Typed>, Error> {
        if self.extreme_of(e).is_some() {
            return self.extreme(e).map(Some);
        }
        match &e.node {
            Expr::Call { expr, args } => self.number_cast(expr, args),
            Expr::Binary { op, left, right }
                if is_comparison(op) && (self.is_numeric(left) || self.is_numeric(right)) =>
            {
                self.number_comparison(op, left, right).map(Some)
            }
            // Arithmetic over a `dyn` extreme's typed value: the checker left several overloads.
            Expr::Binary { op, left, right } if self.is_numeric(left) || self.is_numeric(right) => {
                self.number_arithmetic(op, left, right)
            }
            Expr::Unary {
                op: UnaryOp::Neg,
                expr,
            } if self.is_numeric(expr) => self.number_negation(expr).map(Some),
            _ => Ok(None),
        }
    }

    /// A cast of a number form to `int`, `uint`, or `double`.
    fn number_cast(
        &mut self,
        callee: &'a SpannedExpr,
        args: &'a [SpannedExpr],
    ) -> Result<Option<Typed>, Error> {
        let (Expr::Ident(to), [arg]) = (&callee.node, args) else {
            return Ok(None);
        };
        let (func, ty) = match to.as_str() {
            "int" => ("number_to_int", Type::Int),
            "uint" => ("number_to_uint", Type::Uint),
            "double" => ("number_to_double", Type::Double),
            _ => return Ok(None),
        };
        if !self.is_numeric(arg) {
            return Ok(None);
        }
        let x = wrap(self.number(arg)?)?;
        Ok(Some(Typed::call(func, vec![x], ty)))
    }

    /// A comparison `op` of `left` and `right`, one a number form, each as a number.
    fn number_comparison(
        &mut self,
        op: &BinaryOp,
        left: &'a SpannedExpr,
        right: &'a SpannedExpr,
    ) -> Result<Typed, Error> {
        let id = match op {
            BinaryOp::Eq => "equals",
            BinaryOp::Ne => "not_equals",
            BinaryOp::Lt => "less_number",
            BinaryOp::Le => "less_equals_number",
            BinaryOp::Gt => "greater_number",
            _ => "greater_equals_number",
        };
        let a = wrap(self.number(left)?)?;
        let b = wrap(self.number(right)?)?;
        Ok(Typed::call(id, vec![a, b], Type::Bool))
    }

    /// Arithmetic `op` of `left` and `right`, one a number form, on their typed values.
    fn number_arithmetic(
        &mut self,
        op: &BinaryOp,
        left: &'a SpannedExpr,
        right: &'a SpannedExpr,
    ) -> Result<Option<Typed>, Error> {
        let sym = match op {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Mod => "%",
            _ => return Ok(None),
        };
        let (a, b) = (self.number(left)?, self.number(right)?);
        if a.ty == Type::Number {
            return self.as_value(left, a).map(Some);
        }
        if b.ty == Type::Number {
            return self.as_value(right, b).map(Some);
        }
        if !matches!(a.ty, Type::Int | Type::Double) {
            return unsupported(format!("arithmetic on {}", a.ty.display()));
        }
        Typed::binary(sym, a, b)
            .map(Some)
            .ok_or_else(|| Error::Unsupported("arithmetic on mixed operands".into()))
    }

    /// The negation of `expr`, a number form.
    fn number_negation(&mut self, expr: &'a SpannedExpr) -> Result<Typed, Error> {
        let x = self.number(expr)?;
        let (id, ty) = match x.ty {
            Type::Number => return self.as_value(expr, x),
            Type::Int => ("negate_int64", Type::Int),
            Type::Double => ("negate_double", Type::Double),
            _ => return unsupported(format!("negation of {}", x.ty.display())),
        };
        Ok(Typed::call(id, vec![x], ty))
    }

    /// `e` as an operand of a number form: a uint literal, or whatever [`Self::number_form`] or
    /// [`Self::expr`] gives.
    pub(crate) fn number(&mut self, e: &'a SpannedExpr) -> Result<Typed, Error> {
        if let Some(t) = self.number_form(e)? {
            return Ok(t);
        }
        self.expr(e)
    }

    /// `t`, the number form `e` lowered to, as the value of `e` in an expression: refused where it
    /// is a mixed number or a uint, which no declaration holds.
    pub(crate) fn as_value(&self, e: &SpannedExpr, t: Typed) -> Result<Typed, Error> {
        match t.ty {
            Type::Number => {
                let name = match self.extreme_of(e) {
                    Some((false, _)) => "least",
                    _ => "greatest",
                };
                unsupported(format!(
                    "a mixed math.{name} feeds only a cast or comparison: double(math.{name}(…))"
                ))
            }
            _ => Ok(t),
        }
    }

    /// Whether `e` is a number form [`Self::number_form`] lowers: a `dyn` extreme, or a cast of
    /// one, judged from the checker's types alone.
    fn is_numeric(&self, e: &'a SpannedExpr) -> bool {
        if self.extreme_of(e).is_some() {
            return true;
        }
        match &e.node {
            Expr::Call { expr, args } => match (&expr.node, args.as_slice()) {
                (Expr::Ident(to), [arg]) => {
                    matches!(to.as_str(), "int" | "uint" | "double") && self.is_numeric(arg)
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// `e` as `math.greatest` (true) or `math.least`, with its arguments, where the checker typed
    /// its call `dyn`.
    pub(crate) fn extreme_of<'e>(&self, e: &'e SpannedExpr) -> Option<(bool, &'e [SpannedExpr])> {
        let Expr::Call { expr, args } = &e.node else {
            return None;
        };
        if !matches!(self.info.get_type(e.id), Some(CelType::Dyn)) {
            return None;
        }
        match self.named(e, expr)? {
            "math.greatest" => Some((true, args.as_slice())),
            "math.least" => Some((false, args.as_slice())),
            _ => None,
        }
    }
}

/// `x` as a [`Type::Number`]: a number already, or an int, uint, or double wrapped as one.
pub(crate) fn wrap(x: Typed) -> Result<Typed, Error> {
    let id = match x.ty {
        Type::Number => return Ok(x),
        Type::Int => "number_of_int",
        Type::Uint => "number_of_uint",
        Type::Double => "number_of_double",
        _ => return unsupported("a mixed math extreme over other than numbers"),
    };
    Ok(Typed::call(id, vec![x], Type::Number))
}

fn is_comparison(op: &BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
    )
}

#[cfg(test)]
mod tests {
    use crate::{lower, Decls, Error, Node};

    #[test]
    fn a_mixed_extreme_is_refused_unless_a_cast_or_comparison_takes_it() {
        let refused = lower("math.greatest(1, 2.5)", &Decls::default());
        assert!(matches!(
            &refused,
            Err(Error::Unsupported(m)) if m.contains("feeds only a cast or comparison: double(math.greatest(")
        ));
        assert!(lower("math.least(5.4, 10, 3u)", &Decls::default()).is_err());
        assert!(lower("double(math.greatest(1, 2.5))", &Decls::default()).is_ok());
        assert!(lower("int(math.least(1u, 2.5)) + 1", &Decls::default()).is_ok());
        assert!(lower("math.greatest(1, 1.0) == 1", &Decls::default()).is_ok());
        assert!(lower("uint(math.greatest(1, 2.5))", &Decls::default()).is_ok());
        assert!(lower("math.greatest(1, 2, 3)", &Decls::default()).is_ok());
        assert!(lower("math.greatest(1, 2, 3) - 1 > 0", &Decls::default()).is_ok());
        assert!(lower("math.greatest(1, 2.5) + 1 > 0", &Decls::default()).is_err());
    }

    #[test]
    fn a_cast_to_string_takes_the_extreme_by_its_typed_overload() {
        for (source, id) in [
            ("string(math.greatest([1, 3, 2]))", "int_to_string"),
            ("string(math.least([1.5, 0.5]))", "double_to_string"),
            ("string(math.greatest(1u, 2u))", "uint_to_string"),
        ] {
            let typed = lower(source, &Decls::default()).unwrap();
            assert!(
                matches!(&typed.node, Node::Call(f, _) if f == id),
                "{source}"
            );
        }
    }
}
