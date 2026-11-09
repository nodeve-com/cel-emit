//! The CEL environment an expression checks against: each name a variable, each message a message,
//! and each type in CEL.

use std::collections::{BTreeMap, BTreeSet};

use cel_core::{CelType, Env, FunctionDecl, OverloadDecl, Value};

use crate::decls::{Decl, Decls};
use crate::types::Type;

/// The CEL environment `decls` make: the standard library, the math, optionals, and string
/// extensions, cel-emit's functions and the embedder's, each name, and each message. A dotted
/// name's prefixes are messages: `prev.?Smooth.watts` selects through them.
pub fn environment(decls: &Decls) -> Env {
    let mut scope = Scope {
        messages: Messages {
            enums: decls.enums.keys().cloned().collect(),
            ..Messages::default()
        },
        roots: BTreeMap::new(),
    };
    let mut declared = crate::functions::declarations();
    let embedder = decls.functions.iter().map(|f| &f.decl);
    let embedder = embedder.chain(decls.expansions.iter().map(|x| &x.decl));
    declared.extend(embedder.map(|d| declared_as(d, &mut scope.messages)));
    let mut env = Env::with_standard_library().with_extension(declared);
    for (name, ty) in &decls.messages {
        let held = message_name(ty);
        scope.messages.declare(&held, ty);
        scope.messages.names.insert(name.clone(), held);
    }
    for (name, n) in &decls.names {
        scope.add(name, &n.ty);
    }
    let Scope {
        messages,
        mut roots,
    } = scope;
    // A message's name is also a type value, a function's type argument, unless
    // a name in scope already takes it.
    for (name, held) in &messages.names {
        roots
            .entry(name.clone())
            .or_insert_with(|| CelType::type_of(CelType::message(held)));
    }
    for (root, ty) in roots {
        env.add_variable(root, ty);
    }
    env.with_proto_registry(std::sync::Arc::new(messages))
}

/// What the declared names make: their messages, and the variables the environment holds.
struct Scope {
    messages: Messages,
    roots: BTreeMap<String, CelType>,
}

impl Scope {
    /// Declares the name `name` of type `ty`. A dotted name's first part is a variable holding a
    /// message, and each part before the last a message field of the one before.
    fn add(&mut self, name: &str, ty: &Type) {
        let Some((root, rest)) = name.split_once('.') else {
            let held = cel_type(ty, &mut self.messages);
            self.roots.insert(name.into(), held);
            return;
        };
        let message =
            (self.messages.names.get(root).cloned()).unwrap_or_else(|| format!("{MESSAGE}{root}"));
        self.roots.insert(root.into(), CelType::message(&message));
        let leaf = cel_type(ty, &mut self.messages);
        self.messages.path(message, rest, leaf);
    }
}

/// Each message type's name starts so.
const MESSAGE: &str = "message.";

/// The message types a module's names make: each one's fields and their types.
#[derive(Debug, Default)]
struct Messages {
    fields: BTreeMap<String, BTreeMap<String, CelType>>,
    /// Each declared message's name, and its CEL name: equal messages are one type.
    names: BTreeMap<String, String>,
    /// The enum names an expression may write, as [`Decls::enums`] keys them.
    enums: BTreeSet<String>,
}

impl Messages {
    fn field(&mut self, message: String, field: &str, ty: CelType) {
        self.fields
            .entry(message)
            .or_default()
            .insert(field.into(), ty);
    }

    /// The dotted fields `path` of `message`, the last of type `leaf`, each other a message.
    fn path(&mut self, mut message: String, path: &str, leaf: CelType) {
        let mut segments = path.split('.').peekable();
        while let Some(segment) = segments.next() {
            if segments.peek().is_none() {
                self.field(message.clone(), segment, leaf.clone());
                continue;
            }
            // A declared message's field is a message already.
            let inner = match self.fields.get(&message).and_then(|f| f.get(segment)) {
                Some(CelType::Message(held)) => held.to_string(),
                _ => format!("{message}.{segment}"),
            };
            self.field(message, segment, CelType::message(&inner));
            message = inner;
        }
    }

    /// A message's fields under the CEL name `name`.
    fn declare(&mut self, name: &str, ty: &Type) {
        self.fields.entry(name.into()).or_default();
        for f in ty.fields() {
            // CEL reads an optional field as its value; `has` and `.?` ask its presence.
            let held = match &f.ty {
                Type::Optional(t) => t,
                t => t,
            };
            let held = cel_type(held, self);
            self.field(name.into(), &f.name, held);
        }
    }
}

