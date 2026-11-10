# cel-emit-schema-bridge

Builds what cel-emit's [computed properties](../docs/computed.md) take from a [JSON Schema](https://json-schema.org), through [schema-emit](https://github.com/nodeve-com/schema-emit). A program holding a schema for its record hands this crate the schema and gets back all three:

- **Each property's type.** A model schema-emit reads becomes a cel-emit record type: a field per field, a nested model a nested record. A type cel-emit can't hold fails with its reason, never widened.
- **The record's Rust type**, schema-emit's, and where each property sits in it.
- **Each property's check**, the one schema-emit writes for its schema. A failure names its place inside the value, as `label.text`.

[cel-emit-cli](../cel-emit-cli/README.md#computed-properties) builds its programs this way.
