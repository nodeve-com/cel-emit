#!/bin/sh
# Instructions each program executes per phase, by callgrind, collecting only inside that phase's
# function, `measured_<phase>`: each program's decode of its input text (decode) and its
# expression from that input to output text (expression), and the interpreter's load of its
# checked expression (load).
# Prints the total and the costliest functions by their own instructions. Linux with valgrind;
# elsewhere, through scripts/linux.sh. The references test builds the programs, unstripped, and
# lists each phase in programs.tsv.
set -e
cd "$(dirname "$0")/.."
CARGO_PROFILE_RELEASE_STRIP=false cargo nextest run --cargo-quiet -p cel-emit-cli \
  --test references >/dev/null
out=${CARGO_TARGET_DIR:-target}/tmp/programs
tab=$(printf '\t')
while IFS="$tab" read -r example program phase exe input; do
  log="$out/$example-$program-$phase.callgrind"
  valgrind --tool=callgrind --toggle-collect="*measured_$phase*" --callgrind-out-file="$log" \
    "$exe" <"$input" >/dev/null 2>&1
  echo "## $example $program $phase"
  callgrind_annotate "$log" | sed -n '/PROGRAM TOTALS/p;/^Ir  *file:function/,$p' |
    sed 's| \[.*\]$||' | head -n 14
done <"$out/programs.tsv"
