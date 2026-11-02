//! CEL conformance: every eval case of the cel-spec suite, from cel-core-conformance's checkout,
//! lowered over its bindings, emitted, built natively against the prebuilt cel-emit-runtime rlib, and
//! run. A file's cases build in chunks, one rustc each, on threads. Each case passes, is
//! unsupported (cel-emit refuses it, or its bindings or expectation are nothing cel-emit holds),
//! is wrong (it ran and disagrees, panicked, or its Rust didn't build), or is n/a (outside the
//! spec's scope). The gate: no case wrong, and exactly the cases `pass.txt` names pass.
//!
//! `CEL_SPEC=string,string_ext` runs those files alone, gated by their part of the baseline, and
//! prints each case that doesn't pass, with why.

// Helpers outside a `#[test]` fn panic as the tests do (clippy.toml).
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

mod case;
#[path = "../common/mod.rs"]
mod common;
mod message;
mod mixed;
mod native;
mod scope;
mod types;
mod values;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use cel_core_conformance::{build_descriptor_pool, load_test_file};
use cel_core_proto::ProstProtoRegistry;

use case::Prepared;
use native::Answer;

/// A file's name, the verdicts reached before building, and the cases to build.
type File = (String, Vec<(String, String, Verdict)>, Vec<Prepared>);

pub enum Verdict {
    Pass,
    Unsupported(String),
    Wrong(String),
    NotApplicable(&'static str),
}

/// Cases one program holds at most: rustc's time grows with the program.
const CHUNK: usize = 64;

#[test]
fn the_cel_spec_suite_passes_its_baseline() {
    let registry = ProstProtoRegistry::from_pool(build_descriptor_pool().unwrap());
    let only: Option<BTreeSet<String>> = std::env::var("CEL_SPEC")
        .ok()
        .map(|v| v.split(',').map(|f| f.trim().to_string()).collect());
    let mut files: Vec<PathBuf> = std::fs::read_dir(testdata())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "textproto"))
        .collect();
    files.sort();
    let (mut passed, mut wrong, mut reasons) = (BTreeSet::new(), BTreeMap::new(), BTreeMap::new());
    let mut ran = BTreeSet::new();
    let mut seen = BTreeMap::new();
    println!(
        "{:<16} {:>5} {:>5} {:>5} {:>5}",
        "file", "pass", "unsup", "wrong", "n/a"
    );
    let mut prepared = Vec::new();
    for path in &files {
        let file = load_test_file(path).unwrap();
        if only.as_ref().is_some_and(|o| !o.contains(&file.name)) {
            continue;
        }
        ran.insert(file.name.clone());
        let mut verdicts = Vec::new();
        let mut cases = Vec::new();
        for section in &file.section {
            for test in &section.test {
                // The suite repeats a few names within a section: the repeat takes its ordinal.
                let mut name = format!("{}/{}/{}", file.name, section.name, test.name);
                let n = seen.entry(name.clone()).or_insert(0);
                *n += 1;
                if *n > 1 {
                    name = format!("{name}#{n}");
                }
                match case::prepare(test, &registry) {
                    Ok(c) => cases.push((name, test.expr.clone(), c)),
                    Err(v) => verdicts.push((name, test.expr.clone(), v)),
                }
            }
        }
        prepared.push((file.name, verdicts, cases));
    }
    let mut built = build_all(&prepared);
    for (file, mut verdicts, cases) in prepared {
        for (name, expr, case) in &cases {
            let answer = built.remove(&(name.clone(), expr.clone())).unwrap();
            verdicts.push((name.clone(), expr.clone(), case::judge(case, answer)));
        }
        let mut counts = [0; 4];
        for (name, expr, verdict) in verdicts {
            let (slot, reason) = match verdict {
                Verdict::Pass => {
                    passed.insert(name.clone());
                    (0, None)
                }
                // Check's messages each name their expression: blanked, then grouped.
                Verdict::Unsupported(why) if why.starts_with("rejected") => {
                    (1, Some(format!("unsupported: {}", generic(&why))))
                }
                Verdict::Unsupported(why) => (1, Some(format!("unsupported: {why}"))),
                Verdict::Wrong(why) => {
                    wrong.insert(name.clone(), format!("{expr}: {why}"));
                    (2, None)
                }
                Verdict::NotApplicable(why) => (3, Some(format!("n/a: {why}"))),
            };
            counts[slot] += 1;
            if let Some(reason) = reason {
                if only.is_some() {
                    println!("  {name} — {expr} — {reason}");
                }
                *reasons.entry(reason).or_insert(0) += 1;
            }
        }
        let [p, u, w, n] = counts;
        println!("{:<16} {p:>5} {u:>5} {w:>5} {n:>5}", file);
    }
    let mut reasons: Vec<_> = reasons.into_iter().collect();
    reasons.sort_by_key(|r| std::cmp::Reverse(r.1));
    println!("the most common reasons:");
    for (why, n) in reasons.iter().take(25) {
        println!("{n:>5} {why}");
    }
    for (name, why) in &wrong {
        println!("wrong: {name} — {why}");
    }

    assert!(wrong.is_empty(), "wrong, worse than refused: {wrong:#?}");
    let baseline: BTreeSet<String> = fixture("pass.txt")
        .into_iter()
        .filter(|c| ran.iter().any(|f| c.starts_with(&format!("{f}/"))))
        .collect();
    let lost: Vec<_> = baseline.difference(&passed).collect();
    assert!(lost.is_empty(), "stopped passing:\n{lost:#?}");
    let gained: Vec<_> = passed.difference(&baseline).cloned().collect();
    assert!(
        gained.is_empty(),
        "newly passing: add to pass.txt\n{}",
        gained.join("\n")
    );
}

