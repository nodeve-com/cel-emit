//! JSON: cel-emit-runtime's `json`, the overloads `json.decode` and `json.encode` resolve to.

use crate::functions::Std;

pub(super) const FUNCTIONS: &[Std] = &[
    Std::reads("json_decode", "json::decode"),
    Std::reads("json_encode", "json::encode"),
];
