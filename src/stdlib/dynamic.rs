//! Dyn: cel-emit-runtime's `dynamic`. These ids are cel-emit's own, made where an operand is dyn:
//! the checker resolves a call on one to several overloads, or to none.

use crate::functions::Std;

pub(super) const FUNCTIONS: &[Std] = &[
    Std::reads("dyn_get", "dynamic::get"),
    Std::reads("dyn_at", "dynamic::at"),
    Std::reads("dyn_has", "dynamic::has"),
    Std::reads("dyn_optional_get", "dynamic::optional_get"),
    Std::reads("dyn_chain", "dynamic::chain"),
    Std::reads("dyn_contains", "dynamic::contains"),
    Std::reads("dyn_size", "dynamic::size"),
    Std::any("dyn_to_int", "dynamic::cast::to_int"),
    Std::any("dyn_to_uint", "dynamic::cast::to_uint"),
    Std::any("dyn_to_double", "dynamic::cast::to_double"),
    Std::any("dyn_to_string", "dynamic::cast::to_str"),
    Std::any("dyn_to_bool", "dynamic::cast::to_bool"),
    Std::any("dyn_list", "dynamic::to_list"),
];