impl cel_core::ProtoTypeResolver for Messages {
    fn get_field_type(&self, message: &str, field: &str) -> Option<CelType> {
        self.fields.get(message)?.get(field).cloned()
    }
    fn has_message(&self, message: &str) -> bool {
        self.fields.contains_key(message)
    }
    fn is_extension(&self, _: &str, _: &str) -> bool {
        false
    }
    fn get_enum_value(&self, _: &str, _: &str) -> Option<i32> {
        None
    }
    fn resolve_qualified(&self, parts: &[&str], _: &str) -> Option<cel_core::ResolvedProtoType> {
        let name = parts.join(".");
        self.enums
            .contains(&name)
            .then(|| cel_core::ResolvedProtoType::Enum {
                cel_type: CelType::enum_type(&name),
                name,
            })
    }
    fn resolve_message_name(&self, name: &str, _: &str) -> Option<String> {
        self.names.get(name).cloned()
    }
    fn message_field_names(&self, message: &str) -> Option<Vec<String>> {
        Some(self.fields.get(message)?.keys().cloned().collect())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Only the checker reads it: no expression runs over a message here.
impl cel_core::ProtoRegistry for Messages {
    fn construct_message(&self, name: &str, _: &[cel_core::StructFieldValue], _: bool) -> Value {
        Value::error(format!("{name}: no message values"))
    }
    fn message_field_access(
        &self,
        _: &dyn cel_core::MessageValue,
        field: &str,
        _: bool,
        _: bool,
    ) -> Value {
        Value::error(format!("{field}: no message values"))
    }
    fn message_has_field(&self, _: &dyn cel_core::MessageValue, field: &str) -> Value {
        Value::error(format!("{field}: no message values"))
    }
    fn get_extension_value(
        &self,
        _: &dyn cel_core::MessageValue,
        _: &str,
        _: bool,
        _: bool,
    ) -> Option<Value> {
        None
    }
    fn has_extension(&self, _: &dyn cel_core::MessageValue, _: &str) -> Option<bool> {
        None
    }
}

/// `decl` in CEL, each message it names declared in `messages`.
fn declared_as(decl: &Decl, messages: &mut Messages) -> FunctionDecl {
    let mut made = FunctionDecl::new(decl.name.clone());
    for o in &decl.overloads {
        let mut params: Vec<CelType> = o.params.iter().map(|p| cel_type(p, messages)).collect();
        if let Some(message) = &o.names {
            params.push(CelType::type_of(cel_type(message, messages)));
        }
        let result = cel_type(&o.result, messages);
        made = made.with_overload(OverloadDecl::function(o.id.clone(), params, result));
    }
    made
}

/// `ty` in CEL: a message named by its fields, declared in `messages`.
fn cel_type(ty: &Type, messages: &mut Messages) -> CelType {
    match ty {
        Type::Int => CelType::Int,
        Type::Uint => CelType::UInt,
        Type::Double => CelType::Double,
        Type::Bool => CelType::Bool,
        Type::String => CelType::String,
        Type::Bytes => CelType::Bytes,
        Type::Null => CelType::Null,
        Type::Timestamp => CelType::Timestamp,
        Type::Duration => CelType::Duration,
        Type::Dyn => CelType::Dyn,
        // A declared name is never a mixed number: a dyn that lowering refuses.
        Type::Number => CelType::Dyn,
        Type::Narrow(w) => cel_type(&w.cel(), messages),
        Type::Optional(t) => CelType::optional(cel_type(t, messages)),
        Type::Nullable(t) => CelType::wrapper(cel_type(t, messages)),
        Type::List(t) => CelType::list(cel_type(t, messages)),
        Type::Map(k, v) => CelType::map(cel_type(k, messages), cel_type(v, messages)),
        Type::Message(_) => {
            let name = message_name(ty);
            messages.declare(&name, ty);
            CelType::message(&name)
        }
    }
}

/// A message type's CEL name: its name and its fields with their types, so equal messages share
/// one.
fn message_name(ty: &Type) -> String {
    let fields: Vec<String> = ty
        .fields()
        .iter()
        .map(|f| match &f.ty {
            Type::Message(_) => format!("{}:{{{}}}", f.name, message_name(&f.ty)),
            t => format!("{}:{}", f.name, t.display()),
        })
        .collect();
    let name = match ty {
        Type::Message(m) => m
            .name
            .as_deref()
            .map_or(String::new(), |n| format!("[{n}]")),
        _ => String::new(),
    };
    format!("{MESSAGE}message{name}<{}>", fields.join(","))
}

/// Each message type in `ty`, by its CEL name, into `messages`.
pub(crate) fn register(ty: &Type, messages: &mut BTreeMap<String, Type>) {
    match ty {
        Type::Message(m) => {
            for f in &m.fields {
                register(&f.ty, messages);
            }
            messages.insert(message_name(ty), ty.clone());
        }
        Type::List(t) | Type::Map(_, t) | Type::Optional(t) | Type::Nullable(t) => {
            register(t, messages)
        }
        _ => {}
    }
}
