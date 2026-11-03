//! What CEL's time functions leave out: a local time in a zone only known while running, and an
//! ISO 8601 duration. Behind the `civil` feature, as the first carries a zone database.

use chrono::{NaiveDateTime, TimeDelta, TimeZone};

use crate::{DateTime, Error, Result, Utc};

/// `to_utc(local, zone)`: a local time such as `2030-01-15T20:00:00` in an IANA zone. A time the
/// zone's clocks skip or repeat is an error.
pub fn to_utc(local: &str, zone: &str) -> Result<DateTime<Utc>> {
    let zone: chrono_tz::Tz = zone.parse().map_err(|_| Error::Function)?;
    let local: NaiveDateTime = local.parse().map_err(|_| Error::Cast)?;
    let zoned = zone.from_local_datetime(&local).single();
    Ok(zoned.ok_or(Error::Range)?.to_utc())
}

/// `iso_duration(text)`: an ISO 8601 duration such as `P1DT12H`, a day being 24 hours. A month or
/// year has no fixed length, so it is an error.
pub fn iso_duration(text: &str) -> Result<TimeDelta> {
    let span: jiff::Span = text.parse().map_err(|_| Error::Cast)?;
    let span = span.to_duration(jiff::SpanRelativeTo::days_are_24_hours());
    let span = span.map_err(|_| Error::Range)?;
    let whole = TimeDelta::try_seconds(span.as_secs()).ok_or(Error::Overflow)?;
    whole
        .checked_add(&TimeDelta::nanoseconds(i64::from(span.subsec_nanos())))
        .ok_or(Error::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_time_reads_its_zone() {
        let utc = to_utc("2030-07-01T12:00:00", "Europe/Paris");
        assert_eq!(utc.unwrap().to_rfc3339(), "2030-07-01T10:00:00+00:00");
        assert_eq!(
            to_utc("2030-07-01T12:00:00", "Nowhere"),
            Err(Error::Function)
        );
    }

    #[test]
    fn a_time_the_clocks_skip_is_an_error() {
        let skipped = to_utc("2030-03-31T02:30:00", "Europe/Paris");
        assert_eq!(skipped, Err(Error::Range));
    }

    #[test]
    fn iso_durations_read_days_as_24_hours() {
        assert_eq!(iso_duration("P1DT12H"), Ok(TimeDelta::hours(36)));
        assert_eq!(iso_duration("P1M"), Err(Error::Range));
        assert_eq!(iso_duration("soon"), Err(Error::Cast));
    }
}
