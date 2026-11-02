//! One case: lowered to the leaves it answers with, and judged by what a native run wrote.

use cel_core::Value;
use cel_core_conformance::SimpleTest;
use cel_core_proto::gen::cel::expr::conformance::test::simple_test::ResultMatcher;
use cel_core_proto::{proto_value_to_value, ProstProtoRegistry};
use cel_emit::decls::Name;
use cel_emit::{Node, Type, Typed};

use super::native::{Answer, LEAF};
use super::Verdict::{self, NotApplicable, Pass, Unsupported, Wrong};
use super::{common, message, scope, types, values};

/// What a case expects: a value, or any error.
pub enum Expect {
    Value(Value),
    Error,
}

/// A case ready to build: what it expects, its inputs, and the leaves its result is.
pub struct Case {
    pub expect: Expect,
    pub inputs: Vec<values::Input>,
    /// Each leaf of the result by its path: one, with no path, unless the result is a message.
    pub leaves: Vec<(Vec<String>, Typed)>,
    pub message: bool,
}

/// A case's name, its expression, and the case.
pub type Prepared = (String, String, Case);

/// `test` lowered, or its verdict without running.
pub fn prepare(test: &SimpleTest, registry: &ProstProtoRegistry) -> Result<Case, Verdict> {
    if test.check_only {
        return Err(NotApplicable("check only: the suite runs nothing"));
    }
    // Typed CEL, no `dyn`. An unchecked case runs dynamically.
    if test.disable_check || test.expr.contains("dyn(") {
        return Err(NotApplicable("dyn"));
    }
    // Proto2 extensions and `Any` values: the case's own names and values, found structurally.
    if let Some(why) = scope::outside(test, registry) {
        return Err(NotApplicable(why));
    }
    let value = |v| {
        proto_value_to_value(v, registry)
            .map(Expect::Value)
            .map_err(|e| Unsupported(format!("an expected value that can't be held: {e}")))
    };
    let expect = match &test.result_matcher {
        None => Expect::Value(Value::Bool(true)),
        Some(ResultMatcher::Value(v)) => value(v)?,
        Some(ResultMatcher::TypedResult(t)) => match &t.result {
            None => Expect::Value(Value::Bool(true)),
            Some(v) => value(v)?,
        },
        Some(ResultMatcher::EvalError(_) | ResultMatcher::AnyEvalErrors(_)) => Expect::Error,
        Some(ResultMatcher::Unknown(_) | ResultMatcher::AnyUnknowns(_)) => {
            return Err(Unsupported("an unknown: no partial evaluation".into()))
        }
    };
    if test.disable_macros {
        return Err(Unsupported("macros off".into()));
    }
    // cel-emit has no container. Without declared names, one could only resolve a message or a
    // namespaced function, which check then rejects: never a different answer.
    let declares = !test.type_env.is_empty() || !test.bindings.is_empty();
    if !test.container.is_empty() && declares {
        return Err(Unsupported("a container over declared names".into()));
    }
    let inputs = values::inputs(test, registry)?;
    let mut decls = types::declared(registry, &test.container);
    for (i, (name, ty, _)) in inputs.iter().enumerate() {
        decls.names.insert(name.clone(), Name::var(ty.clone(), i));
    }
    let lowered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cel_emit::lower(&test.expr, &decls)
    }));
    let tree = match lowered {
        Err(_) => return Err(Wrong("lowering panicked".into())),
        Ok(Err(cel_emit::Error::Unsupported(m))) => return Err(Unsupported(m)),
        Ok(Err(cel_emit::Error::Invalid(m))) => return Err(Unsupported(format!("rejected: {m}"))),
        Ok(Ok(tree)) => tree,
    };
    let whole = matches!(tree.ty, Type::Message(_));
    let leaves = flatten(&[], tree);
    if let Some((_, t)) = leaves.iter().find(|(_, t)| !common::values::shown(&t.ty)) {
        return Err(Unsupported(format!("a {} value", t.ty.display())));
    }
    // A message of fields is a message only if the case expects one, or an error: a map literal
    // lowers alike.
    let message = matches!(&expect, Expect::Value(v) if message::of(v).is_some());
    if whole && !message && !matches!(expect, Expect::Error) {
        return Err(Unsupported("a map as the result".into()));
    }
    Ok(Case {
        expect,
        inputs,
        leaves,
        message: whole,
    })
}

/// Each leaf of `tree` under `at`, in order, by its path: a message's fields, each a leaf of the
/// bindings its lets name.
fn flatten(at: &[String], tree: Typed) -> Vec<(Vec<String>, Typed)> {
    if !matches!(tree.ty, Type::Message(_)) {
        return vec![(at.to_vec(), tree)];
    }
    let cel_emit::Split { lets, fields } = cel_emit::split(tree);
    let mut leaves = Vec::new();
    for (name, field) in fields {
        // A value filling a nullable field is the field's value itself.
        let mut body = match field.node {
            Node::Some(x) if matches!(field.ty, Type::Nullable(_)) => *x,
            _ => field,
        };
        for (n, value) in lets.iter().rev() {
            let ty = body.ty.clone();
            body = Typed::new(ty, Node::Let(*n, Box::new(value.clone()), Box::new(body)));
        }
        leaves.extend(flatten(&[at, &[name]].concat(), body));
    }
    leaves
}

/// What `case` expects, against what a native run answered.
pub fn judge(case: &Case, answer: Answer) -> Verdict {
    let got = match answer {
        Ok(got) => got,
        Err(v) => return v,
    };
    if got == "panic" {
        return Wrong("panicked".into());
    }
    let want = match &case.expect {
        Expect::Error if got == "err" => return Pass,
        Expect::Error => return Wrong(format!("want an error, got {got}")),
        Expect::Value(want) => want,
    };
    if got == "err" {
        return Wrong(format!("want {want:?}, got an error"));
    }
    if let Some(msg) = message::of(want) {
        let leaves: Vec<message::Leaf> = match case.message {
            true if got.is_empty() => Vec::new(),
            true => got
                .split(LEAF)
                .filter_map(|l| l.split_once('='))
                .map(|(p, v)| (p.split('.').map(String::from).collect(), v.to_string()))
                .collect(),
            false => vec![(Vec::new(), got)],
        };
        return message::differs(msg, &leaves).map_or(Pass, Wrong);
    }
    let shown = common::values::show(want);
    if got == shown {
        Pass
    } else {
        Wrong(format!("want {shown}, got {got}"))
    }
}
