//! CEL semantics where cel-emit follows cel-go over cel-core, or the cel-spec suite (the conformance
//! test) lacks the case: each test's cases built together as one native program and compared
//! (cases.rs), plus what lowering refuses. One binary, one link.

// Helpers outside a `#[test]` fn panic as the tests do (clippy.toml).
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

mod calls;
mod cases;
#[path = "../common/mod.rs"]
mod common;
mod lists;
mod lists_ext;
mod math;
mod optional;
mod optional_message;
mod same_name;
mod strings;
mod time;
mod widths;
