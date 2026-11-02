//! The math extension where cel-emit follows cel-go over cel-core. The cel-spec suite (the
//! conformance test) covers the rest of `math`.

use cel_core::Value::{Double as D, Int as I, UInt};

use crate::cases::{check, list, refusal, Got};

/// Only here: `round` is exact where cel-core adds a half; `greatest` and `least` err on a NaN
/// they must order, as cel-go does; a list of a thousand folds; a mixed int, uint, and double
/// extreme compiles under a cast and compile refuses it bare.
#[test]
fn math_follows_cel_go_where_cel_core_differs() {
    check(|cases| {
        for x in [0.49999999999999994, 4503599627370497.0, -4503599627370497.0] {
            cases.gives("math.round(x)", &[("x", D(x))], Got::Value(D(x.round())));
        }
        cases.gives("math.round(x)", &[("x", D(-0.4))], Got::Value(D(-0.0)));
        cases.gives("int(math.round(x))", &[("x", D(2.5))], Got::Value(I(3)));
        cases.gives("int(math.round(x))", &[("x", D(1e19))], Got::Error);
        for x in [2.5, -2.5, 0.5, -0.5, 1e300, f64::INFINITY, f64::NAN] {
            cases.agrees("math.round(x)", &[("x", D(x))]);
        }
        let nan = f64::NAN;
        for f in ["greatest", "least"] {
            for (x, y) in [(nan, 1.0), (1.0, nan)] {
                let inputs = [("x", D(x)), ("y", D(y))];
                cases.gives(&format!("math.{f}(x, y)"), &inputs, Got::Error);
            }
            let xs = [("xs", list(vec![D(1.0), D(nan)]))];
            cases.gives(&format!("math.{f}(xs)"), &xs, Got::Error);
            // One alone is never compared.
            let one = [("x", D(nan))];
            cases.gives(&format!("math.{f}(x)"), &one, Got::Value(D(nan)));
        }
        let long = (0..1000).map(|i| D(f64::from((i * 7919) % 1000))).collect();
        let inputs = [("xs", list(long))];
        cases.gives("math.greatest(xs)", &inputs, Got::Value(D(999.0)));
        cases.gives("math.least(xs)", &inputs, Got::Value(D(0.0)));
        let big = "double(math.greatest(1.797693e308, 1))";
        cases.gives(big, &[], Got::Value(D(1.797693e308)));
        cases.gives("uint(math.greatest(5u, -7))", &[], Got::Value(UInt(5)));
        cases.gives("double(math.least(5u, -7))", &[], Got::Value(D(-7.0)));
    });
    for source in ["math.greatest(1.797693e308, 1)", "math.greatest(5u, -7)"] {
        assert!(
            matches!(
                refusal(source, &[]),
                Some(cel_emit::Error::Unsupported(m)) if m.contains("double(math.greatest(")
            ),
            "{source}"
        );
    }
}
