//! The lists extension: cel-emit-runtime's `lists`. A list of dyn flattens by the kind of each
//! element; any other list of lists spreads its inner lists.

use crate::functions::Std;
use crate::types::Type;

use super::cmp::{equal, ordered};

/// The element type of `args`' first argument, a list.
fn element(args: &[Type]) -> Option<&Type> {
    match args.first() {
        Some(Type::List(t)) => Some(t),
        _ => None,
    }
}

pub(super) const FUNCTIONS: &[Std] = &[
    Std {
        id: "list_flatten",
        path: "lists::flatten_dyn",
        takes: |args| element(args) == Some(&Type::Dyn),
        borrows: false,
    },
    Std {
        id: "list_flatten",
        path: "lists::flatten",
        takes: |args| matches!(element(args), Some(Type::List(_))),
        borrows: false,
    },
    Std {
        id: "list_flatten_depth",
        path: "lists::flatten_depth",
        takes: |args| element(args) == Some(&Type::Dyn),
        borrows: false,
    },
    Std::reads("list_slice", "lists::slice"),
    Std::any("list_reverse", "lists::reverse"),
    Std {
        id: "list_distinct",
        path: "lists::distinct",
        takes: |args| element(args).is_some_and(equal),
        borrows: false,
    },
    Std {
        id: "list_sort",
        path: "lists::sort",
        takes: |args| element(args).is_some_and(ordered),
        borrows: false,
    },
    Std {
        id: "list_sort_by_keys",
        path: "lists::sort_by_keys",
        takes: |args| matches!(args.get(1), Some(Type::List(k)) if ordered(k)),
        borrows: false,
    },
    Std::any("lists_range", "lists::range"),
];
