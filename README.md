# cel-emit

Translates [CEL](https://cel.dev) to Rust: CEL text in, Rust source out. The resulting program carries no interpreter.

## In short

Given `temp` (v0) and `limit` (v1) as doubles, the CEL string `temp > limit ? "hot" : "ok"` becomes the Rust string:

```rust,ignore
(if s::cmp::gt(v0, v1)? { String::from("hot") } else { String::from("ok") })
```

A message is the caller's type: cel-emit type-checks its fields, and [`emit::Messages`] names its Rust. A message may carry a name, an opaque string the caller chooses. The name is part of its identity: messages with equal fields and different names are different types, each with its own Rust. The caller writes each struct [`emit::Items::messages`] lists.

A result type works the same way. A number's **width** is the Rust size that holds it, such as `i8`. CEL computes in `int64`, `uint64`, and `float64`, and a [`types::Type::Narrow`] holds any other. [`lower::lower_as`] takes the type an expression returns, and the translated code returns that type, holding each width as a message field does. A result type `int8` returns an `i8`; a value past `i8`'s range is cel-emit-runtime's `Error::Range`, and a `float32` rounds. A list, a map, an optional, or a nullable holds the width of each element or value it carries, by the same rule.

A stored value keeps its width through anything whose answer doesn't depend on the width it computes in:

- passing it to a parameter or field of its width
- a ternary's branch
- `==` or `<` against the same width
- wrapping it in an optional or a list
- `orValue`

It widens to CEL's 64 bits at arithmetic, at a mix of widths, and at a call whose parameter is CEL's type. Widening a `float32` to a double is exact, so the answer is still CEL's. A 128-bit value past 64 bits fails `Range` only where it widens. A written function's parameter of a width takes its argument the way a field of that width does.

Each `s::` piece is a ready-made CEL operation from **cel-emit-runtime**, cel-emit's companion library. **cel-core** is cel-emit's front end: it parses the text and type-checks it, and cel-emit translates the tree and types it gets back. A mistake such as `temp > "x"` stops there. cel-core runs only during translation, so the built program doesn't contain it; its interpreter runs only in cel-emit's tests.

## Why

Other CEL crates for Rust, as of October 2026, fall in two groups:

- **Interpreters**, most of them: they ship a whole CEL engine inside your program and work out each expression while it runs. Built for wasm, cel-core's took 7.3 MB of memory before its first expression ran.
- **Translators**, protovalidate-buffa and ferricel: neither type-checks. Both turn what they can't handle into code that fails when it runs, and let a number that grows too big crash the program or silently wrap. Both also tie you to their own data formats.

A benchmark timed Rust written by hand the way cel-emit writes it, on a 1 GHz single-core ARM board. It ran 4 to 26 times faster than cel-core's interpreter in wasm. It used a few hundred KB of memory. The one exception was regex matching. Now `matches` answers a constant pattern from a search table built at translation ([`pattern_emit`](https://docs.rs/pattern-emit)), checked once by the generated `load_patterns`. A `wasm32` module of one is 39 KB, against 128 KB with the regex engine a dynamic pattern links. Against a hand-built CEL-to-wasm compiler, it ran within about 20%, and handled the lists and maps that compiler refused.

## Features

- **Refused, never stubbed.** An expression that fails cel-core's check, or that cel-emit can't translate, returns an [`Error`].
- **Run-time errors are values.** Overflow, division by zero, or a missing key comes back as a CEL error the caller handles, never a panic.
- **No guessing while it runs.** The type check settles what kind every value is before the program starts: a number stays a plain number, text stays plain text ([`types::Type`]).
- **Small output.** Each CEL operation is its own small function in cel-emit-runtime, so a program includes only the ones its expressions use.
- **Same answer everywhere.** The program never reads the clock: an expression that needs the current time takes it as an input. It never reads the machine's time zones either. A zone written in the expression, such as `"America/New_York"`, becomes that zone's UTC offsets from 1970 to 2100, daylight saving included; `"+05:30"` becomes one offset. All math goes through one library, `libm`, so every machine agrees to the last digit.
- **Computed properties** ([`computed`]): each filled by its own expression, after what it reads.
- **Add your own functions.** A new function is its CEL signature plus the Rust that runs it ([`functions::Function`]), or its typed parameters and a CEL body ([`extras`]).

What it doesn't translate yet, largest gap first: [`missing`]. Functions beyond CEL's standard library: [`extras`]. Where it departs from CEL, and its conformance test: [`conformance`].

## Use

Translation takes two steps. First, [`lower`](mod@lower) rewrites the checked expression in cel-emit's own simpler form: a tree of small pieces, each marked with its type. Compilers call that step lowering. Next, [`emit`] writes the tree as Rust that uses cel-emit-runtime.

cel-emit knows nothing about the program around it. [`decls::Decls`] tells it what an expression reads and calls, [`emit::Vars`] how to read each input, and your program writes the Rust around the translated code. [`cel-emit-cli`](https://github.com/nodeve-com/cel-emit/tree/main/cel-emit-cli) is the shortest way to see it work. It takes a CEL expression over JSON, with a JSON Schema if you have one, and writes a whole Rust program, with no Rust of your own. [`examples/by_hand`](https://github.com/nodeve-com/cel-emit/tree/main/examples/by_hand) uses cel-emit as a library: one expression over a struct you write, translated in a `build.rs`.

cel-emit-runtime's functions sit in [`stdlib`]'s tables; its README gives their conventions.
