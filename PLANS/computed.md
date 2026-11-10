# Computed properties

Computed properties and their index in cel-emit ([computed properties](../docs/computed.md)), and the CLI's `--computed` on top ([cel-emit-cli § computed properties](../cel-emit-cli/README.md#computed-properties)).

## Build

1. **The entry's type in cel-emit-runtime**: a property's key, the properties it reads, and its function from the record to a failure. A failure holds its place and its reason.
2. **The order.** Each property after the ones its expression reads and its `after`, ties in key order; a cycle fails, naming its properties.
3. **A property's function, in cel-emit.** It lowers the expression over the record's properties, emits it, and wraps it: compute, call the caller's check, store. The caller gives each property's expression, CEL text or a typed tree, and its `after`, as Rust values; and each property's type, the record's Rust type, where each property sits, and each check.
4. **The index**: the entries in the order they fill. An entry whose input is empty doesn't run.
5. **`into`.** The computed properties fill one object property; its properties join the record's as names.
6. **[cel-emit-schema-bridge](../cel-emit-schema-bridge/README.md)**, a workspace member: `cel-emit-cli`'s `schema_types` moves into it, and each property's schema-emit check joins it.
7. **The CLI's computed properties**: two or more `.cel` files, each keyed by its file's name, with `--into`, through the bridge from `--schema`. Its `main` walks the index: it prints each failure, and exits non-zero when the schema requires a property left empty.
8. **`answer_rows` as computed properties**: `rows.cel` and [`location.cel`](../examples/answer_rows/location.cel) run together. The record's schema holds `input`, the answer with its `from` and `to` declared; `rows`; and `location`, a `Summary`. `location` reads `rows`, so `rows` fills first though `location` sorts before it. Its `rows` matches `expect.json`.
9. **The docs page as a module.** `docs/computed.md` becomes the doc of the module holding computed properties, and the README links it by name, as it does [`missing`](../docs/missing.md).

cel-emit's tests cover each bullet of [computed properties](../docs/computed.md); the bridge's tests cover its README; a test program under `cel-emit-cli/tests/programs` covers the CLI's section.

## Open

- A failing field inside a message an expression builds: `emit`'s expression gives one result for the whole literal, so naming `label.text` needs the literal emitted a field at a time.
- Values computed properties share: whether one may fill a property the record's type doesn't print.
- An expression reading something beside the record it fills, such as a second record.
- A record whose property is another record's type, or its own.
