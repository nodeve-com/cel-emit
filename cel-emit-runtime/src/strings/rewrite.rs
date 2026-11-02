//! Strings that come out as new strings or lists: case, reversal, replacement, splitting, and
//! joining. Each is an overload of CEL's string extension.

use crate::Result;

/// Only ASCII letters change case.
pub fn lower_ascii(s: &str) -> Result<String> {
    Ok(s.to_ascii_lowercase())
}

/// Only ASCII letters change case.
pub fn upper_ascii(s: &str) -> Result<String> {
    Ok(s.to_ascii_uppercase())
}

/// By code point.
pub fn reverse(s: &str) -> Result<String> {
    Ok(s.chars().rev().collect())
}

/// Every `old` replaced by `new`. An empty `old` matches at each code point boundary.
pub fn replace(s: &str, old: &str, new: &str) -> Result<String> {
    Ok(s.replace(old, new))
}

/// As [`replace`], but at most `limit` replacements, the first ones; a negative `limit` replaces
/// all, zero none.
pub fn replace_limit(s: &str, old: &str, new: &str, limit: i64) -> Result<String> {
    Ok(match usize::try_from(limit) {
        Ok(n) => s.replacen(old, new, n),
        Err(_) => s.replace(old, new),
    })
}

/// `s` cut at each `sep`, as [`split_limit`] with no limit.
pub fn split(s: &str, sep: &str) -> Result<Vec<String>> {
    split_limit(s, sep, -1)
}

/// `s` cut at each `sep` into at most `limit` pieces, the last one holding the rest. A negative
/// `limit` gives every piece, zero none. An empty `sep` cuts `s` into its code points.
pub fn split_limit(s: &str, sep: &str, limit: i64) -> Result<Vec<String>> {
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    Ok(match (limit, sep.is_empty()) {
        (0, _) => Vec::new(),
        (1, _) => vec![s.to_string()],
        (_, true) => explode(s, limit),
        (_, false) => s.splitn(limit, sep).map(str::to_string).collect(),
    })
}

/// `s`'s code points, one piece each, the last piece holding the rest once `limit` pieces are
/// out.
fn explode(s: &str, limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if out.len().checked_add(1) == Some(limit) {
            let mut last = c.to_string();
            last.extend(chars);
            out.push(last);
            break;
        }
        out.push(c.to_string());
    }
    out
}

/// `xs` joined with nothing between.
pub fn join(xs: &[String]) -> Result<String> {
    join_with(xs, "")
}

/// `xs` joined with `sep` between each two; none gives the empty string.
pub fn join_with(xs: &[String], sep: &str) -> Result<String> {
    Ok(xs.join(sep))
}

/// `s` in double quotes, its backslashes and double quotes escaped, and its control characters
/// `\a \b \f \n \r \t \v` as those escapes; any other text as it is.
pub fn quote(s: &str) -> Result<String> {
    let mut out = String::from('"');
    for c in s.chars() {
        match c {
            '\u{7}' => out.push_str("\\a"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{b}' => out.push_str("\\v"),
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(c),
        }
    }
    out.push('"');
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &'static str) -> String {
        String::from(v)
    }

    #[test]
    fn quote_escapes_as_the_spec_does() {
        assert_eq!(quote("first\nsecond").as_deref(), Ok("\"first\\nsecond\""));
        assert_eq!(quote("bell\u{7}").as_deref(), Ok("\"bell\\a\""));
        assert_eq!(
            quote("double \\\\ slash").as_deref(),
            Ok("\"double \\\\\\\\ slash\"")
        );
        assert_eq!(quote("mid \" quote").as_deref(), Ok("\"mid \\\" quote\""));
        assert_eq!(
            quote("single-quote with 'x'").as_deref(),
            Ok("\"single-quote with 'x'\"")
        );
        assert_eq!(
            quote("printable \u{1F600}").as_deref(),
            Ok("\"printable \u{1F600}\"")
        );
        assert_eq!(quote("").as_deref(), Ok("\"\""));
    }

    #[test]
    fn ascii_case_changes_only_ascii() {
        assert_eq!(lower_ascii("TacoCÆt").as_deref(), Ok("tacocÆt"));
        assert_eq!(upper_ascii("tacoCαt").as_deref(), Ok("TACOCαT"));
    }

    #[test]
    fn reverse_is_by_code_point() {
        assert_eq!(reverse("Ta©oCαt").as_deref(), Ok("tαCo©aT"));
        assert_eq!(reverse("").as_deref(), Ok(""));
    }

    #[test]
    fn replace_limits_how_many() {
        assert_eq!(
            replace("{0} days {0}", "{0}", "2").as_deref(),
            Ok("2 days 2")
        );
        assert_eq!(replace_limit("aaa", "a", "b", 2).as_deref(), Ok("bba"));
        assert_eq!(replace_limit("aaa", "a", "b", -1).as_deref(), Ok("bbb"));
        assert_eq!(replace_limit("aaa", "a", "b", 0).as_deref(), Ok("aaa"));
        assert_eq!(replace("ab", "", "-").as_deref(), Ok("-a-b-"));
    }

    #[test]
    fn split_cuts_at_each_separator() {
        assert_eq!(split("hello world", " ").unwrap(), ["hello", "world"]);
        assert_eq!(split("", ",").unwrap(), [""]);
        assert_eq!(split("a,,b,", ",").unwrap(), ["a", "", "b", ""]);
        assert_eq!(split("o©o©o", "©").unwrap(), ["o", "o", "o"]);
    }

    #[test]
    fn split_limit_keeps_the_rest_in_the_last_piece() {
        let limited = |sep, limit| split_limit("a b c", sep, limit).unwrap();
        assert_eq!(limited(" ", 0), Vec::<&str>::new());
        assert_eq!(limited(" ", 1), ["a b c"]);
        assert_eq!(limited(" ", 2), ["a", "b c"]);
        assert_eq!(limited(" ", -1), ["a", "b", "c"]);
        assert_eq!(limited("", 2), ["a", " b c"]);
        assert_eq!(limited("", -1), ["a", " ", "b", " ", "c"]);
        assert_eq!(split_limit("", "", -1).unwrap(), Vec::<&str>::new());
        assert_eq!(split_limit("", "", 1).unwrap(), [""]);
    }

    #[test]
    fn join_puts_the_separator_between() {
        let xs = vec![s("x"), s("y")];
        assert_eq!(join(&xs).as_deref(), Ok("xy"));
        assert_eq!(join_with(&xs, "-").as_deref(), Ok("x-y"));
        let none: Vec<String> = Vec::new();
        assert_eq!(join(&none).as_deref(), Ok(""));
        assert_eq!(join_with(&none, "-").as_deref(), Ok(""));
    }
}
