//! A program's steps up to the bridge: lower the record's schema, make its standard form with
//! each computed property a field filled later, and bridge it.

use cel_emit_schema_bridge::{bridge, Asked, Bridged};
use schema_emit::emit::std_form::{names, StdForm};
use serde_json::{Map, Value};

/// What a program asks of a record's schema.
pub struct Ask<'a> {
    /// The record's model name.
    pub name: &'a str,
    pub into: Option<&'a str>,
    pub computed: &'a [&'a str],
    pub module: &'a str,
    /// Each model another module holds, by name, and that module's path.
    pub external: &'a [(String, &'a str)],
    /// Whether the standard form names each computed property a field filled later.
    pub later: bool,
}

/// Each computed property's path from the record: `<into>.<key>`, or `<key>` with no `into`.
fn paths(ask: &Ask) -> Vec<String> {
    let path = |key: &&str| match ask.into {
        Some(into) => format!("{into}.{key}"),
        None => (*key).to_string(),
    };
    match ask.later {
        true => ask.computed.iter().map(path).collect(),
        false => Vec::new(),
    }
}

/// `schema` bridged as `ask` asks.
pub fn bridged(schema: &Value, ask: &Ask) -> Result<Bridged, String> {
    let lowered = schema_emit::json_schema::lower(ask.name, schema, &Map::new(), &[]);
    let (lowered, root) = lowered.map_err(|e| e.to_string())?;
    let named = names(&lowered);
    let name = |i: usize| named.get(i).cloned().unwrap_or_default();
    let theirs = |i: usize| {
        let model = lowered.models.get(i)?;
        let held = ask.external.iter().find(|(name, _)| *name == model.name);
        held.map(|(_, module)| (*module).to_string())
    };
    let paths = paths(ask);
    let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
    let std = StdForm::new(&lowered, &name).with_external(&theirs);
    let std = std.with_empty(root, &paths).map_err(|e| e.to_string())?;
    let asked = Asked {
        std: &std,
        root,
        into: ask.into,
        computed: ask.computed,
        module: ask.module,
    };
    bridge(&asked).map_err(|e| e.to_string())
}
