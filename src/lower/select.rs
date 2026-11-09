//! Field selection on a message: a struct literal, a bound name standing for one, or a message's
//! field that is one itself. A field the value sets reads as set; one it leaves out reads its
//! zero value, as proto3 gives every field a message declares.

use super::parts::{is_fields, is_message, parts, rebuild};
use super::scope::wrap;
use super::*;
use crate::types::Field;
use crate::walk::nodes;

impl<'a> Lower<'a> {
    /// `e` as a message's fields, when `e` is one: a struct literal, a bound name standing for one,
    /// or a message's field that is one.
    pub(crate) fn selected(&mut self, e: &'a SpannedExpr) -> Result<Option<Typed>, Error> {
        match &e.node {
            Expr::Struct { .. } => Ok(Some(self.message(e)?).filter(is_message)),
            Expr::Ident(name) => Ok(self.bound(name).filter(|v| is_message(v)).cloned()),
            Expr::Member { .. } => self.selected_field(e),
            _ => Ok(None),
        }
    }

    /// `e`, a field of a message, as the optional it is stored as: an optional field read
    /// whole, unwrapped nowhere, so `has()` and `.?` on what lies under it test it first. `None`
    /// where `e` is no optional field of a message.
    pub(crate) fn optional_stored(&mut self, e: &'a SpannedExpr) -> Result<Option<Typed>, Error> {
        Ok(self
            .stored_value(e)?
            .filter(|v| matches!(v.ty, Type::Optional(_))))
    }

    /// A message name, or a field chain from one through messages, as stored: an optional field
    /// is still an optional.
    fn stored_value(&mut self, e: &'a SpannedExpr) -> Result<Option<Typed>, Error> {
        match &e.node {
            Expr::Ident(name) => {
                if let Some(v) = self.bound(name) {
                    return Ok(Some(v.clone()).filter(is_message));
                }
                Ok(match self.found(name) {
                    Some(At::Var(v)) => Some(self.declared(name, v)),
                    _ => None,
                }
                .filter(|v| matches!(v.ty, Type::Message(_))))
            }
            Expr::Member { expr, field, .. } => {
                let Some(base) = self.stored_value(expr)? else {
                    return Ok(None);
                };
                if is_fields(&base) {
                    let held = parts(base).into_iter().find(|(n, _)| n == field);
                    return Ok(held.map(|(_, v)| v));
                }
                let Type::Message(fields) = base.ty.clone() else {
                    return Ok(None);
                };
                let named = fields.iter().any(|f| f.name == *field);
                Ok(named
                    .then(|| super::has::select_field(base, &fields, field))
                    .transpose()?)
            }
            _ => Ok(None),
        }
    }

    /// `e` as a field selected from a message, read as that field's value: a scalar, a list, a map,
    /// or a message. `None` when what it selects from is no message.
    pub(crate) fn selected_field(&mut self, e: &'a SpannedExpr) -> Result<Option<Typed>, Error> {
        let Expr::Member { expr, field, .. } = &e.node else {
            return Ok(None);
        };
        let Ok(Type::Message(fields)) = self.ty(expr) else {
            return Ok(None);
        };
        match self.selected(expr)? {
            Some(base) => select(base, &fields, field).map(Some),
            None => Ok(None),
        }
    }
}

/// `field` of `base`, a message of `fields`: what `base` sets, else the zero of the field's type.
fn select(base: Typed, fields: &[Field], field: &str) -> Result<Typed, Error> {
    let Some(declared) = fields.iter().find(|f| f.name == field) else {
        return unsupported(format!(
            "field {field} of a message with no type in cel-emit"
        ));
    };
    if !is_fields(&base) {
        return invalid(format!("field {field} of a value that is no message"));
    }
    let mut checks = Vec::new();
    let mut chosen = None;
    for (name, v) in parts(base) {
        if name == field {
            chosen = Some(v);
        } else {
            leaves(&v, &mut checks);
        }
    }
    let v = match (&declared.ty, chosen) {
        (Type::Optional(inner), chosen) => unset(chosen, inner),
        (ty, None) => zero(ty)?,
        (_, Some(v)) => v,
    };
    let lets: Vec<_> = checks.into_iter().map(|c| (usize::MAX, c)).collect();
    Ok(checked(v, &lets))
}

/// An optional field's plain read: the value a literal sets it to, else absent, as a read of
/// a stored one is. `set` is what the literal gives the field.
fn unset(set: Option<Typed>, inner: &Type) -> Typed {
    let held = Type::Optional(Box::new(inner.cel()));
    match set {
        Some(Typed {
            node: Node::Some(x),
            ..
        }) => *x,
        Some(v) => Typed::call("optional_value", vec![v], inner.cel()),
        None => Typed::call(
            "optional_value",
            vec![Typed::new(held, Node::None)],
            inner.cel(),
        ),
    }
}

