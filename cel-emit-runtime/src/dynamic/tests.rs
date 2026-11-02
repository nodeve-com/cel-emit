use super::*;

fn text(s: &str) -> Dyn {
    Dyn::Str(s.to_string())
}

fn same(a: Dyn, b: Dyn) -> bool {
    a.equal(&b).unwrap()
}

#[test]
fn a_number_fills_only_its_own_kind() {
    assert_eq!(to_int(&Dyn::Int(-7)), Ok(-7));
    assert_eq!(to_uint(&Dyn::UInt(u64::MAX)), Ok(u64::MAX));
    assert_eq!(to_double(&Dyn::Double(1.5)), Ok(1.5));
    assert_eq!(to_double(&Dyn::Float32(9.3)), Ok(9.300000190734863));
    assert!(to_double(&Dyn::Double(f64::NAN)).is_ok_and(f64::is_nan));
    for other in [
        Dyn::Double(3.0),
        Dyn::Float32(2.0),
        Dyn::UInt(7),
        text("1"),
        Dyn::Null,
        Dyn::Bool(true),
    ] {
        assert_eq!(to_int(&other), Err(Error::Cast), "{other:?}");
    }
    for other in [Dyn::Int(5), Dyn::Double(4.0), text("1"), Dyn::Null] {
        assert_eq!(to_uint(&other), Err(Error::Cast), "{other:?}");
    }
    for other in [Dyn::Int(2), Dyn::UInt(3), text("1.5"), Dyn::Null] {
        assert_eq!(to_double(&other), Err(Error::Cast), "{other:?}");
    }
}

#[test]
fn an_optional_read_keeps_absence_and_a_nullable_one_takes_null() {
    assert_eq!(optional(None, to_int), Ok(None));
    assert_eq!(optional(Some(&Dyn::Int(1)), to_int), Ok(Some(1)));
    assert_eq!(optional(Some(&Dyn::Null), to_int), Err(Error::Cast));
    assert_eq!(nullable(None, to_int), Ok(None));
    assert_eq!(nullable(Some(&Dyn::Null), to_int), Ok(None));
    assert_eq!(nullable(Some(&Dyn::Int(1)), to_int), Ok(Some(1)));
    assert_eq!(nullable(Some(&Dyn::Bool(true)), to_int), Err(Error::Cast));
}

#[test]
fn other_kinds_take_only_themselves() {
    assert_eq!(to_bool(&Dyn::Bool(true)), Ok(true));
    assert_eq!(to_bool(&Dyn::Int(1)), Err(Error::Cast));
    assert_eq!(to_str(&text("a")).as_deref(), Ok("a"));
    assert_eq!(to_str(&Dyn::Int(1)).err(), Some(Error::Cast));
    assert!(to_list(&Dyn::Map(Vec::new())).is_err());
    assert!(to_map(&Dyn::List(Vec::new())).is_err());
    assert!(to_list(&Dyn::List(Vec::new())).is_ok());
}

#[test]
fn typed_values_lift_to_dyn() {
    let xs = from_list(vec![1_i64, 2]).unwrap();
    assert!(same(xs, Dyn::List(vec![Dyn::Int(1), Dyn::Int(2)])));
    let m = vec![(String::from("a"), 1.5_f64)];
    assert!(matches!(get(&from_map(m).unwrap(), "a"), Ok(Dyn::Double(d)) if *d == 1.5));
    assert!(matches!(Dyn::from(Null), Dyn::Null));
    assert!(matches!(Dyn::from(Some(1i64)), Dyn::Int(1)));
    assert!(matches!(Dyn::from(None::<i64>), Dyn::Null));
}

#[test]
fn get_and_size_read_maps_lists_and_strings() {
    let m = Dyn::Map(vec![(String::from("a"), Dyn::Int(1))]);
    assert!(matches!(get(&m, "a"), Ok(Dyn::Int(1))));
    assert_eq!(get(&m, "b").err(), Some(Error::NoSuchKey));
    assert_eq!(get(&Dyn::Int(1), "a").err(), Some(Error::Cast));
    assert_eq!(
        (size(&m), size(&text("héé")), size(&Dyn::Int(1))),
        (Ok(1), Ok(3), Err(Error::Cast))
    );
}

