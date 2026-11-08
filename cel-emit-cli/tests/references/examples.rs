//! The examples holding a reference, and each variant of one.

use std::path::{Path, PathBuf};

/// An example holding a reference, or one variant of it.
pub(super) struct Example {
    pub(super) name: String,
    pub(super) cel: String,
    pub(super) reference: String,
    pub(super) schema: Option<String>,
    pub(super) functions: Option<String>,
    pub(super) input: String,
    pub(super) input_path: PathBuf,
    pub(super) expect: String,
}

pub(super) fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Each example in `dir` that holds a reference: the `<name>.cel` beside its `<name>.rs`, and each
/// variant `<name>-<variant>.cel`, which the same reference measures, with its functions in
/// `<name>-<variant>.functions.json`.
pub(super) fn examples_in(dir: &Path) -> Vec<Example> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let files: Vec<PathBuf> = entries.map(|e| e.unwrap().path()).collect();
    let named = |p: &PathBuf| p.file_name().unwrap().to_string_lossy().into_owned();
    let with = |suffix: &str| files.iter().find(|p| named(p).ends_with(suffix));
    let inputs = files.iter().find(|p| {
        let file = named(p);
        let other = ["expect.json", ".schema.json", ".functions.json"];
        file.ends_with(".json") && !other.iter().any(|o| file.ends_with(o))
    });
    let Some(input) = inputs else {
        return Vec::new();
    };
    let schema = with(".schema.json").map(|p| read(p));
    let cels = files
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "cel"));
    let shared = Shared { input, schema };
    cels.filter_map(|cel| variant(dir, cel, &shared)).collect()
}

/// What an example's variants share: the input and the schema.
struct Shared<'a> {
    pub(super) input: &'a PathBuf,
    pub(super) schema: Option<String>,
}

/// The example `cel` is, if a `<name>.rs` beside it is its reference: `<name>` is what its file
/// name holds before a `-`.
fn variant(dir: &Path, cel: &Path, shared: &Shared) -> Option<Example> {
    let stem = cel.file_stem()?.to_string_lossy().into_owned();
    let base = stem.split('-').next()?;
    let reference = std::fs::read_to_string(dir.join(format!("{base}.rs"))).ok()?;
    let functions = dir.join(format!("{stem}.functions.json"));
    let dir_name = dir.file_name()?.to_string_lossy();
    let variant = stem.strip_prefix(base).unwrap_or("");
    Some(Example {
        name: format!("{dir_name}{variant}").replace('-', "_"),
        cel: read(cel),
        reference,
        schema: shared.schema.clone(),
        functions: functions.exists().then(|| read(&functions)),
        input: read(shared.input),
        input_path: shared.input.clone(),
        expect: read(&dir.join("expect.json")),
    })
}
