#![doc = include_str!("../README.md")]

use std::collections::BTreeMap;

use cel_emit::computed::{Property, Record};
use cel_emit::emit::Messages;
use cel_emit::{Message, Type};
use schema_emit::emit::std_form::{self, names, StdForm};
use schema_emit::emit::Emitter;
use schema_emit::ir::{Base, Field, Schema};
use schema_emit::Error;
use serde_json::Value;

mod held;
mod relax;
pub mod types;

pub use held::Held;

/// What to bridge: a record's JSON Schema, and the properties expressions fill.
pub struct Asked<'a> {
    pub schema: &'a Value,
    /// The record's name: its Rust type's, in camel case.
    pub name: &'a str,
    /// The one object property the computed properties fill, where they don't fill the record's
    /// own.
    pub into: Option<&'a str>,
    /// Each computed property's key.
    pub computed: &'a [&'a str],
    /// The Rust path of the module [`Bridged::items`] sits in.
    pub module: &'a str,
    /// The modules holding models the schema reads, as the generic model a record fills the slots
    /// of. [`Bridged::items`] writes none of them.
    pub held: &'a [Held<'a>],
}

/// A schema as computed properties take it.
pub struct Bridged {
    /// The record: its Rust type, and each property's type, place, and check.
    pub record: Record,
    /// Each type the schema's `$defs` names, for an expression to build by that name.
    pub messages: BTreeMap<String, Type>,
    /// Each message's Rust.
    pub models: Models,
    /// The types and checks, for a module holding `use ::schema_emit_runtime as __s;`, in a
    /// crate linking cel-emit-runtime.
    pub items: String,
    /// The computed properties the schema requires. Each sits as one that may be empty, so a
    /// failure can leave it so, and whoever walks the index judges it.
    pub required: Vec<String>,
    /// The Rust path of the function decoding the record.
    pub decode: String,
    /// The Rust path of each function checking a check's pattern tables, to call once.
    pub loads: Vec<String>,
}

/// Each message as the struct, or alias, schema-emit writes for its model.
#[derive(Clone)]
pub struct Models {
    records: Vec<(Message, String)>,
}

impl Messages for Models {
    fn path(&mut self, _: usize, message: &Message) -> String {
        let found = self.records.iter().find(|(held, _)| held == message);
        found.map_or_else(String::new, |(_, rust)| rust.clone())
    }

    fn fields(&self, message: &Message) -> Vec<String> {
        std_form::fields(message.fields.iter().map(|f| f.name.as_str()))
    }
}

impl Models {
    /// Each model of `schema` that is a record, its struct in `module`.
    pub fn new(schema: &Schema, module: &str) -> Models {
        let named = names(schema);
        let name = |i: usize| named.get(i).cloned().unwrap_or_default();
        let none = vec![None; schema.models.len()];
        Models::held(&StdForm::new(schema, &name), &none, module)
    }

    /// Each model of the form's schema that is a record, its Rust in `module`, or in the module
    /// holding it: `external` gives each model's, by index.
    pub fn held(std: &StdForm, external: &[Option<String>], module: &str) -> Models {
        let each = (0..std.schema.models.len()).zip(external);
        let records = each.filter_map(|(i, held)| match types::record(std.schema, i) {
            Ok(Type::Message(message)) => {
                let at = held.as_deref().unwrap_or(module);
                Some((message, format!("{at}::{}", std.model_name(i))))
            }
            _ => None,
        });
        Models {
            records: records.collect(),
        }
    }
}

/// The helper every check goes through: the first failure a check reports, as cel-emit-runtime's.
const CHECKED: &str = "
pub fn checked<T>(
    value: T,
    check: impl FnOnce(&T, &mut __s::Failures<1>),
) -> ::core::result::Result<T, ::cel_emit_runtime::computed::Failure> {
    let mut failures = __s::Failures::<1>::default();
    check(&value, &mut failures);
    let failed = failures.iter().next().map(|f| (f.to_string(), f.reason.name()));
    match failed {
        Some((place, reason)) => Err(::cel_emit_runtime::computed::Failure::check(place, reason)),
        None => Ok(value),
    }
}
";

/// One bridging: the lowered schema, what was asked, and the checks written so far.
struct Bridge<'a> {
    schema: &'a Schema,
    asked: &'a Asked<'a>,
    std: &'a StdForm<'a>,
    checks: String,
    /// How many checks are written.
    count: usize,
    loads: Vec<String>,
}

