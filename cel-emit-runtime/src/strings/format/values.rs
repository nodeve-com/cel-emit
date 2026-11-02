//! The text each CEL value writes under each clause: the impls of [`Arg`].

use super::{Arg, Clause};
use chrono::TimeDelta;

use crate::{Error, Result};

/// NaN and the infinities, by the names CEL gives them; none for a finite number.
fn special(x: f64) -> Option<&'static str> {
    if x.is_nan() {
        Some("NaN")
    } else if x.is_infinite() {
        Some(if x.is_sign_negative() {
            "-Infinity"
        } else {
            "Infinity"
        })
    } else {
        None
    }
}

/// `x` in fixed point with `p` digits after the point, rounded to nearest, ties to even.
fn fixed(x: f64, p: usize) -> String {
    match special(x) {
        Some(s) => s.to_string(),
        None => format!("{x:.p$}"),
    }
}

/// `x` in scientific notation with `p` digits after the point, its exponent a sign and at least
/// two digits, as C and cel-go write it.
fn scientific(x: f64, p: usize) -> String {
    if let Some(s) = special(x) {
        return s.to_string();
    }
    let text = format!("{x:.p$e}");
    // Rust writes the exponent bare (`e3`, `e-7`); the mantissa is Rust's own digits.
    let parts = text
        .split_once('e')
        .and_then(|(m, e)| e.parse::<i32>().ok().map(|e| (m, e)));
    match parts {
        Some((m, e)) => format!(
            "{m}e{}{:02}",
            if e < 0 { '-' } else { '+' },
            e.unsigned_abs()
        ),
        None => text,
    }
}

/// An integer's digits in the clause's base, a minus sign before its magnitude as cel-go writes
/// it. Any clause but the integer ones is [`Error::Function`].
fn radix(negative: bool, magnitude: u64, clause: Clause) -> Result<String> {
    let sign = if negative { "-" } else { "" };
    Ok(match clause {
        Clause::String | Clause::Decimal => format!("{sign}{magnitude}"),
        Clause::Binary => format!("{sign}{magnitude:b}"),
        Clause::Octal => format!("{sign}{magnitude:o}"),
        Clause::Hex { upper: false } => format!("{sign}{magnitude:x}"),
        Clause::Hex { upper: true } => format!("{sign}{magnitude:X}"),
        Clause::Fixed(_) | Clause::Scientific(_) => return Err(Error::Function),
    })
}

/// Bytes in hexadecimal, two digits each.
fn hex(bytes: &[u8], upper: bool) -> String {
    bytes
        .iter()
        .map(|b| {
            if upper {
                format!("{b:02X}")
            } else {
                format!("{b:02x}")
            }
        })
        .collect()
}

/// An integer under a clause: as a number for `%f` and `%e`, else its digits in the clause's base.
fn integer(negative: bool, magnitude: u64, number: f64, clause: Clause) -> Result<String> {
    match clause {
        Clause::Fixed(p) => Ok(fixed(number, p)),
        Clause::Scientific(p) => Ok(scientific(number, p)),
        _ => radix(negative, magnitude, clause),
    }
}

impl Arg for i64 {
    fn render(self, clause: Clause) -> Result<String> {
        integer(self < 0, self.unsigned_abs(), self as f64, clause)
    }
}

impl Arg for u64 {
    fn render(self, clause: Clause) -> Result<String> {
        integer(false, self, self as f64, clause)
    }
}

impl Arg for f64 {
    fn render(self, clause: Clause) -> Result<String> {
        match clause {
            Clause::String => Ok(match special(self) {
                Some(s) => s.to_string(),
                None => format!("{self}"),
            }),
            // NaN and the infinities only: a finite double isn't an integer.
            Clause::Decimal => special(self).map(str::to_string).ok_or(Error::Function),
            Clause::Fixed(p) => Ok(fixed(self, p)),
            Clause::Scientific(p) => Ok(scientific(self, p)),
            Clause::Binary | Clause::Octal | Clause::Hex { .. } => Err(Error::Function),
        }
    }
}

/// A double's text, but `%s` in its own shortest digits.
impl Arg for f32 {
    fn render(self, clause: Clause) -> Result<String> {
        match clause {
            Clause::String if self.is_finite() => Ok(format!("{self}")),
            clause => f64::from(self).render(clause),
        }
    }
}

impl Arg for bool {
    fn render(self, clause: Clause) -> Result<String> {
        match clause {
            Clause::String => Ok(self.to_string()),
            Clause::Binary => Ok(if self { "1" } else { "0" }.to_string()),
            _ => Err(Error::Function),
        }
    }
}

impl Arg for String {
    fn render(self, clause: Clause) -> Result<String> {
        match clause {
            Clause::String => Ok(self),
            Clause::Hex { upper } => Ok(hex(self.as_bytes(), upper)),
            _ => Err(Error::Function),
        }
    }
}

impl Arg for Vec<u8> {
    fn render(self, clause: Clause) -> Result<String> {
        match clause {
            Clause::String => String::from_utf8(self).map_err(|_| Error::Function),
            Clause::Hex { upper } => Ok(hex(&self, upper)),
            _ => Err(Error::Function),
        }
    }
}

impl Arg for crate::DateTime<crate::Utc> {
    fn render(self, clause: Clause) -> Result<String> {
        match clause {
            Clause::String => Ok(crate::time::timestamp_text(self)),
            _ => Err(Error::Function),
        }
    }
}

impl Arg for TimeDelta {
    fn render(self, clause: Clause) -> Result<String> {
        match clause {
            Clause::String => Ok(crate::time::duration_text(self)),
            _ => Err(Error::Function),
        }
    }
}

/// A list's text under `%s`: its elements' texts, comma-separated, in brackets.
impl<T: Arg> Arg for Vec<T> {
    fn render(self, clause: Clause) -> Result<String> {
        if clause != Clause::String {
            return Err(Error::Function);
        }
        let parts = self
            .into_iter()
            .map(|x| x.render(Clause::String))
            .collect::<Result<Vec<String>>>()?;
        Ok(format!("[{}]", parts.join(", ")))
    }
}
