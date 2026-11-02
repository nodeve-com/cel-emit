//! Constant `matches` patterns' tables: each a `static`, and one function checking them all, for
//! the embedder to call once ahead of any message.

use std::fmt::Write as _;

use super::Emitter;
use crate::functions::STD;

impl Emitter {
    /// Each pattern emitted so far, with its table's size in bytes.
    pub fn tables(&self) -> Vec<(String, usize)> {
        let sized = |(source, table): &(String, Vec<u8>)| (source.clone(), table.len());
        self.patterns.iter().map(sized).collect()
    }

    /// Each table as a `static P{n}`, and `load_patterns`, which checks them all (only if there
    /// are any).
    pub(super) fn pattern_items(&self) -> String {
        let mut out = String::new();
        for (n, (_, table)) in self.patterns.iter().enumerate() {
            let _ = writeln!(
                out,
                "static P{n}: {STD}::Pattern = {STD}::Pattern::new({});",
                pattern_emit::byte_string(table)
            );
        }
        if !self.patterns.is_empty() {
            let loads: Vec<String> = (0..self.patterns.len())
                .map(|n| format!("P{n}.load()?;"))
                .collect();
            let _ = writeln!(
                out,
                "fn load_patterns() -> Result<(), {STD}::Unusable> {{ {} Ok(()) }}",
                loads.join(" ")
            );
        }
        out
    }
}
