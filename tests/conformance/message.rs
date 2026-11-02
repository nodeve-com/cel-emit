//! A message as a case's expected result: compared with a module's outputs, one per leaf field
//! the expression sets. A field no output names must be unset or at its default, as proto3
//! gives every field one.

use std::sync::Arc;

use cel_core::{MapKey, Value, ValueMap};
use cel_core_proto::{maybe_unwrap_well_known, ProstMessage};
use prost_reflect::{DynamicMessage, ReflectMessage, Value as Proto};

use crate::common::values;

/// A result leaf: its path under the result, and its value as `values::show` writes it.
pub type Leaf = (Vec<String>, String);

/// `want` as a message, if it is one.
pub fn of(want: &Value) -> Option<&DynamicMessage> {
    match want {
        Value::Message(m) => Some(m.as_any().downcast_ref::<ProstMessage>()?.message()),
        _ => None,
    }
}

/// Why `got`, each leaf by its path under the result, is not `want`, if it is not.
pub fn differs(want: &DynamicMessage, got: &[Leaf]) -> Option<String> {
    check(want, &[], got).err()
}

fn check(msg: &DynamicMessage, at: &[String], got: &[Leaf]) -> Result<(), String> {
    for fd in msg.descriptor().fields() {
        let here = [at, &[fd.name().to_string()]].concat();
        let value = msg.get_field(&fd);
        let under: Vec<_> = got
            .iter()
            .filter(|(p, _)| p.starts_with(&here[..]))
            .collect();
        if under.is_empty() {
            if msg.has_field(&fd) && *value != Proto::default_value_for_field(&fd) {
                return Err(format!(
                    "{} is set, but the result leaves it out",
                    here.join(".")
                ));
            }
        } else if let [(p, v)] = under.as_slice() {
            if p.len() == here.len() {
                let want = cel(&value).ok_or_else(|| format!("{}: unreadable", here.join(".")))?;
                if values::show(&want) != *v {
                    return Err(format!("{}: want {want:?}, got {v:?}", here.join(".")));
                }
                continue;
            }
            nested(&value, &here, got)?;
        } else {
            nested(&value, &here, got)?;
        }
    }
    Ok(())
}

fn nested(value: &Proto, at: &[String], got: &[Leaf]) -> Result<(), String> {
    match value {
        Proto::Message(m) => check(m, at, got),
        _ => Err(format!(
            "{}: a result under a field of no fields",
            at.join(".")
        )),
    }
}

/// `v` as a CEL value, where it is a scalar, a wrapper of one, a list, a string-keyed map, or a
/// time.
fn cel(v: &Proto) -> Option<Value> {
    Some(match v {
        Proto::F32(f) => Value::Double((*f).into()),
        Proto::I32(i) => Value::Int((*i).into()),
        Proto::I64(i) => Value::Int(*i),
        Proto::U32(u) => Value::UInt((*u).into()),
        Proto::U64(u) => Value::UInt(*u),
        Proto::F64(d) => Value::Double(*d),
        Proto::Bool(b) => Value::Bool(*b),
        Proto::String(s) => Value::String(s.as_str().into()),
        Proto::Bytes(b) => Value::Bytes(b.as_ref().into()),
        Proto::EnumNumber(n) => Value::Int((*n).into()),
        Proto::List(items) => {
            Value::List(items.iter().map(cel).collect::<Option<Vec<_>>>()?.into())
        }
        Proto::Map(m) => Value::Map(Arc::new(ValueMap::from_entries(
            m.iter()
                .map(|(k, v)| match k {
                    prost_reflect::MapKey::String(k) => {
                        Some((MapKey::String(k.as_str().into()), cel(v)?))
                    }
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()?,
        ))),
        Proto::Message(m) => match maybe_unwrap_well_known(m.clone()) {
            v @ (Value::Timestamp(_)
            | Value::Duration(_)
            | Value::Int(_)
            | Value::UInt(_)
            | Value::Double(_)
            | Value::Bool(_)
            | Value::String(_)
            | Value::Bytes(_)) => v,
            _ => return None,
        },
    })
}
