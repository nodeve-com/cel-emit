//! What `sortBy` refuses.

use cel_emit::Type;

use super::field;
use crate::cases::refusal;

/// Only here: what lowering refuses stays refused, and a bad first argument is a parse error.
#[test]
fn sort_by_refuses_what_it_cannot_order() {
    let message = Type::Message(vec![field("a", Type::Int)]);
    let names = [
        ("rs", Type::list(message)),
        ("ls", Type::list(Type::list(Type::Int))),
        ("a", Type::Dyn),
        ("t", Type::String),
    ];
    for source in [
        "rs.sortBy(r, r)",
        "ls.sortBy(l, l)",
        "[1].sortBy(x, [x])",
        "[1].sortBy(x, {'a': x})",
        "[1].sortBy(x, null)",
        "[1].sortBy(x, optional.of(x))",
        "[1].sortBy(x + 1, x)",
        "[1].sortBy('x', x)",
        "[1].sortBy(x)",
        "[1].sortBy(x, x, x)",
        "[1].sortBy(x.y, x)",
        "1.sortBy(x, x)",
        "'a'.sortBy(x, x)",
        "a.sortBy(x, x)",
        "json.decode(t).sortBy(x, x)",
        "xs.sortBy(x, x)",
    ] {
        assert!(refusal(source, &names).is_some(), "{source}");
    }
    for source in [
        "rs.sortBy(r, r.a).size()",
        "ls.sortBy(l, l.size()).size()",
        "ls.sortBy(l, l.size()).map(l, l.size())",
    ] {
        assert_eq!(refusal(source, &names), None, "{source}");
    }
}
