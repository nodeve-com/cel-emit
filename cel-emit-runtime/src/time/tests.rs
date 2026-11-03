//! The time accessors and conversions, tested.

use super::*;

/// 2026-08-16T23:30:45.123Z, a Sunday.
const AT: i64 = 1_786_923_045_123;

fn at() -> DateTime<Utc> {
    timestamp_from_millis(AT).unwrap_or_default()
}

#[test]
fn fields_count_as_cel_does() {
    let f = |which| field(at(), 0, which);
    assert_eq!(f(Field::FullYear), Ok(2026));
    assert_eq!(f(Field::Month), Ok(7));
    assert_eq!(f(Field::DayOfMonth), Ok(15));
    assert_eq!(f(Field::Date), Ok(16));
    assert_eq!(f(Field::DayOfYear), Ok(227));
    assert_eq!(f(Field::DayOfWeek), Ok(0));
    assert_eq!(f(Field::Hours), Ok(23));
    assert_eq!(f(Field::Minutes), Ok(30));
    assert_eq!(f(Field::Seconds), Ok(45));
    assert_eq!(f(Field::Milliseconds), Ok(123));
}

#[test]
fn an_offset_moves_the_civil_fields_across_midnight() {
    // +01:00: Monday 00:30.
    assert_eq!(field(at(), 3600, Field::Hours), Ok(0));
    assert_eq!(field(at(), 3600, Field::DayOfWeek), Ok(1));
    let before = timestamp_from_millis(-1).unwrap_or_default();
    assert_eq!(field(before, 0, Field::Milliseconds), Ok(999));
    let table = zone::encode(&[(0, 3600)], AT);
    let earlier = timestamp_from_millis(AT - 1).unwrap_or_default();
    assert_eq!(hours_in(earlier, &table), Ok(0));
    assert_eq!(hours_in(at(), &table), Err(Error::Function));
}

#[test]
fn a_duration_counts_whole_units_toward_zero() {
    let d = |ms| duration_from_millis(ms).unwrap_or_default();
    assert_eq!(duration_hours(d(5_400_000)), Ok(1));
    assert_eq!(duration_hours(d(-5_400_000)), Ok(-1));
    assert_eq!(duration_milliseconds(d(1500)), Ok(500));
    assert_eq!(duration_milliseconds(d(-123_321)), Ok(-321));
}

#[test]
fn past_cels_range_overflows() {
    let last = timestamp_at(MAX_SECONDS, 999_000_000);
    assert!(last.is_ok());
    let second = duration_from_millis(1000).unwrap_or_default();
    assert_eq!(add(last.unwrap_or_default(), second), Err(Error::Overflow));
    assert_eq!(from_seconds(MIN_SECONDS - 1), Err(Error::Overflow));
    assert!(duration_at(MAX_DURATION_SECONDS, 999_999_999).is_ok());
    assert_eq!(
        duration_at(MAX_DURATION_SECONDS + 1, 0),
        Err(Error::Overflow)
    );
}

/// Year 0000, a value chrono holds and CEL does not.
#[test]
fn a_year_before_cels_range_gives_a_value_or_an_error() {
    use crate::cmp::{Equal, Order};
    let year_zero = DateTime::from_timestamp(MIN_SECONDS - 86_400, 0).unwrap();
    let inside = at();
    assert_eq!(year_zero.order(&inside), Ok(Some(std::cmp::Ordering::Less)));
    assert_eq!(year_zero.equal(&year_zero), Ok(true));
    assert!(between(year_zero, inside).is_ok());
    assert!(between(inside, year_zero).is_ok());
    assert_eq!(
        timestamp_string(year_zero).as_deref(),
        Ok("0000-12-31T00:00:00Z")
    );
    assert_eq!(full_year(year_zero), Ok(0));
    let table = zone::encode(&[(0, 0)], MAX_SECONDS * 1000);
    assert_eq!(hours_in(year_zero, &table), Err(Error::Function));
    assert!(add(year_zero, TimeDelta::seconds(1)).is_err());
    assert!(subtract(year_zero, TimeDelta::seconds(1)).is_err());
}

/// Spans out to chrono's own ends, which CEL does not hold.
#[test]
fn spans_past_cels_range_give_a_value_or_an_error() {
    use crate::cmp::{Equal, Order};
    let year_zero = DateTime::from_timestamp(MIN_SECONDS - 86_400, 0).unwrap();
    let inside = at();
    let spans = [
        TimeDelta::MAX,
        TimeDelta::MIN,
        TimeDelta::seconds(MAX_DURATION_SECONDS + 1),
        TimeDelta::seconds(-MAX_DURATION_SECONDS - 1),
    ];
    for span in spans {
        assert!(span.order(&TimeDelta::zero()).is_ok());
        assert_eq!(span.equal(&span), Ok(true));
        assert!(duration_string(span).is_ok());
        assert_eq!(add(inside, span), Err(Error::Overflow));
        assert_eq!(subtract(inside, span), Err(Error::Overflow));
        assert!(matches!(add(year_zero, span), Ok(_) | Err(Error::Overflow)));
        assert_eq!(add_durations(span, span), Err(Error::Overflow));
        assert_eq!(subtract_durations(span, span), Ok(TimeDelta::zero()));
        assert!(duration_hours(span).is_ok());
        assert!(duration_seconds(span).is_ok());
        assert!(duration_milliseconds(span).is_ok());
        assert!(parse_duration(&duration_text(span)).is_err());
    }
}
