use chrono::{DateTime, TimeDelta, Utc};
use serde::Deserialize;

use super::*;

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Big,
}

#[derive(Debug, PartialEq, Deserialize)]
struct Inner {
    n: u8,
}

#[derive(Debug, PartialEq, Deserialize)]
struct Message {
    name: String,
    count: i64,
    ratio: f64,
    kind: Kind,
    at: DateTime<Utc>,
    tags: Vec<String>,
    inner: Inner,
    items: Vec<Inner>,
    maybe: Option<bool>,
    absent: Option<bool>,
    #[serde(default)]
    missing: u32,
}

fn entry(k: &str, v: impl Into<Dyn>) -> (String, Dyn) {
    (k.into(), v.into())
}

#[test]
fn a_map_fills_a_derived_struct() {
    let at = DateTime::from_timestamp(86_400, 0).unwrap_or_default();
    let n = |n: i64| Dyn::Map(vec![entry("n", n)]);
    let x = Dyn::Map(vec![
        entry("name", "a".to_string()),
        entry("count", -3_i64),
        entry("ratio", 1.5_f32),
        entry("kind", "big".to_string()),
        entry("at", at),
        entry("tags", Dyn::List(vec![Dyn::Str("t".into())])),
        entry("inner", n(7)),
        entry("items", Dyn::List(vec![n(1), n(2)])),
        entry("maybe", true),
        entry("absent", Dyn::Null),
    ]);
    let got: Message = to(&x).unwrap_or_else(|_| panic!("reads"));
    assert_eq!(got.name, "a");
    assert_eq!((got.count, got.ratio, got.kind), (-3, 1.5, Kind::Big));
    assert_eq!((got.at, got.tags), (at, vec!["t".to_string()]));
    assert_eq!(got.inner, Inner { n: 7 });
    assert_eq!(got.items, vec![Inner { n: 1 }, Inner { n: 2 }]);
    assert_eq!((got.maybe, got.absent, got.missing), (Some(true), None, 0));
}

#[test]
fn a_list_fills_a_vec() {
    let xs = Dyn::List(vec![Dyn::Int(1), Dyn::Int(2)]);
    assert_eq!(to::<Vec<i64>>(&xs), Ok(vec![1, 2]));
}

#[test]
fn time_reads_as_its_json_text() {
    let d: String = to(&Dyn::Duration(TimeDelta::seconds(90))).unwrap_or_default();
    assert_eq!(d, duration_text(TimeDelta::seconds(90)));
}

#[test]
fn a_wrong_kind_is_a_cast() {
    assert_eq!(to::<Inner>(&Dyn::Int(1)), Err(Error::Cast));
    assert_eq!(
        to::<Inner>(&Dyn::Map(vec![entry("n", "x".to_string())])),
        Err(Error::Cast)
    );
    assert_eq!(to::<u8>(&Dyn::Double(1.5)), Err(Error::Cast));
}

#[test]
fn a_number_past_the_width_is_a_range() {
    assert_eq!(
        to::<Inner>(&Dyn::Map(vec![entry("n", 300_i64)])),
        Err(Error::Range)
    );
    assert_eq!(to::<u32>(&Dyn::Int(-1)), Err(Error::Range));
    assert_eq!(to::<i8>(&Dyn::Double(200.0)), Err(Error::Range));
    assert_eq!(to::<f32>(&Dyn::Double(1e300)), Err(Error::Range));
    assert_eq!(to::<f32>(&Dyn::Double(1.5)), Ok(1.5));
}

#[test]
fn whole_numbers_and_doubles_cross_in_a_read() {
    assert_eq!(to::<f64>(&Dyn::Int(5)), Ok(5.0));
    assert_eq!(to::<f32>(&Dyn::Int(5)), Ok(5.0));
    assert_eq!(to::<i64>(&Dyn::Double(5.0)), Ok(5));
    assert_eq!(to::<u8>(&Dyn::Double(5.0)), Ok(5));
}
