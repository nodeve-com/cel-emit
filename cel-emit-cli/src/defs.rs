//! A second schema file's definitions, joined to the `--schema` file's.

use serde_json::{Map, Value};

/// `schema`, a JSON Schema document, with `defs`'s definitions among its `$defs`: `defs`'s root
/// as `name`, and each of its own `$defs` by its name. A name both define is an error.
pub fn with_defs(schema: &str, name: &str, defs: &str) -> Result<String, String> {
    let parse = |text: &str| serde_json::from_str::<Value>(text).map_err(|e| e.to_string());
    let (mut document, mut root) = (parse(schema)?, parse(defs)?);
    let theirs = match root.as_object_mut().and_then(|o| o.remove("$defs")) {
        Some(Value::Object(theirs)) => theirs,
        _ => Map::new(),
    };
    let object = document.as_object_mut().ok_or("a schema is an object")?;
    let ours = object
        .entry("$defs")
        .or_insert_with(|| Value::Object(Map::new()));
    let ours = ours.as_object_mut().ok_or("`$defs` is an object")?;
    for (key, value) in theirs.into_iter().chain([(name.to_string(), root)]) {
        if ours.contains_key(&key) {
            return Err(format!("--defs: both files define {key}"));
        }
        ours.insert(key, value);
    }
    Ok(document.to_string())
}
