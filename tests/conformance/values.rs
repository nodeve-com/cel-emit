//! A case's bindings: each one's name, type, and value, from the suite.

use std::collections::BTreeMap;

use cel_core::Value;
use cel_core_conformance::SimpleTest;
use cel_core_proto::gen::cel::expr::decl::DeclKind;
use cel_core_proto::gen::cel::expr::{expr_value, r#type::TypeKind};
use cel_core_proto::{cel_type_from_proto, proto_value_to_value, ProstProtoRegistry};
use cel_emit::types::of_cel;
use cel_emit::Type;

use super::common::values::{fits, held, infer};
use super::Verdict::{self, NotApplicable, Unsupported};

/// A binding: its name, its type, and its value.
pub type Input = (String, Type, Value);

/// Each of `test`'s bindings by name: its declared type, or one from its value, and its value;
/// or why they can't be supplied.
pub fn inputs(test: &SimpleTest, registry: &ProstProtoRegistry) -> Result<Vec<Input>, Verdict> {
    let mut declared = declared_types(test)?;
    let mut inputs = Vec::new();
    for (name, bound) in &test.bindings {
        let Some(expr_value::Kind::Value(v)) = &bound.kind else {
            return Err(Unsupported("a binding that is no value".into()));
        };
        let value = proto_value_to_value(v, registry)
            .map_err(|e| Unsupported(format!("a binding that can't be held: {e}")))?;
        let unheld = || Unsupported(format!("a binding of value: {}", kind(&value)));
        let ty = match declared.remove(name) {
            Some(t) => t,
            None => infer(&value, None).ok_or_else(unheld)?,
        };
        if !fits(&value, &ty) {
            return Err(unheld());
        }
        inputs.push((name.clone(), ty, value));
    }
    if let Some(name) = declared.keys().next() {
        return Err(Unsupported(format!(
            "a declared name without a value: {name}"
        )));
    }
    inputs.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(inputs)
}

/// The type each of `test`'s declared names has, or why one can't be supplied.
fn declared_types(test: &SimpleTest) -> Result<BTreeMap<String, Type>, Verdict> {
    let mut declared = BTreeMap::new();
    for decl in &test.type_env {
        let Some(DeclKind::Ident(ident)) = &decl.decl_kind else {
            return Err(Unsupported("a declared function".into()));
        };
        let Some(ty) = &ident.r#type else {
            return Err(Unsupported("a declaration without a type".into()));
        };
        if matches!(ty.type_kind, Some(TypeKind::Dyn(_))) {
            return Err(NotApplicable("dyn"));
        }
        let cel = cel_type_from_proto(ty);
        // No message: the suite's are protobuf's, never a declared message.
        let Some(t) = of_cel(&cel, &BTreeMap::new()).filter(held) else {
            return Err(Unsupported(format!(
                "a declared type: {}",
                cel.display_name()
            )));
        };
        declared.insert(decl.name.clone(), t);
    }
    Ok(declared)
}

/// `v`'s kind, for a reason.
fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bytes(_) => "bytes",
        Value::List(_) => "an empty or mixed list",
        Value::Map(_) => "an empty or non-string-keyed map",
        Value::Type(_) => "type",
        Value::Message(_) => "message",
        Value::Enum(_) => "enum",
        _ => "other",
    }
}
