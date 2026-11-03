//! A JSON value's extent, found by scanning its bytes: its syntax checked as serde_json checks a
//! value it ignores, nothing parsed or built. Each step takes a slice and returns what follows
//! the part it read, or `None` where the text breaks JSON's grammar. A run of string characters,
//! digits, or whitespace goes eight bytes a step. Nesting has no limit: which brackets are open
//! takes one bit each, so only a value nested past 64 levels allocates, a word per 64.

#[cfg(test)]
mod tests;

/// `b` past the JSON whitespace at its start.
#[inline(always)]
pub(super) fn ws(b: &[u8]) -> &[u8] {
    match b.first() {
        Some(&c) if c > b' ' => b,
        _ => run(b, not_ws, |c| !matches!(c, b' ' | b'\t' | b'\n' | b'\r')),
    }
}

/// `b` past the one JSON value at its start, whitespace before it included.
#[inline(always)]
pub(super) fn value(b: &[u8]) -> Option<&[u8]> {
    let b = ws(b);
    let (&c, rest) = b.split_first()?;
    // A number first: most values are.
    if c.is_ascii_digit() || c == b'-' {
        return number(b);
    }
    match c {
        b'"' => string(rest).map(|(rest, _)| rest),
        b'{' | b'[' => nested(b),
        b't' => rest.strip_prefix(b"rue"),
        b'f' => rest.strip_prefix(b"alse"),
        b'n' => rest.strip_prefix(b"ull"),
        _ => None,
    }
}

/// `b` past the object or array at its start.
fn nested(mut b: &[u8]) -> Option<&[u8]> {
    let mut open = Open::default();
    loop {
        b = descend(b, &mut open)?;
        let (rest, done) = ascend(b, &mut open)?;
        if done {
            return Some(rest);
        }
        b = rest;
    }
}

/// `b` past the value at its start, opening each bracket that starts it: an empty one is a value.
fn descend<'a>(mut b: &'a [u8], open: &mut Open) -> Option<&'a [u8]> {
    loop {
        let (&c, rest) = ws(b).split_first()?;
        if !matches!(c, b'{' | b'[') {
            return value(b);
        }
        let object = c == b'{';
        let inner = ws(rest);
        match inner.split_first() {
            Some((&d, after)) if d == if object { b'}' } else { b']' } => return Some(after),
            _ => {}
        }
        open.push(object);
        b = if object { member(inner)? } else { inner };
    }
}

/// `b` past a value, closing each bracket it ends: the rest and whether none is left open, else
/// the rest starts the next value.
fn ascend<'a>(mut b: &'a [u8], open: &mut Open) -> Option<(&'a [u8], bool)> {
    loop {
        let Some(object) = open.top() else {
            return Some((b, true));
        };
        let (&c, rest) = ws(b).split_first()?;
        match c {
            b',' if object => return Some((member(ws(rest))?, false)),
            b',' => return Some((rest, false)),
            b'}' if object => open.pop(),
            b']' if !object => open.pop(),
            _ => return None,
        }
        b = rest;
    }
}

/// `b` past an object member's key and the colon after it.
#[inline(always)]
fn member(b: &[u8]) -> Option<&[u8]> {
    let (rest, _) = string(b.strip_prefix(b"\"")?)?;
    ws(rest).strip_prefix(b":")
}

/// `b`, just past a string's opening quote, past its closing one, and whether it holds an
/// escape. Its characters are any but a control character, a quote, or a backslash, which
/// starts an escape; a `\u` escape names any four hex digits, as serde_json ignores them.
#[inline(always)]
pub(super) fn string(mut b: &[u8]) -> Option<(&[u8], bool)> {
    let mut escaped = false;
    loop {
        b = run(b, ends_string, |c| c == b'"' || c == b'\\' || c < 0x20);
        let (&c, rest) = b.split_first()?;
        b = match c {
            b'"' => return Some((rest, escaped)),
            b'\\' => match rest.split_first()? {
                (b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't', rest) => rest,
                (b'u', rest) => {
                    let (hex, rest) = rest.split_first_chunk::<4>()?;
                    if !hex.iter().all(u8::is_ascii_hexdigit) {
                        return None;
                    }
                    rest
                }
                _ => return None,
            },
            _ => return None,
        };
        escaped = true;
    }
}

