//! Strings: cel-emit-runtime's `strings`.

use crate::functions::Std;
use crate::types::{Type, Width};

pub(super) const FUNCTIONS: &[Std] = &[
    Std::reads("string_contains_string", "strings::contains"),
    Std::reads("string_starts_with_string", "strings::starts_with"),
    Std::reads("string_ends_with_string", "strings::ends_with"),
    Std::reads("string_matches_re", "strings::matches"),
    Std::reads("matches_string_re", "strings::matches"),
    Std::reads("size_string", "strings::size"),
    Std::reads("string_size", "strings::size"),
    Std::reads("add_string_string", "strings::concat"),
    Std::reads("string_trim", "strings::trim"),
    Std::reads("string_index_of_string", "strings::index_of"),
    Std::reads("string_index_of_string_int", "strings::index_of_from"),
    Std::reads("string_last_index_of_string", "strings::last_index_of"),
    Std::reads(
        "string_last_index_of_string_int",
        "strings::last_index_of_from",
    ),
    Std::reads("string_substring_int", "strings::substring"),
    Std::reads("string_substring_int_int", "strings::substring_between"),
    Std::reads("string_char_at_int", "strings::char_at"),
    Std::reads("string_lower_ascii", "strings::lower_ascii"),
    Std::reads("string_upper_ascii", "strings::upper_ascii"),
    Std::reads("string_reverse", "strings::reverse"),
    Std::reads("string_replace_string_string", "strings::replace"),
    Std::reads("string_replace_string_string_int", "strings::replace_limit"),
    Std::reads("string_split_string", "strings::split"),
    Std::reads("string_split_string_int", "strings::split_limit"),
    Std::reads("list_string_join", "strings::join"),
    Std::reads("list_string_join_string", "strings::join_with"),
    Std {
        id: "string_format",
        path: "strings::format",
        takes: |args| matches!(args.get(1), Some(Type::List(t)) if formattable(t)),
        borrows: true,
    },
    Std::reads("strings_quote_string", "strings::quote"),
];

/// Whether `string.format` takes a list of `ty`: a value cel-emit-runtime's `format` writes.
fn formattable(ty: &Type) -> bool {
    match ty {
        Type::Int
        | Type::Uint
        | Type::Double
        | Type::Bool
        | Type::String
        | Type::Bytes
        | Type::Timestamp
        | Type::Duration
        | Type::Narrow(Width::F32) => true,
        Type::List(t) => formattable(t),
        _ => false,
    }
}
