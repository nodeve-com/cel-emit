//! What an expression fills: a message built, a map literal, a message read whole, or one value; and what a `cel.bind` name stands for.

use cel_core::types::MapEntry;

use super::*;

impl<'a> Lower<'a> {
    /// `e` as what it fills: a message built, a map literal, a message read whole, or one value.
    pub(crate) fn value(&mut self, e: &'a SpannedExpr) -> Result<Typed, Error> {
        match &e.node {
            Expr::Bind {
                var_name,
                init,
                body,
            } => {
                let (v, lets) = self.binding(var_name, init, |s| s.value(body))?;
                Ok(super::scope::wrap(v, &lets))
            }
            Expr::Struct { .. } => self.message(e),
            Expr::Map(entries) => self.map_value(entries),
            Expr::Ident(name) => match self.bound(name) {
                Some(v) => Ok(v.clone()),
                None if self.local(name).is_none() && self.whole(name) => self.expand(name),
                None => self.expr(e),
            },
            Expr::Member { .. } => match self.selected_field(e)? {
                Some(v) => Ok(v),
                None => self.expr(e),
            },
            _ => self.expr(e),
        }
    }

    /// A map literal of string keys as a message of its entries.
    fn map_value(&mut self, entries: &'a [MapEntry]) -> Result<Typed, Error> {
        let mut made = Vec::with_capacity(entries.len());
        for entry in entries {
            let Expr::String(key) = &entry.key.node else {
                return unsupported("a map key other than a string literal");
            };
            if made.iter().any(|(k, _, _)| k == key) {
                return invalid(format!("map key '{key}' is repeated"));
            }
            made.push((key.clone(), self.value(&entry.value)?, entry.optional));
        }
        super::parts::literal_map(made)
    }

    /// Whether `name` is a message read whole: one with a field in scope.
    pub(crate) fn whole(&self, name: &str) -> bool {
        let prefix = format!("{name}.");
        self.decls.names.keys().any(|n| n.starts_with(&prefix))
    }

    /// The message `name`'s fields, each a name in scope, nested as its messages are.
    pub(crate) fn expand(&self, name: &str) -> Result<Typed, Error> {
        let prefix = format!("{name}.");
        let mut fields: Vec<(String, Typed)> = Vec::new();
        let leaves: Vec<(String, Name)> = self
            .decls
            .names
            .iter()
            .filter_map(|(n, v)| Some((n.strip_prefix(&prefix)?.to_string(), v.clone())))
            .collect();
        for (rest, n) in leaves {
            let (head, tail) = rest
                .split_once('.')
                .map_or((rest.as_str(), None), |(h, t)| (h, Some(t)));
            if tail.is_some() && fields.iter().any(|(f, _)| f == head) {
                continue;
            }
            let value = match (tail, n.at) {
                (None, At::Var(v)) => Typed::new(n.ty.clone(), Node::Var(v)),
                (None, At::Constant(c)) => c,
                (Some(_), _) => self.expand(&format!("{prefix}{head}"))?,
            };
            fields.push((head.to_string(), value));
        }
        super::parts::anonymous(fields)
    }

    /// The tag an expansion gives `e`, and how a field refuses it, through a `cel.bind` name.
    pub(crate) fn tagged(&self, e: &'a SpannedExpr) -> Option<(String, crate::Mismatch)> {
        let e = match &e.node {
            Expr::Ident(name) => self.bound_init(name)?,
            _ => e,
        };
        let Expr::Call { expr, args } = &e.node else {
            return None;
        };
        let name = self.named(e, expr)?;
        let mut expansions = self.decls.expansions.iter();
        let expansion = expansions.find(|x| x.decl.name == name)?;
        match &args.last()?.node {
            Expr::String(tag) => Some((tag.clone(), expansion.mismatch)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::types::{Field, Type};
    use crate::{lowered, Decls};

    #[test]
    fn a_map_literal_is_told_through_binds_and_branches() {
        let mut decls = Decls::default();
        let n = Field {
            name: "n".into(),
            ty: Type::Int,
            required: false,
        };
        decls.messages.insert("M".into(), Type::message(vec![n]));
        let map = |cel| lowered(cel, &decls).unwrap().map;
        assert!(map(r#"{"n": 1}"#));
        assert!(map(r#"cel.bind(x, 1, {"n": x})"#));
        assert!(map(r#"1 < 2 ? {"n": 1} : {"n": 2}"#));
        assert!(!map("M{n: 1}"));
        assert!(!map("cel.bind(x, 1, M{n: x})"));
        assert!(!map("1 < 2 ? M{n: 1} : M{n: 2}"));
    }
}