#[test]
fn numbers_are_equal_across_kinds() {
    assert!(same(Dyn::Int(1), Dyn::UInt(1)));
    assert!(same(Dyn::Int(1), Dyn::Double(1.0)));
    assert!(same(Dyn::UInt(1), Dyn::Float32(1.0)));
    assert!(!same(Dyn::Int(1), Dyn::Double(1.5)));
    assert!(!same(Dyn::Int(-1), Dyn::UInt(u64::MAX)));
    assert!(!same(Dyn::Double(f64::NAN), Dyn::Double(f64::NAN)));
    // 2^53 + 1 is not the double 2^53.
    assert!(!same(
        Dyn::Int(9_007_199_254_740_993),
        Dyn::Double(9_007_199_254_740_992.0)
    ));
    assert!(!same(
        Dyn::Int(i64::MAX),
        Dyn::Double(9_223_372_036_854_775_808.0)
    ));
    assert!(!same(
        Dyn::UInt(u64::MAX),
        Dyn::Double(1.8446744073709552e19)
    ));
}

#[test]
fn other_kinds_are_equal_only_to_their_own() {
    assert!(same(Dyn::Null, Dyn::Null));
    assert!(!same(Dyn::Null, Dyn::Bool(false)));
    assert!(!same(Dyn::Int(1), Dyn::Bool(true)));
    assert!(!same(Dyn::Int(1), text("1")));
    let a = Dyn::List(vec![Dyn::Int(1), text("x")]);
    let b = Dyn::List(vec![Dyn::Double(1.0), text("x")]);
    assert!(same(a.clone(), b));
    assert!(!same(a, Dyn::List(vec![Dyn::Int(1)])));
}

