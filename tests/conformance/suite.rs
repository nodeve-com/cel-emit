//! The suite run: its files prepared, and each file's verdicts counted against the baseline.

use std::collections::{BTreeMap, BTreeSet};

use cel_core_conformance::load_test_file;
use cel_core_proto::ProstProtoRegistry;

use super::case::Prepared;
use super::{case, fixture, testdata, File, Verdict};

/// Every file of the suite (or those `only` names) prepared, and the names of those run.
pub fn prepare(
    registry: &ProstProtoRegistry,
    only: Option<&BTreeSet<String>>,
) -> (Vec<File>, BTreeSet<String>) {
    let mut files: Vec<_> = std::fs::read_dir(testdata())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "textproto"))
        .collect();
    files.sort();
    let (mut prepared, mut ran, mut seen) = (Vec::new(), BTreeSet::new(), BTreeMap::new());
    for path in &files {
        let file = load_test_file(path).unwrap();
        if only.is_some_and(|o| !o.contains(&file.name)) {
            continue;
        }
        ran.insert(file.name.clone());
        let (verdicts, cases) = sort_cases(&file, registry, &mut seen);
        prepared.push((file.name, verdicts, cases));
    }
    (prepared, ran)
}

/// `file`'s cases: those decided before building, and those to build. Names repeat within a
/// section in the suite: the repeat takes its ordinal, counted in `seen`.
fn sort_cases(
    file: &cel_core_conformance::SimpleTestFile,
    registry: &ProstProtoRegistry,
    seen: &mut BTreeMap<String, usize>,
) -> (Vec<(String, String, Verdict)>, Vec<Prepared>) {
    let (mut verdicts, mut cases) = (Vec::new(), Vec::new());
    for test in file
        .section
        .iter()
        .flat_map(|s| s.test.iter().map(move |t| (s, t)))
    {
        let (section, test) = test;
        let mut name = format!("{}/{}/{}", file.name, section.name, test.name);
        let n = seen.entry(name.clone()).or_insert(0);
        *n += 1;
        if *n > 1 {
            name = format!("{name}#{n}");
        }
        match case::prepare(test, registry) {
            Ok(c) => cases.push((name, test.expr.clone(), c)),
            Err(v) => verdicts.push((name, test.expr.clone(), v)),
        }
    }
    (verdicts, cases)
}

/// The cases that passed or were wrong, and how often each reason for not passing came.
#[derive(Default)]
pub struct Tally {
    passed: BTreeSet<String>,
    wrong: BTreeMap<String, String>,
    reasons: BTreeMap<String, usize>,
}

impl Tally {
    /// Counts the `verdicts` of `file`, printing its row, and each case with a reason if `each`.
    pub fn file(&mut self, file: &str, verdicts: Vec<(String, String, Verdict)>, each: bool) {
        let mut counts = [0; 4];
        for (name, expr, verdict) in verdicts {
            let (slot, reason) = self.score(&name, &expr, verdict);
            counts[slot] += 1;
            let Some(reason) = reason else { continue };
            if each {
                println!("  {name} — {expr} — {reason}");
            }
            *self.reasons.entry(reason).or_insert(0) += 1;
        }
        let [p, u, w, n] = counts;
        println!("{file:<16} {p:>5} {u:>5} {w:>5} {n:>5}");
    }

    /// The column `verdict` counts in, and its reason if it did not pass or was not wrong.
    fn score(&mut self, name: &str, expr: &str, verdict: Verdict) -> (usize, Option<String>) {
        match verdict {
            Verdict::Pass => {
                self.passed.insert(name.to_string());
                (0, None)
            }
            // Check's messages each name their expression: blanked, then grouped.
            Verdict::Unsupported(why) if why.starts_with("rejected") => {
                (1, Some(format!("unsupported: {}", generic(&why))))
            }
            Verdict::Unsupported(why) => (1, Some(format!("unsupported: {why}"))),
            Verdict::Wrong(why) => {
                self.wrong
                    .insert(name.to_string(), format!("{expr}: {why}"));
                (2, None)
            }
            Verdict::NotApplicable(why) => (3, Some(format!("n/a: {why}"))),
        }
    }

    /// Prints the common reasons and the wrong cases, and gates on the baseline of the files `ran`.
    pub fn conclude(self, ran: &BTreeSet<String>) {
        let mut reasons: Vec<_> = self.reasons.into_iter().collect();
        reasons.sort_by_key(|r| std::cmp::Reverse(r.1));
        println!("the most common reasons:");
        for (why, n) in reasons.iter().take(25) {
            println!("{n:>5} {why}");
        }
        for (name, why) in &self.wrong {
            println!("wrong: {name} — {why}");
        }
        assert!(
            self.wrong.is_empty(),
            "wrong, worse than refused: {:#?}",
            self.wrong
        );
        let baseline: BTreeSet<String> = fixture("pass.txt")
            .into_iter()
            .filter(|c| ran.iter().any(|f| c.starts_with(&format!("{f}/"))))
            .collect();
        let lost: Vec<_> = baseline.difference(&self.passed).collect();
        assert!(lost.is_empty(), "stopped passing:\n{lost:#?}");
        let gained: Vec<_> = self.passed.difference(&baseline).cloned().collect();
        assert!(
            gained.is_empty(),
            "newly passing: add to pass.txt\n{}",
            gained.join("\n")
        );
    }
}

/// A check message with the names of one expression's fields, messages, and literals blanked,
/// so cases failing alike are counted as one reason.
fn generic(why: &str) -> String {
    let mut out = String::new();
    let mut rest = why;
    while let Some(i) = rest.find(['\'', '"']) {
        let quote = &rest[i..=i];
        out.push_str(&rest[..i]);
        match rest[i + 1..].find(quote) {
            Some(j) => {
                out.push('_');
                rest = &rest[i + j + 2..];
            }
            None => {
                rest = &rest[i..];
                break;
            }
        }
    }
    out.push_str(rest);
    out.split(' ')
        .map(|w| {
            if w.len() > 1 && (w.contains('_') || w.contains('.')) {
                "_"
            } else {
                w
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
