//! Functions an expression may call, each a CEL declaration and, per overload, the Rust running
//! it: CEL's standard library and math and strings extensions (their Rust in [`crate::stdlib`]),
//! the lists extension, cel-emit's own (`json`, a list's `first()` and `last()`, `math.pow`,
//! `math.sum`), and any an embedder adds through [`Function`] or [`crate::Expansion`].

use std::collections::HashMap;

use cel_core::ext::{encoders_extension, math_extension, optionals_extension, string_extension};
use cel_core::{CelType, FunctionDecl, OverloadDecl, Value};

use crate::decls::Decl;
use crate::types::Type;

mod json;
mod lists;

/// A function an embedder adds: its CEL declaration, and each overload's Rust, by its id: the
/// path of a function taking the overload's arguments in order, a method's receiver first, and
/// returning `cel_emit_runtime::Result` of its result, as cel-emit-runtime's do.
#[derive(Clone, Debug)]
pub struct Function {
    pub decl: Decl,
    pub paths: Vec<(String, String)>,
}

/// One overload's Rust in cel-emit-runtime: its id, its path under the crate, and whether it takes
/// arguments of these types, where its declaration leaves them open.
pub struct Std {
    pub id: &'static str,
    pub path: &'static str,
    pub takes: fn(&[Type]) -> bool,
    /// Whether it reads its arguments of other than [`Type::copied`] types by reference.
    pub borrows: bool,
}

impl Std {
    /// For an overload whose declaration fixes its types.
    pub const fn any(id: &'static str, path: &'static str) -> Std {
        Std {
            id,
            path,
            takes: |_| true,
            borrows: false,
        }
    }

    /// For an overload whose declaration fixes its types and which only reads its arguments.
    pub const fn reads(id: &'static str, path: &'static str) -> Std {
        Std {
            borrows: true,
            ..Std::any(id, path)
        }
    }
}

/// What the Rust cel-emit writes calls cel-emit-runtime by.
pub const STD: &str = "s";

/// Overload ids to the Rust running them, built once: an embedder's first, then cel-emit-runtime's.
#[derive(Default)]
pub(crate) struct Index(HashMap<String, Vec<Entry>>);

enum Entry {
    Own(String),
    Std(&'static Std),
}

impl Index {
    pub(crate) fn new(functions: &[Function]) -> Index {
        let mut map: HashMap<String, Vec<Entry>> = HashMap::new();
        for (id, path) in functions.iter().flat_map(|f| &f.paths) {
            map.entry(id.clone())
                .or_default()
                .push(Entry::Own(path.clone()));
        }
        for s in crate::stdlib::ALL.iter().flat_map(|table| table.iter()) {
            map.entry(s.id.into()).or_default().push(Entry::Std(s));
        }
        Index(map)
    }

    /// The Rust path running overload `id` on arguments of `args`' types, if one does.
    pub(crate) fn path(&self, id: &str, args: &[Type]) -> Option<String> {
        self.0.get(id)?.iter().find_map(|e| match e {
            Entry::Own(path) => Some(path.clone()),
            Entry::Std(s) => (s.takes)(args).then(|| format!("{STD}::{}", s.path)),
        })
    }

    /// Whether overload `id` on `args`' types reads them by reference.
    pub(crate) fn borrows(&self, id: &str, args: &[Type]) -> bool {
        self.0.get(id).is_some_and(|entries| {
            entries.iter().any(|e| match e {
                Entry::Own(_) => false,
                Entry::Std(s) => s.borrows && (s.takes)(args),
            })
        })
    }
}

/// The parameters each declared overload takes, by id.
pub(crate) fn params<'a>(
    declared: impl IntoIterator<Item = &'a FunctionDecl>,
) -> HashMap<String, Vec<CelType>> {
    declared
        .into_iter()
        .flat_map(|f| &f.overloads)
        .map(|o| (o.id.clone(), o.params.clone()))
        .collect()
}

/// Every declaration an expression checks against beyond the standard library and an embedder's.
pub(crate) fn declarations() -> Vec<FunctionDecl> {
    let mut all = typed_math();
    all.extend(optionals_extension());
    all.extend(string_extension());
    all.extend(encoders_extension());
    all.extend(own());
    all
}

/// cel-emit's own: `json.decode` and `json.encode`, a list's `first()` and `last()`, and the lists
/// extension.
fn own() -> Vec<FunctionDecl> {
    let end = |name: &str, id: &str| {
        FunctionDecl::new(name).with_overload(
            OverloadDecl::method(
                id,
                vec![CelType::list(CelType::type_param("T"))],
                CelType::optional(CelType::type_param("T")),
            )
            .with_type_params(vec!["T".to_string()]),
        )
    };
    let mut all = vec![end("first", "list_first"), end("last", "list_last")];
    all.extend(json::declarations());
    all.extend(lists::declarations());
    all
}

/// The math extension without its `dyn` overloads, plus `math.pow` and `math.sum`. Where a `dyn`
/// overload matches beside a typed one, the checker joins their results to `dyn`; without them,
/// every call it accepts has one type. `math.greatest` and `math.least` keep theirs: a mixed call
/// is a number that only a cast or comparison takes ([`crate::lower`]), and a call the checker
/// joins to `dyn` but whose operands agree lowers as its typed overload.
fn typed_math() -> Vec<FunctionDecl> {
    let dyn_free = |t: &CelType| !matches!(t, CelType::Dyn) && *t != CelType::list(CelType::Dyn);
    let pow = OverloadDecl::function(
        "math_pow_double_double",
        vec![CelType::Double, CelType::Double],
        CelType::Double,
    )
    .with_impl(|args| match args {
        [Value::Double(x), Value::Double(y)] => Value::Double(libm::pow(*x, *y)),
        _ => Value::error("math.pow takes two doubles"),
    });
    math_extension()
        .into_iter()
        .map(|mut f| {
            if !matches!(f.name.as_str(), "math.greatest" | "math.least") {
                f.overloads
                    .retain(|o| dyn_free(&o.result) && o.params.iter().all(dyn_free));
            }
            f
        })
        .chain([
            FunctionDecl::new("math.pow").with_overload(pow),
            FunctionDecl::new("math.sum").with_overloads([sum(CelType::Int), sum(CelType::Double)]),
        ])
        .collect()
}

/// `math.sum` of a list of `ty`, int or double: over none, zero; an int past int64 an error.
fn sum(ty: CelType) -> OverloadDecl {
    let id = format!("math_sum_list_{}", ty.display_name());
    let double = ty == CelType::Double;
    OverloadDecl::function(id, vec![CelType::list(ty.clone())], ty).with_impl(move |args| {
        let [Value::List(items)] = args else {
            return Value::error("math.sum takes a list");
        };
        let mut total = if double {
            Value::Double(0.0)
        } else {
            Value::Int(0)
        };
        for item in items.iter() {
            total = match (total, item) {
                (Value::Int(a), Value::Int(b)) => match a.checked_add(*b) {
                    Some(v) => Value::Int(v),
                    None => return Value::error("math.sum: int overflow"),
                },
                (Value::Double(a), Value::Double(b)) => Value::Double(a + b),
                _ => return Value::error("math.sum of other than ints or doubles"),
            };
        }
        total
    })
}
