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
            result,
        });
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

/// How a dyn value fills a list or message place, which CEL's own rule cannot read it into: `x`, a dyn, as a
/// value of `ty`, given how many comprehensions enclose the place. An embedder with a contract
/// of its own supplies it; without one, only a string or bool fills a field from a dyn.
pub type Fill = fn(x: Typed, ty: &Type, depth: usize) -> Result<Typed, crate::Error>;

/// An expression's declarations.
#[derive(Clone, Default)]
pub struct Decls {
    /// Each name, dotted or not: a dotted name's prefixes are messages, so `a.?b.c` selects
    /// through them.
    pub names: BTreeMap<String, Name>,
    /// Each name standing for a map of messages, key to message, given by column: each of its
    /// fields a name of its own, a map by the same keys; and the field whose keys it loops over.
    pub columns: BTreeMap<String, String>,
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
    /// Calls rewritten as they lower.
    pub expansions: Vec<Expansion>,
    /// How a message's field takes a dyn value.
    pub fill: Option<Fill>,
    /// The name of the string field that `json.decode(text, Message)` fills, where a message has it,
    /// with a JSON object text of the keys the message does not declare (none, when there are
    /// none). The decode never reads that name as a key, and no expression writes it.
    /// `json.encode` writes the field back: its text's entries join the object holding it, each
    /// as the text has it, and never replace a key the object has.
    pub rest: Option<String>,
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
