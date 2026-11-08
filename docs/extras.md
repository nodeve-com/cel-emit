# Extras

What expressions get beyond CEL's standard library.

An embedder also adds expansions, which rewrite a call as it lowers from an argument's name or literal text, as a CEL macro does. An embedder's function may take a message type as its last argument, as in `f(x, Message)`: the type picks the overload declared for that message and passes nothing at run time. An unknown type is an error. The function may share a name with one of cel-emit's, such as `json.decode`: the type argument tells them apart.

An embedder may also write a function in CEL: its name, its parameters in order, each a name and a type, and a body that reads only them. cel-emit checks the body once and writes it as one Rust function returning the body's type, which every call checks its arguments against and runs. An error inside the body is the call's error. A body may call another written this way, never in a cycle.

Beyond CEL's standard library, expressions get CEL's math, strings, optional-value, and lists extensions, plus:

- `duration('7d')`: as [`humantime`](https://docs.rs/humantime) reads it, signed: CEL's units plus `d`, `w`, `M` (30.44 days), and `y` (365.25 days).
- `json.decode(text)`: JSON text as an [any-value](https://github.com/nodeve-com/any-value) `Value`, which owns how numbers and repeated keys build; malformed text is an error. A dynamic value fills a typed place by any-value's `to` rule, in any-value's README: a wrong kind or a fraction in a whole-number place is a cast error, and a number past the place's width a range error. It fills an expression's result type the same way ([`lower::lower_as`](crate::lower::lower_as)), as cel-emit's README says. A dynamic value fills a message field, or a list of messages, through the type's own `serde::Deserialize` where the embedder's `Messages::deserializes` says it has one.
- `json.encode(value)`: a value as JSON text; NaN or infinity an error. With `Decls.skip_failed`, it leaves out a failing field, unless none remains.
- `list.first()` and `list.last()`: a list's first or last item, if it has one.
- `math.pow(x, y)`: `x` to the power `y`.
- `math.sum(list)`: a list of numbers added up; an empty list sums to zero.
- The lists extension is cel-go's, `sortBy` included, but `sort()` of a list of dyn orders one kind only and `flatten(depth)` takes a list of dyn.
