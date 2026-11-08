//! The functions a `--functions` file writes in CEL: each key a name, each value its `params`,
//! a list of `{key, schema}`, and its `cel` body.

use cel_emit::{Type, Written};
use serde_json::{Map, Value};

use crate::schema_types;

/// The functions `file` holds, JSON, each param's type read from its JSON Schema. A `$ref` in one
/// resolves against `schema`'s `$defs`, `schema` being the document `--schema` names.
pub(crate) fn written(file: &str, schema: Option<&str>) -> Result<Vec<Written>, String> {
    let file: Value = serde_json::from_str(file).map_err(|e| format!("functions: {e}"))?;
    let Value::Object(all) = file else {
        return Err("functions: a JSON object of functions by name".into());
    };
    let defs = match schema {
        Some(text) => serde_json::from_str::<Value>(text)
            .map_err(|e| e.to_string())?
            .get("$defs")
            .cloned(),
        None => None,
    };
    all.iter().map(|(name, f)| one(name, f, &defs)).collect()
}

fn one(name: &str, f: &Value, defs: &Option<Value>) -> Result<Written, String> {
    let at = |why: &str| format!("function {name}: {why}");
    let valid = name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !valid {
        return Err(at("a name of letters, digits, and `_`"));
    }
    let params = f.get("params").and_then(Value::as_array);
    let params = params.ok_or_else(|| at("`params` is a list"))?;
    let cel = f.get("cel").and_then(Value::as_str);
    let cel = cel.ok_or_else(|| at("`cel` is text"))?;
    let mut typed = Vec::new();
    for p in params {
        let key = p.get("key").and_then(Value::as_str);
        let key = key.ok_or_else(|| at("a param has a text `key`"))?;
        let schema = p
            .get("schema")
            .ok_or_else(|| at("a param has a `schema`"))?;
        typed.push((
            key.to_string(),
            param(schema, defs).map_err(|e| at(&format!("{key}: {e}")))?,
        ));
    }
    Ok(Written {
        name: name.into(),
        params: typed,
        cel: cel.into(),
        path: format!("fn_{name}"),
    })
}

/// `schema` as the type it describes, its `$ref`s resolving in `defs`.
fn param(schema: &Value, defs: &Option<Value>) -> Result<Type, String> {
    let mut document = schema.clone();
    if let (Some(defs), Some(object)) = (defs, document.as_object_mut()) {
        object.insert("$defs".into(), defs.clone());
    }
    let (ir, root) = schema_emit::json_schema::lower("Param", &document, &Map::new(), &[])
        .map_err(|e| e.to_string())?;
    schema_types::record(&ir, root).map_err(|e| e.to_string())
}
