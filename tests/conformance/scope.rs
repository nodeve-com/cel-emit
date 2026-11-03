//! What a case uses that the suite leaves out of cel-emit's scope: a proto2 extension, a well-known
//! type cel-emit doesn't narrow (`Any`, `Value`, `Struct`, `ListValue`, `NullValue`), or a presence
//! a message doesn't carry. Found in the case itself: its expression's names, struct literals and
//! field selections, its declared types, and its values, checked against the descriptor pool.

use std::collections::BTreeMap;

use cel_core::{Expr, SpannedExpr, StructField, Value};
use cel_core_conformance::SimpleTest;
use cel_core_proto::gen::cel::expr::conformance::test::simple_test::ResultMatcher;
use cel_core_proto::gen::cel::expr::expr_value;
use cel_core_proto::{proto_value_to_value, ProstMessage, ProstProtoRegistry};
use prost_reflect::{FieldDescriptor, Kind, Syntax};

pub const EXTENSION: &str = "proto2 extension";
pub const ANY: &str = "google.protobuf.Any";
pub const UNNARROWED: &str = "unnarrowed Value/Struct/ListValue";
pub const PROTO2_PRESENCE: &str = "proto2 presence";
pub const ONEOF_PRESENCE: &str = "oneof presence";
pub const MIXED_EXTREME: &str = "mixed math.greatest/least used bare: cast it";

/// The messages and enums a case may not use, each with why.
const OUT_OF_SCOPE: [(&str, &str); 5] = [
    ("google.protobuf.Any", ANY),
    ("google.protobuf.Value", UNNARROWED),
    ("google.protobuf.Struct", UNNARROWED),
    ("google.protobuf.ListValue", UNNARROWED),
    ("google.protobuf.NullValue", UNNARROWED),
];

/// Why `test` uses a proto2 extension or an out-of-scope type, if it does.
pub fn outside(test: &SimpleTest, registry: &ProstProtoRegistry) -> Option<&'static str> {
    let scope = Scope {
        registry,
        container: &test.container,
        declared: super::types::declared_messages(test),
    };
    if let Some(ast) = cel_core::parse(&test.expr).ast {
        if let Some(why) = super::mixed::bare(&ast).or_else(|| scope.expr(&ast)) {
            return Some(why);
        }
    }
    if let Some(why) = scope.declared.values().find_map(|n| scope.type_scope(n)) {
        return Some(why);
    }
    let mut values = Vec::new();
    for bound in test.bindings.values() {
        if let Some(expr_value::Kind::Value(v)) = &bound.kind {
            values.push(v);
        }
    }
    match &test.result_matcher {
        Some(ResultMatcher::Value(v)) => values.push(v),
        Some(ResultMatcher::TypedResult(t)) => values.extend(t.result.as_ref()),
        _ => {}
    }
    values.into_iter().find_map(|v| {
        let decoded = proto_value_to_value(v, registry).ok()?;
        decoded_scope(&decoded)
    })
}

/// The out-of-scope reason for a type's full name, if it is one.
fn out_reason(name: &str) -> Option<&'static str> {
    OUT_OF_SCOPE
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, why)| *why)
}

/// The out-of-scope reason for a field whose type is one, if it is one.
fn field_scope(fd: &FieldDescriptor) -> Option<&'static str> {
    match fd.kind() {
        Kind::Message(m) => out_reason(m.full_name()),
        Kind::Enum(e) => out_reason(e.full_name()),
        _ => None,
    }
}

/// The dotted name a chain of identifiers and field selections spells, if it is one.
pub(super) fn dotted(e: &SpannedExpr) -> Option<String> {
    match &e.node {
        Expr::Ident(n) | Expr::RootIdent(n) => Some(n.trim_start_matches('.').to_string()),
        Expr::Member {
            expr,
            field,
            optional: false,
        } => Some(format!("{}.{field}", dotted(expr)?)),
        _ => None,
    }
}

/// The case's names, as the registry and its container resolve them.
struct Scope<'a> {
    registry: &'a ProstProtoRegistry,
    container: &'a str,
    /// The message type each declared name has, by its full name.
    declared: BTreeMap<String, String>,
}

