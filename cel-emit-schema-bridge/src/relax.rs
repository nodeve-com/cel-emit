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

/// The object schema at `path` of `document`.
fn object<'v>(document: &'v mut Value, path: &[String]) -> Option<&'v mut Value> {
    path.iter().try_fold(document, |v, key| v.get_mut(key))
}

/// `asked`'s schema, each computed property out of the `required` of the object holding it, and
/// the keys taken out. A computed key the object doesn't declare is an error.
pub(crate) fn relaxed(asked: &Asked) -> Result<(Value, Vec<String>), Error> {
    let mut document = asked.schema.clone();
    let path = match asked.into {
        Some(into) => target(&document, &["properties", into]),
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
