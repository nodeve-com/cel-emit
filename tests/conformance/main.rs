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
mod suite;
mod types;
mod values;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use cel_core_conformance::build_descriptor_pool;
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
    println!(
        "{:<16} {:>5} {:>5} {:>5} {:>5}",
        "file", "pass", "unsup", "wrong", "n/a"
    );
    let (prepared, ran) = suite::prepare(&registry, only.as_ref());
    let mut built = build_all(&prepared);
    let mut tally = suite::Tally::default();
    for (file, mut verdicts, cases) in prepared {
        for (name, expr, case) in &cases {
            let answer = built.remove(&(name.clone(), expr.clone())).unwrap();
            verdicts.push((name.clone(), expr.clone(), case::judge(case, answer)));
        }
        tally.file(&file, verdicts, only.is_some());
    }
    tally.conclude(&ran);
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
            s.spawn(|| build_jobs(&jobs, &next, &scratch, &done));
        }
    });
    let _ = std::fs::remove_dir_all(&scratch);
    done.into_inner().unwrap()
}

/// Builds the jobs `next` hands out until none is left, adding each case's answer to `done`.
fn build_jobs(
    jobs: &[&[Prepared]],
    next: &AtomicUsize,
    scratch: &Path,
    done: &Mutex<BTreeMap<(String, String), Answer>>,
) {
    loop {
        let i = next.fetch_add(1, Ordering::Relaxed);
        let Some(chunk) = jobs.get(i) else { break };
        let answers = native::build(&scratch.join(i.to_string()), chunk);
        let mut done = done.lock().unwrap();
        for ((name, expr, _), a) in chunk.iter().zip(answers) {
            done.insert((name.clone(), expr.clone()), a);
        }
    }
}

/// The suite's data, in cel-core-conformance's checkout: cargo fetches its cel-spec submodule.
/// Offline, metadata resolves only this build's host: cargo fetched no other's crates.
fn testdata() -> PathBuf {
    let host = host();
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = std::process::Command::new(cargo)
        .args([
            "metadata",
            "--format-version=1",
            "--offline",
            "--filter-platform",
            &host,
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

/// The target triple rustc builds for.
fn host() -> String {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let version = std::process::Command::new(rustc)
        .arg("-vV")
        .output()
        .unwrap();
    let version = String::from_utf8(version.stdout).unwrap();
    let host = version.lines().find_map(|l| l.strip_prefix("host: "));
    host.unwrap().to_string()
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
