//! A zone's UTC offsets as cel-emit fixes them into the Rust it writes, and as [`super`] reads
//! them there: each change's instant and the offset from it on, in order, one fixed-width entry
//! each in a string literal, the last marking where it stops knowing.

/// One entry: the instant, epoch ms, then the offset, seconds east of UTC, each two's
/// complement in lowercase hex.
pub const ENTRY: usize = 24;

/// The offset of the last entry: past it, no offset is known.
const END: i32 = i32::MAX;

/// `changes`, ascending by instant, known until `until`, as a table.
pub fn encode(changes: &[(i64, i32)], until: i64) -> String {
    changes
        .iter()
        .copied()
        .chain([(until, END)])
        .map(|(at, offset)| format!("{:016x}{:08x}", at as u64, offset as u32))
        .collect()
}

/// The offset `table` gives at `ms`; none before its first change or from its end.
pub fn offset(table: &str, ms: i64) -> Option<i32> {
    let entry = |i: usize| {
        let at = table.get(i.checked_mul(ENTRY)?..)?;
        let ts = u64::from_str_radix(at.get(..16)?, 16).ok()? as i64;
        let offset = u32::from_str_radix(at.get(16..ENTRY)?, 16).ok()? as i32;
        Some((ts, offset))
    };
    let n = table.len().checked_div(ENTRY)?;
    // The first entry after `ms`; the one before it holds.
    let (mut lo, mut hi) = (0usize, n);
    while lo < hi {
        let mid = lo.checked_add(hi.checked_sub(lo)? / 2)?;
        if entry(mid)?.0 <= ms {
            lo = mid.checked_add(1)?;
        } else {
            hi = mid;
        }
    }
    let (_, offset) = entry(lo.checked_sub(1)?)?;
    (offset != END).then_some(offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_offset_holds_from_its_change_until_the_next_and_none_past_the_end() {
        let table = encode(&[(-1000, 3600), (0, 7200), (5000, -1800)], 9000);
        assert_eq!(table.len(), 4 * ENTRY);
        assert_eq!(offset(&table, -1001), None);
        assert_eq!(offset(&table, -1000), Some(3600));
        assert_eq!(offset(&table, -1), Some(3600));
        assert_eq!(offset(&table, 0), Some(7200));
        assert_eq!(offset(&table, 4999), Some(7200));
        assert_eq!(offset(&table, 5000), Some(-1800));
        assert_eq!(offset(&table, 8999), Some(-1800));
        assert_eq!(offset(&table, 9000), None);
        assert_eq!(offset("", 0), None);
        assert_eq!(offset("zz", 0), None);
    }
}
