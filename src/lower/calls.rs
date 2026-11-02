//! Calls: expansions, `duration()` and `timestamp()` literals, and the rest by overload.

use cel_emit_runtime::time::{parse_timestamp, string_to_duration};
use cel_emit_runtime::Error as StdError;
use chrono::TimeDelta;

use super::*;

impl<'a> Lower<'a> {
    pub(crate) fn call(
        &mut self,
        e: &'a SpannedExpr,
        callee: &'a SpannedExpr,
        args: &'a [SpannedExpr],
    ) -> Result<Node, Error> {
        if self.is_enum_constructor(e) {
            return self.enum_constructor(e, args);
        }
        let name = self.named(e, callee);
        let mut expansions = self.decls.expansions.iter();
        let expansion = expansions.find(|x| name == Some(x.decl.name.as_str()));
        if let Some(expand) = expansion.map(|x| x.expand) {
            return self.expanded(expand, args);
        }
        match (name, args) {
            (Some(f @ ("duration" | "timestamp")), [arg]) => match &arg.node {
                Expr::String(text) => literal(f, text),
                _ => self.by_overload(e),
            },
            _ => self.by_overload(e),
        }
    }

    fn by_overload(&mut self, e: &'a SpannedExpr) -> Result<Node, Error> {
        match self.overload(e)? {
            Some(node) => Ok(node),
            None => unsupported(self.unresolved(e)),
        }
    }

    /// A call an [`crate::Expansion`] rewrites.
    fn expanded(&mut self, expand: crate::Expand, args: &'a [SpannedExpr]) -> Result<Node, Error> {
        let mut read = Vec::with_capacity(args.len());
        for a in args {
            read.push(crate::Arg {
                path: self.path(a),
                text: match &a.node {
                    Expr::String(text) => Some(text.clone()),
                    _ => None,
                },
                value: self.expr(a)?,
            });
        }
        Ok(expand(&read, &self.decls.tags)?.node)
    }
}

/// A duration in whole ms, read as `duration(x)` reads text: not negative, no finer than a ms.
pub fn ms(text: &str) -> Result<i64, String> {
    let d = string_to_duration(text).map_err(|e| format!("{text}: {}", why(e)))?;
    if d < TimeDelta::zero() {
        return Err("negative".into());
    }
    if d.subsec_nanos() % 1_000_000 != 0 {
        return Err("finer than a ms".into());
    }
    Ok(d.num_milliseconds())
}

/// Why a time reads no value.
fn why(e: StdError) -> &'static str {
    match e {
        StdError::Overflow => "outside CEL's range",
        _ => "not readable",
    }
}

/// `duration('90m')`, read as `duration(x)` reads text, or
/// `timestamp('2026-01-01T00:00:00Z')`, RFC 3339: to the ns, inside CEL's range.
pub fn literal(function: &str, text: &str) -> Result<Node, Error> {
    let invalid = |e| Error::Invalid(format!("{function}('{text}'): {}", why(e)));
    if function == "duration" {
        let d = string_to_duration(text).map_err(invalid)?;
        return Ok(Node::Duration(d.num_seconds(), d.subsec_nanos()));
    }
    let t = parse_timestamp(text).map_err(invalid)?;
    Ok(Node::Timestamp(t.timestamp(), t.timestamp_subsec_nanos()))
}
