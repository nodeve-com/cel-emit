//! CEL's types as the typed tree carries them, and each one's Rust in what is emitted.

use cel_core::CelType;

/// A value's type. A message is a [`Type::Message`]: its fields in order.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Type {
    Int,
    Uint,
    Double,
    Bool,
    String,
    Bytes,
    Null,
    Timestamp,
    Duration,
    /// A value of no one kind until it is read: what `json.decode` gives, and a list or map
    /// literal mixing kinds holds. It fills a typed place as [`crate::tree::Node::FromDyn`], and
    /// a typed value enters it as [`crate::tree::Node::ToDyn`].
    Dyn,
    /// An int, uint, or double as a mixed `math.greatest` or `math.least` gives it: only a cast
    /// or comparison takes it ([`crate::lower`](mod@crate::lower)), so no declaration or message holds one.
    Number,
    List(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Message(Vec<Field>),
    /// Absent, or a value of the type: an `Option` in the emitted Rust, never a message's field
    /// ([`crate::emit`]).
    Optional(Box<Type>),
    /// Null, or a value of the type: a message's `Option` field, apart from its presence. A
    /// non-null read yields the type itself.
    Nullable(Box<Type>),
    /// A number stored at a width CEL does not compute in, wherever it sits: a field, a list's
    /// element, a map's key or value. CEL sees [`Width::cel`] ([`Type::cel`]); only a stored
    /// type, a message's field or what a read of one widens ([`crate::tree::Node::Widen`]), holds it.
    Narrow(Width),
}

/// A number's width where it is not CEL's own (`int64`, `uint64`, `float64`): it is stored as
/// that Rust type, a write outside it is an error, and a read widens to [`Width::cel`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Width {
    I8,
    I16,
    I32,
    I128,
    U8,
    U16,
    U32,
    U128,
    F32,
}