/// `b` past the number at its start: an optional minus, an integer with no leading zero, then
/// an optional fraction and an optional exponent, each with a digit at least.
#[inline(always)]
fn number(b: &[u8]) -> Option<&[u8]> {
    let b = b.strip_prefix(b"-").unwrap_or(b);
    let b = match b.split_first()? {
        (b'0', rest) => rest,
        (b'1'..=b'9', rest) => digits(rest),
        _ => return None,
    };
    let b = match b.strip_prefix(b".") {
        Some(fraction) => some_digits(fraction)?,
        None => b,
    };
    match b.split_first() {
        Some((b'e' | b'E', rest)) => {
            let sign = rest.strip_prefix(b"+").or_else(|| rest.strip_prefix(b"-"));
            some_digits(sign.unwrap_or(rest))
        }
        _ => Some(b),
    }
}

/// `b` past the digits at its start.
#[inline(always)]
fn digits(b: &[u8]) -> &[u8] {
    run(b, not_digit, |c| !c.is_ascii_digit())
}

/// `b` past the digits at its start, of which there is one at least.
#[inline(always)]
fn some_digits(b: &[u8]) -> Option<&[u8]> {
    let rest = digits(b);
    (rest.len() < b.len()).then_some(rest)
}

/// `b` from its first byte that `ends`, or empty: eight bytes a step while `stops` flags none.
/// `stops` flags such bytes of a little-endian word by their high bits, the lowest it flags
/// always one; a borrow or carry may flag a byte above it.
#[inline(always)]
fn run(mut b: &[u8], stops: impl Fn(u64) -> u64, ends: impl Fn(u8) -> bool) -> &[u8] {
    while let Some((word, rest)) = b.split_first_chunk::<8>() {
        let flagged = stops(u64::from_le_bytes(*word));
        if flagged != 0 {
            return b
                .get(flagged.trailing_zeros() as usize / 8..)
                .unwrap_or_default();
        }
        b = rest;
    }
    let n = b.iter().position(|&c| ends(c)).unwrap_or(b.len());
    b.get(n..).unwrap_or_default()
}

/// One in each byte.
const ONES: u64 = u64::MAX / 255;
/// Each byte's high bit.
const HIGH: u64 = ONES << 7;

/// Each byte of `x` that isn't zero, by its high bit. Exact for each byte: no carry crosses one.
#[inline(always)]
fn nonzero(x: u64) -> u64 {
    ((x & !HIGH).wrapping_add(!HIGH) | x) & HIGH
}

/// The bytes of `x` that are zero, the lowest flagged one of them.
#[inline(always)]
fn zero(x: u64) -> u64 {
    x.wrapping_sub(ONES) & !x & HIGH
}

/// Each byte that ends a run of string characters: a quote, a backslash, a control character.
#[inline(always)]
fn ends_string(x: u64) -> u64 {
    let quote = zero(x ^ ONES.wrapping_mul(u64::from(b'"')));
    let backslash = zero(x ^ ONES.wrapping_mul(u64::from(b'\\')));
    let control = x.wrapping_sub(ONES.wrapping_mul(0x20)) & !x & HIGH;
    quote | backslash | control
}

/// Each byte that isn't a digit: one whose distance from `0`, as its bits differ, is past 9.
#[inline(always)]
fn not_digit(x: u64) -> u64 {
    let from_zero = x ^ ONES.wrapping_mul(u64::from(b'0'));
    (from_zero.wrapping_add(ONES.wrapping_mul(127 - 9)) | from_zero) & HIGH
}

/// Each byte that isn't JSON whitespace.
#[inline(always)]
fn not_ws(x: u64) -> u64 {
    let not = |c: u8| nonzero(x ^ ONES.wrapping_mul(u64::from(c)));
    not(b' ') & not(b'\t') & not(b'\n') & not(b'\r')
}

/// The brackets open, innermost last, an object's as a set bit: `n` of them in `bits`, and the
/// words of 64 outside those in `outer`.
#[derive(Default)]
struct Open {
    bits: u64,
    n: u32,
    outer: Vec<u64>,
}

impl Open {
    fn push(&mut self, object: bool) {
        if self.n == u64::BITS {
            self.outer.push(self.bits);
            self.n = 0;
        }
        self.bits = self.bits.wrapping_shl(1) | u64::from(object);
        self.n = self.n.wrapping_add(1);
    }

    /// Whether the innermost open bracket is an object's; `None` while none is open.
    fn top(&self) -> Option<bool> {
        (self.n > 0).then_some(self.bits & 1 == 1)
    }

    fn pop(&mut self) {
        self.bits = self.bits.wrapping_shr(1);
        self.n = self.n.saturating_sub(1);
        if self.n == 0 {
            if let Some(word) = self.outer.pop() {
                (self.bits, self.n) = (word, u64::BITS);
            }
        }
    }
}