fn map(entries: &[(&str, Dyn)]) -> Dyn {
    Dyn::Map(
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}

fn key(k: &str) -> String {
    k.to_string()
}

#[test]
fn a_typed_collection_lifts_to_dynamic() {
    let xs = lift_list(Vec::from([1i64, 2])).unwrap();
    assert!(same(
        Dyn::List(xs),
        from_list(Vec::from([1i64, 2])).unwrap()
    ));
    let m = vec![(key("k"), 1.5f64)];
    assert!(same(from_map(m).unwrap(), map(&[("k", Dyn::Double(1.5))])));
    let none = lift_list::<f64>(Vec::new()).unwrap();
    assert_eq!(size(&Dyn::List(none)), Ok(0));
}

#[test]
fn a_dynamic_keyed_map_needs_string_keys() {
    let entry = |key: Dyn| vec![(key, Dyn::Int(1))];
    assert!(same(
        from_keyed(entry(text("k"))).unwrap(),
        map(&[("k", Dyn::Int(1))])
    ));
    assert_eq!(from_keyed(entry(Dyn::Int(1))).map(|_| ()), Err(Error::Cast));
    assert_eq!(size(&from_keyed(Vec::new()).unwrap()), Ok(0));
}

#[test]
fn a_dynamic_map_answers_has_and_optional_get() {
    let m = map(&[("a", Dyn::Int(1))]);
    assert_eq!(has(&m, "a"), Ok(true));
    assert_eq!(has(&m, "b"), Ok(false));
    assert_eq!(has(&Dyn::Int(1), "a"), Err(Error::Cast));
    assert!(optional_get(&m, "a").unwrap().is_some());
    assert!(optional_get(&m, "b").unwrap().is_none());
    assert_eq!(
        optional_get(&Dyn::Null, "a").map(|o| o.is_some()),
        Err(Error::Cast)
    );
    assert!(chain(None, "a").unwrap().is_none());
    assert!(chain(Some(&m), "a").unwrap().is_some());
}

#[test]
fn a_dynamic_list_indexes_by_int() {
    let xs = Dyn::List(vec![Dyn::Int(5), text("x")]);
    assert!(same(at(&xs, 1).unwrap().clone(), text("x")));
    assert!(at(&xs, 2).is_err());
    assert!(at(&xs, -1).is_err());
    assert_eq!(at(&Dyn::Int(1), 0).map(|_| ()), Err(Error::Cast));
}

#[test]
fn in_finds_an_element_or_a_string_key() {
    let xs = Dyn::List(vec![Dyn::Int(5), text("x")]);
    assert_eq!(contains(&Dyn::Double(5.0), &xs), Ok(true));
    assert_eq!(contains(&Dyn::Int(6), &xs), Ok(false));
    let m = map(&[("a", Dyn::Int(1))]);
    assert_eq!(contains(&text("a"), &m), Ok(true));
    assert_eq!(contains(&text("b"), &m), Ok(false));
    assert_eq!(contains(&Dyn::Int(1), &m), Ok(false));
    assert_eq!(contains(&Dyn::Int(1), &Dyn::Int(1)), Err(Error::Cast));
}

#[test]
fn a_cast_converts_by_the_value_kind() {
    use cast::{to_bool, to_double, to_int, to_str, to_uint};
    assert_eq!(to_int(&text("12")), Ok(12));
    assert_eq!(to_int(&Dyn::Double(2.9)), Ok(2));
    assert_eq!(to_int(&Dyn::Double(-2.9)), Ok(-2));
    assert_eq!(to_int(&Dyn::UInt(u64::MAX)), Err(Error::Overflow));
    assert!(to_int(&text("x")).is_err());
    assert_eq!(to_int(&Dyn::Bool(true)), Err(Error::Cast));
    assert_eq!(to_int(&Dyn::Null), Err(Error::Cast));
    assert_eq!(to_uint(&Dyn::Int(-1)), Err(Error::Overflow));
    assert_eq!(to_uint(&text("7")), Ok(7));
    assert_eq!(to_double(&Dyn::Int(2)), Ok(2.0));
    assert_eq!(to_double(&text("1.5")), Ok(1.5));
    assert_eq!(to_bool(&text("true")), Ok(true));
    assert_eq!(to_bool(&Dyn::Int(1)), Err(Error::Cast));
    assert_eq!(to_str(&Dyn::Int(-3)).as_deref(), Ok("-3"));
    assert_eq!(to_str(&Dyn::Bool(false)).as_deref(), Ok("false"));
    assert_eq!(to_str(&Dyn::Double(1.5)).as_deref(), Ok("1.5"));
    assert_eq!(to_str(&Dyn::Null).map(|_| ()), Err(Error::Cast));
}

#[test]
fn a_time_fills_its_own_place_from_itself_or_its_text() {
    let at = crate::time::timestamp_at(1_700_000_000, 5).unwrap();
    let span = crate::time::duration_at(1, 500_000_000).unwrap();
    assert_eq!(to_timestamp(&Dyn::from(at)), Ok(at));
    assert_eq!(
        to_timestamp(&text(&crate::time::timestamp_text(at))),
        Ok(at)
    );
    assert_eq!(to_duration(&Dyn::from(span)), Ok(span));
    assert_eq!(to_duration(&text("1.5s")), Ok(span));
    assert_eq!(to_str(&Dyn::from(at)), Err(Error::Cast));
    assert_eq!(to_timestamp(&Dyn::from(span)), Err(Error::Cast));
    assert_eq!(to_duration(&Dyn::from(at)), Err(Error::Cast));
}

#[test]
fn a_time_casts_as_cel_does_and_equals_only_its_own_kind() {
    let at = crate::time::timestamp_at(1_700_000_000, 0).unwrap();
    let span = crate::time::duration_at(90, 0).unwrap();
    assert_eq!(cast::to_int(&Dyn::from(at)), Ok(1_700_000_000));
    assert_eq!(
        cast::to_str(&Dyn::from(at)).as_deref(),
        Ok("2023-11-14T22:13:20Z")
    );
    assert_eq!(cast::to_str(&Dyn::from(span)).as_deref(), Ok("90s"));
    assert!(same(Dyn::from(at), Dyn::from(at)));
    assert!(!same(Dyn::from(at), text("2023-11-14T22:13:20Z")));
    assert!(!same(Dyn::from(span), text("90s")));
}
