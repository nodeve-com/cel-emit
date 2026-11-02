//! JSON: cel-emit-runtime's `json`, the overloads `json.decode` and `json.encode` resolve to, and the
//! field reads a `json.decode(text, Message)` lowers to.

use crate::functions::Std;

pub(super) const FUNCTIONS: &[Std] = &[
    Std::reads("json_decode", "json::decode"),
    Std::reads("json_encode", "json::encode"),
    Std::reads("json_encode_without", "json::encode_without"),
    Std::reads("json_field", "json::field"),
    Std::reads("json_has_field", "json::has_field"),
    Std::reads("json_optional_field", "json::optional_field"),
];
