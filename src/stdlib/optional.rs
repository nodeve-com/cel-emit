//! Optionals: cel-emit-runtime's `optional`, each the overload an optionals call resolves to, or a
//! call cel-emit makes for `.?` and `[?]`.

use crate::functions::Std;
use crate::types::Type;

use super::cmp::equal;

pub(super) const FUNCTIONS: &[Std] = &[
    Std::any("optional_of", "optional::of"),
    Std {
        id: "optional_of_non_zero_value",
        path: "optional::of_non_zero_value",
        takes: |args| args.first().is_some_and(zeroable),
        borrows: false,
    },
    Std::reads("optional_has_value", "optional::has_value"),
    Std::any("optional_value", "optional::value"),
    Std::any("optional_or_value", "optional::or_value"),
    Std::any("optional_or_optional", "optional::or"),
    Std::reads("list_first", "optional::first"),
    Std::reads("list_last", "optional::last"),
    Std::reads("optional_index_list", "optional::index_list"),
    Std {
        id: "optional_index_map",
        path: "optional::index_map",
        takes: |args| args.get(1).is_some_and(equal),
        borrows: true,
    },
];

/// Whether `ofNonZeroValue` can tell a zero of `ty`: a value cel-emit-runtime compares and gives a
/// zero, which a timestamp or duration does not (their zero is no CEL zero).
fn zeroable(ty: &Type) -> bool {
    !matches!(
        ty,
        Type::Timestamp | Type::Duration | Type::Message(_) | Type::Optional(_)
    ) && equal(ty)
}
