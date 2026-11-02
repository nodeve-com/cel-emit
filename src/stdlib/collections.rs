//! Lists and maps: cel-emit-runtime's `collections`. A comprehension is a node of its own
//! ([`crate::tree::Node::Map`] and the rest), emitted over the functions there.

use crate::functions::Std;
use crate::types::Type;

use super::cmp::equal;

pub(super) const FUNCTIONS: &[Std] = &[
    Std::reads("size_list", "collections::list_size"),
    Std::reads("list_size", "collections::list_size"),
    Std::reads("size_map", "collections::map_size"),
    Std::reads("map_size", "collections::map_size"),
    Std::reads("index_list", "collections::index_list"),
    Std::reads("add_list_list", "collections::concat"),
    Std::reads("size_bytes", "collections::bytes_size"),
    Std::reads("bytes_size", "collections::bytes_size"),
    Std::reads("add_bytes_bytes", "collections::bytes_concat"),
    Std {
        id: "index_map",
        path: "collections::index_map",
        takes: |args| args.get(1).is_some_and(equal),
        borrows: true,
    },
    Std {
        id: "in_list",
        path: "collections::in_list",
        takes: |args| args.first().is_some_and(equal),
        borrows: true,
    },
    Std {
        id: "in_map",
        path: "collections::in_map",
        takes: |args| args.first().is_some_and(equal),
        borrows: true,
    },
];

/// Whether a list or map literal of type `ty` emits: a list of other than messages, a map whose
/// keys `==` compares.
pub(crate) fn literal(ty: &Type) -> bool {
    match ty {
        Type::List(t) => !matches!(**t, Type::Message(_)),
        Type::Map(k, _) => equal(k),
        _ => false,
    }
}
