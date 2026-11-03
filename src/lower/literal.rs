//! List and map literals as values.

use cel_core::types::{ListElement, MapEntry};

use super::dynamic::fits;
use super::*;

impl<'a> Lower<'a> {
    /// `e` where it is a list or map literal of type `ty`.
    pub(super) fn collection(
        &mut self,
        e: &'a SpannedExpr,
        ty: &Type,
    ) -> Result<Option<Typed>, Error> {
        if !crate::stdlib::literal(ty) {
            return Ok(None);
        }
        match (&e.node, ty) {
            (Expr::List(items), Type::List(elem)) => self.list_literal(items, elem, ty),
            (Expr::Map(entries), Type::Map(..)) => self.map_literal(entries, ty),
            (Expr::List(_), _) => invalid("a list literal of other than a list"),
            (Expr::Map(_), _) => invalid("a map literal of other than a map"),
            _ => Ok(None),
        }
    }

    /// A list literal of `items`, of type `ty` with elements `elem`.
    fn list_literal(
        &mut self,
        items: &'a [ListElement],
        elem: &Type,
        ty: &Type,
    ) -> Result<Option<Typed>, Error> {
        let mut made = Vec::with_capacity(items.len());
        for item in items {
            made.push((self.expr(&item.expr)?, item.optional));
        }
        if made.iter().any(|(_, optional)| *optional) {
            return super::optional::list(made, elem).map(Some);
        }
        let made = made.into_iter().map(|(x, _)| fits(x, elem));
        let node = Node::List(made.collect::<Result<_, _>>()?);
        Ok(Some(self.finish(ty.clone(), node)))
    }

    /// A map literal of `entries`, of the map type `ty`.
    fn map_literal(&mut self, entries: &'a [MapEntry], ty: &Type) -> Result<Option<Typed>, Error> {
        let Type::Map(key, value) = ty else {
            return invalid("a map literal of other than a map");
        };
        let mut made = Vec::with_capacity(entries.len());
        for entry in entries {
            let (k, v) = (self.expr(&entry.key)?, self.expr(&entry.value)?);
            let v = if entry.optional { v } else { fits(v, value)? };
            made.push((fits(k, key)?, v, entry.optional));
        }
        let node = if made.iter().any(|(_, _, optional)| *optional) {
            Node::MapOpt(made)
        } else {
            Node::MapOf(made.into_iter().map(|(k, v, _)| (k, v)).collect())
        };
        Ok(Some(self.finish(ty.clone(), node)))
    }
}
