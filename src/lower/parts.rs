//! A message the lowering knows the fields of (a struct literal, a map literal, a message read
//! whole), taken apart to select a field, test its presence, or write it, and put back together.

use super::*;
use crate::tree::split;
use crate::types::Field;

/// Whether `t` is a message literal with no `?` entry, which sets its fields only where a value
/// holds one.
pub(crate) fn is_message(t: &Typed) -> bool {
    match (&t.ty, &t.node) {
        (Type::Message(_), Node::Message(made)) => made.iter().all(|(_, _, flagged)| !flagged),
        _ => false,
    }
}

/// Whether `t` is a message whose fields lowering can take apart: a message literal, or a bind
/// around a message.
pub(crate) fn is_fields(t: &Typed) -> bool {
    is_message(t) || matches!((&t.ty, &t.node), (Type::Message(_), Node::Let(..)))
}

/// A field's value as the field itself holds it, without the `Some` a nullable field wraps.
pub(crate) fn held_by_field(v: Typed) -> Typed {
    match v.node {
        Node::Some(x) if matches!(v.ty, Type::Nullable(_)) => *x,
        _ => v,
    }
}

/// The fields of `t`, which [`is_fields`], each by name and as the field itself holds it; the
/// locals `t` binds are bound around each, for a caller reading one field at a time.
pub(crate) fn parts(t: Typed) -> Vec<(String, Typed)> {
    let s = split(t);
    s.fields
        .into_iter()
        .map(|(name, v)| (name, super::scope::wrap(held_by_field(v), &s.lets)))
        .collect()
}

/// A message of type `ty` setting these fields, each held as its field holds it.
pub(crate) fn rebuild(ty: &Type, fields: Vec<(String, Typed)>) -> Result<Typed, Error> {
    let mut entries = Vec::with_capacity(fields.len());
    for (name, v) in fields {
        let i = ty
            .fields()
            .iter()
            .position(|f| f.name == name)
            .ok_or_else(|| Error::Invalid(format!("no field {name}")))?;
        let want = ty.fields().get(i).map_or(Type::Int, |f| f.ty.clone());
        entries.push((i, super::message::held(v, &want), false));
    }
    Ok(Typed::new(ty.clone(), Node::Message(entries)))
}

/// A map literal with string keys: a message of these fields, each typed as its value and
/// flagged where its entry is `?`.
pub(crate) fn literal_map(fields: Vec<(String, Typed, bool)>) -> Result<Typed, Error> {
    let flags: Vec<bool> = fields.iter().map(|(_, _, flagged)| *flagged).collect();
    let made = anonymous(fields.into_iter().map(|(k, v, _)| (k, v)).collect())?;
    let Node::Message(entries) = made.node else {
        return Err(Error::Invalid("a map literal that is no message".into()));
    };
    let flagged = entries.into_iter().zip(flags);
    let entries = flagged.map(|((i, v, _), f)| (i, v, f)).collect();
    Ok(Typed::new(made.ty, Node::Message(entries)))
}

/// A message of these fields, each typed as its value.
pub(crate) fn anonymous(fields: Vec<(String, Typed)>) -> Result<Typed, Error> {
    let declared = fields
        .iter()
        .map(|(name, v)| Field {
            name: name.clone(),
            ty: v.ty.clone(),
            required: false,
        })
        .collect();
    rebuild(&Type::Message(declared), fields)
}
