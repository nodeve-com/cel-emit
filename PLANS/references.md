# References

What cel-emit writes should cost about what a person would write for the same job. A reference is the example written by hand: ordinary Rust reading its input and giving its expected output. A standing harness reports each reference beside the generated program, so every change to the generator shows what it moved. cel-core's interpreter runs the same expression as a third program: the cost a reader skips by using cel-emit, and the figures README's Why quotes. Everything here is native Rust; the embedder chooses which target a program builds for.

## Layout

- An example with a reference holds `<name>.rs` beside its `<name>.cel`: one public function, the example's input text in, its output text out, the shape of the generated program's `output`. It measures the example, never runs it: the example still needs no Rust.
- The harness takes every example holding both.
- The interpreter program runs cel-core at its best: the harness parses and checks the example's `.cel` ahead of time and saves the result as a `CheckedExpr` protobuf (`cel-core-proto`), the way cel-emit translates ahead of time. The program loads that saved expression, turns the input text into cel-core values, evaluates, and writes the output text. Its cost never includes parse or check. Load and run count apart: an embedder pays load once and run per record. Turning text into values counts with run, since a user of the interpreter pays it too.

## Measures

Each program, generated, by hand, and interpreted, built with the same rustc and flags:

| Measure | What | Where |
| - | - | - |
| Source | lines and bytes of Rust, the generated program's own and the reference's | every test run |
| Compile effort | LLVM IR lines and defined functions, for the crate and for the expression's function alone, from `--emit=llvm-ir`; generated and by hand only | every test run |
| Binary | bytes of each stripped release executable | every test run |
| Allocations | calls and bytes over one run, counted by a `#[global_allocator]` the harness adds to each build; the interpreter's load and run apart | every test run |
| Peak memory | most bytes live at once, from the same allocator | every test run |
| Instructions | executed, total and per function, by callgrind with collection on only inside the expression's function, or the interpreter's load and run | on Linux, by script |

Wall-clock time is no measure here: load and frequency move it between runs. Instruction counts repeat run to run, and compare only within one CPU feature set, libc, and toolchain.

## Steps

1. **The harness.** `cel-emit-cli/tests/references.rs` builds each reference and its generated program (with the schema when the example has one) the way `answer_rows.rs` builds the program. It fails when either output differs from `expect.json`, and prints the measures as a table. From step 5, allocation calls gate: each pair fails when the generated program's count runs past the reference's by more than the pair's ratio, set at the goal, never at a measured figure. The table only shows every other measure: source lines gain nothing from a ceiling, and IR moves with each rustc.
2. **Instructions.** `scripts/instructions.sh` builds the same pairs and runs each under callgrind, printing totals and the costliest functions. It runs on any Linux host with valgrind; nothing runs it automatically.
3. **`answer_rows` as the first pair.** `examples/answer_rows/rows.rs` exists; it joins the harness with step 1.
4. **A finding per gap.** Each measure where the generated program runs far past its reference gets a cause, then a fix in the generator or a reason it stays. An allocation gap that stays has its reason in research/, and its pair's ratio widens by what that reason accounts for.
5. **The allocation gate on.** Once step 4 has fixed or explained every allocation gap, the harness enforces the ratios.
6. **The interpreter beside them.** The harness and `scripts/instructions.sh` save each example's `CheckedExpr`, build the interpreter program for it, and add its column. Its output must match `expect.json` too; it never gates cost.
7. **Why from the harness.** README's Why states the interpreter's cost against cel-emit's from these figures, approximately, and names the command that prints them. The board benchmark and the CEL-to-wasm comparison leave README: nothing in this repo reruns them.

## Gaps already seen in `answer_rows`

Read from the generated `output`, unmeasured:

- An output map with constant keys becomes a dynamic map per row, each key a new `String`, encoded afterwards; a struct with `Serialize` writes it directly.
- Work that doesn't change inside a `filter` or `first` closure runs per element: the closure indexes `row.legs[0]` for every location tried.
- Runtime functions take `String` by value, so callers clone; `to_utc` and `iso_duration` among them.
- String `==`, `+`, and `&&` over known types each return a `Result`. Why is unread; the reason, if any, goes here before they change.
- The schema's decode reads text into a `serde_json::Value`, then into structs: two passes. That decode is schema-emit's.
