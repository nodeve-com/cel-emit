use super::*;

/// Whether `text` is one JSON value and nothing after it, by the scan.
fn scans(text: &str) -> bool {
    value(text.as_bytes()).is_some_and(|rest| ws(rest).is_empty())
}

/// Whether serde_json, ignoring it, takes `text` as one JSON value.
fn serde_takes(text: &str) -> bool {
    serde_json::from_str::<serde::de::IgnoredAny>(text).is_ok()
}

/// Texts of numbers, strings, brackets, and whitespace, valid and not.
const TEXTS: &[&str] = &[
    "",
    " ",
    "0",
    "-0",
    "-",
    "01",
    "-01",
    "1.",
    "1.5",
    ".5",
    "1e",
    "1e+",
    "1E-7",
    "1e5",
    "2.50",
    "123456789012345678901234567890",
    "1.2.3",
    "+1",
    "true",
    "tru",
    "truex",
    "false",
    "null",
    "nul",
    r#""""#,
    r#"""#,
    r#""a\"b""#,
    r#""\u00e9""#,
    r#""\u00g9""#,
    r#""\x""#,
    r#""\/""#,
    "\"tab\there\"",
    "\"é ü\"",
    "[]",
    "[ ]",
    "[1,]",
    "[,1]",
    "[1 2]",
    "[1,2]",
    "{}",
    "{ }",
    r#"{"a":1}"#,
    r#"{"a":1,}"#,
    r#"{"a" : [ 1 , { "b" : null } ] }"#,
    r#"{"a"}"#,
    r#"{"a":}"#,
    r#"{a:1}"#,
    r#"{"a":1 "b":2}"#,
    "[}",
    "{]",
    "[[]",
    "[]]",
    "{} {}",
    "1 2",
    " 1 ",
    "\n[\r\t]",
    r#"{"a":{"b":{"c":[true,false,null,"x",-1.5e3]}}}"#,
];

#[test]
fn the_scan_takes_what_serde_json_takes() {
    let deep = |n: usize| format!("{}{}", "[{\"a\":".repeat(n), "1}]".repeat(n));
    let long = [deep(70), deep(200), deep(200).replacen(']', "}", 1)];
    for text in TEXTS.iter().copied().chain(long.iter().map(String::as_str)) {
        assert_eq!(scans(text), serde_takes(text), "{text:?}");
    }
}

/// Every byte at every place in a run longer than a word: in a string, a number, whitespace.
#[test]
fn a_run_ends_at_its_first_stop_wherever_that_falls() {
    let stops = [
        "\"", "\\\"", "\\u0041", "\\q", "\u{1}", "\t", "é", "x", "0", " ", ",", "]",
    ];
    for at in 0..20 {
        for stop in stops {
            let (head, tail) = ("abcdefghijklmnopqrstu".split_at(at), "vwxyz0123456789");
            let texts = [
                format!(r#"["{}{stop}{}"]"#, head.0, tail),
                format!("[1{}{stop}{}]", &"12345678901234567890"[..at], "123"),
                format!("[1.{}{stop}5]", &"12345678901234567890"[..at]),
                format!("[{}{stop}1]", " ".repeat(at)),
                format!("[1{}{stop}]", "\n\r\t ".repeat(at)),
            ];
            for text in texts {
                assert_eq!(scans(&text), serde_takes(&text), "{text:?}");
            }
        }
    }
}

#[test]
fn a_scan_stops_at_its_values_end() {
    let rest = |text: &'static str| value(text.as_bytes()).map(|r| core::str::from_utf8(r).ok());
    assert_eq!(rest(r#"12,"b":2}"#), Some(Some(r#","b":2}"#)));
    assert_eq!(rest(r#" {"x":[1]} ,"b""#), Some(Some(r#" ,"b""#)));
    assert_eq!(rest(r#""s\"t"}"#), Some(Some("}")));
    assert_eq!(rest("1.5e3]"), Some(Some("]")));
}
