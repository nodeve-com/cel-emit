# References

What cel-emit writes should cost about what a person would write for the same job. A reference is the example written by hand: ordinary Rust reading its input and giving its expected output. A standing harness reports each reference beside the generated program, so every change to the generator shows what it moved. Everything here is native Rust; which target a program is built for is the embedder's concern.

## Layout

- An example with a reference holds `<name>.rs` beside its `<name>.cel`: one public function, the example's input text in, its output text out, the shape of the generated program's `output`. It measures the example, never runs it: the example still needs no Rust.
- The harness takes every example holding both.

## Measures

Each pair, generated and by hand, built with the same rustc and flags:

| Measure | What | Where |
| - | - | - |
| Source | lines and bytes of Rust, the generated program's own and the reference's | every test run |
| Compile effort | LLVM IR lines and defined functions, for the crate and for the expression's function alone, from `--emit=llvm-ir` | every test run |
| Allocations | calls and bytes over one run, counted by a `#[global_allocator]` the harness adds to each build | every test run |
| Instructions | executed, total and per function, by callgrind with collection on only inside the expression's function | on Linux, by script |

Wall-clock time is no measure here. Instruction counts compare only within one machine and one toolchain.

## Steps

1. **The harness.** `cel-emit-cli/tests/references.rs` builds each reference and its generated program (with the schema when the example has one) the way `answer_rows.rs` builds the program. It fails when either output differs from `expect.json`, and prints the measures as a table. Figures are reported, never gates.
2. **Instructions.** `scripts/instructions.sh` builds the same pairs and runs each under callgrind, printing totals and the costliest functions. It runs on any Linux host with valgrind; nothing runs it automatically.
3. **`answer_rows` as the first pair.** `examples/answer_rows/rows.rs` exists; it joins the harness with step 1.
4. **A finding per gap.** Each measure where the generated program runs far past its reference gets a cause, then a fix in the generator or a reason it stays.

## Gaps already seen in `answer_rows`

Read from the generated `output`, unmeasured:

- An output map with constant keys becomes a dynamic map per row, each key a new `String`, encoded afterwards; a struct with `Serialize` writes it directly.
- Work that doesn't change inside a `filter` or `first` closure runs per element: `row.legs[0]` is indexed for every location tried.
- Runtime functions take `String` by value, so callers clone; `to_utc` and `iso_duration` among them.
- String `==`, `+`, and `&&` over known types each return a `Result`. Why is unread; the reason, if any, goes here before they change.
- The schema's decode reads text into a `serde_json::Value`, then into structs: two passes. That decode is schema-emit's.