/// `v` with each of `lets` bound around every field it sets, so a message stays one.
fn checked(v: Typed, lets: &[(usize, Typed)]) -> Typed {
    match (&v.ty, v.node) {
        (Type::Message(_), Node::Message(made)) if !lets.is_empty() => Typed::new(
            v.ty.clone(),
            Node::Message(
                made.into_iter()
                    .map(|(i, f, flagged)| (i, checked(f, lets), flagged))
                    .collect(),
            ),
        ),
        (_, node) => wrap(Typed { node, ..v }, lets),
    }
}

/// Every value under `v` that can fail: a literal cannot, so it needs no check.
fn leaves(v: &Typed, out: &mut Vec<Typed>) {
    if is_fields(v) {
        return parts(v.clone()).iter().for_each(|(_, v)| leaves(v, out));
    }
    let mut sure = true;
    nodes(v, &mut |n| sure &= is_literal(&n.node));
    if !sure {
        out.push(v.clone());
    }
}

/// Whether `n` is a literal, which evaluates without an error.
pub(super) fn is_literal(n: &Node) -> bool {
    matches!(
        n,
        Node::Int(_) | Node::Uint(_) | Node::Bool(_) | Node::Str(_) | Node::None
    )
}

/// The zero value of `ty`, which an unset proto3 field reads: an empty message is each of its
/// fields' zero.
fn zero(ty: &Type) -> Result<Typed, Error> {
    let ty = &ty.cel();
    let t = match ty {
        Type::Message(m) => {
            let mut made = Vec::with_capacity(m.fields.len());
            for f in &m.fields {
                made.push((f.name.clone(), zero(&f.ty)?));
            }
            return rebuild(ty, made);
        }
        Type::Int => Typed::int(0),
        Type::Uint => Typed::new(ty.clone(), Node::Uint(0)),
        Type::Double => Typed::double(0.0),
        Type::Bool => Typed::bool(false),
        Type::String => Typed::string(""),
        Type::Bytes => Typed::new(ty.clone(), Node::Bytes(Vec::new())),
        Type::Timestamp => Typed::new(ty.clone(), Node::Timestamp(0, 0)),
        Type::Duration => Typed::new(ty.clone(), Node::Duration(0, 0)),
        Type::List(_) => Typed::new(ty.clone(), Node::List(Vec::new())),
        Type::Map(..) => Typed::new(ty.clone(), Node::MapOf(Vec::new())),
        Type::Nullable(_) => Typed::new(ty.clone(), Node::None),
        Type::Optional(t) if !matches!(**t, Type::Nullable(_)) => {
            Typed::new(ty.clone(), Node::None)
        }
        Type::Null | Type::Optional(_) => return unsupported("a null or optional field"),
        Type::Narrow(_) => return invalid("a stored width outside its type"),
        Type::Number => return invalid("a field holding a mixed number"),
        Type::Dyn => return unsupported("a dyn field"),
    };
    Ok(t)
}

#[cfg(test)]
mod tests {
    use crate::types::{Field, Type, Width};
    use crate::{lower, Decls, Node};

    fn field(name: &str, ty: Type) -> Field {
        Field {
            name: name.into(),
            ty,
            required: false,
        }
    }

    fn selected(source: &str) -> Node {
        let mut d = Decls::default();
        let fields = vec![
            field("narrow", Type::Narrow(Width::I32)),
            field("other", Type::Int),
            field("third", Type::Int),
        ];
        d.messages.insert("M".into(), Type::message(fields));
        lower(source, &d).unwrap().node
    }

    fn checks_other(source: &str) {
        let Node::Let(_, check, body) = selected(source) else {
            panic!("no check");
        };
        assert!(matches!(check.node, Node::Within(..)));
        assert_eq!(body.node, Node::Int(2));
    }

    #[test]
    fn an_unselected_out_of_width_field_is_still_checked() {
        checks_other("M{narrow: 2147483648, other: 2}.other");
    }

    #[test]
    fn an_unselected_division_by_zero_is_still_checked() {
        checks_other("M{narrow: 1 / 0, other: 2}.other");
    }

    #[test]
    fn a_literal_whose_fields_cannot_fail_is_read_bare() {
        assert_eq!(selected("M{other: 2, third: 3}.other"), Node::Int(2));
    }

    fn map_selected(source: &str) -> Node {
        lower(source, &Decls::default()).unwrap().node
    }

    #[test]
    fn a_map_literal_is_built_whole_before_a_field_is_read() {
        for source in [
            "{'a': 1, 'b': 2, 'b': 3}.a",
            "{'a': 1, 'b': 1 / 0}.a",
            "{'a': 1, 'a': 2}.a",
        ] {
            assert!(
                matches!(map_selected(source), Node::Call(ref f, _) if f == "index_map"),
                "{source}"
            );
        }
    }

    #[test]
    fn a_repeated_key_in_a_result_map_is_invalid() {
        let err = lower("{'a': 1, 'a': 2}", &Decls::default()).unwrap_err();
        assert!(err.to_string().contains("repeated"), "{err}");
    }
}
