//! A repeated subtree computed once, and `filter(…)[0]` reading no further than its match, on
//! values that fail where a subtree left its place would.

use cel_core::Value::{self, Bool as B, Int as I};

use crate::cases::{check, list, map, s, Cases, Got};

/// A map lacking `a`, whose `c` holds a `b`.
fn lacking() -> Value {
    map(&[("c", map(&[("b", s("x"))]))])
}

/// Each expression gives what `m`, lacking `a`, leaves it, as it does repeating nothing.
fn stays(cases: &mut Cases, m: &str, inputs: &[(&str, Value)]) {
    let twice = format!("({m}.a.b + 't') + ({m}.a.b + 't')");
    let held = |text: String| text.replace("TWICE", &twice).replace("M", m);
    let none = Got::Value(s("none"));
    cases.gives(&held("has(M.a) ? TWICE : 'none'".into()), inputs, none);
    let branches = "has(M.a) ? (TWICE == '' ? TWICE : TWICE) : 'none'";
    let none = Got::Value(s("none"));
    cases.gives(&held(branches.into()), inputs, none);
    let absorbed = "TWICE == 'x' && false";
    cases.gives(&held(absorbed.into()), inputs, Got::Value(B(false)));
    let either = "(TWICE == 'x' || true) && (TWICE == 'y' || true)";
    cases.gives(&held(either.into()), inputs, Got::Value(B(true)));
    let empty = "[0].filter(i, i > 0).map(i, TWICE).size() == 0";
    cases.gives(&held(empty.into()), inputs, Got::Value(B(true)));
    let decided = "[1, 2].exists(i, i == 2 || TWICE == 'x')";
    cases.gives(&held(decided.into()), inputs, Got::Value(B(true)));
    cases.gives(&held("[TWICE, TWICE]".into()), inputs, Got::Error);
}

/// Only here: nothing lifted past a condition, an absorbing `&&` or `||`, or a loop fails where
/// the expression gives a value; a subtree that always runs fails as it did.
#[test]
fn a_repeated_subtree_fails_only_where_it_ran() {
    check(|cases| {
        stays(cases, "m", &[("m", lacking())]);
        let text = s(r#"{"c": {"b": "x"}}"#);
        stays(cases, "json.decode(t)", &[("t", text)]);
        let held = map(&[("a", map(&[("b", s("x"))]))]);
        let present = "has(m.a) ? (m.a.b + 't') + (m.a.b + 't') : 'none'";
        cases.gives(present, &[("m", held)], Got::Value(s("xtxt")));
        cases.gives(
            "[1, 2].map(i, [i + 1, i + 1])[1][0] + [1, 2].map(j, j + 1)[0]",
            &[],
            Got::Value(I(5)),
        );
    });
}

/// Only here: a match gives its element, and no match the error an index past the list is; an
/// element before the match still fails it, and one after is never read.
#[test]
fn the_first_match_stands_for_the_filter() {
    check(|cases| {
        let xs = [("xs", list(vec![I(1), I(2), I(3)]))];
        cases.gives("xs.filter(x, x > 1)[0]", &xs, Got::Value(I(2)));
        cases.gives("xs.filter(x, x > 5)[0]", &xs, Got::Error);
        cases.gives("xs.filter(x, x > 1)[1]", &xs, Got::Value(I(3)));
        let edges = [("xs", list(vec![I(0), I(1)]))];
        cases.gives("xs.filter(x, 1 / x == 1)[0]", &edges, Got::Error);
        let edges = [("xs", list(vec![I(1), I(0)]))];
        cases.gives("xs.filter(x, 1 / x == 1)[0]", &edges, Got::Value(I(1)));
        let rows = [("t", s(r#"[{"id": "a", "v": "1"}, {"id": "b", "v": "2"}]"#))];
        let found = "json.decode(t).filter(r, r.id == 'b')[0].v + ''";
        cases.gives(found, &rows, Got::Value(s("2")));
        let none = "json.decode(t).filter(r, r.id == 'q')[0].v + ''";
        cases.gives(none, &rows, Got::Error);
    });
}

/// Only here: a local holds a read of a dyn where what it reads lasts, and a copy where that is
/// a temporary.
#[test]
fn a_local_holds_a_dyn_read_either_way() {
    check(|cases| {
        let doc = [("t", s(r#"{"a": {"b": "x"}, "l": ["y", "z"]}"#))];
        for (body, want) in [
            ("cel.bind(e, d.a, e.b + '' + d.a.b)", "xx"),
            ("d.?a.?b.orValue('n') + 'm'", "xm"),
            ("d.?q.?b.orValue('n') + 'm'", "nm"),
            ("cel.bind(f, d.l.filter(x, x == 'z')[0], f + '' + f)", "zz"),
        ] {
            let source = format!("cel.bind(d, json.decode(t), {body})");
            cases.gives(&source, &doc, Got::Value(s(want)));
        }
        let temporary = "cel.bind(e, json.decode(t).a, e.b + '' + e.b)";
        cases.gives(temporary, &doc, Got::Value(s("xx")));
        let list = "cel.bind(d, json.decode(t), d.l.slice(1, 2).size() + d.l.slice(0, 2).size())";
        cases.gives(list, &doc, Got::Value(I(3)));
    });
}