impl Scope<'_> {
    /// The full names `name` may stand for: relative to the container, then as written.
    fn candidates(&self, name: &str) -> Vec<String> {
        if self.container.is_empty() {
            vec![name.to_string()]
        } else {
            vec![format!("{}.{name}", self.container), name.to_string()]
        }
    }

    fn message(&self, name: &str) -> Option<prost_reflect::MessageDescriptor> {
        self.candidates(name)
            .iter()
            .find_map(|n| self.registry.get_message(n))
    }

    /// Why a type name is out of scope, if it is: a message or enum the registry knows, or the
    /// name itself.
    fn type_scope(&self, name: &str) -> Option<&'static str> {
        let full = self
            .candidates(name)
            .iter()
            .find_map(|n| {
                self.registry
                    .get_message(n)
                    .map(|m| m.full_name().to_string())
                    .or_else(|| self.registry.get_enum(n).map(|e| e.full_name().to_string()))
            })
            .unwrap_or_else(|| name.to_string());
        out_reason(&full)
    }

    /// Whether `name` is a proto2 extension in the registry: a field selection `msg.\`name\`` or a
    /// name a function argument such as `proto.hasExt` takes.
    fn extension(&self, name: &str) -> Option<&'static str> {
        self.registry
            .get_extension_by_name(name)
            .is_some()
            .then_some(EXTENSION)
    }

    /// Why `base.field` reads an out-of-scope field: `base` a message literal, or a name the case
    /// declares a message type for.
    fn selected(&self, base: &SpannedExpr, field: &str) -> Option<&'static str> {
        let message = match &base.node {
            Expr::Struct { type_name, .. } => self.message(&dotted(type_name)?)?,
            Expr::Ident(n) | Expr::RootIdent(n) => self.message(self.declared.get(n)?)?,
            _ => return None,
        };
        field_scope(&message.get_field_by_name(field)?)
    }

    /// Why `has(base.field)` tests a presence a message doesn't carry, if it does: `base` a message
    /// literal, `field` a singular field of a message in a proto2 file (proto2's presence) or in a
    /// oneof (proto3's, for a field set to its default). Lists and maps have none to test.
    fn presence_outside(&self, base: &SpannedExpr, field: &str) -> Option<&'static str> {
        let Expr::Struct { type_name, .. } = &base.node else {
            return None;
        };
        let message = self.message(&dotted(type_name)?)?;
        let fd = message.get_field_by_name(field)?;
        if fd.is_list() || fd.is_map() {
            return None;
        }
        if message.parent_file().syntax() == Syntax::Proto2 {
            return Some(PROTO2_PRESENCE);
        }
        fd.containing_oneof().is_some().then_some(ONEOF_PRESENCE)
    }

    /// A field selection `e`, which is `base.field`.
    fn select(&self, e: &SpannedExpr, base: &SpannedExpr, field: &str) -> Option<&'static str> {
        self.extension(field)
            .or_else(|| dotted(e).as_deref().and_then(|n| self.extension(n)))
            .or_else(|| dotted(e).as_deref().and_then(|n| self.type_scope(n)))
            .or_else(|| dotted(base).as_deref().and_then(|n| self.type_scope(n)))
            .or_else(|| self.selected(base, field))
            .or_else(|| self.expr(base))
    }

    /// Why a message literal of `type_name` setting `fields` is out of scope.
    fn literal_scope(
        &self,
        type_name: &SpannedExpr,
        fields: &[StructField],
    ) -> Option<&'static str> {
        let name = dotted(type_name);
        name.as_deref()
            .and_then(|n| self.type_scope(n))
            .or_else(|| {
                // A field the message declares with an out-of-scope type.
                let message = self.message(name.as_deref()?);
                fields.iter().find_map(|f| {
                    message
                        .as_ref()?
                        .get_field_by_name(&f.name)
                        .and_then(|fd| field_scope(&fd))
                })
            })
            .or_else(|| fields.iter().find_map(|f| self.expr(&f.value)))
    }

    fn expr(&self, e: &SpannedExpr) -> Option<&'static str> {
        let sub = |e: &SpannedExpr| self.expr(e);
        match &e.node {
            Expr::Ident(n) | Expr::RootIdent(n) => self.extension(n).or_else(|| self.type_scope(n)),
            Expr::MemberTestOnly { expr: base, field } => self
                .presence_outside(base, field)
                .or_else(|| self.select(e, base, field)),
            Expr::Member { expr, field, .. } => self.select(e, expr, field),
            Expr::Struct { type_name, fields } => self.literal_scope(type_name, fields),
            Expr::Unary { expr, .. } => sub(expr),
            Expr::Binary { left, right, .. } => sub(left).or_else(|| sub(right)),
            Expr::Ternary {
                cond,
                then_expr,
                else_expr,
            } => sub(cond)
                .or_else(|| sub(then_expr))
                .or_else(|| sub(else_expr)),
            Expr::Index { expr, index, .. } => sub(expr).or_else(|| sub(index)),
            Expr::Call { expr, args } => sub(expr).or_else(|| args.iter().find_map(sub)),
            Expr::List(items) => items.iter().find_map(|i| sub(&i.expr)),
            Expr::Map(entries) => entries
                .iter()
                .find_map(|m| sub(&m.key).or_else(|| sub(&m.value))),
            Expr::Comprehension(c) => [
                &c.iter_range,
                &c.accu_init,
                &c.loop_condition,
                &c.loop_step,
                &c.result,
            ]
            .into_iter()
            .find_map(|x| sub(x)),
            Expr::Bind { init, body, .. } => sub(init).or_else(|| sub(body)),
            _ => None,
        }
    }
}

/// A decoded value: a message in it, or in a list or map in it, is checked.
fn decoded_scope(v: &Value) -> Option<&'static str> {
    match v {
        Value::Message(m) => message_scope(m.as_any().downcast_ref::<ProstMessage>()?),
        Value::List(items) => items.iter().find_map(decoded_scope),
        Value::Map(map) => map.iter().find_map(|(_, v)| decoded_scope(v)),
        _ => None,
    }
}

/// A message is out of scope, holds a set extension, or holds one of those in a field.
fn message_scope(m: &ProstMessage) -> Option<&'static str> {
    if let Some(why) = out_reason(m.descriptor().full_name()) {
        return Some(why);
    }
    let fields = m.message();
    if fields.extensions().next().is_some() {
        return Some(EXTENSION);
    }
    for (fd, v) in fields.fields() {
        if let Some(why) = field_scope(&fd) {
            return Some(why);
        }
        // The messages a field holds: itself, a repeated field's, or a map's values.
        let mut nested: Vec<ProstMessage> = Vec::new();
        if let Some(sub) = v.as_message() {
            nested.push(ProstMessage::new(sub.clone()));
        }
        if let Some(items) = v.as_list() {
            nested.extend(
                items
                    .iter()
                    .filter_map(|i| i.as_message())
                    .map(|s| ProstMessage::new(s.clone())),
            );
        }
        if let Some(map) = v.as_map() {
            nested.extend(
                map.values()
                    .filter_map(|i| i.as_message())
                    .map(|s| ProstMessage::new(s.clone())),
            );
        }
        if let Some(why) = nested.iter().find_map(message_scope) {
            return Some(why);
        }
    }
    None
}
