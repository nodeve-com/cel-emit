//! A mixed `math.greatest` or `math.least`'s number, and the casts and comparisons that take it:
//! cel-emit-runtime's `number`.

use crate::functions::Std;

pub(super) const FUNCTIONS: &[Std] = &[
    Std::any("number_of_int", "number::of_int"),
    Std::any("number_of_uint", "number::of_uint"),
    Std::any("number_of_double", "number::of_double"),
    Std::any("number_to_int", "number::to_int"),
    Std::any("number_to_uint", "number::to_uint"),
    Std::any("number_to_double", "number::to_double"),
];
