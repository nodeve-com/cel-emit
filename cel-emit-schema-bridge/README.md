# cel-emit-schema-bridge

Builds what cel-emit's [computed properties](../docs/computed.md) take from a [JSON Schema](https://json-schema.org), through [schema-emit](https://github.com/nodeve-com/schema-emit). A program holding a schema for its record hands this crate the schema, the computed properties' keys, and `into` where one object property holds them. It gets back:

- **Each property's type.** A model schema-emit reads becomes a cel-emit record type: a field per field, a nested model a nested record. A type cel-emit can't hold fails with its reason, never widened.
- **The record's Rust type**, schema-emit's, and where each property sits in it.
- **Each property's check**, the one schema-emit writes for its schema. A failure names its place inside the value, as `label.text`.
- **Each named type**: a `$defs` entry, which an expression builds by its name, as `Label{ text: 'a' }`.
- **The computed properties the schema requires.** The Rust type holds each as one that may be empty, since a failed expression leaves it so. Whoever walks the index judges them.

[cel-emit-cli](../cel-emit-cli/README.md#computed-properties) builds its programs this way.
