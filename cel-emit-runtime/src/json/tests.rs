use super::*;
use crate::dynamic::{get, size};

fn dec(text: &str) -> Result<Dyn> {
    decode(text)
}

fn enc(x: Dyn) -> Result<String> {
    encode(&x)
}

fn round(text: &str) -> Result<String> {
    enc(dec(text)?)
}

#[test]
fn documents_round_trip() {
    for text in [
        "null",
        "true",
        "-3",
        "1.5",
        r#""héllo \" \n""#,
        "[]",
        "{}",
        r#"{"a":[1,{"b":null},[]],"c":{"d":"e"}}"#,
        "[[[[1]]]]",
    ] {
        assert_eq!(round(text).as_deref(), Ok(text), "{text}");
    }
    assert_eq!(round(" [ 1 , 2 ] ").as_deref(), Ok("[1,2]"));
}

#[test]
fn null_and_absent_differ() {
    let m = dec(r#"{"a":null}"#).unwrap();
    assert!(matches!(get(&m, "a"), Ok(Dyn::Null)));
    assert!(matches!(get(&m, "b"), Err(Error::NoSuchKey)));
    assert_eq!(round("{}").as_deref(), Ok("{}"));
    assert_eq!(size(&m), Ok(1));
}

#[test]
fn numbers_keep_their_kind_at_the_boundaries() {
    assert!(matches!(
        dec("9007199254740993"),
        Ok(Dyn::Int(9_007_199_254_740_993))
    ));
    assert_eq!(round("9007199254740993").as_deref(), Ok("9007199254740993"));
    assert!(matches!(dec("9223372036854775807"), Ok(Dyn::Int(i64::MAX))));
    assert!(matches!(
        dec("-9223372036854775808"),
        Ok(Dyn::Int(i64::MIN))
    ));
    assert!(matches!(
        dec("9223372036854775808"),
        Ok(Dyn::UInt(9_223_372_036_854_775_808))
    ));
    assert!(matches!(
        dec("18446744073709551615"),
        Ok(Dyn::UInt(u64::MAX))
    ));
    assert!(matches!(dec("18446744073709551616"), Ok(Dyn::Double(_))));
    assert!(matches!(dec("-9223372036854775809"), Ok(Dyn::Double(_))));
    assert!(matches!(dec("1.0"), Ok(Dyn::Double(d)) if d == 1.0));
    assert!(matches!(dec("1e3"), Ok(Dyn::Double(d)) if d == 1000.0));
    assert!(matches!(dec("0"), Ok(Dyn::Int(0))));
    assert_eq!(
        round("18446744073709551615").as_deref(),
        Ok("18446744073709551615")
    );
}

#[test]
fn a_double_reads_to_the_nearest_and_writes_shortest() {
    assert!(matches!(dec("0.1"), Ok(Dyn::Double(d)) if d == 0.1));
    assert!(matches!(
        dec("1.7976931348623157e308"),
        Ok(Dyn::Double(d)) if d == f64::MAX
    ));
    assert_eq!(
        round("0.30000000000000004").as_deref(),
        Ok("0.30000000000000004")
    );
    assert_eq!(
        enc(Dyn::Double(9.300000190734863)).as_deref(),
        Ok("9.300000190734863")
    );
}

#[test]
fn a_float32_writes_its_own_digits() {
    assert_eq!(enc(Dyn::Float32(9.3)).as_deref(), Ok("9.3"));
    assert_eq!(
        enc(Dyn::from(f64::from(9.3_f32))).as_deref(),
        Ok("9.300000190734863")
    );
}

#[test]
fn what_json_cannot_hold_is_refused() {
    for x in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(enc(Dyn::Double(x)), Err(Error::Range));
        assert_eq!(enc(Dyn::Float32(x as f32)), Err(Error::Range));
    }
    let nested = Dyn::List(vec![Dyn::Double(f64::NAN)]);
    assert_eq!(enc(nested), Err(Error::Range));
}

#[test]
fn malformed_text_is_a_cast() {
    for bad in [
        "",
        "{",
        "[1,]",
        "{\"a\":}",
        "nul",
        "01",
        "1 2",
        "'a'",
        "{\"a\":1,}",
        "NaN",
        "\"\\x\"",
    ] {
        assert_eq!(dec(bad).err(), Some(Error::Cast), "{bad:?}");
    }
    let deep = format!("{}{}", "[".repeat(1000), "]".repeat(1000));
    assert_eq!(dec(&deep).err(), Some(Error::Cast));
}