impl Bridge<'_> {
    /// `field` as a property sitting at `place`, with its check where an expression fills it.
    fn property(&mut self, field: &Field, place: String) -> Result<Property, Error> {
        let key = field.name.as_str();
        let ty = types::value(self.schema, &field.ty).map_err(|e| e.at(key))?;
        let computed = self.asked.computed.contains(&key);
        Ok(Property {
            key: key.into(),
            ty,
            place,
            empty: !field.held(),
            check: computed.then(|| self.check(field)).transpose()?,
        })
    }

    /// Writes `field`'s check, and gives what calls it with a value.
    fn check(&mut self, field: &Field) -> Result<String, Error> {
        if field.ty.nullable {
            return Err(Error::unsupported("a nullable computed property").at(&field.name));
        }
        let module = self.asked.module;
        let name = format!("check_{}", self.count);
        self.count = self.count.saturating_add(1);
        let check = Emitter::new(self.std).check_value(&field.ty, &field.name, &name)?;
        if check.contains(&format!("pub fn {name}_load(")) {
            self.loads.push(format!("{module}::{name}_load"));
        }
        self.checks.push_str(&check);
        Ok(format!(
            "(|value| {module}::checked(value, {module}::{name}))"
        ))
    }

    /// Model `m`'s fields, a slot's holding the type filling it.
    fn fields(&self, m: usize) -> Vec<Field> {
        let filled = self.schema.filled(m);
        let model = filled.as_ref().or_else(|| self.schema.models.get(m));
        model.map(|m| m.fields.clone()).unwrap_or_default()
    }

    /// Each property an expression reads by name: the record's, and in place of `into` its own.
    fn properties(&mut self, root: usize) -> Result<Vec<Property>, Error> {
        let own = self.fields(root);
        let rust = std_form::fields(own.iter().map(|f| f.name.as_str()));
        let mut properties = Vec::new();
        for (field, place) in own.iter().zip(rust) {
            if self.asked.into != Some(field.name.as_str()) {
                properties.push(self.property(field, place)?);
                continue;
            }
            let inner = match (&field.ty.base, field.held()) {
                (Base::Model(m), true) => self.fields(*m),
                _ => return Err(Error::invalid("`into` names no required object").at(&field.name)),
            };
            let inside = std_form::fields(inner.iter().map(|f| f.name.as_str()));
            for (field, at) in inner.iter().zip(inside) {
                properties.push(self.property(field, format!("{place}.{at}"))?);
            }
        }
        Ok(properties)
    }
}

/// Each type `document`'s `$defs` names that is a record.
fn messages(document: &Value, schema: &Schema) -> BTreeMap<String, Type> {
    let defs = document.get("$defs").and_then(Value::as_object);
    let named = defs.into_iter().flat_map(|defs| defs.keys());
    let found = named.filter_map(|key| {
        let at = schema.models.iter().position(|m| m.name == *key)?;
        match types::record(schema, at) {
            Ok(ty @ Type::Message(_)) => Some((key.clone(), ty)),
            _ => None,
        }
    });
    found.collect()
}

/// `asked`'s schema as computed properties take it.
pub fn bridge(asked: &Asked) -> Result<Bridged, Error> {
    let (document, required) = relax::relaxed(asked)?;
    let shared = serde_json::Map::new();
    let (schema, root) = schema_emit::json_schema::lower(asked.name, &document, &shared, &[])?;
    let named = names(&schema);
    let name = |i: usize| named.get(i).cloned().unwrap_or_default();
    let external = held::modules(&schema, asked.held)?;
    let theirs = |i: usize| external.get(i).cloned().flatten();
    let std = StdForm::new(&schema, &name).with_external(&theirs);
    let mut bridge = Bridge {
        schema: &schema,
        asked,
        std: &std,
        checks: String::new(),
        count: 0,
        loads: Vec::new(),
    };
    let properties = bridge.properties(root)?;
    let module = asked.module;
    // A filler has no decoder of its own: the generic model's decodes it.
    let generic = schema.filling(root);
    let home = external
        .get(generic)
        .and_then(Option::as_deref)
        .unwrap_or(module);
    Ok(Bridged {
        record: Record {
            rust: format!("{module}::{}", std.model_name(root)),
            properties,
        },
        messages: messages(&document, &schema),
        models: Models::held(&std, &external, module),
        items: format!("{}{}{CHECKED}", std.items()?, bridge.checks),
        required,
        decode: format!("{home}::{}", std.decoder(generic)),
        loads: bridge.loads,
    })
}
