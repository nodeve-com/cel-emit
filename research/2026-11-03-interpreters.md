# cel-emit against interpreters

Is translating CEL ahead of time worth it, against interpreting it? `answer_rows` ran as five programs. cel-emit generated one and a person wrote the reference; cel-core's interpreter and the `cel` crate's (0.15.0) ran the expression. The fifth did the same job in Python 3.14, written as an agent writes it, since agents reach for Python first. Each counts by phase: `decode` reads the input text into the program's values, `expression` turns them into the output text, and an interpreter's `load` readies the expression once. The first three come from `cel-emit-cli/tests/references.rs` and `scripts/instructions.sh`. The `cel` and Python programs were one-offs outside the repository, the `cel` one built the same way.

## Result

Instructions on aarch64 Linux, by callgrind collecting inside each phase. Allocation calls from a counting allocator, the same on every run and platform.

| Instructions | generated | reference | cel-core | `cel` | Python |
| - | - | - | - | - | - |
| decode | 1,091,221 | 242,478 | 872,908 | 746,099 | 1,342,629 |
| expression | 89,844 | 42,291 | 1,064,462 | 2,318,632 | 391,887 |
| load, once | - | - | 2,825,481 | 15,549,441 | 51,493,905 |

| Allocations | generated | reference | cel-core | `cel` | Python |
| - | - | - | - | - | - |
| decode | 1,351 | 51 | 1,820 | 863 | 1,149 |
| expression | 145 | 38 | 1,172 | 3,297 | 274 |
| load, once | - | - | 4,640 | 28,384 | 23,439 |

Stripped release binaries on macOS: generated 2.09 MB, reference 2.04 MB, cel-core 3.99 MB, `cel` 3.41 MB.

## What this says

- **The expression is cel-emit's case.** The generated expression runs about 12 times fewer instructions than cel-core's and 26 times fewer than `cel`'s, within about 2 times of the reference.
- **cel-core is the interpreter to beat.** `cel` runs about twice cel-core's instructions and allocations per expression, building every output map as a `HashMap` hashed with SipHash. Its load costs about 5.5 times cel-core's, nearly every instruction in the ANTLR parser, and checks no types.
- **Python's expression beats both interpreters, not cel-emit.** It runs about 4.4 times the generated expression's instructions and 9 times the reference's, but about a third of cel-core's: its datetime, regex, and JSON encode run in C, and the program indexes locations and products by id once. Its decode, `json.loads` in C, runs the most of all five, about 5.5 times the reference's. Its load, imports and the first record's zone and regex setup, costs about 18 times cel-core's, before the interpreter's own start. Nothing bounds what it does: it may loop forever or reach files and the network, where a CEL expression always ends and touches only its input.
- **The decode loses to both interpreters.** The generated decode parses the text into a `serde_json::Value`, then reads that into schema-emit's structs: most of its instructions build the intermediate tree's ordered maps. The reference's serde derive reads straight into its structs, skipping fields it lacks. A decode from text is schema-emit's to write.

## What it leaves out

- One example. Another expression or input shape may weigh decode and expression differently.
- cel-core's input is a JSON map, not typed messages: it builds no message values without protobuf descriptors. The expression still checks against the schema's types.
- cel-core's binary links cel-emit for its environment and JSON functions, so its size overstates cel-core's.
- `cel`'s decode is a serde visitor written for the one-off, reading straight into the crate's values. Its own path, through `serde_json::Value` and `cel::to_value`, made 5,106 decode allocations and raised the expression's to 6,195.
- Python's figures are per record in steady state: the difference between runs of 1 and 101 records over the same input, divided by 100. The Rust programs' are a single record's. Its load leaves out the interpreter's own start, 72,446,647 instructions and 36,923 allocations. Its allocations are calls to `malloc` with `PYTHONMALLOC=malloc`, counted by memcheck; Python's free lists for small objects still skip some.
- Python's output matched `expect.json` byte for byte.
- `cel`'s output matched `expect.json` as JSON values; its maps order keys arbitrarily. Its added `json.encode` was not checked against cel-emit's on fractional seconds, durations, or whole-number doubles, which this example never writes.
