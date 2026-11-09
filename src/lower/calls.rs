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

#[cfg(test)]
mod tests {
    use crate::decls::Decl;
    use crate::emit::{Emitter, Messages, Vars};
    use crate::functions::Function;
    use crate::types::{Field, Type};
    use crate::{lower, Decls, Error, Name};

    struct Named;

    impl Messages for Named {
        fn path(&mut self, i: usize, _: &[Field]) -> String {
            format!("Declared{i}")
        }

        fn fields(&self, fields: &[Field]) -> Vec<String> {
            (0..fields.len()).map(|n| format!("f{n}")).collect()
        }
    }

    /// Named messages, serializing, deserializing, both or neither.
    struct Opts(bool, bool);

    impl Messages for Opts {
        fn path(&mut self, i: usize, fields: &[Field]) -> String {
            Named.path(i, fields)
        }

        fn fields(&self, fields: &[Field]) -> Vec<String> {
            Named.fields(fields)
        }

        fn serializes(&self, _: &[Field]) -> bool {
            self.0
        }

        fn deserializes(&self, _: &[Field]) -> bool {
            self.1
        }
    }

    struct Text;

    impl Vars for Text {
        fn read(&self, _: usize) -> Result<String, Error> {
            Ok("text".into())
        }
    }

    fn message(field: &str, ty: Type) -> Type {
        Type::Message(vec![Field {
            name: field.into(),
            ty,
            required: false,
        }])
    }

    /// `json.decode(text, A)` and `json.decode(text, B)` beside cel-emit's `json.decode(text)`,
    /// one overload per message, each with its own Rust.
    fn declared() -> (Decls, Vec<Function>) {
        let (a, b) = (message("n", Type::Int), message("s", Type::String));
        let decl = Decl::new("json.decode")
            .with_overload("decode_a", vec![Type::String], a.clone())
            .naming(a.clone())
            .with_overload("decode_b", vec![Type::String], b.clone())
            .naming(b.clone());
        let paths = [
            ("decode_a", "types::decode_a"),
            ("decode_b", "types::decode_b"),
        ];
        let function = Function {
            decl,
            paths: paths.map(|(id, path)| (id.into(), path.into())).to_vec(),
        };
        let mut decls = Decls::default();
        decls.functions.push(function.clone());
        decls.messages.insert("A".into(), a);
        decls.messages.insert("B".into(), b);
        decls
            .names
            .insert("text".into(), Name::var(Type::String, 0));
        (decls, vec![function])
    }

    fn rust(source: &str) -> Result<(Type, String), Error> {
        let (decls, functions) = declared();
        let tree = lower(source, &decls)?;
        let code = Emitter::new(&functions, Box::new(Named)).expr(&tree, &Text)?;
        Ok((tree.ty, code))
    }

    #[test]
    fn a_type_argument_picks_the_overload_declared_for_that_message() {
        let (ty, code) = rust("json.decode(text, A)").unwrap();
        assert_eq!(ty, message("n", Type::Int));
        assert!(code.starts_with("types::decode_a("), "{code}");
        assert!(!code.contains('A'), "{code}");
        let (ty, code) = rust("json.decode(text, B).s").unwrap();
        assert_eq!(ty, Type::String);
        assert!(code.contains("types::decode_b("), "{code}");
    }

    #[test]
    fn the_call_with_no_type_argument_stays_the_dynamic_decode() {
        let (ty, code) = rust("json.decode(text)").unwrap();
        assert_eq!(ty, Type::Dyn);
        assert!(code.contains("json::decode("), "{code}");
    }

    #[test]
    fn a_type_argument_that_names_no_message_is_an_error() {
        for source in ["json.decode(text, C)", "json.decode(text, int)"] {
            assert!(rust(source).is_err(), "{source}");
        }
    }

    #[test]
    fn a_message_that_serializes_itself_encodes_through_its_serialize() {
        let (decls, functions) = declared();
        let tree = lower("json.encode(json.decode(text, A))", &decls).unwrap();
        let own = Emitter::new(&functions, Box::new(Opts(true, false)))
            .expr(&tree, &Text)
            .unwrap();
        assert!(own.contains("json::own("), "{own}");
        let plain = Emitter::new(&functions, Box::new(Named))
            .expr(&tree, &Text)
            .unwrap();
        assert!(!plain.contains("json::own("), "{plain}");
    }

    #[test]
    fn a_dyn_filling_a_message_place_is_read_through_its_deserialize() {
        let (mut decls, functions) = declared();
        let outer = message("a", message("n", Type::Int));
        decls.messages.insert("Outer".into(), outer);
        let tree = lower("Outer{a: json.decode(text)}", &decls).unwrap();
        let reads = Emitter::new(&functions, Box::new(Opts(false, true)))
            .expr(&tree, &Text)
            .unwrap();
        assert!(reads.contains("dynamic::to::<"), "{reads}");
        let named = Emitter::new(&functions, Box::new(Named)).expr(&tree, &Text);
        assert!(named.is_err());
    }
}
