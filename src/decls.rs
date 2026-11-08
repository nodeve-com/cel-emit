//! What an expression may read and call: its declarations, made by whoever embeds cel-emit.

use std::collections::BTreeMap;

use crate::functions::Function;
use crate::tree::Typed;
use crate::types::Type;

/// A name an expression may read: its type, and what it is.
#[derive(Clone, Debug)]
pub struct Name {
    pub ty: Type,
    pub at: At,
}

#[derive(Clone, Debug)]
pub enum At {
    /// A variable, numbered by the embedder, read as [`crate::emit::Vars`] says.
    Var(usize),
    /// A literal, inlined where it is read.
    Constant(Typed),
}

impl Name {
    pub fn var(ty: Type, id: usize) -> Name {
        Name {
            ty,
            at: At::Var(id),
        }
    }
}

/// A call's argument as an [`Expansion`] reads it.
pub struct Arg {
    /// Its dotted name, where it is one.
    pub path: Option<String>,
    /// Its text, where it is a string literal.
    pub text: Option<String>,
    /// It, lowered.
    pub value: Typed,
}

/// A function as CEL declares it: its name and each overload.
#[derive(Clone, Debug)]
pub struct Decl {
    pub name: String,
    pub overloads: Vec<Overload>,
}

/// One way to call a [`Decl`]: its id, what it takes, and what it returns.
#[derive(Clone, Debug)]
pub struct Overload {
    pub id: String,
    pub params: Vec<Type>,
    /// The message type a call names as its last argument, as in `f(x, Message)`. The call picks
    /// this overload for that message, and passes nothing at run time for the argument.
    pub names: Option<Type>,
    pub result: Type,
}

impl Decl {
    pub fn new(name: impl Into<String>) -> Decl {
        Decl {
            name: name.into(),
            overloads: Vec::new(),
        }
    }

    pub fn with_overload(mut self, id: impl Into<String>, params: Vec<Type>, result: Type) -> Decl {
        self.overloads.push(Overload {
            id: id.into(),
            params,
            names: None,
            result,
        });
        self
    }

    /// The last overload added also takes the message type `message` as its last argument, as in
    /// `f(x, Message)`: `params` stay the arguments before it. Declare one overload per message,
    /// each with its own id.
    pub fn naming(mut self, message: Type) -> Decl {
        if let Some(o) = self.overloads.last_mut() {
            o.names = Some(message);
        }
        self
    }
}

/// What an [`Expansion`]'s call becomes, given its arguments and [`Decls::tags`].
pub type Expand = fn(&[Arg], &BTreeMap<String, String>) -> Result<Typed, crate::Error>;

/// Why message field `field`, tagged `declared`, can't take a value tagged `given`.
pub type Mismatch = fn(field: &str, declared: &str, given: &str) -> String;

/// A call rewritten as it lowers, as a CEL macro is: for what runs on an argument's name or
/// literal text, which no [`Function`] sees. A string literal last argument tags its value, which
/// a message field with a [`Decls::tags`] entry of its own must match.
#[derive(Clone)]
pub struct Expansion {
    pub decl: Decl,
    pub expand: Expand,
    pub mismatch: Mismatch,
}

/// A function written in CEL: its name, its parameters in order, each a name and a type, and a
/// body that reads only them. Its result is its body's type. One Rust function emits for it,
/// taking each parameter not `Copy` by reference, and returning `cel_emit_runtime::Result` of its
/// result.
#[derive(Clone, Debug)]
pub struct Written {
    /// The name an expression calls it by.
    pub name: String,
    pub params: Vec<(String, Type)>,
    /// The body, in CEL.
    pub cel: String,
    /// The Rust path of the function: a call writes it, and the function's name is its last part.
    pub path: String,
}

/// An expression's declarations.
#[derive(Clone, Default)]
pub struct Decls {
    /// Each name, dotted or not: a dotted name's prefixes are messages, so `a.?b.c` selects
    /// through them.
    pub names: BTreeMap<String, Name>,
    /// A note on each name, for an [`Expansion`] to read.
    pub tags: BTreeMap<String, String>,
    /// Each message an expression may build (`Comfort{…}`) or read whole, by name. A field a
    /// message leaves unset reads its zero value, as a proto3 field does.
    pub messages: BTreeMap<String, Type>,
    /// Each enum by the name an expression writes it, its values by name. `Enum(n)` and
    /// `Enum('NAME')` construct one; an enum's values are names of their own, in `names`.
    pub enums: BTreeMap<String, BTreeMap<String, i32>>,
    /// Functions beyond CEL's and cel-emit's own.
    pub functions: Vec<Function>,
    /// Functions written in CEL.
    pub written: Vec<Written>,
    /// Calls rewritten as they lower.
    pub expansions: Vec<Expansion>,
    /// `json.encode` of a message, struct literal, or map literal leaves out a field whose value
    /// errors or holds a NaN or an infinity, as it does an absent one, where CEL fails the whole expression. A value written
    /// whole still fails.
    pub skip_failed: bool,
}

impl Decls {
    pub fn new(names: BTreeMap<String, Name>) -> Decls {
        Decls {
            names,
            ..Decls::default()
        }
    }
}
