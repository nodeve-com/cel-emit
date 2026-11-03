# Extras

What expressions get beyond CEL's standard library.

An embedder also adds expansions, which rewrite a call as it lowers from an argument's name or literal text, as a CEL macro does.

Beyond CEL's standard library, expressions get CEL's math, strings, optional-value, and lists extensions, plus:

- `duration('7d')`: as [`humantime`](https://docs.rs/humantime) reads it, signed: CEL's units plus `d`, `w`, `M` (30.44 days), and `y` (365.25 days).
- `json.decode(text)`: JSON text as a dynamic value: a whole number an int, others a double; a repeated key its last value; malformed text an error. A dynamic value fills a typed field only of its own kind.
- `json.decode(text, Message)`: JSON text as a `Message`, reading only its declared fields and skipping the rest unread, or keeping them as one JSON object text, as they arrived, in the field `Decls.rest` names; an integral number such as `2.0` fills an int field. `Decls.fill` says how a field takes a dynamic value.
- `json.encode(value)`: a value as JSON text; NaN or infinity an error. With `Decls.skip_failed`, it leaves out a failing field, unless none remains.
- `list.first()` and `list.last()`: a list's first or last item, if it has one.
- `math.pow(x, y)`: `x` to the power `y`.
- `math.sum(list)`: a list of numbers added up; an empty list sums to zero.
- The lists extension is cel-go's, `sortBy` included, but `sort()` of a list of dyn orders one kind only and `flatten(depth)` takes a list of dyn.
