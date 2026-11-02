# Missing

What cel-emit refuses today that CEL allows, largest gap first. cel-emit refuses each with its reason, never translating it wrong. `cargo nextest run -p cel-emit --test conformance --no-capture` counts the cases each reason costs. A gap closes when a definition needs it.

## Values of no single type

A `dyn`, a mixed list or map, or a decoded JSON value runs as one dynamic value: selection, index, `has`, `.?`, `size`, equality, `in`, and casts take it. A loop reads it as a list of dyn, so a map held in one fails where CEL would walk its keys. Arithmetic, ordering, and `type()` over it are missing, as is a dyn read as a number where an operand wants one. cel-emit refuses `optional.none()` with nothing to say what it holds, by design.

## Wrappers

A wrapper built on its own, such as `google.protobuf.Int32Value{}`.

## Types as values

`type(x) == int`, `type(int)`, and a type named in full, such as `google.protobuf.Timestamp`.

## Names in a container

An expression checked inside a container (a namespace), reading or building a declared name by its short or full name.

## Smaller gaps

- **`null` as a value of its own type**, outside a nullable field.
- **A list literal of records**, such as `[Sample{v: 1}]`: refused as "a list of records".
- **A list of records in `json.decode(text, Message)`**: refused as "a field in a decode".
- **Selecting through a nullable message**: `undefined field`, as CEL's checker reads it as a wrapper. An optional field holds one instead.
- **`flatten(depth)` of a typed nested list**, such as a list of lists of ints: no conversion makes a dyn list of one nested two deep, so only a list of dyn takes the depth form. `flatten()` takes either. `[].flatten()` has no element type to read.
- **A comprehension written out by hand**, rather than through a macro such as `all` or `map`; and `transformMap` over a non-empty map.
- **A duration written `.5s`, `.5h`, or `5.s`**: `humantime` reads none of them, and CEL does.
- **A constant that always fails**, such as `duration('320000000000s')` or a `matches` pattern Go rejects: refused when checked, where CEL fails it when run.
- **A constant `matches` pattern with no table**: one RE2 reads unlike the Rust parser, or whose table passes pattern-emit's cap (Unicode classes, long counted repetition). The refusal says which.

## Beyond expressions

- **Null as a step's output**, and a nullable field in a record the host reads: neither has a layout yet.
- **A narrow field that may be missing**: a nullable or optional one `json.decode` fills, or one a `?`-entry or a value of another type writes.
