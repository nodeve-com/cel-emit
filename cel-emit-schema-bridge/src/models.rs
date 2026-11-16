//! Each message and tuple of a schema as cel-emit's emitter asks for it.

use cel_emit::emit::Messages;
use cel_emit::{Message, Type};
use schema_emit::emit::std_form::{self, StdForm};

use crate::tuples::{self, Tuple};
use crate::types;

/// Each message as the struct, or alias, schema-emit writes for its model.
#[derive(Clone)]
pub struct Models {
    records: Vec<(Message, String)>,
    tuples: Vec<Tuple>,
}

impl Messages for Models {
    fn path(&mut self, _: usize, message: &Message) -> String {
        let found = self.records.iter().find(|(held, _)| held == message);
        found.map_or_else(String::new, |(_, rust)| rust.clone())
    }

    fn fields(&self, message: &Message) -> Vec<String> {
        std_form::fields(message.fields.iter().map(|f| f.name.as_str()))
    }

    fn writes(&self, rust: &str) -> Option<String> {
        let found = self.tuples.iter().find(|t| t.rust == rust);
        found.map(|t| t.writes.clone())
    }

    fn reads(&self, rust: &str) -> Option<String> {
        let found = self.tuples.iter().find(|t| t.rust == rust);
        found.map(|t| t.reads.clone())
    }
}

impl Models {
    /// Each model of `std`'s schema that is a record, its Rust in `module`, or in the module
    /// `std` names as holding it.
    pub fn new(std: &StdForm, module: &str) -> Models {
        let filled = std.schema.filled();
        let records = (0..filled.models.len()).filter_map(|k| {
            let made = filled
                .models
                .get(k)
                .filter(|m| !m.is_value())
                .map(|_| types::model(&filled, Some(std), k));
            let Some(Ok(Type::Message(message))) = made else {
                return None;
            };
            // A model is the struct holding it: a struct with type parameters infers them from
            // its fields.
            let s = filled.filling(k);
            let at = (std.external)(s).unwrap_or_else(|| module.into());
            Some((message, format!("{at}::{}", std.model_name(s))))
        });
        Models {
            records: records.collect(),
            tuples: tuples::tuples(&filled, std, module),
        }
    }
}
