use super::*;
use crate::time::{duration_at, duration_text, timestamp_at};
use crate::Error;

fn s(v: &'static str) -> String {
    String::from(v)
}

/// `fmt` over `xs`, a list of one type.
fn run<T: Arg + Clone>(fmt: &'static str, xs: Vec<T>) -> Result<String> {
    format(fmt, &xs)
}

#[test]
fn percent_is_escaped_and_extra_arguments_go_unused() {
    assert_eq!(
        run::<i64>("no substitution", vec![]),
        Ok("no substitution".into())
    );
    assert_eq!(run("%%%s%%", vec![s("text")]), Ok("%text%".into()));
    assert_eq!(run("%s%%", vec![s("right")]), Ok("right%".into()));
    assert_eq!(
        run("%%escaped %s%%", vec![s("percent")]),
        Ok("%escaped percent%".into())
    );
    assert_eq!(run("%s", vec![s("a"), s("b")]), Ok("a".into()));
}

#[test]
fn missing_arguments_and_bad_clauses_are_errors() {
    assert_eq!(run("%d %d %d", vec![0i64, 1]), Err(Error::Function));
    assert_eq!(run("%a", vec![1i64]), Err(Error::Function));
    assert_eq!(run("%E", vec![0.5f64]), Err(Error::Function));
    assert_eq!(run("%", vec![1i64]), Err(Error::Function));
    assert_eq!(run("%.s", vec![1i64]), Err(Error::Function));
    assert_eq!(run("%.101f", vec![1.0f64]), Err(Error::Function));
}

#[test]
fn integers_in_each_base_and_sign() {
    assert_eq!(
        run("this is 5 in binary: %b", vec![5i64]),
        Ok("this is 5 in binary: 101".into())
    );
    assert_eq!(run("%b", vec![u64::MAX]).map(|s| s.len()), Ok(64));
    assert_eq!(run("%o", vec![11i64]), Ok("13".into()));
    assert_eq!(run("%o", vec![65535u64]), Ok("177777".into()));
    assert_eq!(run("%x is 20", vec![30i64]), Ok("1e is 20".into()));
    assert_eq!(run("%X is 20", vec![30i64]), Ok("1E is 20".into()));
    assert_eq!(run("%X", vec![6000u64]), Ok("1770".into()));
    assert_eq!(run("%d", vec![-42i64]), Ok("-42".into()));
    assert_eq!(run("%x", vec![-255i64]), Ok("-ff".into()));
    assert_eq!(
        run("%d", vec![999_999_999_999i64]),
        Ok("999999999999".into())
    );
    assert_eq!(run("%d", vec![64u64]), Ok("64".into()));
}

#[test]
fn bools_and_bytes_in_their_clauses() {
    assert_eq!(run("%b", vec![true]), Ok("1".into()));
    assert_eq!(run("%b", vec![false]), Ok("0".into()));
    assert_eq!(run("%s, %s", vec![true, false]), Ok("true, false".into()));
    assert_eq!(
        run("%x", vec![s("Hello world!")]),
        Ok("48656c6c6f20776f726c6421".into())
    );
    assert_eq!(
        run("%X", vec![s("Hello world!")]),
        Ok("48656C6C6F20776F726C6421".into())
    );
    assert_eq!(
        run("%x", vec![Vec::from(b"byte string")]),
        Ok("6279746520737472696e67".into())
    );
    assert_eq!(run("%s", vec![Vec::from(b"xyz")]), Ok("xyz".into()));
    assert_eq!(run("%s", vec![Vec::from(&[0xff_u8])]), Err(Error::Function));
}

