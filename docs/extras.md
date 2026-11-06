# Extras

What expressions get beyond CEL's standard library.

An embedder also adds expansions, which rewrite a call as it lowers from an argument's name or literal text, as a CEL macro does. An embedder's function may take a message type as its last argument, as in `f(x, Message)`: the type picks the overload declared for that message and passes nothing at run time. An unknown type is an error. The function may share a name with one of cel-emit's, such as `json.decode`: the type argument tells them apart.

Beyond CEL's standard library, expressions get CEL's math, strings, optional-value, and lists extensions, plus:

- `duration('7d')`: as [`humantime`](https://docs.rs/humantime) reads it, signed: CEL's units plus `d`, `w`, `M` (30.44 days), and `y` (365.25 days).
- `json.decode(text)`: JSON text as a dynamic value: a whole number an int, others a double; a repeated key its last value; malformed text an error. A dynamic value fills a typed scalar field of its own kind, a whole number fills a double field, an int or uint fills the other when it fits, and a double with no fraction fills an int or uint one; a fraction is a cast error, and a number past the field's width a range error. A dynamic value fills an expression's result type the same way ([`lower::lower_as`](crate::lower::lower_as)); the README gives the rule. A dynamic value fills a message field, or a list of messages, through the type's own `serde::Deserialize` where the embedder's `Messages::deserializes` says it has one.
- `json.encode(value)`: a value as JSON text; NaN or infinity an error. With `Decls.skip_failed`, it leaves out a failing field, unless none remains.
- `list.first()` and `list.last()`: a list's first or last item, if it has one.
- `math.pow(x, y)`: `x` to the power `y`.
- `math.sum(list)`: a list of numbers added up; an empty list sums to zero.
- The lists extension is cel-go's, `sortBy` included, but `sort()` of a list of dyn orders one kind only and `flatten(depth)` takes a list of dyn.
