//! Models another module holds: written there, read here by their path.

use schema_emit::ir::Schema;
use schema_emit::json_schema::lower_defs;
use schema_emit::Error;
use serde_json::{Map, Value};

/// A module holding models the bridge writes none of.
pub struct Held<'a> {
    /// The document the module's models are written from, its `$defs` the models: each model of
    /// the record's schema the same name as one of them is the module's.
    pub schema: &'a Value,
    /// The module's Rust path, one that resolves both inside [`crate::Bridged::items`] and in the
    /// code using them, as `crate::generic`.
    pub module: &'a str,
}

/// The module holding each of `schema`'s models, by index, where one does.
pub(crate) fn modules(schema: &Schema, held: &[Held]) -> Result<Vec<Option<String>>, Error> {
    let mut theirs: Vec<(String, &str)> = Vec::new();
    for each in held {
        let (lowered, _) = lower_defs(each.schema, &Map::new(), &[])?;
        theirs.extend(lowered.models.into_iter().map(|m| (m.name, each.module)));
    }
    let found = schema.models.iter().map(|m| {
        let hold = theirs.iter().find(|(name, _)| *name == m.name);
        hold.map(|(_, module)| (*module).to_string())
    });
    Ok(found.collect())
}
