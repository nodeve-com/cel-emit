use super::*;

const INNER: Shape = Shape::Message(&[("k", 6, Shape::Int)]);
const DEEP: Shape = Shape::Message(&[("m", 8, INNER_DEEP)]);
const INNER_DEEP: Shape = Shape::Message(&[("j", 9, Shape::Int)]);
const KEYS: &[Key] = &[
    ("i", 1, Shape::Int),
    ("u", 2, Shape::Uint),
    ("d", 3, Shape::Double),
    ("s", 4, Shape::Plain),
    ("n", 5, INNER),
    ("o", 7, DEEP),
];
static SHAPE: Shape = Shape::Message(KEYS);

fn dec(text: &str) -> Result<Slots> {
    decode_shaped(text, &SHAPE, 10)
}

/// The slots `dec` fills.
fn filled(x: &Slots) -> usize {
    x.iter().filter(|slot| slot.is_some()).count()
}

fn field_of(text: &str, at: i64) -> Result<Dyn> {
    field(&dec(text)?, 0, at)
}

#[test]
fn only_declared_keys_are_kept() {
    let x = dec(r#"{"x":{"a":[1,{"b":null}]},"i":3,"y":1e999,"s":"t","z":[[]]}"#).unwrap();
    assert_eq!(filled(&x), 3);
    assert!(matches!(field(&x, 0, 1), Ok(Dyn::Int(3))));
    assert!(matches!(field(&x, 0, 2), Err(Error::NoSuchKey)));
    assert_eq!(filled(&dec("{}").unwrap()), 1);
}

#[test]
fn a_repeated_key_keeps_its_last_value_declared_or_not() {
    let x = dec(r#"{"x":{"k":1,"k":2},"i":1,"x":[],"i":2,"n":{"k":1,"k":4}}"#).unwrap();
    assert_eq!(filled(&x), 4);
    assert!(matches!(field(&x, 0, 1), Ok(Dyn::Int(2))));
    assert!(matches!(field(&x, 5, 6), Ok(Dyn::Int(4))));
}

#[test]
fn a_repeated_message_keeps_only_its_last_value() {
    let x = dec(r#"{"o":{"m":{"j":1}},"o":{"z":1}}"#).unwrap();
    assert_eq!(has_field(&x, 7, 8), Ok(false));
    assert!(matches!(field(&x, 8, 9), Err(Error::NoSuchKey)));
    let x = dec(r#"{"o":1,"o":{"m":{"j":2}}}"#).unwrap();
    assert!(matches!(field(&x, 8, 9), Ok(Dyn::Int(2))));
}

#[test]
fn a_number_is_read_as_its_field_has_it() {
    assert!(matches!(field_of(r#"{"i":2.0}"#, 1), Ok(Dyn::Int(2))));
    assert!(matches!(field_of(r#"{"i":1e2}"#, 1), Ok(Dyn::Int(100))));
    assert!(matches!(field_of(r#"{"i":-0.0}"#, 1), Ok(Dyn::Int(0))));
    assert!(matches!(field_of(r#"{"i":2.5}"#, 1), Ok(Dyn::Double(_))));
    assert!(matches!(field_of(r#"{"i":1e30}"#, 1), Ok(Dyn::Double(_))));
    assert!(matches!(
        field_of(r#"{"i":9223372036854775808}"#, 1),
        Ok(Dyn::UInt(_))
    ));
    assert!(matches!(field_of(r#"{"u":3.0}"#, 2), Ok(Dyn::UInt(3))));
    assert!(matches!(field_of(r#"{"u":3}"#, 2), Ok(Dyn::UInt(3))));
    assert!(matches!(field_of(r#"{"u":-1}"#, 2), Ok(Dyn::Int(-1))));
    assert!(matches!(field_of(r#"{"d":1}"#, 3), Ok(Dyn::Double(d)) if d == 1.0));
    assert!(matches!(field_of(r#"{"s":1}"#, 4), Ok(Dyn::Int(1))));
}

#[test]
fn a_nested_message_keeps_only_its_declared_keys() {
    let x = dec(r#"{"n":{"zz":[1],"k":6.0}}"#).unwrap();
    assert_eq!(filled(&x), 3);
    assert!(matches!(field(&x, 5, 6), Ok(Dyn::Int(6))));
}

#[test]
fn an_object_or_key_the_text_lacks_is_no_key_never_a_default() {
    let none = dec("{}").unwrap();
    assert_eq!(has_field(&none, 0, 5), Ok(false));
    assert!(matches!(field(&none, 5, 6), Err(Error::NoSuchKey)));
    assert!(matches!(optional_field(&none, 5, 6), Err(Error::NoSuchKey)));
    assert!(matches!(field(&none, 8, 9), Err(Error::NoSuchKey)));
    let empty = dec(r#"{"n":{}}"#).unwrap();
    assert_eq!(has_field(&empty, 5, 6), Ok(false));
    assert!(matches!(field(&empty, 5, 6), Err(Error::NoSuchKey)));
    assert!(matches!(optional_field(&empty, 5, 6), Ok(None)));
}

#[test]
fn a_value_of_another_kind_is_left_for_the_read_to_refuse() {
    assert!(matches!(field_of(r#"{"n":1}"#, 5), Ok(Dyn::Int(1))));
    assert!(matches!(field_of(r#"{"i":null}"#, 1), Ok(Dyn::Null)));
    assert!(matches!(
        field_of(r#"{"s":[1,{"a":2}]}"#, 4),
        Ok(Dyn::List(_))
    ));
    let top = dec("[1]").unwrap();
    assert!(matches!(field(&top, 0, 1), Err(Error::Cast)));
    assert!(matches!(has_field(&top, 0, 5), Err(Error::Cast)));
    let x = dec(r#"{"n":1,"o":null}"#).unwrap();
    assert!(matches!(field(&x, 5, 6), Err(Error::Cast)));
    assert!(matches!(optional_field(&x, 5, 6), Err(Error::Cast)));
    assert_eq!(has_field(&x, 7, 8), Err(Error::Cast));
    assert!(matches!(field(&x, 8, 9), Err(Error::Cast)));
}

#[test]
fn malformed_text_and_trailing_text_are_a_cast() {
    for bad in [
        "",
        "{",
        r#"{"i":}"#,
        r#"{"i":1,}"#,
        "{} {}",
        "1 2",
        r#"{"x":[1,],"i":1}"#,
    ] {
        assert_eq!(dec(bad).err(), Some(Error::Cast), "{bad:?}");
    }
}

#[test]
fn nesting_past_the_limit_is_a_cast_where_a_value_is_read() {
    let deep = format!("{}{}", "[".repeat(1000), "]".repeat(1000));
    assert_eq!(dec(&format!(r#"{{"s":{deep}}}"#)).err(), Some(Error::Cast));
}

#[test]
fn a_skipped_value_is_not_built_so_its_depth_costs_nothing() {
    let deep = format!("{}{}", "[".repeat(1000), "]".repeat(1000));
    assert!(dec(&format!(r#"{{"x":{deep},"i":4}}"#)).is_ok());
}

static OPEN: Shape = Shape::Open(&[("a", 1, Shape::Int)], 2);

fn open(text: &str) -> Slots {
    decode_shaped(text, &OPEN, 3).unwrap()
}

fn rest(x: Slots) -> Option<String> {
    match field(&x, 0, 2) {
        Ok(Dyn::Str(s)) => Some(s),
        _ => None,
    }
}

#[test]
fn an_open_shape_keeps_the_other_keys_as_text_as_they_arrived() {
    let x = open(r#"{"a":1,"extra":20,"x":{"y":[1, 2.50]},"n":null}"#);
    assert!(matches!(field(&x, 0, 1), Ok(Dyn::Int(1))));
    let text = rest(x).unwrap();
    assert_eq!(text, r#"{"extra":20,"n":null,"x":{"y":[1, 2.50]}}"#);
}

#[test]
fn an_open_shape_with_no_other_key_has_no_text_and_a_repeated_one_keeps_its_last() {
    assert_eq!(rest(open(r#"{"a":1}"#)), None);
    let x = open(r#"{"z":1,"a":2,"z":[3]}"#);
    assert_eq!(rest(x).as_deref(), Some(r#"{"z":[3]}"#));
}
