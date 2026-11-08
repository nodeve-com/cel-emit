# References

What cel-emit writes should cost about what a person would write for the same job. A reference is the example written by hand: ordinary Rust reading its input and giving its expected output. A standing harness reports each reference beside the generated program, so every change to the generator shows what it moved. cel-core's interpreter runs the same expression as a third program: the cost a reader skips by using cel-emit, and the figures README's Why quotes. Everything here is native Rust; the embedder chooses which target a program builds for.

## Layout

- An example with a reference holds `<name>.rs` beside its `<name>.cel`: two public functions, in order, the shape of the generated program's `decode` and `expression`. The first reads the example's input text into the reference's own input type; the second takes that input and gives the output text. It measures the example, never runs it: the example still needs no Rust.
- The harness takes every example holding both. A variant, `<name>-<variant>.cel`, does the same job another way and measures against the same `<name>.rs`; `<name>-<variant>.functions.json` beside it holds the functions it calls, as the CLI's `--functions` reads them.
- The interpreter program runs cel-core at its best: the harness parses and checks the example's `.cel` ahead of time and saves the result as a `CheckedExpr` protobuf (`cel-core-proto`), the way cel-emit translates ahead of time. The program loads that saved expression, turns the input text into cel-core values, evaluates, and writes the output text. Its cost never includes parse or check.
- Each program counts by phase, each phase its own `measured_<phase>` function that callgrind collects inside: `decode`, input text to the program's typed input (serde_json and schema-emit's decode, the reference's serde derive, or cel-core values); `expression`, that input to output text, its JSON encode included; and the interpreter's `load`, which an embedder pays once, where it pays decode and expression per record. Decode hands its value to expression, so neither phase's cost lands in the other.

## Measures

Each program, generated, by hand, and interpreted, built with the same rustc and flags:

| Measure | What | Where |
| - | - | - |
| Source | lines and bytes of Rust: the generated program's decode, schema-emit's, apart from its expression and the functions that calls, cel-emit's; and the reference's | every test run |
| Compile effort | LLVM IR lines and defined functions, for the crate and for the expression's function alone, from `--emit=llvm-ir`; generated and by hand only | every test run |
| Binary | bytes of each stripped release executable | every test run |
| Allocations | calls and bytes per phase, counted by a `#[global_allocator]` the harness adds to each build, and a record's total, decode and expression | every test run |
| Peak memory | most bytes live at once within a phase, from the same allocator | every test run |
| Instructions | executed per phase, total and per function, by callgrind collecting only inside that phase's function | on Linux, by script |

Wall-clock time is no measure here: load and frequency move it between runs. Instruction counts repeat run to run, and compare only within one CPU feature set, libc, and toolchain.

## Steps

`cel-emit-cli/tests/references.rs` builds each pair and prints the table (`--no-capture`); `scripts/instructions.sh` runs the pairs under callgrind, on Linux; `scripts/linux.sh` runs it on the ssh host `CEL_EMIT_LINUX_HOST` names, from the environment or `.env`. `answer_rows` is the first pair.

1. **A finding per gap.** Each measure where the generated program runs far past its reference gets a cause, then a fix in the generator or a reason it stays in research/.
2. **Why from the harness.** README's Why states the interpreter's cost against cel-emit's from these figures, approximately, and names the command that prints them. The board benchmark and the CEL-to-wasm comparison leave README: nothing in this repo reruns them.

## Gaps in `answer_rows`

Measured by phase:

- **The decode** allocates what the reference's serde derive does and executes about half again its instructions, under the interpreter's decode. The rest sits in schema-emit-runtime's own reader: a serde_json deserializer per token, and a hand-written skip over unknown keys where derive skips through serde_json.
- **The expression** allocates and executes about what the reference's rows do.
