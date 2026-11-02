//! Conversions between CEL's types: `int()`, `double()`, `string()`, and the rest, plus the
//! encoders extension's base64.

use base64::alphabet::STANDARD;
use base64::engine::general_purpose::STANDARD as ENCODE_STANDARD;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use base64::Engine as _;

use crate::{Error, Result};

/// Accepts padded and unpadded input; nonzero bits past the last whole byte are ignored.
const DECODE_STANDARD: GeneralPurpose = GeneralPurpose::new(
    &STANDARD,
    GeneralPurposeConfig::new()
        .with_decode_padding_mode(DecodePaddingMode::Indifferent)
        .with_decode_allow_trailing_bits(true),
);

/// `base64.encode(b)`: standard alphabet, padded.
pub fn base64_encode(b: &[u8]) -> Result<String> {
    Ok(ENCODE_STANDARD.encode(b))
}

/// `base64.decode(s)`: standard alphabet; invalid input is [`Error::Function`].
pub fn base64_decode(s: &str) -> Result<Vec<u8>> {
    DECODE_STANDARD.decode(s).map_err(|_| Error::Function)
}

/// A value to its own type: `int(1)`.
pub fn same<T>(x: T) -> Result<T> {
    Ok(x)
}

/// Correctly rounded.
pub fn int_to_double(i: i64) -> Result<f64> {
    Ok(i as f64)
}

/// Past uint64's range, negative: overflows.
pub fn int_to_uint(i: i64) -> Result<u64> {
    u64::try_from(i).map_err(|_| Error::Overflow)
}

/// Correctly rounded.
pub fn uint_to_double(u: u64) -> Result<f64> {
    Ok(u as f64)
}

/// Past int64 overflows.
pub fn uint_to_int(u: u64) -> Result<i64> {
    i64::try_from(u).map_err(|_| Error::Overflow)
}

pub fn uint_to_string(u: u64) -> Result<String> {
    Ok(u.to_string())
}

/// Toward zero; negative, past uint64, or NaN overflows.
pub fn double_to_uint(d: f64) -> Result<u64> {
    // 2^64 is an exact double; uint64's range excludes it. The test is false for NaN.
    const EDGE: f64 = 18_446_744_073_709_551_616.0;
    if (0.0..EDGE).contains(&d) {
        Ok(d as u64)
    } else {
        Err(Error::Overflow)
    }
}

/// `uint(s)`: a decimal integer.
pub fn string_to_uint(s: &str) -> Result<u64> {
    s.parse().map_err(|_| Error::Cast)
}

/// Toward zero; past int64, or NaN, overflows.
pub fn double_to_int(d: f64) -> Result<i64> {
    // ±2^63 are exact doubles; int64's range excludes both.
    const EDGE: f64 = 9_223_372_036_854_775_808.0;
    if d > -EDGE && d < EDGE {
        Ok(d as i64)
    } else {
        Err(Error::Overflow)
    }
}

/// `double(s)`: a finite decimal, with no surrounding space, or the name of NaN or an infinity as
/// CEL's spec spells them.
pub fn string_to_double(s: &str) -> Result<f64> {
    match s {
        "NaN" => return Ok(f64::NAN),
        "Infinity" => return Ok(f64::INFINITY),
        "-Infinity" => return Ok(f64::NEG_INFINITY),
        _ => {}
    }
    let plain = s
        .bytes()
        .all(|b| b.is_ascii_digit() || b"+-.eE".contains(&b));
    plain
        .then(|| s.parse().ok())
        .flatten()
        .filter(|d: &f64| d.is_finite())
        .ok_or(Error::Cast)
}

/// `bool(s)`: one of CEL's spellings of true or false, and no other.
pub fn string_to_bool(s: &str) -> Result<bool> {
    match s {
        "1" | "t" | "true" | "TRUE" | "True" => Ok(true),
        "0" | "f" | "false" | "FALSE" | "False" => Ok(false),
        _ => Err(Error::Cast),
    }
}

/// `int(s)`: a decimal integer.
pub fn string_to_int(s: &str) -> Result<i64> {
    s.parse().map_err(|_| Error::Cast)
}

/// `bytes(s)`: the bytes of `s`'s UTF-8.
pub fn string_to_bytes(s: &str) -> Result<Vec<u8>> {
    Ok(s.as_bytes().to_vec())
}

/// `string(b)`: `b` as text, where its bytes are UTF-8; else a conversion error.
pub fn bytes_to_string(b: &[u8]) -> Result<String> {
    core::str::from_utf8(b)
        .map(str::to_string)
        .map_err(|_| Error::Cast)
}

pub fn int_to_string(i: i64) -> Result<String> {
    Ok(i.to_string())
}

/// `string(x)` of a double, as cel-core writes it: an integral value with `.0`.
pub fn double_to_string(d: f64) -> Result<String> {
    Ok(format_double(d))
}

/// `string(x)` of a stored float32: [`double_to_string`]'s rules, in its own shortest digits.
pub fn float32_to_string(f: f32) -> Result<String> {
    Ok(format_float(f64::from(f), f))
}

fn format_double(d: f64) -> String {
    format_float(d, d)
}

