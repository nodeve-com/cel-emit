//! A record's schema with its computed properties free to be empty: a required one would sit as
//! a plain value, which a failed expression couldn't leave empty.

use schema_emit::Error;
use serde_json::Value;

use crate::Asked;

/// The schema `at` refers to: itself, or the `$defs` entry its `$ref` names.
fn target(document: &Value, at: &[&str]) -> Vec<String> {
    let mut path: Vec<String> = at.iter().map(|s| (*s).to_string()).collect();
    let held = at.iter().try_fold(document, |v, key| v.get(key));
    let named = held
        .and_then(|v| v.get("$ref"))
        .and_then(Value::as_str)
        .and_then(|r| r.strip_prefix("#/$defs/"));
    if let Some(name) = named {
        path = vec!["$defs".into(), name.into()];
    }
    path
}

/// The slot the property `into` of a model in `document` holds: the name its `$dynamicRef` gives.
fn anchor<'v>(document: &'v Value, into: &str) -> Option<&'v str> {
    let named = document.get("properties").and_then(|p| p.get(into));
    let named = named
        .and_then(|p| p.get("$dynamicRef"))
        .and_then(Value::as_str);
    if let Some(name) = named {
        return name.strip_prefix('#');
    }
    let inside: Vec<&Value> = match document {
        Value::Object(map) => map.values().collect(),
        Value::Array(list) => list.iter().collect(),
        _ => Vec::new(),
    };
    inside.into_iter().find_map(|v| anchor(v, into))
}

/// The `$defs` entry of the record's schema that fills the slot property `into` holds, where the
/// record is a filler with no properties of its own.
fn slot(document: &Value, into: &str) -> Option<Vec<String>> {
    let name = anchor(document, into)?;
    let defs = document.get("$defs").and_then(Value::as_object)?;
    let fills = |def: &Value| def.get("$dynamicAnchor").and_then(Value::as_str) == Some(name);
    let (key, _) = defs.iter().find(|(_, def)| fills(def))?;
    Some(vec!["$defs".into(), key.clone()])
}

/// The object schema at `path` of `document`.
fn object<'v>(document: &'v mut Value, path: &[String]) -> Option<&'v mut Value> {
    path.iter().try_fold(document, |v, key| v.get_mut(key))
}

/// Where the object `into` names sits: a property of the record, or a slot's filling.
fn into_path(document: &Value, into: &str) -> Result<Vec<String>, Error> {
    let own = document.get("properties").and_then(|p| p.get(into));
    match own {
        Some(_) => Ok(target(document, &["properties", into])),
        None => slot(document, into).ok_or_else(|| Error::invalid("`into` names no property")),
    }
}

/// `asked`'s schema, each computed property out of the `required` of the object holding it, and
/// the keys taken out. A computed key the object doesn't declare is an error.
pub(crate) fn relaxed(asked: &Asked) -> Result<(Value, Vec<String>), Error> {
    let mut document = asked.schema.clone();
    let path = match asked.into {
        Some(into) => into_path(&document, into)?,
        None => Vec::new(),
    };
    let holder =
        object(&mut document, &path).ok_or_else(|| Error::invalid("`into` names no property"))?;
    for key in asked.computed {
        if holder.get("properties").and_then(|p| p.get(key)).is_none() {
            return Err(Error::invalid("a computed property the schema doesn't declare").at(key));
        }
    }
    let mut taken = Vec::new();
    if let Some(required) = holder.get_mut("required").and_then(Value::as_array_mut) {
        required.retain(|key| {
            let computed = key.as_str().filter(|k| asked.computed.contains(k));
            taken.extend(computed.map(String::from));
            computed.is_none()
        });
    }
    Ok((document, taken))
}
