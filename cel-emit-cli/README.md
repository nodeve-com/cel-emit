# cel-emit-cli

Write [CEL](https://cel.dev), get a Rust program. You write one CEL expression in a file, and `cel-emit` writes a Cargo package that reads JSON on stdin, computes the expression over it, and prints the result as JSON. You write no Rust, no types, and no schema.

It's the shortest way to see [cel-emit](../README.md) work. The program it writes carries no CEL interpreter.

## In short

`orders.cel` turns nested orders into flat rows:

```cel
// One flat row per order, its customer looked up by code.
input.orders.map(order, {
  "id": order.id,
  "customer": input.customers.filter(c, c.id == order.customer)[0].name,
  "total_minor": int(order.qty) * int(order.price),
  "placed_utc": to_utc(order.placed, input.zone),
  "note": has(order.note) ? order.note : null,
})
```

`orders.json` is the data:

```json
{
  "zone": "Europe/Paris",
  "customers": [{"id": "c1", "name": "Ada"}, {"id": "c2", "name": "Lin"}],
  "orders": [
    {"id": 1, "customer": "c1", "qty": 2, "price": 350, "placed": "2030-07-01T12:00:00", "note": "gift"},
    {"id": 2, "customer": "c2", "qty": 1, "price": 900, "placed": "2030-07-01T18:30:00"}
  ]
}
```

From this repo's root, write the program, then run it:

```sh
cargo run -p cel-emit-cli --bin cel-emit -- orders.cel target/orders
cargo run --manifest-path target/orders/Cargo.toml < orders.json
```

It prints one line of JSON, shown here across two:

```json
[{"id":1,"customer":"Ada","total_minor":700,"placed_utc":"2030-07-01T10:00:00Z","note":"gift"},
 {"id":2,"customer":"Lin","total_minor":900,"placed_utc":"2030-07-01T16:30:00Z","note":null}]
```

`target/orders/src/main.rs` is the generated Rust, if you want to read it. [`examples/answer_rows`](../examples/answer_rows) is a larger expression over a deeply nested answer.

## Expressions

The file holds one CEL expression of any shape: a list, a map, or a single value such as `input.orders.size()`. It reads `input`, the decoded JSON, and the program prints its value as JSON. `//` starts a comment. `cel.bind(name, value, rest)` names a value used more than once.

## Functions

Expressions get everything [cel-emit](../README.md#extras) gives them, plus:

- `to_utc(local, zone)`: a local time such as `2030-07-01T12:00:00` in an IANA zone, as a timestamp. A time the zone's clocks skip or repeat is an error.
- `iso_duration(text)`: an ISO 8601 duration such as `P1DT12H` as a duration, a day being 24 hours. A month or year is an error, having no fixed length.

A generated program links the time zone database and `jiff` only if its expression calls one of these.

## Limits

- **JSON values have no declared types**, so CEL can't know a value from the JSON is a number. Comparing, copying, and selecting work as written, but arithmetic needs a cast: `int(order.qty) * 2`, not `order.qty * 2`, which is refused when translating.
- **One failure stops the run.** An expression fails whole, so a lookup that finds nothing ends the program with a non-zero exit and prints nothing. The error names its kind, not its place.
- **The whole input is read into memory.**
- **The program builds inside this repo.** Its `Cargo.toml` points at this checkout's cel-emit-runtime by path.

What cel-emit itself can't translate yet: [`MISSING.md`](../MISSING.md).
