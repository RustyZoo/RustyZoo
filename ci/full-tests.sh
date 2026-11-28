#!/usr/bin/env bash
# The tests that need a full install, on one install: the ignored tests (and with --include-ignored all the others).
# Usage: ci/full-tests.sh <game folder> <--ignored|--include-ignored>
# SHARD=<i>/<n> runs only every n-th test of the sorted list (see ci/shard.sh). Only the failed test names and the
# totals are printed (and, for a killed test binary, cargo's crash reason, which holds a path and a signal): a panic message can quote a game file. Exit status: 1 if a test failed.
set -u
. "$(dirname "$0")/shard.sh"
export ZT_DATA_DIR=$1
mode=$2
out=$(mktemp)
trap 'rm -f "$out"' EXIT

names=()
total=0
while IFS= read -r name; do
  total=$((total + 1))
  ! shard_has "$total" || names+=("$name")
done < <(cargo test -- --list "$mode" 2>/dev/null | sed -n 's/: test$//p' | sort -u)
shard_done tests "${#names[@]}" "$total" || exit 1
[ "$total" -gt 0 ] || { echo "FAILED: no test listed"; exit 1; }

filter=()
[ -z "${SHARD:-}" ] || filter=(--exact "${names[@]}")
code=0
cargo test -- "$mode" "${filter[@]}" >"$out" 2>&1 || code=$?
grep -E '\.\.\. FAILED$|^test result:|^error(\[|:)|^Caused by:|^ +process didn.t exit' "$out" | cut -c1-300 || true
echo "exit $code"
exit "$code"
