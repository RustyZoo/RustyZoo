#!/usr/bin/env bash
# The checks to pass before a pull request, one summary line per step. Usage: ci/gate.sh [trials dir]
#
#   fmt      cargo fmt --check
#   clippy   cargo clippy --all-targets -- -D warnings
#   test     cargo test
#   full     cargo test -- --ignored, only when ZT_DATA_DIR is set (the full game)
#   full-fr  the same with ZT_DATA_DIR set to $ZT_DATA_DIR_FR (a second install, in another language;
#            the tests pick their expected values by the language of lang0.dll); skipped when unset
#   trials   release build, then ci/check-trials.sh on the trials dir (see ci/fetch-trials.sh);
#            skipped without the argument
#
# Runs on the repository that contains the current directory, so a copy of this script can gate
# another checkout. Every step runs, even after a failure; the output of a failed step follows its
# line. Exit status: 0 when no step failed (skipped steps do not fail), 1 otherwise.
set -u
cd "$(git rev-parse --show-toplevel)" || exit 1
trials=${1:-}
logs=$(mktemp -d)
trap 'rm -rf "$logs"' EXIT
export RUSTYZOO_USER_DIR=$logs/user # tests never touch the real user folder
ok=0 failed=0 skipped=0

step() { # step <name> <command...>
  name=$1
  shift
  start=$SECONDS
  if "$@" >"$logs/$name" 2>&1; then
    printf 'ok      %-8s %ss\n' "$name" $((SECONDS - start))
    ok=$((ok + 1))
  else
    printf 'FAILED  %-8s %ss\n' "$name" $((SECONDS - start))
    tail -n 40 "$logs/$name" | sed -E 's/\x1b(\[[0-9;]*[A-Za-z]|\(B)|\x0f//g; s/^/        /'
    failed=$((failed + 1))
  fi
}

skip() { # skip <name> <reason>
  printf 'skipped %-8s %s\n' "$1" "$2"
  skipped=$((skipped + 1))
}

trials_step() {
  cargo build --release && ci/check-trials.sh "${CARGO_TARGET_DIR:-target}/release/rustyzoo" "$trials"
}

step fmt cargo fmt --check
step clippy cargo clippy --all-targets -- -D warnings
step test cargo test
if [ -n "${ZT_DATA_DIR:-}" ]; then step full cargo test -- --ignored; else skip full "ZT_DATA_DIR is not set"; fi
if [ -n "${ZT_DATA_DIR_FR:-}" ]; then
  step full-fr env ZT_DATA_DIR="$ZT_DATA_DIR_FR" cargo test -- --ignored
else skip full-fr "ZT_DATA_DIR_FR is not set"; fi
if [ -n "$trials" ]; then step trials trials_step; else skip trials "no trials dir given"; fi

echo "gate: $ok ok, $failed failed, $skipped skipped"
[ "$failed" = 0 ]
