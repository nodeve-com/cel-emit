use super::*;
use serde_json::{json, Value};

fn map(schema: Value) -> Result<Type, Error> {
    let (s, root) = schema_emit::json_schema::lower("m", &schema, &Default::default(), &[])?;
    record(&s, root)
}

fn one(prop: Value) -> Result<Field, Error> {
    let ty = map(json!({"type": "object", "properties": {"a": prop}, "required": ["a"]}))?;
    Ok(ty.fields()[0].clone())
}

fn refused(prop: Value, why: &str) {
    let e = one(prop).expect_err(why);
    assert!(e.message.contains(why), "{}", e.message);
}

#[test]
fn each_width() {
    let ints = [
        ("int8", Type::Narrow(Width::I8)),
        ("int16", Type::Narrow(Width::I16)),
        ("int32", Type::Narrow(Width::I32)),
        ("int64", Type::Int),
        ("uint8", Type::Narrow(Width::U8)),
        ("uint16", Type::Narrow(Width::U16)),
        ("uint32", Type::Narrow(Width::U32)),
        ("uint64", Type::Uint),
    ];
    for (format, ty) in ints {
        let f = one(json!({"type": "integer", "format": format})).unwrap();
        assert_eq!((f.ty, f.required), (ty, true), "{format}");
    }
    assert_eq!(one(json!({"type": "integer"})).unwrap().ty, Type::Int);
    let f = one(json!({"type": "number", "format": "float"})).unwrap();
    assert_eq!(f.ty, Type::Narrow(Width::F32));
    assert_eq!(one(json!({"type": "number"})).unwrap().ty, Type::Double);
}

#[test]
fn scalars_and_containers() {
    assert_eq!(one(json!({"type": "boolean"})).unwrap().ty, Type::Bool);
    assert_eq!(one(json!({"type": "string"})).unwrap().ty, Type::String);
    let list = json!({"type": "array", "items": {"type": "string"}});
    assert_eq!(one(list).unwrap().ty, Type::list(Type::String));
    let same = json!({"type": "array", "prefixItems": [{"type": "number"}, {"type": "number"}],
        "items": false});
    assert_eq!(one(same).unwrap().ty, Type::list(Type::Double));
}

/// The front end does not lower maps yet, so these are built as the IR.
#[test]
fn maps() {
    let model = |item: Ty| {
        let field = schema_emit::ir::Field {
            name: "a".into(),
            ty: Ty::of(Base::Map(Box::new(item))),
            required: true,
        };
        let model = schema_emit::ir::Model {
            name: "m".into(),
            fields: vec![field],
            checks: vec![],
            others: None,
            additional: schema_emit::ir::Additional::Fail,
        };
        record(
            &Schema {
                models: vec![model],
                warnings: vec![],
            },
            0,
        )
    };
    let ok = model(Ty::of(Base::Int(Int::I64))).unwrap();
    assert_eq!(ok.fields()[0].ty, Type::map(Type::Int));
    let narrow = model(Ty::of(Base::Float(Float::F32))).unwrap();
    assert_eq!(narrow.fields()[0].ty, Type::map(Type::Narrow(Width::F32)));
}

#[test]
fn a_list_holds_its_elements_width() {
    let list = json!({"type": "array", "items": {"type": "integer", "format": "uint8"}});
    assert_eq!(one(list).unwrap().ty, Type::list(Type::Narrow(Width::U8)));
    let tuple = json!({"type": "array", "prefixItems": [{"type": "number", "format": "float"}],
        "items": {"type": "number", "format": "float"}});
    assert_eq!(one(tuple).unwrap().ty, Type::list(Type::Narrow(Width::F32)));
}

#[test]
fn nullable_and_optional() {
    let f = one(json!({"type": ["integer", "null"], "format": "uint8"})).unwrap();
    assert_eq!(f.ty, Type::Nullable(Box::new(Type::Narrow(Width::U8))));
    let ty = map(json!({"type": "object", "properties": {"a": {"type": "string"}}})).unwrap();
    assert!(!ty.fields()[0].required);
}

#[test]
fn nested_model() {
    let ty = map(json!({"type": "object", "required": ["in"], "properties": {
        "in": {"type": "object", "properties": {"x": {"type": "integer", "format": "int16"}}}}}))
    .unwrap();
    let inner = Type::message(vec![Field {
        name: "x".into(),
        ty: Type::Optional(Box::new(Type::Narrow(Width::I16))),
        required: false,
    }]);
    assert_eq!(ty.fields()[0].ty, inner);
}

#[test]
fn refusals() {
    refused(json!(true), "any value");
    refused(json!({"type": "null"}), "null alone");
    refused(
        json!({"type": "array", "prefixItems": [{"type": "string"}, {"type": "integer"}]}),
        "mixed-kind tuple has no type",
    );
    let e = map(json!({"type": "object", "properties": {"b": {"type": "null"}}})).unwrap_err();
    assert!(e.message.starts_with("m.b:"), "{}", e.message);
}

#[test]
fn a_model_holding_itself_is_refused() {
    let field = schema_emit::ir::Field {
        name: "next".into(),
        ty: Ty::of(Base::Model(0)),
        required: false,
    };
    let model = schema_emit::ir::Model {
        name: "n".into(),
        fields: vec![field],
        checks: vec![],
        others: None,
        additional: schema_emit::ir::Additional::Fail,
    };
    let schema = Schema {
        models: vec![model],
        warnings: vec![],
    };
    let e = record(&schema, 0).unwrap_err();
    assert!(e.message.contains("holding itself"), "{}", e.message);
}

#[test]
fn a_root_that_is_no_object_is_the_type_it_holds() {
    assert_eq!(
        map(json!({"type": "integer", "minimum": 0})).unwrap(),
        Type::Int
    );
    let items = json!({"type": "array", "items": {"type": "string"}});
    assert_eq!(map(items).unwrap(), Type::list(Type::String));
    let (s, root) =
        schema_emit::json_schema::lower("m", &json!({"type": "boolean"}), &Default::default(), &[])
            .unwrap();
    assert!(s.models[root].is_value());
}
