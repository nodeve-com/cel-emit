#![doc = include_str!("../README.md")]

pub mod decls;
pub mod emit;
#[doc(hidden)]
pub mod env;
pub mod functions;
pub mod lower;
#[doc(hidden)]
pub mod macros;
#[doc = include_str!("../MISSING.md")]
pub mod missing {}
pub mod stdlib;
pub mod tree;
pub mod types;
pub mod walk;
pub mod zone;

pub use decls::{Arg, At, Decl, Decls, Expand, Expansion, Fill, Mismatch, Name, Overload};
pub use lower::{lower, lowered, Lowered};
pub use tree::{split, unbound, Node, Part, Split, Typed};
pub use types::{Field, Type, Width};

/// Why an expression doesn't lower or emit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// A mistake in it.
    Invalid(String),
    /// Something cel-emit doesn't emit yet.
    Unsupported(String),
}

impl Error {
    /// The same error, saying where.
    pub fn at(self, place: &str) -> Error {
        match self {
            Error::Invalid(m) => Error::Invalid(format!("{place}: {m}")),
            Error::Unsupported(m) => Error::Unsupported(format!("{place}: {m}")),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Invalid(m) => write!(f, "invalid: {m}"),
            Error::Unsupported(m) => write!(f, "unsupported: {m}"),
        }
    }
}
