//! A model, as schema-emit lowers it, mapped to the cel-emit record that holds it.
//! Only types map; the checks stay schema-emit's. What cel-emit cannot hold is refused with its
//! reason, never widened.

use cel_emit::{Field, Type, Width};
use schema_emit::ir::{Base, Float, Int, Schema, Ty};
use schema_emit::Error;

#[cfg(test)]
mod tests;

/// Model `root` of `schema` as a record: one field per field, a nested model a nested record. A
/// model wrapping a root that is no object ([`schema_emit::ir::Model::is_value`]) is the type it
/// holds.
pub fn record(schema: &Schema, root: usize) -> Result<Type, Error> {
    let value = schema.models.get(root).filter(|m| m.is_value());
    match value.and_then(|m| m.fields.first()) {
        Some(f) => ty(schema, &f.ty, &mut Vec::new()),
        None => model(schema, root, &mut Vec::new()),
    }
}

fn model(schema: &Schema, at: usize, open: &mut Vec<usize>) -> Result<Type, Error> {
    let m = schema
        .models
        .get(at)
        .ok_or_else(|| Error::invalid(format!("model {at} is not in the schema")))?;
    if open.contains(&at) {
        return Err(Error::unsupported(format!(
            "{}: a model holding itself has no finite record",
            m.name
        )));
    }
    if m.others.is_some() {
        return Err(Error::unsupported(format!(
            "{}: keys no field names have no record",
            m.name
        )));
    }
    open.push(at);
    let mut fields = Vec::new();
    for f in &m.fields {
        let ty = ty(schema, &f.ty, open).map_err(|e| e.at(&format!("{}.{}", m.name, f.name)))?;
        // A field a record may lack is an option of its type.
        let ty = match f.required {
            true => ty,
            false => Type::Optional(Box::new(ty)),
        };
        fields.push(Field {
            name: f.name.clone(),
            ty,
            required: f.required,
        });
    }
    open.pop();
    Ok(Type::message(fields))
}

/// The type, a number at its stored width wherever it sits.
fn ty(schema: &Schema, t: &Ty, open: &mut Vec<usize>) -> Result<Type, Error> {
    let base = match &t.base {
        Base::Bool => Type::Bool,
        Base::Text => Type::String,
        Base::Timestamp => Type::Timestamp,
        Base::Duration => Type::Duration,
        Base::Bytes => Type::Bytes,
        Base::Int(i) => int(*i),
        Base::Float(Float::F32) => Type::Narrow(Width::F32),
        Base::Float(Float::F64) => Type::Double,
        Base::List(item) => Type::list(ty(schema, item, open)?),
        Base::Map(item) => Type::map(ty(schema, item, open)?),
        Base::Tuple { prefix, rest } => tuple(schema, prefix, rest.as_deref(), open)?,
        Base::Model(m) => model(schema, *m, open)?,
        Base::Null => return Err(Error::unsupported("null alone has no type")),
        Base::Any => return Err(Error::unsupported("any value has no type")),
        Base::Union(_) => return Err(Error::unsupported("unions are not on the edge")),
    };
    Ok(if t.nullable {
        Type::Nullable(Box::new(base))
    } else {
        base
    })
}

fn int(i: Int) -> Type {
    match i {
        Int::I8 => Type::Narrow(Width::I8),
        Int::I16 => Type::Narrow(Width::I16),
        Int::I32 => Type::Narrow(Width::I32),
        Int::I64 => Type::Int,
        Int::U8 => Type::Narrow(Width::U8),
        Int::U16 => Type::Narrow(Width::U16),
        Int::U32 => Type::Narrow(Width::U32),
        Int::U64 => Type::Uint,
        Int::I128 => Type::Narrow(Width::I128),
        Int::U128 => Type::Narrow(Width::U128),
    }
}

/// An array whose positions all share one type is a list; any other mix has no type.
fn tuple(
    schema: &Schema,
    prefix: &[Ty],
    rest: Option<&Ty>,
    open: &mut Vec<usize>,
) -> Result<Type, Error> {
    let mut all = prefix.iter().chain(rest);
    let first = all
        .next()
        .ok_or_else(|| Error::unsupported("an empty array has no type"))?;
    if all.any(|t| t != first) {
        return Err(Error::unsupported("a mixed-kind tuple has no type"));
    }
    Ok(Type::list(ty(schema, first, open)?))
}
