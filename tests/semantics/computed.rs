//! Computed properties: an index built natively and walked over records, each line an entry's
//! outcome; and what translation refuses.

use std::sync::atomic::{AtomicUsize, Ordering};

use cel_emit::computed::{Compute, Computed, Expression, Property, Record};
use cel_emit::emit::Emitter;
use cel_emit::{Decls, Error, Field, Type};

use crate::common::bind::Bound;
use crate::common::{build_run, items, Positional};

mod refused;

fn label() -> Type {
    let text = Field {
        name: "text".into(),
        ty: Type::String,
        required: true,
    };
    Type::named("Label", vec![text])
}

fn property(key: &str, ty: Type, place: &str) -> Property {
    Property {
        key: key.into(),
        ty,
        place: place.into(),
        empty: place.starts_with("data.") && key != "limit",
        check: (key == "total").then(|| "check_total".into()),
    }
}

/// What the input gives, `qty` and `tag` perhaps empty, and under `data` what expressions fill,
/// `limit` always holding a value.
pub fn record() -> Record {
    let optional = |ty| Type::Optional(Box::new(ty));
    let mut properties = vec![
        property("text", Type::String, "text"),
        property("qty", Type::Int, "qty"),
        property("price", Type::Int, "price"),
        property("tag", optional(label()), "tag"),
        property("bonus", optional(Type::Int), "data.bonus"),
        property("label", label(), "data.label"),
        property("shown", Type::String, "data.shown"),
        property("tagged", Type::Bool, "data.tagged"),
    ];
    for key in ["early", "last", "limit", "total"] {
        properties.push(property(key, Type::Int, &format!("data.{key}")));
    }
    if let Some(qty) = properties.get_mut(1) {
        qty.empty = true;
    }
    Record {
        rust: "Record".into(),
        properties,
    }
}

pub fn compute(key: &str, cel: &str) -> Compute {
    Compute {
        key: key.into(),
        expression: Expression::Cel(cel.into()),
        after: Vec::new(),
        required: key == "total",
    }
}

pub fn decls() -> Decls {
    let mut decls = Decls::default();
    decls.messages.insert("Label".into(), label());
    decls
}

/// Listed out of the order they fill in. `early` reads nothing computed and fills after `total`;
/// `last` is a tree its caller lowered.
fn computes(computed: &Computed) -> Vec<Compute> {
    let last = cel_emit::lower("limit + 1", &computed.decls().unwrap()).unwrap();
    vec![
        compute("label", "Label{text: text + ': ' + string(total)}"),
        compute("shown", "string(bonus.orValue(0))"),
        compute("bonus", "100 / (total - 6)"),
        compute("total", "qty * price"),
        compute("tagged", "has(tag.text)"),
        compute("limit", "price / (qty - 2)"),
        Compute {
            after: vec!["total".into()],
            ..compute("early", "price + 1")
        },
        Compute {
            expression: Expression::Tree(last),
            ..compute("last", "")
        },
    ]
}

