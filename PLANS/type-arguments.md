# Type arguments

An embedder's function may take a message type as an argument, as in `decode(text, Message)`. The type picks the overload and passes nothing at run time. The embedder declares one overload per message, each with its own Rust path. Part of schema-emit `PLANS/one-decode.md`.

## Deletions first

- `cel-emit-runtime/src/json/shaped.rs`, `src/lower/shaped.rs`, and Shape emission in `src/emit/dynamic.rs`.
- `Decls.rest` and `Decls.fill`.
- The `json.decode` case in `src/lower/special.rs`.
- The `json.decode(text, Message)` line in `docs/extras.md`. `json.decode(text)` stays, returning a dynamic value.

## Build

- Lowering: a call whose argument is a message type picks the overload declared for that type and drops the argument.
- Tests: an embedder function with one overload per message, picked by its type argument; an unknown type is a compile error.