#[test]
fn fixed_and_scientific_take_numbers_with_their_precision() {
    assert_eq!(run("%.3f", vec![1.2345f64]), Ok("1.234".into()));
    assert_eq!(run("%.5f", vec![-1.2f64]), Ok("-1.20000".into()));
    assert_eq!(run("%f", vec![2.75f64]), Ok("2.750000".into()));
    assert_eq!(run("%f", vec![2i64]), Ok("2.000000".into()));
    assert_eq!(run("%f", vec![3u64]), Ok("3.000000".into()));
    assert_eq!(
        run("%.6e", vec![1052.032911275f64]),
        Ok("1.052033e+03".into())
    );
    assert_eq!(run("%e", vec![2.75f64]), Ok("2.750000e+00".into()));
    assert_eq!(run("%e", vec![2i64]), Ok("2.000000e+00".into()));
    assert_eq!(run("%e", vec![3u64]), Ok("3.000000e+00".into()));
    assert_eq!(run("%e", vec![0.0f64]), Ok("0.000000e+00".into()));
    assert_eq!(run("%e", vec![1e-7f64]), Ok("1.000000e-07".into()));
}

#[test]
fn fixed_rounds_to_nearest_even_on_exact_ties() {
    assert_eq!(run("%.0f", vec![123.000000f64]), Ok("123".into()));
    assert_eq!(run("%.0f", vec![3.5001f64]), Ok("4".into()));
    assert_eq!(run("%.0f", vec![3.4999f64]), Ok("3".into()));
    assert_eq!(run("%.0f", vec![1.5f64]), Ok("2".into()));
    assert_eq!(run("%.0f", vec![2.5f64]), Ok("2".into()));
    assert_eq!(run("%.3f", vec![123.4999f64]), Ok("123.500".into()));
    assert_eq!(run("%.3f", vec![-123.4994f64]), Ok("-123.499".into()));
}

#[test]
fn non_finite_doubles_by_name() {
    let nan = f64::NAN;
    let inf = f64::INFINITY;
    assert_eq!(run("%e", vec![nan]), Ok("NaN".into()));
    assert_eq!(run("%e", vec![inf]), Ok("Infinity".into()));
    assert_eq!(run("%e", vec![-inf]), Ok("-Infinity".into()));
    assert_eq!(run("%d", vec![nan]), Ok("NaN".into()));
    assert_eq!(run("%d", vec![inf]), Ok("Infinity".into()));
    assert_eq!(run("%d", vec![-inf]), Ok("-Infinity".into()));
    assert_eq!(run("%f", vec![nan]), Ok("NaN".into()));
    assert_eq!(run("%f", vec![-inf]), Ok("-Infinity".into()));
    assert_eq!(run("%s", vec![inf]), Ok("Infinity".into()));
}

#[test]
fn clauses_refuse_the_types_they_do_not_write() {
    assert_eq!(run("%d", vec![0.5f64]), Err(Error::Function));
    assert_eq!(run("%x", vec![0.5f64]), Err(Error::Function));
    assert_eq!(run("%b", vec![s("abc")]), Err(Error::Function));
    assert_eq!(run("%o", vec![s("a string")]), Err(Error::Function));
    assert_eq!(run("%d", vec![true]), Err(Error::Function));
    assert_eq!(
        run("%d", vec![duration_at(1800, 0).unwrap_or_default()]),
        Err(Error::Function)
    );
    assert_eq!(
        run("%x", vec![timestamp_at(0, 0).unwrap_or_default()]),
        Err(Error::Function)
    );
}

#[test]
fn time_values_as_text() {
    let t = timestamp_at(1_675_467_080, 0);
    assert_eq!(
        run("%s", vec![t.unwrap_or_default()]),
        Ok("2023-02-03T23:31:20Z".into())
    );
    let d = duration_at(6347, 0);
    assert_eq!(run("%s", vec![d.unwrap_or_default()]), Ok("6347s".into()));
    assert_eq!(
        duration_at(1, 500_000_000).map(duration_text),
        Ok("1.5s".into())
    );
    assert_eq!(
        duration_at(-1, -500_000_000).map(duration_text),
        Ok("-1.5s".into())
    );
}

#[test]
fn lists_print_their_elements_as_text() {
    let nan = f64::NAN;
    let inf = f64::INFINITY;
    let doubles = vec![nan, inf, -inf];
    assert_eq!(
        run("%s", vec![doubles]).as_deref(),
        Ok("[NaN, Infinity, -Infinity]")
    );
    let ints = vec![9i64, 8];
    assert_eq!(run("%s", vec![ints]).as_deref(), Ok("[9, 8]"));
    assert_eq!(run("%d", vec![vec![1i64]]), Err(Error::Function));
}
