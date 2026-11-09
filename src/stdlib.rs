//! Each overload cel-emit-runtime runs, by the id cel-core's checker resolves a call to: one table
//! per area, each beside cel-emit-runtime's module of the same name.

use crate::functions::Std;

mod cmp;
mod collections;
mod conversions;
mod dynamic;
mod fields;
mod json;
mod lists;
mod math;
mod number;
mod optional;
mod strings;
mod time;

pub(crate) use cmp::{equal, uses_equality};
pub(crate) use collections::literal;

pub(crate) const ALL: &[&[Std]] = &[
    cmp::FUNCTIONS,
    collections::FUNCTIONS,
    conversions::FUNCTIONS,
    dynamic::FUNCTIONS,
    fields::FUNCTIONS,
    json::FUNCTIONS,
    lists::FUNCTIONS,
    math::FUNCTIONS,
    number::FUNCTIONS,
    optional::FUNCTIONS,
    strings::FUNCTIONS,
    time::FUNCTIONS,
];
