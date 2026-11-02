# cel-emit

Translates [CEL](https://cel.dev) to Rust: CEL text in, Rust source out. The resulting program carries no interpreter.

## In short

Given `temp` (v0) and `limit` (v1) as doubles, the CEL string `temp > limit ? "hot" : "ok"` becomes the Rust string:

```rust,ignore
(if s::cmp::gt(v0, v1)? { String::from("hot") } else { String::from("ok") })
```

A message is the caller's type: cel-emit type-checks its fields, and [`emit::Messages`] names its Rust. The caller writes each struct [`emit::Items::messages`] lists.

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
- **Add your own functions.** A new function is its CEL signature plus the Rust that runs it ([`functions::Function`]).

What it doesn't translate yet, largest gap first: [`missing`].

## Conformance

- **cel-go settles** where cel-core departs from it; interop with cel-go isn't a goal. cel-core comes from a fork (the workspace `Cargo.toml`).
- **A float32's text departs from CEL**, which has only doubles: `string()`, `format`'s `%s`, and `json.encode` write its own shortest digits, `9.3`, not `9.300000190734863`.
- **The cel-spec suite** is cel-emit's own test, `cargo nextest run -p cel-emit --test conformance`. It builds each case natively and checks the expected value. No case may be wrong; the passing cases must match `tests/conformance/pass.txt`.

## Extras

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

## Use

Translation takes two steps. First, [`lower`](mod@lower) rewrites the checked expression in cel-emit's own simpler form: a tree of small pieces, each marked with its type. Compilers call that step lowering. Next, [`emit`] writes the tree as Rust that uses cel-emit-runtime.

cel-emit knows nothing about the program around it. [`decls::Decls`] tells it what an expression reads and calls, [`emit::Vars`] how to read each input, and your program writes the Rust around the translated code.

cel-emit-runtime's functions sit in [`stdlib`]'s tables; its README gives their conventions.
