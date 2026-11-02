//! Patterns against cel-core: cel-emit-runtime's regex cache gives what cel-core does, within the
//! cache and past it. The cel-spec suite (the conformance test) covers the string functions.

use crate::cases::{check, s, Got};

/// Only here: a bad pattern fails its call and an `||` absorbs it; constant patterns, and
/// patterns read from a field at run time, match as cel-core does, a second pass over the
/// cache included and patterns past the cache's slots and arena.
#[test]
fn patterns_match_as_the_reference_does_in_and_past_the_cache() {
    check(|cases| {
        cases.gives("a.matches(b)", &[("a", s("x")), ("b", s("("))], Got::Error);
        cases.agrees("a.matches(b) || true", &[("a", s("x")), ("b", s("("))]);

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

        let texts = ["ACME STORE 12", "", "hello", "ACME", "x(y", "l+o"];
        let patterns = [r"^ACME STORE\b", "l+o$", "(", "", "^ACME", r"\(", "l+o$"];
        let mut cached = Vec::new();
        for _ in 0..2 {
            for p in patterns {
                for t in texts {
                    cached.push(vec![("a", s(t)), ("b", s(p))]);
                }
            }
        }
        cases.messages("a.matches(b)", &cached);

        // Small ones fill every slot; large ones fill the arena.
        let small = (0..12).map(|n| {
            (
                format!("m{n}"),
                [format!("am{n}"), format!("m{n}0"), "m".into()],
            )
        });
        let large = (0..24).map(|n| {
            let texts = [
                format!("item{n}-abc"),
                format!("item{n}-"),
                format!("az{n}"),
            ];
            (format!("^item{n}-[a-z]+$|z{n}"), texts)
        });
        let patterns: Vec<_> = small.chain(large).collect();
        let mut past = Vec::new();
        for _ in 0..3 {
            for (p, texts) in &patterns {
                for t in texts {
                    past.push(vec![("a", s(t)), ("b", s(p))]);
                }
            }
        }
        cases.messages("a.matches(b)", &past);
    });
}
