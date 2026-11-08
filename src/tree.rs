//! The typed tree: what [`crate::emit`] reads, lowered from cel-core's checked tree, so a front end
//! swaps by changing only [`mod@crate::lower`]. Every node carries its type, and every call is
//! resolved to the overload the checker picked.

pub use cel_emit_runtime::Error as Fail;

use crate::types::Type;

mod split;

pub use split::{split, unbound, Split};

#[derive(Clone, Debug, PartialEq)]
pub struct Typed {
    pub ty: Type,
    pub node: Node,
}

impl AsRef<Typed> for Typed {
    fn as_ref(&self) -> &Typed {
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Int(i64),
    Uint(u64),
    Double(f64),
    Bool(bool),
    Str(String),
    Bytes(Vec<u8>),
    Null,
    /// Seconds and nanoseconds past the epoch.
    Timestamp(i64, u32),
    /// Seconds and nanoseconds, of one sign.
    Duration(i64, i32),
    /// The variable the embedder numbers so ([`crate::decls::At::Var`]).
    Var(usize),
    /// Whether that variable holds a value: an expression testing one computes without it.
    Has(usize),
    /// The function an overload id names, on its arguments, a method's receiver first.
    Call(String, Vec<Typed>),
    /// Whether the string matches anywhere, by a pattern table ([`pattern_emit::build`]): a
    /// `matches` whose pattern is constant. The table, then the pattern it came from.
    Matches(Box<Typed>, Vec<u8>, String),
    /// `&&`, `||`: a side that decides absorbs the other's error.
    And(Box<Typed>, Box<Typed>),
    Or(Box<Typed>, Box<Typed>),
    Cond(Box<Typed>, Box<Typed>, Box<Typed>),
    /// Field `i` of a message.
    Field(Box<Typed>, usize),
    /// The value, or an error where it is outside the stored type's widths, element by element;
    /// a float rounds to its width. The node's type is CEL's own ([`Type::cel`]).
    Within(Box<Typed>, Type),
    /// A CEL value as the stored type of the node itself ([`Type::cel`] of it is the value's
    /// type): a float rounds to its width, so past it is infinity, and an integer past its width
    /// is an error, element by element.
    Store(Box<Typed>),
    /// A dyn value filling a place of this type (a field, a call's argument, an operand): the
    /// value if it holds that kind, else an error.
    FromDyn(Box<Typed>, Type),
    /// A typed value entering a dyn place: the node's type is that place's, `dyn` or a list or
    /// map of it.
    ToDyn(Box<Typed>),
    /// A typed value as the dyn JSON writes it, whatever its type holds ([`Type::writable`]): a
    /// message a map of its fields, a float32 in its own digits.
    Written(Box<Typed>),
    /// A stored value, whose type holds widths, as CEL computes it: each number widened, or an
    /// error where it doesn't fit.
    Widen(Box<Typed>),
    /// An optional holding this value.
    Some(Box<Typed>),
    /// An optional holding nothing: its type says of what.
    None,
    /// A list of these items: each unflagged item is one element; a flagged (`?`) item is an
    /// optional, whose value is one element where it holds one.
    ListOpt(Vec<(Typed, bool)>),
    /// A map of these entries, as `ListOpt` gives its items: a flagged entry's value is an
    /// optional, and the entry is kept where it holds one.
    MapOpt(Vec<(Typed, Typed, bool)>),
    /// The element of comprehension `n`, counted from the outermost: a list's, or a map's key or
    /// value.
    Item(usize, Part),
    /// The index of comprehension `n`.
    Position(usize),
    /// `body` for each element of a list or map (`map`, `transformList`): laid out as a list,
    /// or under [`Node::Extreme`], [`Node::Sum`], [`Node::Mean`], or [`Node::Count`], folded as it goes.
    Map(Box<Typed>, Box<Typed>),
    /// `body` for each element of a list or map where `pred` holds (`filter`, `map` with a
    /// predicate), as [`Node::Map`] gives it.
    Keep(Box<Typed>, Box<Typed>, Box<Typed>),
    /// The first element of a list where `pred` holds, read no further, as `filter(…)[0]` gives
    /// it: with none, the error of an index outside the list. `pred` reads no index.
    First(Box<Typed>, Box<Typed>),
    /// Whether `body` holds for every element (`true`: `all`) or some (`exists`), an element's
    /// error absorbed as `&&` and `||` absorb one.
    All(bool, Box<Typed>, Box<Typed>),
    /// `math.greatest` (`true`) or `math.least` of a [`Node::Map`] or [`Node::Keep`]: over none,
    /// absent.
    Extreme(bool, Box<Typed>),
    /// `math.sum` of a [`Node::Map`] or [`Node::Keep`]: over none, zero.
    Sum(Box<Typed>),
    /// `math.mean` of a [`Node::Map`] or [`Node::Keep`]: over none, an error.
    Mean(Box<Typed>),
    /// How many items a [`Node::Map`] or [`Node::Keep`] gives.
    Count(Box<Typed>),
    /// A struct literal: the message's fields it sets, each by its place in the message's type, and
    /// whether it is a flagged (`?`) entry, whose value is an optional that sets the field where
    /// it holds one. A field no entry sets reads its zero value, as proto3 gives it.
    Message(Vec<(usize, Typed, bool)>),
    /// A list of these values.
    List(Vec<Typed>),
    /// A map of these keys and values: a key twice is an error.
    MapOf(Vec<(Typed, Typed)>),
    /// JSON object text, each member written as it computes: what `json.encode` of a message's
    /// fields is.
    Encode(Object),
    /// JSON array text: for each element of a list (`list`), where `pred` holds, the
    /// [`Node::Encode`] `body` written as one item, as [`Node::Keep`] lays out its items.
    EncodeEach(Box<Typed>, Option<Box<Typed>>, Box<Typed>),
    /// `value` bound as local `n` in `body`.
    Let(usize, Box<Typed>, Box<Typed>),
    Local(usize),
    /// No value: this error.
    Fail(Fail),
}

/// An object [`Node::Encode`] writes: its members in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    /// Values its members read as locals, each computed once, first to last: one that fails
    /// fails only the members reading it.
    pub lets: Vec<(usize, Typed)>,
    pub members: Vec<Member>,
}

