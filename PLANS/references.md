# References

What cel-emit writes should cost about what a person would write for the same job. A reference is the example written by hand: ordinary Rust reading its input and giving its expected output. A standing harness reports each reference beside the generated program, so every change to the generator shows what it moved. cel-core's interpreter runs the same expression as a third program: the cost a reader skips by using cel-emit, and the figures README's Why quotes. Everything here is native Rust; the embedder chooses which target a program builds for.

## Layout

- An example with a reference holds `<name>.rs` beside its `<name>.cel`: two public functions, in order, the shape of the generated program's `decode` and `expression`. The first reads the example's input text into the reference's own input type; the second takes that input and gives the output text. It measures the example, never runs it: the example still needs no Rust.
- The harness takes every example holding both.
- The interpreter program runs cel-core at its best: the harness parses and checks the example's `.cel` ahead of time and saves the result as a `CheckedExpr` protobuf (`cel-core-proto`), the way cel-emit translates ahead of time. The program loads that saved expression, turns the input text into cel-core values, evaluates, and writes the output text. Its cost never includes parse or check.
- Each program counts by phase, each phase its own `measured_<phase>` function that callgrind collects inside: `decode`, input text to the program's typed input (serde_json and schema-emit's decode, the reference's serde derive, or cel-core values); `expression`, that input to output text, its JSON encode included; and the interpreter's `load`, which an embedder pays once, where it pays decode and expression per record. Decode hands its value to expression, so neither phase's cost lands in the other.

## Measures

Each program, generated, by hand, and interpreted, built with the same rustc and flags:

| Measure | What | Where |
| - | - | - |
| Source | lines and bytes of Rust, the generated program's own and the reference's | every test run |
| Compile effort | LLVM IR lines and defined functions, for the crate and for the expression's function alone, from `--emit=llvm-ir`; generated and by hand only | every test run |
| Binary | bytes of each stripped release executable | every test run |
| Allocations | calls and bytes per phase, counted by a `#[global_allocator]` the harness adds to each build, and a record's total, decode and expression | every test run |
| Peak memory | most bytes live at once within a phase, from the same allocator | every test run |
| Instructions | executed per phase, total and per function, by callgrind collecting only inside that phase's function | on Linux, by script |

Wall-clock time is no measure here: load and frequency move it between runs. Instruction counts repeat run to run, and compare only within one CPU feature set, libc, and toolchain.

## Steps

`cel-emit-cli/tests/references.rs` builds each pair and prints the table (`--no-capture`); `scripts/instructions.sh` runs the pairs under callgrind, on Linux; `scripts/linux.sh` runs it on the ssh host `CEL_EMIT_LINUX_HOST` names, from the environment or `.env`. `answer_rows` is the first pair.

1. **A finding per gap.** Each measure where the generated program runs far past its reference gets a cause, then a fix in the generator or a reason it stays. An allocation gap that stays has its reason in research/, and its pair's ratio widens by what that reason accounts for.
2. **The allocation gate on.** Once step 1 has fixed or explained every allocation gap, allocation calls gate: each pair fails when the generated program's count runs past the reference's by more than the pair's ratio, set at the goal, never at a measured figure. Every other measure only shows: source lines gain nothing from a ceiling, and IR moves with each rustc.
3. **Why from the harness.** README's Why states the interpreter's cost against cel-emit's from these figures, approximately, and names the command that prints them. The board benchmark and the CEL-to-wasm comparison leave README: nothing in this repo reruns them.

## Gaps in `answer_rows`

Measured by phase, allocation calls:

- **The decode** reads the input text through schema-emit, allocating about what the reference's serde derive does.
- **The expression** allocates about what the reference's rows do. What stays costs instructions, not allocations, each unconfirmed until callgrind on Linux shows it:
  - Work that doesn't change inside a `filter` or `first` closure runs per element: the closure indexes `row.legs[0]` for every location tried.
  - String `==`, `+`, and `&&` over known types each return a `Result`. Why is unread; the reason, if any, goes here before they change.
  - Each output member writes through its own let and typed JSON call, more own IR than one dynamic map encoded once.