#[test]
fn a_repeated_key_keeps_its_last_value() {
    assert_eq!(round(r#"{"a":1,"a":2}"#).as_deref(), Ok(r#"{"a":2}"#));
    assert_eq!(
        round(r#"{"a":1,"b":0,"a":{"c":1,"c":2}}"#).as_deref(),
        Ok(r#"{"a":{"c":2},"b":0}"#)
    );
    assert!(dec(r#"{"a":{"a":1}}"#).is_ok());
}

#[test]
fn nested_values_read_and_keep_order() {
    let x = dec(r#"{"z":[1,2,{"k":"v"}],"a":true}"#).unwrap();
    assert_eq!(size(&x), Ok(2));
    let list = get(&x, "z").unwrap();
    assert_eq!(size(list), Ok(3));
    assert_eq!(round(r#"{"z":1,"a":2}"#).as_deref(), Ok(r#"{"z":1,"a":2}"#));
}

#[test]
fn the_entries_without_some_keys_encode_as_one_object() {
    let without = |text: &str, keys: &[&str]| {
        let keys: Vec<String> = keys.iter().map(|k| (*k).to_string()).collect();
        encode_without(&dec(text).unwrap(), &keys).unwrap()
    };
    let doc = r#"{"a":1,"x":2.50,"$k":{"n":[1,null]},"b":"s"}"#;
    assert_eq!(
        without(doc, &["a", "b"]).as_deref(),
        Some(r#"{"x":2.5,"$k":{"n":[1,null]}}"#)
    );
    assert_eq!(
        without(doc, &[]).as_deref(),
        Some(r#"{"a":1,"x":2.5,"$k":{"n":[1,null]},"b":"s"}"#)
    );
    assert_eq!(without(doc, &["a", "x", "$k", "b", "zz"]), None);
    assert_eq!(without("{}", &[]), None);
    let list = dec("[1]").unwrap();
    assert!(encode_without(&list, &[]).is_err_and(|e| e == Error::Cast));
}

#[test]
fn a_rest_entry_writes_its_keys_into_the_map_holding_it_as_the_text_has_them() {
    let entry = |key: &str, value: Dyn| (key.to_string(), value);
    let rest = |text: &str| rest(text.to_string()).unwrap();
    let held = |text: &str| {
        let entries = vec![
            entry("a", Dyn::Int(1)),
            entry("$x", rest(text)),
            entry("b", Dyn::Null),
        ];
        enc(Dyn::Map(entries))
    };
    assert_eq!(
        held(r#"{"z":[1, 2.50],"a":9,"b":8}"#).as_deref(),
        Ok(r#"{"a":1,"b":null,"z":[1, 2.50]}"#)
    );
    assert_eq!(
        held(r#"{"y":1,"x\"":true,"y":2,"c":{"d":[]}}"#).as_deref(),
        Ok(r#"{"a":1,"b":null,"c":{"d":[]},"x\"":true,"y":2}"#)
    );
    assert_eq!(held("[1]").as_deref(), Ok(r#"{"a":1,"b":null}"#));
    assert_eq!(held("").as_deref(), Ok(r#"{"a":1,"b":null}"#));
    assert_eq!(held(r#"{"z":1} x"#).as_deref(), Ok(r#"{"a":1,"b":null}"#));
    assert_eq!(held(r#"{"z":01}"#).as_deref(), Ok(r#"{"a":1,"b":null}"#));
    assert_eq!(enc(rest("{}")), Err(Error::Cast));
}

#[test]
fn text_cuts_a_failed_member_and_an_empty_nested_object_back_out() {
    let mut t = Text::default();
    t.open();
    t.key("\"a\":");
    t.value(Dyn::Int(1)).unwrap();
    let m = t.key("\"nan\":");
    assert_eq!(t.value(Dyn::Double(f64::NAN)), Err(Error::Range));
    t.cut(m);
    let m = t.key("\"o\":");
    t.open();
    assert!(!t.close_or_cut(m));
    let m = t.key("\"p\":");
    t.open();
    t.key("\"b\":");
    t.value(Dyn::Bool(true)).unwrap();
    assert!(t.close_or_cut(m));
    t.rest(String::from(r#"{"a":9,"z":[1, 2]}"#), &[("a", true)])
        .unwrap();
    t.close();
    assert_eq!(t.done(), r#"{"a":1,"p":{"b":true},"z":[1, 2]}"#);
}

#[test]
fn text_writes_items_and_scalars_in_place() {
    let mut t = Text::default();
    t.open_list();
    for i in 0..2 {
        t.item();
        let mark = t.mark();
        t.open();
        assert!(t.bare(mark));
        t.key("\"s\":");
        t.str("a\"b").unwrap();
        t.key("\"n\":");
        t.int(-i).unwrap();
        t.key("\"u\":");
        t.uint(7).unwrap();
        t.key("\"d\":");
        t.double(1.5).unwrap();
        t.key("\"b\":");
        t.bool(true).unwrap();
        assert!(!t.bare(mark));
        t.close();
    }
    assert_eq!(t.double(f64::NAN), Err(Error::Range));
    t.close_list();
    let one = r#"{"s":"a\"b","n":0,"u":7,"d":1.5,"b":true}"#;
    let two = one.replace(r#""n":0"#, r#""n":-1"#);
    assert_eq!(t.done(), format!("[{one},{two}]"));
}

#[test]
fn text_rest_alone_opens_without_a_comma_and_keeps_a_key_left_unwritten() {
    let mut t = Text::default();
    t.open();
    t.rest(String::from(r#"{"a":9}"#), &[("a", false)]).unwrap();
    t.rest(String::from("[1]"), &[]).unwrap();
    t.close();
    assert_eq!(t.done(), r#"{"a":9}"#);
}

#[test]
fn a_time_writes_its_text() {
    let at = crate::time::timestamp_at(1_700_000_000, 500_000_000).unwrap();
    let span = crate::time::duration_at(1, 500_000_000).unwrap();
    let list = Dyn::List(vec![Dyn::from(at), Dyn::from(span)]);
    assert_eq!(
        encode(&list).as_deref(),
        Ok(r#"["2023-11-14T22:13:20.500Z","1.5s"]"#)
    );
}
