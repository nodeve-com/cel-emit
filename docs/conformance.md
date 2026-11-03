# Conformance

Where cel-emit departs from CEL, and the test that holds it to CEL.

- **cel-go settles** where cel-core departs from it; interop with cel-go isn't a goal. cel-core comes from a fork (the workspace `Cargo.toml`).
- **A float32's text departs from CEL**, which has only doubles: `string()`, `format`'s `%s`, and `json.encode` write its own shortest digits, `9.3`, not `9.300000190734863`.
- **`filter(…)[0]` departs from CEL** past its first match: it stops there, so a predicate that would fail on a later element fails nothing.
- **The cel-spec suite** is cel-emit's own test, `cargo nextest run -p cel-emit --test conformance`. It builds each case natively and checks the expected value. No case may be wrong; the passing cases must match `tests/conformance/pass.txt`.
