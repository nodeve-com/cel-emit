#![doc = include_str!("../README.md")]

use std::collections::BTreeMap;

use cel_emit::computed::{Property, Record};
use cel_emit::emit::Messages;
use cel_emit::{Message, Type};
use schema_emit::emit::std_form::{self, StdForm};
use schema_emit::emit::Emitter;
use schema_emit::ir::{Base, Field, Schema};
use schema_emit::Error;

pub mod types;

/// What to bridge: a record's standard form, and the properties expressions fill.
pub struct Asked<'a> {
    /// The standard form of the record's lowered schema: each model another module holds named
    /// by `with_external`, each computed property a field `with_empty` fills later.
    pub std: &'a StdForm<'a>,
    /// The record's model in the form's schema.
    pub root: usize,
    /// The one object property the computed properties fill, where they don't fill the record's
    /// own.
    pub into: Option<&'a str>,
    /// Each computed property's key: a bare property name of the record, or of the `into`
    /// object where `into` is set. A key the object doesn't declare is refused.
    pub computed: &'a [&'a str],
    /// The Rust path of the module [`Bridged::items`] sits in. A model the form names as written
    /// in another module is read by that module's path, the record's own included.
    pub module: &'a str,
}

/// A schema as computed properties take it.
pub struct Bridged {
    /// The record: its Rust type, and each property's type, place, and check.
    pub record: Record,
    /// Each record type the schema names ([`Schema::named`]), for an expression to build by that
    /// name.
    pub messages: BTreeMap<String, Type>,
    /// Each other type the schema names that cel-emit holds, a list or a scalar, for a call to
    /// name as its type argument (cel-emit's `Decls::types`).
    pub types: BTreeMap<String, Type>,
    /// Each message's Rust.
    pub models: Models,
    /// The types and checks, for a module holding `use ::schema_emit_runtime as __s;`, in a
    /// crate linking cel-emit-runtime.
    pub items: String,
    /// The computed properties the schema requires, as their fields say. Each the program names
    /// a field filled later sits as one that may be empty, so a failure can leave it so, and
    /// whoever walks the index judges it.
    pub required: Vec<String>,
    /// The Rust path of the function decoding the record.
    pub decode: String,
    /// The Rust path of the function writing the record to a serializer, where its serde impl
    /// doesn't: a record filling a dynamic reference with a type that is no object
    /// ([`StdForm::is_bare`]).
    pub encode: Option<String>,
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

/// One bridging: the filled schema, what was asked, and the checks written so far.
struct Bridge<'a> {
    filled: &'a Schema,
    asked: &'a Asked<'a>,
    checks: String,
    /// How many checks are written.
    count: usize,
    loads: Vec<String>,
    required: Vec<String>,
    /// The model whose fields the computed properties are.
    holder: usize,
}

