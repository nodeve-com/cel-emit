//! Timestamps and durations: CEL's arithmetic on them and its accessors, as cel-go defines them,
//! each within CEL's range, past which it overflows. A timestamp's field is UTC's, or, given a
//! zone, read at the offset its [`zone`] table gives: cel-emit turns the zone a call names into
//! that table.
//!
//! As CEL counts: a month, a day of the month, and a day of the year from 0, `getDate` from 1, a
//! week from Sunday, 0. A duration's accessor is its whole length in that unit, toward zero,
//! but `getMilliseconds`, which is the part past its whole second.

use chrono::{DateTime, Datelike, NaiveDateTime, TimeDelta, Timelike, Utc};

use crate::{Error, Result};

mod parse;

pub use parse::{parse_duration, parse_timestamp};

pub mod zone;

/// CEL's earliest and latest timestamp, in whole seconds: 0001-01-01 and 9999-12-31T23:59:59.
pub const MIN_SECONDS: i64 = -62_135_596_800;
pub const MAX_SECONDS: i64 = 253_402_300_799;

/// CEL's longest duration either way, in whole seconds, its fraction aside.
pub const MAX_DURATION_SECONDS: i64 = 315_537_897_598;

/// A timestamp of `seconds` and `nanos` past the epoch.
pub fn timestamp_at(seconds: i64, nanos: u32) -> Result<DateTime<Utc>> {
    let t = DateTime::from_timestamp(seconds, nanos).ok_or(Error::Overflow)?;
    timestamp_within(t)
}

/// A timestamp of epoch ms.
pub fn timestamp_from_millis(ms: i64) -> Result<DateTime<Utc>> {
    timestamp_within(DateTime::from_timestamp_millis(ms).ok_or(Error::Overflow)?)
}

/// `t` as epoch ms, rounded down.
pub fn timestamp_millis(t: DateTime<Utc>) -> i64 {
    t.timestamp_millis()
}

/// `t` as RFC 3339 in UTC, with the fraction only where there is one.
pub fn timestamp_text(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
}

fn timestamp_within(t: DateTime<Utc>) -> Result<DateTime<Utc>> {
    let ok = (MIN_SECONDS..=MAX_SECONDS).contains(&t.timestamp());
    crate::check(t, ok, Error::Overflow)
}

/// A duration of `seconds` and `nanos`, both of one sign.
pub fn duration_at(seconds: i64, nanos: i32) -> Result<TimeDelta> {
    let whole = TimeDelta::try_seconds(seconds).ok_or(Error::Overflow)?;
    let d = whole
        .checked_add(&TimeDelta::nanoseconds(i64::from(nanos)))
        .ok_or(Error::Overflow)?;
    duration_within(d)
}

/// A duration of ms.
pub fn duration_from_millis(ms: i64) -> Result<TimeDelta> {
    duration_within(TimeDelta::try_milliseconds(ms).ok_or(Error::Overflow)?)
}

/// `d` as ms, toward zero.
pub fn duration_millis(d: TimeDelta) -> i64 {
    d.num_milliseconds()
}

/// `d` as seconds, with the fraction only where there is one: `1.5s`, `-7200s`.
pub fn duration_text(d: TimeDelta) -> String {
    let (secs, frac) = (d.num_seconds(), d.subsec_nanos());
    let sign = if secs < 0 || frac < 0 { "-" } else { "" };
    match frac {
        0 => format!("{sign}{}s", secs.unsigned_abs()),
        _ => {
            let digits = format!("{:09}", frac.unsigned_abs());
            format!(
                "{sign}{}.{}s",
                secs.unsigned_abs(),
                digits.trim_end_matches('0')
            )
        }
    }
}

fn duration_within(d: TimeDelta) -> Result<TimeDelta> {
    // Whole seconds toward zero: the fraction never reaches the next second.
    let ok = d.num_seconds().unsigned_abs() <= MAX_DURATION_SECONDS.unsigned_abs();
    crate::check(d, ok, Error::Overflow)
}

/// `timestamp(seconds)`: epoch seconds.
pub fn from_seconds(seconds: i64) -> Result<DateTime<Utc>> {
    timestamp_at(seconds, 0)
}

/// `int(t)`: epoch seconds, rounded down.
pub fn to_seconds(t: DateTime<Utc>) -> Result<i64> {
    Ok(t.timestamp())
}

/// `timestamp(s)`: RFC 3339.
pub fn string_to_timestamp(s: &str) -> Result<DateTime<Utc>> {
    parse_timestamp(s)
}

