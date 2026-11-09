//! A message type and its fields.

use super::Type;

/// A message type: its fields in order, and an optional name the caller chooses. The name is part
/// of its identity: messages with equal fields and different names are different types, and an
/// unnamed message differs from a named one.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Message {
    pub name: Option<String>,
    pub fields: Vec<Field>,
}

impl std::ops::Deref for Message {
    type Target = [Field];

    fn deref(&self) -> &[Field] {
        &self.fields
    }
}

/// A message's field, and whether building the message must give it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Field {
    pub name: String,
    pub ty: Type,
    pub required: bool,
}

impl Type {
    /// An unnamed message of `fields`.
    pub fn message(fields: Vec<Field>) -> Type {
        Type::Message(Message { name: None, fields })
    }

    /// A message of `fields` named `name`.
    pub fn named(name: impl Into<String>, fields: Vec<Field>) -> Type {
        Type::Message(Message {
            name: Some(name.into()),
            fields,
        })
    }
}