impl Bridge<'_> {
    /// `field`, field `at` of model `model`, as a property sitting at `place`, with its check
    /// where an expression fills it.
    fn property(
        &mut self,
        (model, at): (usize, usize),
        field: &Field,
        place: String,
    ) -> Result<Property, Error> {
        let key = field.name.as_str();
        let ty =
            types::value(self.filled, Some(self.asked.std), &field.ty).map_err(|e| e.at(key))?;
        let computed = model == self.holder && self.asked.computed.contains(&key);
        if computed && field.required {
            self.required.push(key.into());
        }
        Ok(Property {
            key: key.into(),
            ty,
            place,
            empty: self.asked.std.can_be_empty(self.filled.filling(model), at),
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
        let check = Emitter::new(self.asked.std).check_value(&field.ty, &field.name, &name)?;
        if check.contains(&format!("pub fn {name}_load(")) {
            self.loads.push(format!("{module}::{name}_load"));
        }
        self.checks.push_str(&check);
        Ok(format!(
            "(|value| {module}::checked(value, {module}::{name}))"
        ))
    }

    /// Model `m`'s fields.
    fn fields(&self, m: usize) -> Vec<Field> {
        let model = self.filled.models.get(m);
        model.map(|m| m.fields.clone()).unwrap_or_default()
    }

    /// The field of `root` that `into` names and the model it holds, where `into` is set.
    fn holder(&self, root: usize) -> Result<Option<(usize, usize)>, Error> {
        let Some(into) = self.asked.into else {
            return Ok(None);
        };
        let own = self.fields(root);
        let at = own.iter().position(|f| f.name == into);
        let at = at.ok_or_else(|| Error::invalid("`into` names no property"))?;
        let held = !self.asked.std.can_be_empty(self.filled.filling(root), at);
        match own.get(at).map(|f| (&f.ty.base, f.ty.nullable)) {
            Some((Base::Model(m), false)) if held => Ok(Some((at, *m))),
            _ => Err(Error::invalid("`into` names no required object").at(into)),
        }
    }

    /// Each property an expression reads by name: the record's, and in place of `into` its own.
    fn properties(&mut self, root: usize) -> Result<Vec<Property>, Error> {
        let holder = self.holder(root)?;
        self.holder = holder.map_or(root, |(_, m)| m);
        let declared = self.fields(self.holder);
        let missing =
            (self.asked.computed.iter()).find(|k| !declared.iter().any(|f| f.name == **k));
        if let Some(key) = missing {
            return Err(Error::invalid("a computed property the schema doesn't declare").at(key));
        }
        let own = self.fields(root);
        let rust = std_form::fields(own.iter().map(|f| f.name.as_str()));
        let mut properties = Vec::new();
        for (i, (field, place)) in own.iter().zip(rust).enumerate() {
            let Some((_, m)) = holder.filter(|(at, _)| *at == i) else {
                properties.push(self.property((root, i), field, place)?);
                continue;
            };
            let inside = std_form::fields(declared.iter().map(|f| f.name.as_str()));
            for (j, (field, at)) in declared.iter().zip(inside).enumerate() {
                properties.push(self.property((m, j), field, format!("{place}.{at}"))?);
            }
        }
        Ok(properties)
    }
}

/// Each type `filled` names that cel-emit holds, by its name: the records, then the rest.
fn named(filled: &Schema, std: &StdForm) -> (BTreeMap<String, Type>, BTreeMap<String, Type>) {
    let held = filled.named.iter().filter_map(|(name, ty)| {
        let ty = types::value(filled, Some(std), ty).ok()?;
        Some((name.clone(), ty))
    });
    held.partition(|(_, ty)| matches!(ty, Type::Message(_)))
}

/// `asked`'s record as computed properties take it.
pub fn bridge(asked: &Asked) -> Result<Bridged, Error> {
    let std = asked.std;
    let filled = std.schema.filled();
    let mut bridge = Bridge {
        filled: &filled,
        asked,
        checks: String::new(),
        count: 0,
        loads: Vec::new(),
        required: Vec::new(),
        holder: asked.root,
    };
    let properties = bridge.properties(asked.root)?;
    let (messages, types) = named(&filled, std);
    let module = asked.module;
    // A filler has no decoder of its own: the struct holding it decodes it. One filling with a
    // type that is no struct reads and writes by functions of its own.
    let bare = std.is_bare(asked.root);
    let reader = if bare {
        asked.root
    } else {
        filled.filling(asked.root)
    };
    let home = (std.external)(reader).unwrap_or_else(|| module.into());
    let decode = std.decoder(reader);
    // The record's type sits where the form says its model is written.
    let own = (std.external)(asked.root).unwrap_or_else(|| module.into());
    let encode = decode.strip_prefix("decode_").filter(|_| bare);
    Ok(Bridged {
        record: Record {
            rust: format!("{own}::{}", std.model_name(asked.root)),
            properties,
        },
        messages,
        types,
        models: Models::new(std, module),
        items: format!("{}{}{CHECKED}", std.items()?, bridge.checks),
        required: bridge.required,
        encode: encode.map(|id| format!("{home}::encode_{id}")),
        decode: format!("{home}::{decode}"),
        loads: bridge.loads,
    })
}