const RECORDS: &str = "
#[derive(Clone, Debug, Default)]
pub struct Data {
    pub bonus: Option<i64>, pub early: Option<i64>, pub label: Option<Declared0>,
    pub last: Option<i64>, pub limit: i64, pub shown: Option<String>, pub tagged: Option<bool>,
    pub total: Option<i64>,
}
#[derive(Clone, Debug, Default)]
pub struct Record {
    pub text: String, pub qty: Option<i64>, pub price: i64, pub tag: Option<Declared0>,
    pub data: Data,
}
fn check_total(value: i64) -> Result<i64, s::computed::Failure> {
    match value > 1000 {
        true => Err(s::computed::Failure::check(\"total\", \"range\")),
        false => Ok(value),
    }
}
fn main() {
    for entry in INDEX.iter() {
        println!(\"{} {:?} {}\", entry.key, entry.reads, entry.required);
    }
    let tag = Some(Declared0 { f0: \"t\".into() });
    for (n, (qty, price, tag)) in [(Some(3), 2, tag), (None, 2, None), (Some(2), 600, None), (Some(4), 4, None)].into_iter().enumerate() {
        let data = Data { limit: 7, ..Data::default() };
        let mut record = Record { text: \"a\".into(), qty, price, tag, data };
        for entry in INDEX.iter() {
            println!(\"{n} {} {:?}\", entry.key, (entry.fill)(&mut record));
        }
        println!(\"{n} {:?}\", record.data);
    }
}
";

/// The lines the walk prints: each entry, then per record each outcome and what `data` holds.
fn walked() -> &'static [String] {
    static LINES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    LINES.get_or_init(built)
}

fn built() -> Vec<String> {
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    let (record, decls) = (record(), decls());
    let computed = Computed {
        record: &record,
        decls: &decls,
        vars: &Bound(Vec::new()),
    };
    let mut emitter = Emitter::new(&[], Box::new(Positional));
    assert_eq!(emitter.rust(&label()).unwrap(), "Declared0");
    let index = computed
        .index("INDEX", &computes(&computed), &mut emitter)
        .unwrap_or_else(|e| panic!("{e}"));
    let items = items(&mut emitter).unwrap();
    let source =
        format!("#![allow(warnings)]\nuse ::cel_emit_runtime as s;\n{items}{index}{RECORDS}");
    let run = RUNS.fetch_add(1, Ordering::Relaxed);
    let name = format!("cel-emit-computed-{}-{run}", std::process::id());
    let dir = std::env::temp_dir().join(name);
    let lines = build_run(&dir, &source);
    let _ = std::fs::remove_dir_all(&dir);
    lines.unwrap_or_else(|e| panic!("rustc: {e}"))
}

/// The lines of record `n`, without its number.
fn of(lines: &[String], n: usize) -> Vec<&str> {
    let prefix = format!("{n} ");
    lines
        .iter()
        .filter_map(|line| line.strip_prefix(&prefix))
        .collect()
}

/// Only here: the index lists each property after what it reads and its `after`, the rest in key
/// order, each entry with its key, reads, and `required`.
#[test]
fn an_index_lists_each_property_after_what_it_reads() {
    let entries: Vec<&str> = walked().iter().take(8).map(String::as_str).collect();
    let want = [
        r#"limit ["price", "qty"] false"#,
        r#"last ["limit"] false"#,
        r#"tagged ["tag"] false"#,
        r#"total ["price", "qty"] true"#,
        r#"bonus ["total"] false"#,
        r#"early ["price"] false"#,
        r#"label ["text", "total"] false"#,
        r#"shown ["bonus"] false"#,
    ];
    assert_eq!(entries, want);
}

/// Only here: the walk over four records: every property filling, an empty input skipping what
/// reads it, a failure and a check's refusal leaving a property as it was, an optional read and
/// an `after` going on regardless.
#[test]
fn a_walk_fills_skips_or_fails_each_property() {
    let lines = walked();
    let division = |key: &str| {
        format!(r#"{key} Failed(Failure {{ place: "{key}", reason: Expression(Division) }})"#)
    };
    let filled = of(lines, 0);
    assert_eq!(filled[4], division("bonus"));
    assert_eq!(
        filled[8],
        r#"Data { bonus: None, early: Some(3), label: Some(Declared0 { f0: "a: 6" }), last: Some(3), limit: 2, shown: Some("0"), tagged: Some(true), total: Some(6) }"#
    );
    let empty = of(lines, 1);
    let skipped = ["limit", "total", "bonus", "label"].map(|key| format!("{key} Skipped"));
    assert_eq!([empty[0], empty[3], empty[4], empty[6]], skipped);
    assert_eq!(
        empty[8],
        r#"Data { bonus: None, early: Some(3), label: None, last: Some(8), limit: 7, shown: Some("0"), tagged: Some(false), total: None }"#
    );
    let refused = of(lines, 2);
    assert_eq!(refused[0], division("limit"));
    assert_eq!(
        refused[3],
        r#"total Failed(Failure { place: "total", reason: Check("range") })"#
    );
    assert_eq!([refused[4], refused[6]], ["bonus Skipped", "label Skipped"]);
    assert!(refused[8].contains("limit: 7") && refused[8].contains("total: None"));
    let all = of(lines, 3);
    assert!(all[..8].iter().all(|line| line.ends_with(" Filled")));
    assert!(all[8].contains("bonus: Some(10)") && all[8].contains(r#"shown: Some("10")"#));
}

/// What translating `computes` over `record` refuses.
pub fn refusal(record: &Record, decls: &Decls, computes: &[Compute]) -> Error {
    let computed = Computed {
        record,
        decls,
        vars: &Bound(Vec::new()),
    };
    let mut emitter = Emitter::new(&[], Box::new(Positional));
    computed.index("INDEX", computes, &mut emitter).unwrap_err()
}
