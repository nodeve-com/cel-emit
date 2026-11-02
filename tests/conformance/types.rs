//! The suite's messages and enums as cel-emit declares them: each enum's values as int constants
//! under their qualified names, and the messages of the package a case's container names, each
//! field from the descriptor pool, every number at its proto width. A field cel-emit has no
//! type for yet is left out, so a case that reads it is refused.

use std::collections::BTreeMap;

use cel_core_proto::ProstProtoRegistry;
use cel_emit::decls::Name;
use cel_emit::tree::{Node, Typed};
use cel_emit::types::{Field, Type, Width};
use cel_emit::{At, Decls};
use prost_reflect::{FieldDescriptor, Kind, MessageDescriptor};

/// The packages the suite declares its `TestAllTypes` in.
const PROTO2: &str = "cel.expr.conformance.proto2";
const PROTO3: &str = "cel.expr.conformance.proto3";
const PACKAGES: [&str; 2] = [PROTO2, PROTO3];
const TEST_ALL_TYPES: &str = "TestAllTypes";
const WRAPPERS: [&str; 9] = [
    "google.protobuf.BoolValue",
    "google.protobuf.BytesValue",
    "google.protobuf.DoubleValue",
    "google.protobuf.FloatValue",
    "google.protobuf.Int32Value",
    "google.protobuf.Int64Value",
    "google.protobuf.StringValue",
    "google.protobuf.UInt32Value",
    "google.protobuf.UInt64Value",
];

/// What a case declares to cel-emit beyond its own inputs: the suite's enums and messages. Each
/// enum's values are named qualified, and, where the container names their package, as the
/// container resolves them. The messages are the container's package's.
pub fn declared(registry: &ProstProtoRegistry, container: &str) -> Decls {
    let mut decls = Decls::default();
    let pkg = PACKAGES.into_iter().find(|p| *p == container);
    for package in PACKAGES {
        for name in [
            format!("{package}.GlobalEnum"),
            format!("{package}.{TEST_ALL_TYPES}.NestedEnum"),
        ] {
            let Some(e) = registry.get_enum(&name) else {
                continue;
            };
            let short = name.strip_prefix(&format!("{package}.")).unwrap_or(&name);
            let values: BTreeMap<String, i32> = e
                .values()
                .map(|v| (v.name().to_string(), v.number()))
                .collect();
            decls.enums.insert(name.clone(), values.clone());
            if pkg == Some(package) {
                decls.enums.insert(short.to_string(), values);
            }
            for v in e.values() {
                let at = || At::Constant(Typed::new(Type::Int, Node::Int(i64::from(v.number()))));
                let qualified = format!("{name}.{}", v.name());
                decls.names.insert(
                    qualified,
                    Name {
                        ty: Type::Int,
                        at: at(),
                    },
                );
                if pkg == Some(package) {
                    let inside = format!("{short}.{}", v.name());
                    decls.names.insert(
                        inside,
                        Name {
                            ty: Type::Int,
                            at: at(),
                        },
                    );
                }
            }
        }
    }
    if let Some(pkg) = pkg {
        for name in [
            TEST_ALL_TYPES.to_string(),
            format!("{TEST_ALL_TYPES}.NestedMessage"),
        ] {
            let Some(msg) = registry.get_message(&format!("{pkg}.{name}")) else {
                continue;
            };
            if let Some(ty) = message(&msg, &mut Vec::new()) {
                decls.messages.insert(name, ty);
            }
        }
    }
    decls
}

/// `msg` as a message of the fields cel-emit has a type for. A message already being built is
/// recursive: none of its messages are declared here.
fn message(msg: &MessageDescriptor, open: &mut Vec<String>) -> Option<Type> {
    let full = msg.full_name().to_string();
    if open.contains(&full) {
        return None;
    }
    open.push(full);
    let fields = msg
        .fields()
        // An unset field reads zero; one with a declared default (proto2's) would read that
        // instead, so it is left out and a case reading it is refused.
        .filter(|f| f.field_descriptor_proto().default_value.is_none())
        .filter_map(|f| {
            Some(Field {
                name: f.name().to_string(),
                ty: field(&f, open)?,
                required: false,
            })
        })
        .collect();
    open.pop();
    Some(Type::Message(fields))
}

/// `f` as a field's type: a list, a map, or one value, each number at its proto width.
fn field(f: &FieldDescriptor, open: &mut Vec<String>) -> Option<Type> {
    if f.is_map() {
        let Kind::Message(entry) = f.kind() else {
            return None;
        };
        let key = kind(&entry.get_field_by_name("key")?.kind(), open)?;
        let value = kind(&entry.get_field_by_name("value")?.kind(), open)?;
        return Some(Type::Map(Box::new(key), Box::new(value)));
    }
    let one = kind(&f.kind(), open)?;
    Some(if f.is_list() { Type::list(one) } else { one })
}

/// One value of `k`'s kind.
fn kind(k: &Kind, open: &mut Vec<String>) -> Option<Type> {
    Some(match k {
        Kind::Bool => Type::Bool,
        Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => Type::Narrow(Width::I32),
        Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => Type::Int,
        Kind::Uint32 | Kind::Fixed32 => Type::Narrow(Width::U32),
        Kind::Uint64 | Kind::Fixed64 => Type::Uint,
        Kind::Bytes => Type::Bytes,
        Kind::Double => Type::Double,
        Kind::Float => Type::Narrow(Width::F32),
        Kind::String => Type::String,
        // A null enum is null in CEL, not an int.
        Kind::Enum(e) if e.full_name() == "google.protobuf.NullValue" => return None,
        Kind::Enum(_) => Type::Narrow(Width::I32),
        Kind::Message(m) => match m.full_name() {
            "google.protobuf.Timestamp" => Type::Timestamp,
            "google.protobuf.Duration" => Type::Duration,
            // A wrapper is null or its scalar.
            name if WRAPPERS.contains(&name) => {
                let value = m.get_field_by_name("value")?;
                Type::Nullable(Box::new(kind(&value.kind(), open)?))
            }
            // Any, Struct and the rest are dynamic, not messages.
            name if name.starts_with("google.protobuf.") => return None,
            _ => message(m, open)?,
        },
    })
}
