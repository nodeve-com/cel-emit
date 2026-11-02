//! `matches` with a constant pattern: its search table, built here.

use super::*;

/// The call of overload `id` on `operands`. A `matches` whose pattern is a string constant becomes
/// a [`Node::Matches`], its table built now: a pattern with no table (one RE2 rejects, or one
/// [`pattern_emit`] declines) is refused, as `matches` of it would fail every time it ran.
pub(super) fn call(id: &str, mut operands: Vec<Typed>) -> Result<Node, Error> {
    if matches!(id, "string_matches_re" | "matches_string_re") {
        if let [_, Typed {
            node: Node::Str(re),
            ..
        }] = operands.as_slice()
        {
            let table = pattern_emit::build(re)
                .or_else(|why| unsupported(format!("the pattern {re:?} of matches: {why}")))?;
            let re = re.clone();
            let subject = operands.swap_remove(0);
            return Ok(Node::Matches(Box::new(subject), table, re));
        }
    }
    Ok(Node::Call(id.into(), operands))
}

#[cfg(test)]
mod tests {
    use crate::{lower, At, Decls, Error, Name, Node, Type, Typed};

    fn named(at: At) -> Decls {
        let mut d = Decls::default();
        d.names.insert(
            "re".into(),
            Name {
                ty: Type::String,
                at,
            },
        );
        d
    }

    #[test]
    fn a_constant_pattern_is_a_table() {
        let d = Decls::default();
        for source in ["'a grey day'.matches('gr(a|e)y')", "matches('x', 'x+')"] {
            let t = lower(source, &d).unwrap();
            assert!(matches!(t.node, Node::Matches(..)), "{source}");
        }
        let d = named(At::Constant(Typed::string("gr(a|e)y")));
        let t = lower("'grey'.matches(re)", &d).unwrap();
        assert!(matches!(t.node, Node::Matches(..)));
    }

    #[test]
    fn a_pattern_from_an_input_stays_a_call() {
        let t = lower("'x'.matches(re)", &named(At::Var(0))).unwrap();
        assert!(matches!(t.node, Node::Call(..)));
    }

    #[test]
    fn a_pattern_with_no_table_is_refused_with_its_reason() {
        // Go rejects the first and third; the second RE2 reads unlike regex-syntax.
        for source in [
            "'x'.matches('(')",
            r"'x'.matches('\\<')",
            "'x'.matches('a**')",
        ] {
            let Err(Error::Unsupported(why)) = lower(source, &Decls::default()) else {
                panic!("{source} not refused");
            };
            assert!(why.contains("of matches"), "{why}");
        }
    }
}
