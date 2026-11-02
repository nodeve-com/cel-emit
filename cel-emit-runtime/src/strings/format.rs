//! `string.format`, the strings extension's: a format string whose clauses each take the next of a
//! list of arguments. A clause is `%s`, `%d`, `%f`, `%e`, `%b`, `%o`, `%x`, or `%X`, `%f` and `%e`
//! optionally with a precision (`%.3f`); `%%` is a percent sign. The list has one element type:
//! cel-emit refuses a mixed one, which lowers as `dyn`.

use crate::{Error, Result};

mod values;

/// Most digits a precision may ask for, as cel-go allows.
const MAX_PRECISION: usize = 100;

/// Digits after the point a `%f` or `%e` writes when its clause gives no precision.
const DEFAULT_PRECISION: usize = 6;

/// A clause, parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clause {
    /// `%s`: the value as text.
    String,
    /// `%d`: an integer in decimal.
    Decimal,
    /// `%f`: a number in fixed point, this many digits after the point.
    Fixed(usize),
    /// `%e`: a number in scientific notation, this many digits after the point.
    Scientific(usize),
    /// `%b`: an integer, or a bool as 1 or 0.
    Binary,
    /// `%o`: an integer in octal.
    Octal,
    /// `%x`, or `%X` when `upper`: an integer, a string, or bytes, in hexadecimal.
    Hex { upper: bool },
}

/// A value a clause writes. Each CEL type a format takes has one, and a list of them.
pub trait Arg {
    /// The clause's text for this value; [`Error::Function`] where the clause can't take it.
    fn render(self, clause: Clause) -> Result<String>;
}

/// `fmt` with each clause written as the next of `args`, and `%%` as a percent sign. Arguments
/// past the last clause go unused. [`Error::Function`] where a clause isn't one of those above,
/// runs out of arguments, or can't take its argument.
pub fn format<T: Arg + Clone>(fmt: &str, args: &[T]) -> Result<String> {
    let mut rest = fmt;
    let mut next = args.iter();
    let mut out = String::with_capacity(rest.len());
    while let Some(at) = rest.find('%') {
        out.push_str(rest.get(..at).ok_or(Error::Function)?);
        rest = rest.get(at.saturating_add(1)..).ok_or(Error::Function)?;
        if let Some(after) = rest.strip_prefix('%') {
            out.push('%');
            rest = after;
            continue;
        }
        let (clause, used) = parse_clause(rest)?;
        let arg = next.next().ok_or(Error::Function)?;
        out.push_str(&arg.clone().render(clause)?);
        rest = rest.get(used..).ok_or(Error::Function)?;
    }
    out.push_str(rest);
    Ok(out)
}

/// The clause `rest` starts with, after its `%`, and the bytes it spans.
fn parse_clause(rest: &str) -> Result<(Clause, usize)> {
    let (precision, used) = match rest.strip_prefix('.') {
        None => (None, 0),
        Some(after) => {
            let digits = after.bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 {
                return Err(Error::Function);
            }
            let text = after.get(..digits).ok_or(Error::Function)?;
            let p: usize = text.parse().map_err(|_| Error::Function)?;
            if p > MAX_PRECISION {
                return Err(Error::Function);
            }
            (Some(p), digits.saturating_add(1))
        }
    };
    let verb = rest
        .get(used..)
        .and_then(|r| r.chars().next())
        .ok_or(Error::Function)?;
    let p = precision.unwrap_or(DEFAULT_PRECISION);
    let clause = match verb {
        's' => Clause::String,
        'd' => Clause::Decimal,
        'f' => Clause::Fixed(p),
        'e' => Clause::Scientific(p),
        'b' => Clause::Binary,
        'o' => Clause::Octal,
        'x' => Clause::Hex { upper: false },
        'X' => Clause::Hex { upper: true },
        _ => return Err(Error::Function),
    };
    Ok((clause, used.saturating_add(verb.len_utf8())))
}

#[cfg(test)]
mod tests;
