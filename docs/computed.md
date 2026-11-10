# Computed properties

One expression gives one value. Computed properties fill an object a property at a time, each property by its own CEL expression, which may read what other properties hold. This page calls the object the record. A spreadsheet works the same way: each cell holds a formula, worked out after the cells it reads.

The caller gives each computed property its key and its expression, as CEL text or a typed tree it built, the kind cel-emit's `lower` gives:

| Key | Expression |
| - | - |
| `label` | `Label{ text: order.id + ': ' + string(total) }` |
| `order` | `json.decode(text)` |
| `total` | `order.qty * order.price` |

- **A key names the property it fills**, a property of the record. Its expression must give that property's type.
- **An expression reads the record's properties by name**: the ones the input gave, here `text`, and the ones other expressions fill.
- **A property fills after the properties its expression reads**, so `order` fills first and `label` last. The order the caller lists them in means nothing: properties nothing orders fill in key order. Properties that depend on each other in a cycle, through reads or `after`, fail translation, which names them.
- **`after`**, a computed property's option, lists properties it fills after, beside the ones its expression reads, for a value the caller hands its expression that depends on them. It only orders: one left empty doesn't stop the property.
- **`required`**, a computed property's option, says the record needs it. It changes nothing in how the property fills: its entry in [the index](#the-index) repeats it, for whoever walks the index to judge by.
- **A property's check runs as it fills**, before any expression reads it. A value that fails leaves the property empty, and the failure names the place, as `total` or `label.text`.
- **An expression whose input is empty doesn't run**, unless it allows the absence, as `has(order.note)` does. Its own property stays empty, without a failure. A property the caller types as an optional reads as one, empty or not: `total.orValue(0)`.
- **A property may always hold a value**, as one with a default does. A failure leaves it holding what it held, so an expression reading it still runs.
- **`into`** names one object property of the record for the computed properties to fill. Its properties are then read by their own names, as the record's are; a name the two share fails translation.

## What the caller gives

cel-emit knows nothing about the program around it, so the caller gives it the record:

- **Each property's type**, as a cel-emit `types::Type`.
- **The record's Rust type**, where each property sits in it, and whether that place can be empty.
- **Each property's check**: a function the generated code calls with the value, returning it or a failure that names its place.

A JSON Schema can supply all three, through [cel-emit-schema-bridge](https://github.com/nodeve-com/cel-emit/tree/main/cel-emit-schema-bridge).

## The index

The generated Rust holds one function per computed property and an index: a list of the properties in the order they fill. Each entry holds the property's key, the properties it reads, its `required`, and its function, which computes the value, checks it, and stores it in the record. The function returns one of three outcomes: filled; skipped, an input being empty; or a failure, which holds its place and its reason. So every entry has one type whatever its property holds.

The caller's program walks the index and decides what a failure means, or a required property left unfilled: stop, skip, or send the record elsewhere. The entry's type comes from cel-emit-runtime, so the code walking it gets written once, for every record.
