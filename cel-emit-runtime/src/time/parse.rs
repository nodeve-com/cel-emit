//! Timestamps and durations read from text, as JSON carries them: RFC 3339 for an instant; for a
//! span, an optional sign and the units [`humantime`] reads (`1.5s`, `90m`, `1h30m`), which
//! includes the text [`super::duration_text`] writes.

use chrono::{DateTime, TimeDelta, Timelike, Utc};

use super::{duration_at, timestamp_at};
use crate::{Error, Result};

/// An RFC 3339 `date-time`, its offset applied.
pub fn parse_timestamp(text: &str) -> Result<DateTime<Utc>> {
    let at = DateTime::parse_from_rfc3339(text).map_err(|_| Error::Cast)?;
    let at = at.with_timezone(&Utc);
    let (seconds, nanos) = (at.timestamp(), at.timestamp_subsec_nanos());
    // A leap second is the last of a UTC day; CEL has none, so it reads as the next second.
    match nanos.checked_sub(1_000_000_000) {
        Some(past) if (at.hour(), at.minute()) == (23, 59) => {
            timestamp_at(seconds.saturating_add(1), past)
        }
        Some(_) => Err(Error::Cast),
        None => timestamp_at(seconds, nanos),
    }
}

/// An optional `-` or `+`, then a span in [`humantime`]'s units, inside CEL's range:
/// [`Error::Overflow`] past it, [`Error::Cast`] for text that is no duration.
#[allow(clippy::disallowed_methods)]
pub fn parse_duration(text: &str) -> Result<TimeDelta> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let span = humantime::parse_duration(digits).map_err(|e| match e {
        humantime::DurationError::NumberOverflow => Error::Overflow,
        _ => Error::Cast,
    })?;
    let seconds = i64::try_from(span.as_secs()).map_err(|_| Error::Overflow)?;
    let nanos = i32::try_from(span.subsec_nanos()).map_err(|_| Error::Overflow)?;
    match negative {
        true => duration_at(seconds.saturating_neg(), nanos.saturating_neg()),
        false => duration_at(seconds, nanos),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{duration_millis, duration_text, timestamp_text, MAX_DURATION_SECONDS};

    fn ms(text: &str) -> Result<i64> {
        parse_duration(text).map(duration_millis)
    }

    #[test]
    fn an_instant_reads_at_its_offset() {
        let at = parse_timestamp("2026-10-11T18:33:00.5+02:00").unwrap();
        assert_eq!(timestamp_text(at), "2026-10-11T16:33:00.500Z");
        assert!(parse_timestamp("2026-10-11").is_err());
    }

    #[test]
    fn spans_read_with_a_sign_and_humantime_units() {
        assert_eq!(ms("1.5s"), Ok(1500));
        assert_eq!(ms("-7200s"), Ok(-7_200_000));
        assert_eq!(ms("+90m"), Ok(5_400_000));
        assert_eq!(ms("1h30m"), Ok(5_400_000));
        assert_eq!(ms("1500ms"), Ok(1500));
        assert_eq!(ms("0"), Ok(0));
    }

    #[test]
    fn what_a_writer_writes_reads_back() {
        let spans = [
            duration_at(MAX_DURATION_SECONDS, 999_999_999),
            duration_at(-MAX_DURATION_SECONDS, -999_999_999),
            duration_at(0, 1),
            duration_at(0, -1),
            duration_at(1, 500_000_000),
            duration_at(-7200, 0),
            duration_at(0, 0),
        ];
        for d in spans {
            let d = d.unwrap();
            assert_eq!(parse_duration(&duration_text(d)), Ok(d), "{d}");
        }
        let top = parse_duration("315537897598.999999999s").unwrap();
        assert_eq!(duration_text(top), "315537897598.999999999s");
    }

    #[test]
    fn past_the_range_overflows_and_junk_is_a_cast() {
        for text in ["315537897599s", "-315537897599s", "99999999999999999999s"] {
            assert_eq!(parse_duration(text), Err(Error::Overflow), "{text}");
        }
        for bad in [
            "",
            "s",
            "P1D",
            "PT1S",
            "x",
            "--5s",
            "+-5s",
            "-",
            "1 fortnight",
        ] {
            assert_eq!(parse_duration(bad), Err(Error::Cast), "{bad}");
        }
    }

    #[test]
    fn a_leap_second_is_the_next_second() {
        let at = |t| timestamp_text(parse_timestamp(t).unwrap());
        assert_eq!(at("2016-12-31T23:59:60Z"), "2017-01-01T00:00:00Z");
        assert_eq!(at("2016-12-31T18:59:60-05:00"), "2017-01-01T00:00:00Z");
        assert!(parse_timestamp("2016-12-31T22:59:60Z").is_err());
    }
}