/// A member of an [`Object`]: written only where `present`, a `bool`, holds.
#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub key: String,
    pub present: Option<Typed>,
    pub out: Out,
}

/// What a [`Member`] writes.
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    /// A dyn value. Where flagged, the member is left out if the value errors or holds a NaN or
    /// an infinity, which JSON can't hold.
    Value(Typed, bool),
    /// An optional: the member holds its value in `local`, and is left out where it holds none.
    /// `value` is a dyn reading `local`. Flagged as [`Out::Value`] is.
    Optional {
        optional: Typed,
        local: usize,
        value: Typed,
        kept: bool,
    },
    /// A nested object, left out where it writes no member.
    Object(Object),
}

/// What a comprehension's element gives: a list's as it is, or a map's key or value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Whole,
    Value,
    Key,
}

impl Typed {
    pub fn new(ty: Type, node: Node) -> Typed {
        Typed { ty, node }
    }

    /// A read of a value stored as `ty`, as CEL computes it: widened where `ty` holds a width.
    pub fn stored(ty: Type, node: Node) -> Typed {
        if ty.narrow() {
            Typed::new(ty.cel(), Node::Widen(Box::new(Typed::new(ty, node))))
        } else {
            Typed::new(ty, node)
        }
    }

    /// What this reads, where that is stored as `ty`, so a place of `ty` takes it as it is.
    pub fn read_as(&self, ty: &Type) -> Option<&Typed> {
        match &self.node {
            Node::Widen(stored) if stored.ty == *ty => Some(stored),
            _ => None,
        }
    }

    pub fn int(v: i64) -> Typed {
        Typed::new(Type::Int, Node::Int(v))
    }

    pub fn double(v: f64) -> Typed {
        Typed::new(Type::Double, Node::Double(v))
    }

    pub fn bool(v: bool) -> Typed {
        Typed::new(Type::Bool, Node::Bool(v))
    }

    pub fn string(v: &str) -> Typed {
        Typed::new(Type::String, Node::Str(v.into()))
    }

    /// The overload `id` on `args`, giving a `ty`.
    pub fn call(id: &str, args: Vec<Typed>, ty: Type) -> Typed {
        Typed::new(ty, Node::Call(id.into(), args))
    }

    pub fn and(a: Typed, b: Typed) -> Typed {
        Typed::new(Type::Bool, Node::And(Box::new(a), Box::new(b)))
    }

    pub fn or(a: Typed, b: Typed) -> Typed {
        Typed::new(Type::Bool, Node::Or(Box::new(a), Box::new(b)))
    }

    pub fn cond(c: Typed, a: Typed, b: Typed) -> Typed {
        Typed::new(
            a.ty.clone(),
            Node::Cond(Box::new(c), Box::new(a), Box::new(b)),
        )
    }

    /// `a op b`, `op` CEL's (`+`, `-`, `*`, `/`, `%`, `==`, `!=`, `<`, `<=`, `>`, `>=`), both sides
    /// of one type: the standard overload for it, if there is one.
    pub fn binary(op: &str, a: Typed, b: Typed) -> Option<Typed> {
        let name = match &a.ty {
            Type::Int => "int64",
            Type::Uint => "uint64",
            Type::Double => "double",
            Type::String => "string",
            Type::Bytes => "bytes",
            Type::Bool => "bool",
            Type::Timestamp => "timestamp",
            Type::Duration => "duration",
            _ => return None,
        };
        let (id, ty) = match op {
            "+" | "-" | "*" | "/" | "%" => {
                let verb = match op {
                    "+" => "add",
                    "-" => "subtract",
                    "*" => "multiply",
                    "/" => "divide",
                    _ => "modulo",
                };
                (format!("{verb}_{name}_{name}"), a.ty.clone())
            }
            "==" => ("equals".into(), Type::Bool),
            "!=" => ("not_equals".into(), Type::Bool),
            "<" => (format!("less_{name}"), Type::Bool),
            "<=" => (format!("less_equals_{name}"), Type::Bool),
            ">" => (format!("greater_{name}"), Type::Bool),
            ">=" => (format!("greater_equals_{name}"), Type::Bool),
            _ => return None,
        };
        (a.ty == b.ty).then(|| Typed::call(&id, vec![a, b], ty))
    }
}
