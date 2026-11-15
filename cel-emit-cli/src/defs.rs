//! A second schema file's definitions, joined to the `--schema` file's.

use serde_json::Value;

/// `schema`, a JSON Schema document, with `defs`'s definitions among its own: `defs`'s root as
/// `name`, and each of its own definitions by its name. A name both define is an error.
pub fn with_defs(schema: &str, name: &str, defs: &str) -> Result<String, String> {
    let parse = |text: &str| serde_json::from_str::<Value>(text).map_err(|e| e.to_string());
    let joined = schema_emit::json_schema::join(&parse(schema)?, name, &parse(defs)?)
        .map_err(|e| format!("--defs: {}", e.message))?;
    Ok(joined.to_string())
}
