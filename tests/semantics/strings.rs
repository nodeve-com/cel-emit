//! Patterns against cel-core: cel-emit-runtime's regex cache gives what cel-core does, within the
//! cache and past it. The cel-spec suite (the conformance test) covers the string functions.

use cel_core::Value;

use crate::cases::{check, s, Cases, Got};

/// Only here: a bad pattern fails its call and an `||` absorbs it; constant patterns, and
/// patterns read from a field at run time, match as cel-core does, a second pass over the
/// cache included and patterns past the cache's slots and arena.
#[test]
fn patterns_match_as_the_reference_does_in_and_past_the_cache() {
    check(|cases| {
        bad_pattern(cases);
        constant_patterns(cases);
        cached_patterns(cases);
        patterns_past_the_cache(cases);
    });
}

fn bad_pattern(cases: &mut Cases) {
    cases.gives("a.matches(b)", &[("a", s("x")), ("b", s("("))], Got::Error);
    cases.agrees("a.matches(b) || true", &[("a", s("x")), ("b", s("("))]);
}

fn constant_patterns(cases: &mut Cases) {
    let texts = ["ACME STORE 12", "", "hello", "ACME", "x(y", "l+o", "ab  c"];
    let each: Vec<_> = texts.iter().map(|t| vec![("a", s(t))]).collect();
    for re in [
        r"^ACME STORE\\b",
        "l+o$",
        "",
        r"\\(",
        "(?i)acme",
        r"b\\s+c",
        "[a-c]{2}",
    ] {
        cases.messages(&format!("a.matches('{re}')"), &each);
    }
}

/// Each of `patterns` against each of `texts`, `passes` times over.
fn pairs<S: AsRef<str>>(
    patterns: &[(String, Vec<S>)],
    passes: usize,
) -> Vec<Vec<(&'static str, Value)>> {
    let one = patterns.iter().flat_map(|(p, texts)| {
        texts
            .iter()
            .map(move |t| vec![("a", s(t.as_ref())), ("b", s(p))])
    });
    std::iter::repeat_n(one.collect::<Vec<_>>(), passes)
        .flatten()
        .collect()
}

fn cached_patterns(cases: &mut Cases) {
    let texts = ["ACME STORE 12", "", "hello", "ACME", "x(y", "l+o"];
    let patterns = [r"^ACME STORE\b", "l+o$", "(", "", "^ACME", r"\(", "l+o$"];
    let cached: Vec<_> = patterns
        .iter()
        .map(|p| (p.to_string(), texts.to_vec()))
        .collect();
    cases.messages("a.matches(b)", &pairs(&cached, 2));
}

/// Small patterns fill every slot of the cache; large ones fill the arena.
fn patterns_past_the_cache(cases: &mut Cases) {
    let small = (0..12).map(|n| {
        let texts = [format!("am{n}"), format!("m{n}0"), "m".into()];
        (format!("m{n}"), texts.to_vec())
    });
    let large = (0..24).map(|n| {
        let texts = [
            format!("item{n}-abc"),
            format!("item{n}-"),
            format!("az{n}"),
        ];
        (format!("^item{n}-[a-z]+$|z{n}"), texts.to_vec())
    });
    let patterns: Vec<_> = small.chain(large).collect();
    cases.messages("a.matches(b)", &pairs(&patterns, 3));
}
