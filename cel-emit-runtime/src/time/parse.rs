//! Timestamps and durations read from text, as JSON carries them: [`any_value`]'s readers, then
//! CEL's range.

use chrono::{DateTime, TimeDelta, Utc};

use super::{duration_within, timestamp_within};
use crate::{Error, Result};

/// An RFC 3339 `date-time`, its offset applied, inside CEL's range: [`Error::Overflow`] past
/// it, [`Error::Cast`] for text that is no such time.
pub fn parse_timestamp(text: &str) -> Result<DateTime<Utc>> {
    timestamp_within(any_value::parse_timestamp(text).map_err(read_error)?)
}

/// An optional `-` or `+`, then a span in `humantime`'s units, inside CEL's range:
/// [`Error::Overflow`] past it, [`Error::Cast`] for text that is no duration.
pub fn parse_duration(text: &str) -> Result<TimeDelta> {
    duration_within(any_value::parse_duration(text).map_err(read_error)?)
}

fn read_error(e: any_value::Error) -> Error {
    match e {
        any_value::Error::Kind => Error::Cast,
        any_value::Error::Range => Error::Overflow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{duration_at, duration_millis, duration_text, MAX_DURATION_SECONDS};

    fn ms(text: &str) -> Result<i64> {
        parse_duration(text).map(duration_millis)
    }

    #[test]
    fn spans_read_inside_cels_range() {
        assert_eq!(ms("1.5s"), Ok(1500));
        assert_eq!(ms("-7200s"), Ok(-7_200_000));
        let top = parse_duration("315537897598.999999999s").unwrap();
        assert_eq!(duration_text(top), "315537897598.999999999s");
        let spans = [
            duration_at(MAX_DURATION_SECONDS, 999_999_999),
            duration_at(-MAX_DURATION_SECONDS, -999_999_999),
        ];
        for d in spans {
            let d = d.unwrap();
            assert_eq!(parse_duration(&duration_text(d)), Ok(d), "{d}");
        }
    }

    #[test]
    fn past_the_range_overflows_and_junk_is_a_cast() {
        for text in ["315537897599s", "-315537897599s", "99999999999999999999s"] {
            assert_eq!(parse_duration(text), Err(Error::Overflow), "{text}");
        }
        for bad in ["", "P1D", "--5s", "1 fortnight"] {
            assert_eq!(parse_duration(bad), Err(Error::Cast), "{bad}");
        }
    }

    #[test]
    fn an_instant_reads_inside_cels_range() {
        assert!(parse_timestamp("2026-10-11T18:33:00.5+02:00").is_ok());
        assert_eq!(parse_timestamp("2026-10-11"), Err(Error::Cast));
        assert_eq!(
            parse_timestamp("0000-12-31T00:00:00Z"),
            Err(Error::Overflow)
        );
        let leap = parse_timestamp("2016-12-31T23:59:60Z").unwrap();
        assert_eq!(parse_timestamp("2017-01-01T00:00:00Z"), Ok(leap));
    }
}
