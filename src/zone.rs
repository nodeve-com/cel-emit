//! A zone a CEL accessor names, fixed as it lowers: its UTC offsets from 1970 until 2100, as
//! cel-emit-runtime reads them (`time::zone`). What is emitted carries the zone database lowering
//! had, so every run reads an instant alike.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, Offset, TimeZone};
use chrono_tz::Tz;

use crate::Error;

/// 1970-01-01 and 2100-01-01, epoch ms.
const FROM: i64 = 0;
const UNTIL: i64 = 4_102_444_800_000;

/// How far apart compile samples an offset: two changes closer than this can go unseen.
const STEP: i64 = 6 * 3_600_000;

/// `zone`, an IANA name or a fixed `[±]HH:MM`, as a table.
pub fn table(zone: &str) -> Result<String, Error> {
    static TABLES: OnceLock<Mutex<BTreeMap<String, String>>> = OnceLock::new();
    let tables = TABLES.get_or_init(Default::default);
    if let Some(t) = tables.lock().ok().and_then(|t| t.get(zone).cloned()) {
        return Ok(t);
    }
    let invalid = || Error::Invalid(format!("zone `{zone}`: not an IANA name or [±]HH:MM"));
    let changes = match fixed(zone) {
        Some(offset) => vec![(FROM, offset)],
        None => changes(zone.parse().map_err(|_| invalid())?).ok_or_else(invalid)?,
    };
    let table = cel_emit_runtime::time::zone::encode(&changes, UNTIL);
    if let Ok(mut t) = tables.lock() {
        t.insert(zone.into(), table.clone());
    }
    Ok(table)
}

/// `[±]HH:MM` in seconds east of UTC.
fn fixed(zone: &str) -> Option<i32> {
    let (sign, rest): (i32, &str) = match zone.split_at_checked(1)? {
        ("+", rest) => (1, rest),
        ("-", rest) => (-1, rest),
        _ => (1, zone),
    };
    let (h, m) = rest.split_once(':')?;
    let two = |s: &str| s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit());
    if !two(h) || !two(m) {
        return None;
    }
    let (h, m): (i32, i32) = (h.parse().ok()?, m.parse().ok()?);
    if h >= 24 || m >= 60 {
        return None;
    }
    sign.checked_mul(h.checked_mul(3600)?.checked_add(m.checked_mul(60)?)?)
}

/// Each change of `tz`'s offset, the first at [`FROM`], each found to the second.
fn changes(tz: Tz) -> Option<Vec<(i64, i32)>> {
    let at = |ms: i64| {
        let utc = DateTime::from_timestamp_millis(ms)?.naive_utc();
        Some(tz.offset_from_utc_datetime(&utc).fix().local_minus_utc())
    };
    let mut changes = vec![(FROM, at(FROM)?)];
    let mut t = FROM;
    while t < UNTIL {
        let next = t.checked_add(STEP)?.min(UNTIL);
        let (was, now) = (at(t)?, at(next)?);
        if was != now {
            // `was` holds at `lo`, `now` at `hi`, both whole seconds.
            let (mut lo, mut hi) = (t, next);
            while hi.checked_sub(lo)? > 1000 {
                let mid =
                    lo.checked_add(hi.checked_sub(lo)?.checked_div(2000)?.checked_mul(1000)?)?;
                if at(mid)? == was {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            changes.push((hi, at(hi)?));
        }
        t = next;
    }
    Some(changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cel_emit_runtime::time::zone::offset;

    #[test]
    fn a_zone_changes_offset_at_its_transitions_to_the_second() {
        let t = table("America/New_York").unwrap_or_default();
        // 2026-03-08 07:00:00Z springs EST to EDT.
        assert_eq!(offset(&t, 1_772_953_199_999), Some(-5 * 3600));
        assert_eq!(offset(&t, 1_772_953_200_000), Some(-4 * 3600));
        assert_eq!(offset(&t, UNTIL), None);
        assert_eq!(offset(&t, -1), None);
    }

    #[test]
    fn a_fixed_offset_or_an_unknown_zone() {
        let t = table("+05:30").unwrap_or_default();
        assert_eq!(offset(&t, 0), Some(5 * 3600 + 1800));
        assert_eq!(fixed("-08:00"), Some(-8 * 3600));
        assert!(table("Middle/Earth").is_err());
        assert_eq!(fixed("02:00"), Some(2 * 3600));
        assert!(table("+5:30").is_err());
    }
}
