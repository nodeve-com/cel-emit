# cel-emit-runtime

What the Rust that `cel-emit` writes links: CEL's semantics, one module per area, over std values. It exports nothing and registers no allocator: the crate cel-emit writes picks both.

## Why

Emitted Rust needs CEL's semantics, not its compiler. Kept apart from cel-emit, this crate builds once per target for `wasm32` without `std`'s clock or IO, and each emitted crate links it, keeping only the functions it calls.

## Conventions

- **A function registers** in cel-emit's table for its area (`cel-emit/src/stdlib/<area>.rs`): the overload id cel-core's checker resolves a call to, and the path of a function here. That function takes CEL's values as Rust's, in the overload's order, a method's receiver first, and returns [`Result`]: `fn contains(s: &str, sub: &str) -> Result<bool>`. One that only reads its arguments takes those not `Copy` by reference, and its table entry says so (`Std::reads`). Its module is its area's: `strings`, `time`, `math`, `conversions`, `collections`, `lists`, `fields`, `cmp`. A type lowering doesn't take yet (`types::of_cel`) or a form no overload names lowers in cel-emit's `lower` and emits in its `emit`, which every area shares.
- **Values** are std's: int `i64`, uint `u64`, double `f64`, bool `bool`, string `String`, bytes `Vec<u8>`, list `Vec<T>`, map `Vec<(K, V)>` in the order written, timestamp chrono's `DateTime<Utc>`, duration [`TimeDelta`], a message a struct cel-emit writes.
- **Errors are values, never panics.** A CEL error is an [`Error`]; the first one raised wins, by `?`, except where [`cmp::and`], [`cmp::or`], and the comprehensions absorb one as CEL does. Its lints deny `unwrap`, indexing, and unchecked arithmetic: a panic traps the whole module.
- **Overflow**: int and uint arithmetic and conversions past their range are [`Error::Overflow`], division or modulus by zero [`Error::Division`]; a double follows IEEE 754, NaN and infinities included; a timestamp or duration past CEL's range overflows.
- **Check** a function with unit tests beside it, and an area against the cel-spec suite.
- **Size**: calling `json.decode` and `json.encode` adds about 67 KB to a wasm32 module (`opt-level=s`, fat LTO); a module calling neither links neither.
- **Reuse crates** before writing one's work: `chrono` for time, `pattern-emit` for a constant `matches` pattern's table, `regex-lite` for a dynamic one (linked only where called; not RE2, so case folds in ASCII only, `\s` takes `\v`, and a `\p` class doesn't parse), `libm` where builds must agree to the bit. A new dependency builds for `wasm32` without `std`'s clock, IO, or a proc macro.
