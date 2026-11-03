#!/bin/sh
# The crates stay general: no word of a product built on them, no postcard, no schema-emit.
set -e
cd "$(dirname "$0")/.."
# The manifest's one `loom` is cel-core's branch name.
words=$(
  {
    grep -rnEiw 'loom|bundles?|postcard' src tests README.md docs \
      cel-emit-runtime/src cel-emit-runtime/README.md
    grep -nEiw 'loom|bundles?|postcard' Cargo.toml cel-emit-runtime/Cargo.toml |
      grep -v 'branch = "loom"'
  } || true
)
[ -z "$words" ] || {
  echo "$words"
  exit 1
}
deps=$(grep -nE '^schema-emit' Cargo.toml cel-emit-runtime/Cargo.toml || true)
[ -z "$deps" ] || {
  echo "$deps"
  exit 1
}
