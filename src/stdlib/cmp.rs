//! Comparisons and logic: cel-emit-runtime's `cmp`. `&&`, `||`, and `?:` are nodes of their own.

use crate::functions::Std;
use crate::types::Type;

/// Whether `==` compares values of `ty`.
pub(crate) fn equal(ty: &Type) -> bool {
    match ty {
        Type::Int | Type::Uint | Type::Double | Type::Bool | Type::String | Type::Bytes => true,
        Type::Number | Type::Null | Type::Dyn => true,
        Type::Timestamp | Type::Duration => true,
        Type::List(t) | Type::Optional(t) | Type::Nullable(t) => equal(t),
        Type::Map(k, v) => equal(k) && equal(v),
        Type::Message(fields) => fields.iter().all(|f| equal(&f.ty)),
        _ => false,
    }
}

/// Whether `sort` orders values of `ty`: a scalar the orderings compare, or a dyn, whose values
/// order only among their own kind. `<` over a dyn stays refused.
pub(crate) fn ordered(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Int
            | Type::Uint
            | Type::Double
            | Type::Bool
            | Type::String
            | Type::Bytes
            | Type::Timestamp
            | Type::Duration
            | Type::Dyn
    )
}

pub(super) const FUNCTIONS: &[Std] = &[
    Std {
        id: "equals",
        path: "cmp::eq",
        takes: |args| args.iter().all(equal),
        borrows: true,
    },
    Std {
        id: "not_equals",
        path: "cmp::ne",
        takes: |args| args.iter().all(equal),
        borrows: true,
    },
    Std::any("logical_not", "cmp::not"),
    Std::reads("less_number", "cmp::lt"),
    Std::reads("less_int64", "cmp::lt"),
    Std::reads("less_uint64", "cmp::lt"),
    Std::reads("less_double", "cmp::lt"),
    Std::reads("less_string", "cmp::lt"),
    Std::reads("less_bytes", "cmp::lt"),
    Std::reads("less_bool", "cmp::lt"),
    Std::reads("less_timestamp", "cmp::lt"),
    Std::reads("less_duration", "cmp::lt"),
    Std::reads("less_equals_number", "cmp::le"),
    Std::reads("less_equals_int64", "cmp::le"),
    Std::reads("less_equals_uint64", "cmp::le"),
    Std::reads("less_equals_double", "cmp::le"),
    Std::reads("less_equals_string", "cmp::le"),
    Std::reads("less_equals_bytes", "cmp::le"),
    Std::reads("less_equals_bool", "cmp::le"),
    Std::reads("less_equals_timestamp", "cmp::le"),
    Std::reads("less_equals_duration", "cmp::le"),
    Std::reads("greater_number", "cmp::gt"),
    Std::reads("greater_int64", "cmp::gt"),
    Std::reads("greater_uint64", "cmp::gt"),
    Std::reads("greater_double", "cmp::gt"),
    Std::reads("greater_string", "cmp::gt"),
    Std::reads("greater_bytes", "cmp::gt"),
    Std::reads("greater_bool", "cmp::gt"),
    Std::reads("greater_timestamp", "cmp::gt"),
    Std::reads("greater_duration", "cmp::gt"),
    Std::reads("greater_equals_number", "cmp::ge"),
    Std::reads("greater_equals_int64", "cmp::ge"),
    Std::reads("greater_equals_uint64", "cmp::ge"),
    Std::reads("greater_equals_double", "cmp::ge"),
    Std::reads("greater_equals_string", "cmp::ge"),
    Std::reads("greater_equals_bytes", "cmp::ge"),
    Std::reads("greater_equals_bool", "cmp::ge"),
    Std::reads("greater_equals_timestamp", "cmp::ge"),
    Std::reads("greater_equals_duration", "cmp::ge"),
];
