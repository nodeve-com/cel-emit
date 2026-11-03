#!/bin/sh
# Instructions each reference and its generated program execute, by callgrind, collecting only
# inside `measured`: the program's one call on its input. Linux with valgrind. The references
# test builds the programs, unstripped, and lists them in programs.tsv.
set -e
cd "$(dirname "$0")/.."
CARGO_PROFILE_RELEASE_STRIP=false cargo nextest run --cargo-quiet -p cel-emit-cli \
  --test references >/dev/null
out=target/tmp/programs
tab=$(printf '\t')
while IFS="$tab" read -r example program exe input; do
  log="$out/$example-$program.callgrind"
  valgrind --tool=callgrind --toggle-collect='*measured*' --callgrind-out-file="$log" \
    "$exe" <"$input" >/dev/null 2>&1
  echo "## $example $program"
  callgrind_annotate --inclusive=yes "$log" | sed -n '/^Ir/,$p' | head -n 22
done <"$out/programs.tsv"
