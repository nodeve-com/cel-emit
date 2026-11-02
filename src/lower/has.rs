//! `has()` of a field or a map key: proto3 presence, which a message can tell. A scalar is present
//! when nonzero, a list or map when nonempty. A message's presence needs a bit a message doesn't
//! carry, so a message field read from a value is refused; a message literal says which fields it
//! sets.

use super::parts::{is_fields, is_message, parts};
use super::*;
use crate::types::Field;

impl<'a> Lower<'a> {
    /// `has(base.field)`: `base` read whole, then tested. A map's key is present when the map
    /// holds it.
    pub(crate) fn has_field(&mut self, base: &'a SpannedExpr, field: &str) -> Result<Node, Error> {
        let message = match &base.node {
            Expr::Struct { .. } | Expr::Ident(_) => self.selected(base)?,
            _ => None,
        };
        if let Some(stored) = self.optional_stored(base)? {
            return has_in(stored, field);
        }
        if let Some(made) = message.filter(is_message) {
            return self.literal_has(base, field, parts(made));
        }
        has_in(self.expr(base)?, field)
    }

    /// `has(Message{…}.field)`: whether the literal sets `field`, from the literal's own fields.
    fn literal_has(
        &mut self,
        base: &SpannedExpr,
        field: &str,
        made: Vec<(String, Typed)>,
    ) -> Result<Node, Error> {
        let Type::Message(fields) = self.ty(base)? else {
            return invalid("a message where a value is wanted");
        };
        let i = field_index(&fields, field)?;
        let nullable = matches!(fields.get(i), Some(f) if matches!(f.ty, Type::Nullable(_)));
        match made.into_iter().find(|(n, _)| n == field) {
            None => Ok(Node::Bool(false)),
            Some((_, v)) if is_fields(&v) => Ok(Node::Bool(true)),
            Some((_, t)) if nullable => Ok(Node::Bool(t.node != Node::None)),
            Some((_, t)) => presence(t),
        }
    }
}

/// `has(base.field)` on a lowered `base`. An optional base has the field when it holds a value
/// that has it, as `base.?field` selects it.
fn has_in(base: Typed, field: &str) -> Result<Node, Error> {
    match base.ty.clone() {
        Type::Optional(held) => {
            let present = Typed::call("optional_has_value", vec![base.clone()], Type::Bool);
            let value = Typed::call("optional_value", vec![base], *held);
            let has = Typed::new(Type::Bool, has_in(value, field)?);
            Ok(Node::Cond(
                Box::new(present),
                Box::new(has),
                Box::new(Typed::bool(false)),
            ))
        }
        Type::Map(k, _) => Ok(Node::Call(
            "in_map".into(),
            vec![super::dynamic::field_key(&k, field)?, base],
        )),
        Type::Dyn => Ok(Node::Call(
            "dyn_has".into(),
            vec![base, Typed::string(field)],
        )),
        Type::Message(fields) => presence(select_field(base, &fields, field)?),
        _ => unsupported("has() of other than a field or key"),
    }
}

/// `base.field` of a message's `fields`, typed as the field.
pub(crate) fn select_field(base: Typed, fields: &[Field], field: &str) -> Result<Typed, Error> {
    let i = field_index(fields, field)?;
    let ty = fields.get(i).map(|f| f.ty.clone()).unwrap_or(Type::Int);
    Ok(Typed::stored(ty, Node::Field(Box::new(base), i)))
}

/// The index of `field` among `fields`, or refused: a field cel-emit has no type for.
pub(crate) fn field_index(fields: &[Field], field: &str) -> Result<usize, Error> {
    fields
        .iter()
        .position(|f| f.name == field)
        .ok_or_else(|| Error::Unsupported(format!("has() of {field}, a field without a type")))
}

/// Whether `value` is present, by proto3's rule for its type.
pub(crate) fn presence(value: Typed) -> Result<Node, Error> {
    let zero = match &value.ty {
        Type::Int => Typed::int(0),
        Type::Uint => Typed::new(Type::Uint, Node::Uint(0)),
        Type::Double => Typed::double(0.0),
        Type::Bool => Typed::bool(false),
        Type::String => Typed::string(""),
        Type::Bytes => Typed::new(Type::Bytes, Node::Bytes(Vec::new())),
        Type::Optional(_) => {
            return Ok(Node::Call("optional_has_value".into(), vec![value]));
        }
        Type::Nullable(_) => {
            let none = Typed::new(value.ty.clone(), Node::None);
            return Typed::binary("!=", value, none)
                .map(|t| t.node)
                .ok_or_else(|| Error::Invalid("has() of a nullable".into()));
        }
        Type::List(_) | Type::Map(..) => {
            let size = if matches!(value.ty, Type::List(_)) {
                "size_list"
            } else {
                "size_map"
            };
            let size = Typed::call(size, vec![value], Type::Int);
            return Typed::binary(">", size, Typed::int(0))
                .map(|t| t.node)
                .ok_or_else(|| Error::Invalid("has() of a size".into()));
        }
        _ => return unsupported("has() of a message field"),
    };
    Typed::binary("!=", value, zero)
        .map(|t| t.node)
        .ok_or_else(|| Error::Invalid("has() of a scalar".into()))
}