/// Every file's chunks built and run on as many threads as the machine has cores: each case's
/// answer by its name and expression.
fn build_all(prepared: &[File]) -> BTreeMap<(String, String), Answer> {
    let jobs: Vec<&[Prepared]> = prepared
        .iter()
        .flat_map(|(_, _, cases)| cases.chunks(CHUNK))
        .collect();
    let scratch = std::env::temp_dir().join(format!("cel-emit-conformance-{}", std::process::id()));
    let (next, done) = (AtomicUsize::new(0), Mutex::new(BTreeMap::new()));
    let threads = std::thread::available_parallelism().map_or(4, usize::from);
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(chunk) = jobs.get(i) else { break };
                let answers = native::build(&scratch.join(i.to_string()), chunk);
                let mut done = done.lock().unwrap();
                for ((name, expr, _), a) in chunk.iter().zip(answers) {
                    done.insert((name.clone(), expr.clone()), a);
                }
            });
        }
    });
    let _ = std::fs::remove_dir_all(&scratch);
    done.into_inner().unwrap()
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

/// The suite's data, in cel-core-conformance's checkout: cargo fetches its cel-spec submodule.
/// Offline, metadata resolves only this build's host: cargo fetched no other's crates.
fn testdata() -> PathBuf {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let version = std::process::Command::new(rustc)
        .arg("-vV")
        .output()
        .unwrap();
    let version = String::from_utf8(version.stdout).unwrap();
    let host = version
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .unwrap();
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = std::process::Command::new(cargo)
        .args([
            "metadata",
            "--format-version=1",
            "--offline",
            "--filter-platform",
            host,
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let package = meta["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "cel-core-conformance")
        .unwrap();
    let manifest = PathBuf::from(package["manifest_path"].as_str().unwrap());
    manifest
        .parent()
        .unwrap()
        .join("cel-spec/tests/simple/testdata")
}

/// A fixture's case names, one per line.
fn fixture(name: &str) -> BTreeSet<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/conformance")
        .join(name);
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}
