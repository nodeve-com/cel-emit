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

`cel-emit-cli/tests/references.rs` builds each pair and prints the table (`--no-capture`); `scripts/instructions.sh` runs the pairs under callgrind. `answer_rows` is the first pair.

1. **A finding per gap.** Each measure where the generated program runs far past its reference gets a cause, then a fix in the generator or a reason it stays. An allocation gap that stays has its reason in research/, and its pair's ratio widens by what that reason accounts for.
2. **The allocation gate on.** Once step 1 has fixed or explained every allocation gap, allocation calls gate: each pair fails when the generated program's count runs past the reference's by more than the pair's ratio, set at the goal, never at a measured figure. Every other measure only shows: source lines gain nothing from a ceiling, and IR moves with each rustc.
3. **The interpreter beside them.** The harness and `scripts/instructions.sh` save each example's `CheckedExpr`, build the interpreter program for it, and add its column. Its output must match `expect.json` too; it never gates cost.
4. **Why from the harness.** README's Why states the interpreter's cost against cel-emit's from these figures, approximately, and names the command that prints them. The board benchmark and the CEL-to-wasm comparison leave README: nothing in this repo reruns them.
5. **Instructions run once.** `scripts/instructions.sh` has never run: no Linux host with valgrind was at hand. Its first run checks it.

## Gaps in `answer_rows`

Measured by phase, allocation calls:

- **The decode's `serde_json::Value`** makes most of the generated program's allocations and nearly all its peak memory. The schema decode after it allocates about what the reference's whole parse does. A decode straight from text, as serde's derive reads, ends the pass. That decode is schema-emit's.
- **The expression** allocates far more than the reference's rows do. Causes read from the generated `expression`, each unconfirmed until its fix moves the count:
  - An output map with constant keys becomes a dynamic map per row, each key a new `String`, encoded afterwards; a struct with `Serialize` writes it directly.
  - Work that doesn't change inside a `filter` or `first` closure runs per element: the closure indexes `row.legs[0]` for every location tried.
  - Runtime functions take `String` by value, so callers clone; `to_utc` and `iso_duration` among them.
  - String `==`, `+`, and `&&` over known types each return a `Result`. Why is unread; the reason, if any, goes here before they change.
