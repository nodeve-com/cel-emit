//! A decoded document's repeated keys, the strict reading of a dyn into a field, and a message
//! encoded: `P` has a field of each kind, a nullable `f`, and a nested `n`; `Q` a required `q` and
//! a nullable `r` (declared.rs). The interpreter has no message values, so each case is read
//! against the CEL in `REFERENCES`; a case with none is evaluated as it is.

/// Expressions the two evaluators must agree on.
pub const CASES: &[&str] = &[
    r#"json.decode('{"a":1,"a":2}').a"#,
    r#"json.decode('{"a":1,"b":0,"a":{"c":1,"c":2}}').a.c"#,
    r#"json.decode('{"a":1,"b":0,"a":3}') == {'a': 3, 'b': 0}"#,
    r#"P{i: json.decode('{"i":2}').i}.i"#,
    r#"P{i: json.decode('{"i":2.0}').i}.i"#,
    r#"P{u: json.decode('{"u":2}').u}.u"#,
    r#"P{d: json.decode('{"d":1}').d}.d"#,
    r#"P{d: json.decode('{"d":1.5}').d}.d"#,
    r#"P{s: json.decode('{"s":"a"}').s}.s"#,
    // A message encodes as a map of its fields, a nullable one null.
    "json.encode(optional.of(M{n: 1}).value())",
    "json.encode(optional.of(P{i: 2, f: 4}).value())",
    "json.encode(optional.of(Q{q: 1}).value())",
    "json.encode(optional.of(Q{q: 2, r: 3}).value())",
];

/// What the interpreter evaluates for a case: the same answer, written without a message. `1 / 0`
/// stands for a failure.
pub const REFERENCES: &[(&str, &str)] = &[
    (r#"P{i: json.decode('{"i":2}').i}.i"#, "2"),
    (r#"P{i: json.decode('{"i":2.0}').i}.i"#, "1 / 0"),
    (r#"P{u: json.decode('{"u":2}').u}.u"#, "1 / 0"),
    (r#"P{d: json.decode('{"d":1}').d}.d"#, "1 / 0"),
    (r#"P{d: json.decode('{"d":1.5}').d}.d"#, "1.5"),
    (r#"P{s: json.decode('{"s":"a"}').s}.s"#, "'a'"),
    (
        "json.encode(optional.of(M{n: 1}).value())",
        r#"'{"f":null,"n":1,"s":""}'"#,
    ),
    (
        "json.encode(optional.of(P{i: 2, f: 4}).value())",
        r#"'{"i":2,"u":0,"d":0.0,"b":false,"s":"","f":4,"n":{"k":0,"t":""}}'"#,
    ),
    (
        "json.encode(optional.of(Q{q: 1}).value())",
        r#"'{"q":1,"r":null}'"#,
    ),
    (
        "json.encode(optional.of(Q{q: 2, r: 3}).value())",
        r#"'{"q":2,"r":3}'"#,
    ),
];
