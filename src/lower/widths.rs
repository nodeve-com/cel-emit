//! Stored widths in a call: what a parameter of a width takes, a comparison at one width, and a
//! stored collection a call reads one element of.

use super::*;

impl<'a> Lower<'a> {
    /// Each operand of an embedder's overload `id` whose parameter holds a width, taken as a field
    /// of that width takes it.
    pub(super) fn declared_widths(&self, id: &str, operands: &mut [Typed]) -> Result<(), Error> {
        let declared = self.decls.functions.iter().flat_map(|f| &f.decl.overloads);
        let Some(o) = declared.into_iter().find(|o| o.id == id) else {
            return Ok(());
        };
        for (operand, param) in operands.iter_mut().zip(&o.params) {
            if param.narrow() {
                let value = std::mem::replace(operand, Typed::bool(false));
                *operand = crate::walk::cast(value, param)?;
            }
        }
        Ok(())
    }
}

/// Whether `id` compares its operands: at their own width, where both read one stored type.
pub(super) fn compares(id: &str) -> bool {
    matches!(id, "equals" | "not_equals") || id.starts_with("less_") || id.starts_with("greater_")
}

/// A stored list or map `id` reads one element of, or the size of, left as stored: the call
/// widens what it reads ([`crate::emit`]) rather than a copy of the whole collection.
pub(super) fn widen_lazily(id: &str, operands: &mut [Typed]) {
    let at = match id {
        "size_list" | "list_size" | "size_map" | "map_size" | "index_list" | "index_map" => 0,
        "in_list" | "in_map" => 1,
        _ => return,
    };
    if let Some(operand) = operands.get_mut(at) {
        if let Node::Widen(stored) = &mut operand.node {
            let stored = std::mem::replace(&mut **stored, Typed::bool(false));
            *operand = stored;
        }
    }
}
