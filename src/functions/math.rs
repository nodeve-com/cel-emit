//! cel-emit's own math functions: `math.pow`, `math.sqrt` (cel-go's, which cel-core lacks), and
//! the statistics of a list: `math.sum`, `math.mean`, `math.variance`, `math.stddev`,
//! `math.median`, and `math.percentile`. Each interpreter runs the cel-emit-runtime function the
//! Rust cel-emit writes calls.

use cel_core::{CelType, FunctionDecl, OverloadDecl, Value};
use cel_emit_runtime::math;

/// `math.pow`, `math.sqrt`, and the statistics.
pub(super) fn declarations() -> Vec<FunctionDecl> {
    let mut all = vec![pow(), sqrt()];
    all.extend(statistics());
    all
}

/// `math.pow(x, y)` of two doubles.
fn pow() -> FunctionDecl {
    let overload = OverloadDecl::function(
        "math_pow_double_double",
        vec![CelType::Double, CelType::Double],
        CelType::Double,
    )
    .with_impl(|args| match args {
        [Value::Double(x), Value::Double(y)] => value("pow", math::pow(*x, *y)),
        _ => Value::error("math.pow takes two doubles"),
    });
    FunctionDecl::new("math.pow").with_overload(overload)
}

/// `math.sqrt(x)` of an int, uint, or double, as a double.
fn sqrt() -> FunctionDecl {
    let overloads = [CelType::Int, CelType::UInt, CelType::Double].map(|ty| {
        let id = format!("math_sqrt_{}", ty.display_name());
        OverloadDecl::function(id, vec![ty], CelType::Double).with_impl(|args| match args {
            [Value::Int(x)] => value("sqrt", math::sqrt_int(*x)),
            [Value::UInt(x)] => value("sqrt", math::sqrt_uint(*x)),
            [Value::Double(x)] => value("sqrt", math::sqrt(*x)),
            _ => Value::error("math.sqrt takes an int, uint, or double"),
        })
    });
    FunctionDecl::new("math.sqrt").with_overloads(overloads)
}

/// `math.<name>`'s result as a CEL value, or its error.
fn value(name: &str, result: cel_emit_runtime::Result<impl Into<Value>>) -> Value {
    match result {
        Ok(v) => v.into(),
        Err(e) => Value::error(format!("math.{name}: {e:?}")),
    }
}

/// The statistics of a list of ints or doubles.
fn statistics() -> Vec<FunctionDecl> {
    let double = || Some(CelType::Double);
    vec![
        statistic("sum", None, &[], of_list!(math::sum_list)),
        statistic("mean", double(), &[], of_list!(math::mean_list)),
        statistic("variance", double(), &[], of_list!(math::variance_list)),
        statistic("stddev", double(), &[], of_list!(math::stddev_list)),
        statistic("median", double(), &[], of_list!(math::median_list)),
        statistic(
            "percentile",
            double(),
            &[CelType::Int, CelType::Double],
            |xs, rest| {
                let p = match rest {
                    [Value::Int(p)] => *p as f64,
                    [Value::Double(p)] => *p,
                    _ => return Err(cel_emit_runtime::Error::Function),
                };
                numbers!(xs, v => math::percentile_list(&v, p))
            },
        ),
    ]
}

/// A list's ints or doubles, as a statistic's interpreter reads them.
enum Numbers {
    Ints(Vec<i64>),
    Doubles(Vec<f64>),
}

/// `run` on `xs`'s ints or doubles, bound to `v`, its result a CEL value.
macro_rules! numbers {
    ($xs:expr, $v:ident => $run:expr) => {
        match $xs {
            Numbers::Ints($v) => $run.map(Value::from),
            Numbers::Doubles($v) => $run.map(Value::from),
        }
    };
}
use numbers;

/// A statistic's interpreter running `f` on the list alone.
macro_rules! of_list {
    ($f:path) => {
        |xs, _| numbers!(xs, v => $f(&v))
    };
}
use of_list;

/// What a statistic's interpreter runs: the list, then the arguments after it.
type Run = fn(Numbers, &[Value]) -> cel_emit_runtime::Result<Value>;

/// `math.<name>` of a list of ints or of doubles, then, where `then` names types, one more
/// argument of any of them, each overload's id naming its types. Its result is of `result`, or
/// with none, of the list's element.
fn statistic(name: &str, result: Option<CelType>, then: &[CelType], run: Run) -> FunctionDecl {
    let then: Vec<Option<&CelType>> = match then {
        [] => vec![None],
        some => some.iter().map(Some).collect(),
    };
    let mut overloads = Vec::new();
    for ty in [CelType::Int, CelType::Double] {
        for after in &then {
            let mut id = format!("math_{name}_list_{}", ty.display_name());
            let mut params = vec![CelType::list(ty.clone())];
            if let Some(after) = after {
                id = format!("{id}_{}", after.display_name());
                params.push((*after).clone());
            }
            let result = result.clone().unwrap_or(ty.clone());
            let (name, doubles) = (name.to_string(), ty == CelType::Double);
            overloads.push(
                OverloadDecl::function(id, params, result)
                    .with_impl(move |args| interpret(&name, doubles, run, args)),
            );
        }
    }
    FunctionDecl::new(format!("math.{name}")).with_overloads(overloads)
}

/// `run` on `args`, a list of doubles or of ints, as `doubles` says, then the rest.
fn interpret(name: &str, doubles: bool, run: Run, args: &[Value]) -> Value {
    let Some((Value::List(items), rest)) = args.split_first() else {
        return Value::error(format!("math.{name} takes a list"));
    };
    let numbers = if doubles {
        items
            .iter()
            .map(f64::try_from)
            .collect::<Result<_, _>>()
            .map(Numbers::Doubles)
    } else {
        items
            .iter()
            .map(i64::try_from)
            .collect::<Result<_, _>>()
            .map(Numbers::Ints)
    };
    match numbers {
        Ok(xs) => value(name, run(xs, rest)),
        Err(_) => Value::error(format!("math.{name} of other than ints or doubles")),
    }
}
