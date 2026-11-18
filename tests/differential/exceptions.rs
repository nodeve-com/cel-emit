//! The cases the differential test lets refuse or disagree.

/// Expressions cel-emit refuses today, by design or as a gap: the harness fails on any other
/// refusal, and on one of these that emits.
pub const REFUSED: &[&str] = &[
    // A dyn in arithmetic, ordering, `type()`, or compared with a typed value: docs/missing.md.
    "json.decode('{\"a\":1}').a + 1",
    "json.decode('{\"a\":1}').a < 2",
    "json.decode('[1,2]').all(x, x > 0)",
    "type(json.decode('1')) == int",
    "cel.bind(x, [], x == [])",
    "cel.bind(m, {}, m == {})",
    // A map literal keyed by other than strings.
    "{1: 'x', 'y': 2}",
];

/// Expressions known to disagree today: the harness fails on any other disagreement, and on one
/// of these that agrees.
pub const KNOWN_FAILING: &[&str] = &[
    // The interpreter reads presence on a dyn that is no map as absent; selecting a field of a
    // value with none is an error in CEL, so the emitted code gives one.
    "has(json.decode('[1]').a)",
    "json.decode('[1]').?a.hasValue()",
];

/// Expressions where the emitted code departs from the interpreter by design: a dyn fills a
/// typed number field by converting when it can, where CEL takes only the value's own kind.
pub const INTENDED_DEPARTURES: &[&str] = &[
    "P{d: json.decode('{\"d\":1}').d}.d",
    "P{i: json.decode('{\"i\":2.0}').i}.i",
    "P{u: json.decode('{\"u\":2}').u}.u",
];
