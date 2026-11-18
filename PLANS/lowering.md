# Lowering settles types and conversions

Lowering turns a checked expression into the tree the emitter writes Rust from. Today it leaves some decisions for the emitter to find again by overload name, so two stages keep matching lists in step.

## Where the stages overlap

- **Widening a stored collection's element.** Reading one element of a stored `list<int32>` gives CEL's `int64`. Lowering drops the widening around the stored list (`widen_lazily`, `src/lower/widths.rs`), and the emitter spots the same overloads again (`widens_each`, `src/emit/narrow.rs`) to widen the element it reads. The two lists already differ by one overload. Membership and map lookup leave the emitter to pick the key and element conversions.
- **Result types.** Lowering gives each node the checker's type. `retype` (`src/lower/dynamic.rs`) then replaces some calls' types through a third match on overload names. Empty literals (`src/lower/empty.rs`) and `orValue` (`src/lower/optional.rs`) each type their operands from context in their own way.

## Work

- Give the tree a node for each conversion that runs during a read: read the stored element, then widen it. The same applies to the key or element conversion in membership and map lookup. Keep reading one element without widening the whole collection, and keep its errors as they are.
- Let the emitter write those nodes as they come, and delete `widens_each` and its overload list.
- Build each call's result type in that call's own lowering rule, and delete `retype`.
- Give typing from context one entry point that takes an optional expected type, used by empty literals, `orValue`, and dyn operands.

## Checks

- A dump of the tree shows where each stored value becomes a CEL value.
- The width tests (`tests/semantics/widths.rs`) pass unchanged.
- New cases mix empty collections, dyn, optionals, and stored widths.

## Differential test, left over

The differential test already fails on any refusal not listed in `REFUSED` and on stale list entries. Two gaps remain:

- It reduces every failure to `err`, so a change in error kind passes. Compare kinds where the interpreter's error maps onto cel-emit-runtime's.
- Its cases are hand-written. Add well-typed generated expressions to reach combinations the hand-written ones miss.