/// `d`, which `own` holds exactly, as `string(x)` writes it, in `own`'s digits.
fn format_float<F: core::fmt::Display>(d: f64, own: F) -> String {
    if d.is_nan() {
        "NaN".into()
    } else if d.is_infinite() {
        if d.is_sign_positive() {
            "+infinity".into()
        } else {
            "-infinity".into()
        }
    } else if d.fract() == 0.0 && d.abs() < 1e15 {
        format!("{own:.1}")
    } else {
        own.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_convert_as_cel_does() {
        assert_eq!(string_to_double("21.5"), Ok(21.5));
        assert_eq!(string_to_double(" 21"), Err(Error::Cast));
        assert_eq!(string_to_double("err"), Err(Error::Cast));
        assert_eq!(string_to_double("-Infinity"), Ok(f64::NEG_INFINITY));
        assert!(string_to_double("NaN").is_ok_and(f64::is_nan));
        assert_eq!(string_to_double("inf"), Err(Error::Cast));
        assert_eq!(format_double(21.0), "21.0");
        assert_eq!(format_double(21.5), "21.5");
        assert_eq!(string_to_int("-3"), Ok(-3));
        assert_eq!(string_to_int("3.0"), Err(Error::Cast));
        assert_eq!(string_to_uint("18446744073709551615"), Ok(u64::MAX));
        assert_eq!(string_to_uint("-1"), Err(Error::Cast));
    }

    #[test]
    fn a_float32_writes_its_own_digits_under_a_doubles_rules() {
        let text = |f| float32_to_string(f).unwrap();
        assert_eq!(text(9.3), "9.3");
        assert_eq!(text(21.0), "21.0");
        assert_eq!(text(f32::NAN), "NaN");
        assert_eq!(text(f32::NEG_INFINITY), "-infinity");
        assert_eq!(text(1e16), "10000000000000000");
    }

    #[test]
    fn uints_convert_past_their_range_as_overflow() {
        assert_eq!(int_to_uint(-1), Err(Error::Overflow));
        assert_eq!(int_to_uint(i64::MAX), Ok(9_223_372_036_854_775_807));
        assert_eq!(uint_to_int(u64::MAX), Err(Error::Overflow));
        assert_eq!(uint_to_int(7), Ok(7));
        assert_eq!(double_to_uint(-0.5), Err(Error::Overflow));
        assert_eq!(double_to_uint(-0.0), Ok(0));
        assert_eq!(double_to_uint(2.9), Ok(2));
        assert_eq!(double_to_uint(1.8446744073709552e19), Err(Error::Overflow));
        assert_eq!(double_to_uint(f64::NAN), Err(Error::Overflow));
        assert_eq!(uint_to_string(42), Ok("42".into()));
    }

    #[test]
    fn strings_convert_to_bool_only_as_cel_spells_them() {
        for t in ["1", "t", "true", "TRUE", "True"] {
            assert_eq!(string_to_bool(t), Ok(true));
        }
        for f in ["0", "f", "false", "FALSE", "False"] {
            assert_eq!(string_to_bool(f), Ok(false));
        }
        for bad in ["TrUe", "FaLsE", "yes", " true", ""] {
            assert_eq!(string_to_bool(bad), Err(Error::Cast));
        }
    }

    #[test]
    fn bytes_convert_as_text_where_utf8() {
        let utf8 = string_to_bytes("ÿ").unwrap();
        assert_eq!(utf8, b"\xc3\xbf");
        assert_eq!(bytes_to_string(&utf8).as_deref(), Ok("ÿ"));
        assert!(matches!(bytes_to_string(b"\x00\xff"), Err(Error::Cast)));
    }

    #[test]
    fn a_double_past_int64_overflows() {
        assert_eq!(double_to_int(-2.9), Ok(-2));
        assert_eq!(double_to_int(9.3e18), Err(Error::Overflow));
        assert_eq!(double_to_int(f64::NAN), Err(Error::Overflow));
    }
}

#[cfg(test)]
mod base64_tests {
    use super::*;

    fn encode(b: &[u8]) -> Result<String> {
        base64_encode(b)
    }

    fn decode(s: &'static str) -> Result<Vec<u8>> {
        base64_decode(s)
    }

    #[test]
    fn encodes_padded() {
        assert_eq!(encode(b"hello").as_deref(), Ok("aGVsbG8="));
        assert_eq!(encode(b"").as_deref(), Ok(""));
        assert_eq!(encode(b"Hello World!").as_deref(), Ok("SGVsbG8gV29ybGQh"));
    }

    #[test]
    fn decodes_padded_and_unpadded() {
        assert_eq!(decode("aGVsbG8="), Ok(b"hello".to_vec()));
        assert_eq!(decode("aGVsbG8"), Ok(b"hello".to_vec()));
        assert_eq!(decode("aGVsbA"), Ok(b"hell".to_vec()));
        assert_eq!(decode(""), Ok(Vec::new()));
    }

    #[test]
    fn invalid_input_is_an_error_value() {
        assert_eq!(decode("aGVs!G8="), Err(Error::Function));
        assert_eq!(decode("a"), Err(Error::Function));
        assert_eq!(decode("aGVs\u{e9}"), Err(Error::Function));
    }
}