impl Width {
    /// The Rust type a message stores.
    pub fn rust(self) -> &'static str {
        match self {
            Width::I8 => "i8",
            Width::I16 => "i16",
            Width::I32 => "i32",
            Width::I128 => "i128",
            Width::U8 => "u8",
            Width::U16 => "u16",
            Width::U32 => "u32",
            Width::U128 => "u128",
            Width::F32 => "f32",
        }
    }

    /// The CEL type a field of this width computes in.
    pub fn cel(self) -> Type {
        match self {
            Width::I8 | Width::I16 | Width::I32 | Width::I128 => Type::Int,
            Width::U8 | Width::U16 | Width::U32 | Width::U128 => Type::Uint,
            Width::F32 => Type::Double,
        }
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
    pub fn list(of: Type) -> Type {
        Type::List(Box::new(of))
    }

    /// A map with string keys.
    pub fn map(of: Type) -> Type {
        Type::Map(Box::new(Type::String), Box::new(of))
    }

    /// Its fields: none but a message's.
    pub fn fields(&self) -> &[Field] {
        match self {
            Type::Message(fields) => fields,
            _ => &[],
        }
    }

    /// This type as CEL computes it: each stored width its [`Width::cel`]. A message's fields
    /// stay as stored.
    pub fn cel(&self) -> Type {
        match self {
            Type::Narrow(w) => w.cel(),
            Type::List(t) => Type::list(t.cel()),
            Type::Map(k, v) => Type::Map(Box::new(k.cel()), Box::new(v.cel())),
            Type::Optional(t) => Type::Optional(Box::new(t.cel())),
            Type::Nullable(t) => Type::Nullable(Box::new(t.cel())),
            other => other.clone(),
        }
    }

    /// Whether a width sits in this type, outside a message.
    pub fn narrow(&self) -> bool {
        match self {
            Type::Narrow(_) => true,
            Type::List(t) | Type::Optional(t) | Type::Nullable(t) => t.narrow(),
            Type::Map(k, v) => k.narrow() || v.narrow(),
            _ => false,
        }
    }

    /// Whether dyn sits anywhere in this type.
    pub fn has_dyn(&self) -> bool {
        match self {
            Type::Dyn => true,
            Type::List(t) | Type::Optional(t) | Type::Nullable(t) => t.has_dyn(),
            Type::Map(k, v) => k.has_dyn() || v.has_dyn(),
            _ => false,
        }
    }

    /// Whether a value of this type becomes `to`, a dyn or a list or map of one: its elements
    /// scalars, a null, or dyn themselves.
    pub fn dyn_into(&self, to: &Type) -> bool {
        let element = |t: &Type| match t {
            Type::Int
            | Type::Uint
            | Type::Double
            | Type::Bool
            | Type::String
            | Type::Timestamp
            | Type::Duration
            | Type::Null => true,
            Type::Dyn => true,
            Type::List(t) => **t == Type::Dyn,
            Type::Map(k, v) => **k == Type::String && **v == Type::Dyn,
            _ => false,
        };
        match (self, to) {
            (t, Type::Dyn) if element(t) => true,
            (Type::List(t), Type::Dyn) => element(t),
            (Type::Map(k, v), Type::Dyn) => match **k {
                Type::String => element(v),
                Type::Dyn => **v == Type::Dyn,
                _ => false,
            },
            (Type::List(t), Type::List(of)) => **of == Type::Dyn && element(t),
            (Type::Map(_, v), Type::Map(_, of)) => **of == Type::Dyn && element(v),
            _ => false,
        }
    }

    /// Whether JSON can hold a value of this type ([`crate::tree::Node::Written`]): a time as
    /// RFC 3339 or seconds, bytes as base64; not an absent value or a map keyed by other than a
    /// string.
    pub fn writable(&self) -> bool {
        match self {
            Type::Int
            | Type::Uint
            | Type::Double
            | Type::Bool
            | Type::String
            | Type::Null
            | Type::Dyn
            | Type::Timestamp
            | Type::Duration
            | Type::Bytes
            | Type::Narrow(_) => true,
            Type::List(t) | Type::Nullable(t) => t.writable(),
            Type::Map(k, v) => **k == Type::String && v.writable(),
            Type::Message(fields) => fields.iter().all(|f| match &f.ty {
                Type::Optional(t) => t.writable(),
                t => t.writable(),
            }),
            _ => false,
        }
    }

    /// Whether a message of this type has a layout: no field holds an absent or null value, bar
    /// a nullable field of one that has none.
    pub fn laid_out(&self) -> bool {
        match self {
            Type::Optional(_) | Type::Nullable(_) | Type::Null => false,
            Type::List(t) => t.laid_out(),
            Type::Map(k, v) => k.laid_out() && v.laid_out(),
            Type::Message(fields) => fields.iter().all(|f| match &f.ty {
                Type::Optional(t) | Type::Nullable(t) => t.laid_out(),
                t => t.laid_out(),
            }),
            _ => true,
        }
    }

    /// Whether its Rust is `Copy`.
    pub fn copied(&self) -> bool {
        matches!(
            self,
            Type::Int
                | Type::Uint
                | Type::Double
                | Type::Bool
                | Type::Null
                | Type::Timestamp
                | Type::Duration
                | Type::Number
                | Type::Narrow(_)
        )
    }

    /// Neither a list, a map, nor a message.
    pub fn scalar(&self) -> bool {
        !matches!(self, Type::List(_) | Type::Map(..) | Type::Message(_))
    }

    /// Its name in CEL, for a message.
    pub fn display(&self) -> String {
        match self {
            Type::Int => "int".into(),
            Type::Uint => "uint".into(),
            Type::Double => "double".into(),
            Type::Bool => "bool".into(),
            Type::String => "string".into(),
            Type::Bytes => "bytes".into(),
            Type::Null => "null_type".into(),
            Type::Timestamp => "google.protobuf.Timestamp".into(),
            Type::Duration => "google.protobuf.Duration".into(),
            Type::Dyn => "dyn".into(),
            Type::Number => "number".into(),
            Type::List(t) => format!("list({})", t.display()),
            Type::Map(k, v) => format!("map({}, {})", k.display(), v.display()),
            Type::Optional(t) => format!("optional({})", t.display()),
            Type::Nullable(t) => format!("nullable({})", t.display()),
            Type::Narrow(w) => w.rust().into(),
            Type::Message(fields) => {
                let fields: Vec<String> = fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name, f.ty.display()))
                    .collect();
                format!("message{{{}}}", fields.join(", "))
            }
        }
    }
}

/// `cel` as a [`Type`], where lowering takes it: a message only where `messages` declares it.
#[doc(hidden)]
pub fn of_cel(cel: &CelType, messages: &std::collections::BTreeMap<String, Type>) -> Option<Type> {
    Some(match cel {
        CelType::Int => Type::Int,
        CelType::UInt => Type::Uint,
        CelType::Double => Type::Double,
        CelType::Bool => Type::Bool,
        CelType::String => Type::String,
        CelType::Bytes => Type::Bytes,
        CelType::Timestamp => Type::Timestamp,
        CelType::Duration => Type::Duration,
        CelType::Null => Type::Null,
        CelType::Dyn => Type::Dyn,
        CelType::Optional(t) => Type::Optional(Box::new(of_cel(t, messages)?)),
        CelType::Wrapper(t) => Type::Nullable(Box::new(of_cel(t, messages)?)),
        CelType::List(t) => Type::list(of_cel(t, messages)?),
        CelType::Map(k, v)
            if matches!(
                **k,
                CelType::Int | CelType::UInt | CelType::Bool | CelType::String | CelType::Dyn
            ) =>
        {
            Type::Map(
                Box::new(of_cel(k, messages)?),
                Box::new(of_cel(v, messages)?),
            )
        }
        CelType::Message(name) => messages.get(name.as_ref())?.clone(),
        _ => return None,
    })
}
