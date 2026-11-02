//! Enum constructors: `Enum(n)` from an int, which must fit an enum's 32 bits, and `Enum('NAME')`
//! from one of its value names. Each is a check on the operand, built from the nodes a comparison
//! already lowers to, so an out-of-range number or unknown name is the error CEL gives.

use std::collections::BTreeMap;

use super::*;

/// The overload the checker resolves an enum constructor to.
const CONSTRUCTOR: &str = "enum_constructor";

impl<'a> Lower<'a> {
    /// Whether the checker resolved `e` to an enum constructor.
    pub(crate) fn is_enum_constructor(&self, e: &SpannedExpr) -> bool {
        self.info
            .get_reference(e.id)
            .is_some_and(|r| r.overload_ids.iter().any(|id| id == CONSTRUCTOR))
    }

    /// `Enum(arg)` where the checker resolved `e` to an enum constructor of a declared enum.
    pub(crate) fn enum_constructor(
        &mut self,
        e: &'a SpannedExpr,
        args: &'a [SpannedExpr],
    ) -> Result<Node, Error> {
        let decls = self.decls;
        let name = self
            .info
            .get_reference(e.id)
            .and_then(|r| r.enum_type.as_deref());
        let (Some(values), [arg]) = (name.and_then(|n| decls.enums.get(n)), args) else {
            return unsupported("an enum constructor of an enum cel-emit doesn't declare");
        };
        let arg = self.expr(arg)?;
        let made = match arg.ty {
            Type::Int => in_i32_range(arg)?,
            Type::String => by_name(arg, values)?,
            _ => return unsupported("an enum constructed from other than an int or a string"),
        };
        Ok(made.node)
    }
}

/// `x` where it fits an enum's 32 bits, else an overflow.
fn in_i32_range(x: Typed) -> Result<Typed, Error> {
    let low = bin(">=", x.clone(), i64::from(i32::MIN))?;
    let high = bin("<=", x.clone(), i64::from(i32::MAX))?;
    Ok(Typed::cond(
        Typed::and(low, high),
        x,
        Typed::new(Type::Int, Node::Fail(crate::tree::Fail::Overflow)),
    ))
}

/// The number of the value `s` names, else a cast error: a literal is looked up now.
fn by_name(s: Typed, values: &BTreeMap<String, i32>) -> Result<Typed, Error> {
    let unknown = Typed::new(Type::Int, Node::Fail(crate::tree::Fail::Cast));
    if let Node::Str(name) = &s.node {
        return Ok(match values.get(name) {
            Some(v) => Typed::int(i64::from(*v)),
            None => unknown,
        });
    }
    values.iter().rev().try_fold(unknown, |rest, (name, v)| {
        let is = Typed::binary("==", s.clone(), Typed::string(name))
            .ok_or_else(|| Error::Invalid("enum name comparison".into()))?;
        Ok(Typed::cond(is, Typed::int(i64::from(*v)), rest))
    })
}

/// `x op n` on ints.
fn bin(op: &str, x: Typed, n: i64) -> Result<Typed, Error> {
    Typed::binary(op, x, Typed::int(n))
        .ok_or_else(|| Error::Invalid("enum range comparison".into()))
}
