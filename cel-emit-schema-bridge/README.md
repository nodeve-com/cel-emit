# cel-emit-schema-bridge

Builds what cel-emit's [computed properties](../docs/computed.md) take from a schema [schema-emit](https://github.com/nodeve-com/schema-emit) has read. This crate reads no schema document and no schema keyword: schema-emit resolves every reference, and this crate reads its result.

A program lowers its record's schema with schema-emit and makes the [standard form](https://github.com/nodeve-com/schema-emit/blob/main/docs/std-form.md) for it, naming each computed property as a [field filled later](https://github.com/nodeve-com/schema-emit/blob/main/docs/std-form.md#fields-filled-later). It hands this crate the lowered schema, that standard form, the computed properties' keys, and `into` where one object property holds them. It gets back:

- **Each property's type.** A model schema-emit reads becomes a cel-emit record type: a field per field, a nested model a nested record. A field the form fills later is a field that may be empty, as the Rust schema-emit writes holds it. A type cel-emit can't hold fails with its reason, never widened. [`types::record`](src/types.rs) types a schema with no field filled later; `types::record_in` takes the standard form, to type a record another expression reads.
- **The record's Rust type**, schema-emit's, where each property sits in it, and whether that place can be empty.
- **Each property's check**, the one schema-emit writes for its schema. A failure names its place inside the value, as `label.text`.
- **Each named type**: a definition the schema names. An expression builds a record by its name, as `Label{ text: 'a' }`. `Bridged::messages` holds the records. `Bridged::types` holds each other one cel-emit has a type for, such as a list of records, for a call to name as its [type argument](../docs/extras.md).
- **The computed properties the schema requires**, as schema-emit's fields say. Whoever walks the index judges them.

[cel-emit-cli](../cel-emit-cli/README.md#computed-properties) builds its programs this way.