/// `duration(s)`: an optional sign, then `humantime`'s units, such as `1.5s` or `90m`.
pub fn string_to_duration(s: &str) -> Result<TimeDelta> {
    parse_duration(s)
}

/// `string(t)`: RFC 3339 in UTC.
pub fn timestamp_string(t: DateTime<Utc>) -> Result<String> {
    Ok(timestamp_text(t))
}

/// `string(d)`: seconds with an `s`, a fraction only where there is one.
pub fn duration_string(d: TimeDelta) -> Result<String> {
    Ok(duration_text(d))
}

pub fn add(t: DateTime<Utc>, d: TimeDelta) -> Result<DateTime<Utc>> {
    timestamp_within(t.checked_add_signed(d).ok_or(Error::Overflow)?)
}

pub fn add_to(d: TimeDelta, t: DateTime<Utc>) -> Result<DateTime<Utc>> {
    add(t, d)
}

pub fn subtract(t: DateTime<Utc>, d: TimeDelta) -> Result<DateTime<Utc>> {
    timestamp_within(t.checked_sub_signed(d).ok_or(Error::Overflow)?)
}

/// `a - b`.
pub fn between(a: DateTime<Utc>, b: DateTime<Utc>) -> Result<TimeDelta> {
    duration_within(a.signed_duration_since(b))
}

pub fn add_durations(a: TimeDelta, b: TimeDelta) -> Result<TimeDelta> {
    duration_within(a.checked_add(&b).ok_or(Error::Overflow)?)
}

pub fn subtract_durations(a: TimeDelta, b: TimeDelta) -> Result<TimeDelta> {
    duration_within(a.checked_sub(&b).ok_or(Error::Overflow)?)
}

/// A timestamp's field CEL reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    FullYear,
    Month,
    DayOfYear,
    DayOfMonth,
    Date,
    DayOfWeek,
    Hours,
    Minutes,
    Seconds,
    Milliseconds,
}

/// `field` of `t` at `offset` seconds east of UTC.
fn field(t: DateTime<Utc>, offset: i32, field: Field) -> Result<i64> {
    let shift = TimeDelta::try_seconds(i64::from(offset)).ok_or(Error::Function)?;
    let local: NaiveDateTime = t
        .naive_utc()
        .checked_add_signed(shift)
        .ok_or(Error::Function)?;
    Ok(match field {
        Field::FullYear => i64::from(local.year()),
        Field::Month => i64::from(local.month0()),
        Field::DayOfYear => i64::from(local.ordinal0()),
        Field::DayOfMonth => i64::from(local.day0()),
        Field::Date => i64::from(local.day()),
        Field::DayOfWeek => i64::from(local.weekday().num_days_from_sunday()),
        Field::Hours => i64::from(local.hour()),
        Field::Minutes => i64::from(local.minute()),
        Field::Seconds => i64::from(local.second()),
        Field::Milliseconds => i64::from(local.nanosecond() / 1_000_000),
    })
}

/// `field` of `t` in the zone `table` holds: [`Error::Function`] where it holds no offset.
fn zoned(t: DateTime<Utc>, table: &str, f: Field) -> Result<i64> {
    let offset = zone::offset(table, timestamp_millis(t)).ok_or(Error::Function)?;
    field(t, offset, f)
}

macro_rules! accessors {
    ($($utc:ident, $in:ident => $field:ident;)*) => {$(
        pub fn $utc(t: DateTime<Utc>) -> Result<i64> {
            field(t, 0, Field::$field)
        }

        pub fn $in(t: DateTime<Utc>, zone: &str) -> Result<i64> {
            zoned(t, zone, Field::$field)
        }
    )*};
}

accessors! {
    full_year, full_year_in => FullYear;
    month, month_in => Month;
    day_of_year, day_of_year_in => DayOfYear;
    day_of_month, day_of_month_in => DayOfMonth;
    date, date_in => Date;
    day_of_week, day_of_week_in => DayOfWeek;
    hours, hours_in => Hours;
    minutes, minutes_in => Minutes;
    seconds, seconds_in => Seconds;
    milliseconds, milliseconds_in => Milliseconds;
}

pub fn duration_hours(d: TimeDelta) -> Result<i64> {
    Ok(d.num_hours())
}

pub fn duration_minutes(d: TimeDelta) -> Result<i64> {
    Ok(d.num_minutes())
}

pub fn duration_seconds(d: TimeDelta) -> Result<i64> {
    Ok(d.num_seconds())
}

/// The milliseconds past the whole second, as CEL defines it: not the length in ms.
pub fn duration_milliseconds(d: TimeDelta) -> Result<i64> {
    Ok(d.num_milliseconds() % 1000)
}

#[cfg(test)]
mod tests;
