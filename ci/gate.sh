#!/usr/bin/env bash
# The checks to pass before a pull request, one summary line per step. Usage: ci/gate.sh [trials dir]
#
#   fmt      cargo fmt --check
#   clippy   cargo clippy --all-targets -- -D warnings
#   test     cargo test
#   full     cargo test -- --ignored, only when ZT_DATA_DIR is set (the full game)
#   full-fr  the same with ZT_DATA_DIR set to $ZT_DATA_DIR_FR (a second install, in another language;
#            the tests pick their expected values by the language of lang0.dll); skipped when unset
#   play     ci/check-playable.sh with ci/playable-full.txt on $ZT_DATA_DIR (the
#            full game; about eight minutes); skipped when unset
#   play-fr  the same on $ZT_DATA_DIR_FR; skipped when unset
#   trials   ci/check-trials.sh on the trials dir (see ci/fetch-trials.sh);
#            skipped without the argument
#   play-trials  as CI does: ci/playable.txt on base-trial and ci/playable-tank.txt on expansion-trial
#            (a trial folder missing from the trials dir fails the step); skipped without the argument
#
# The release binary is built once (a failure shows as step "build"), then play, play-fr, trials and
# play-trials run at the same time, each with its own log; their lines are printed together at the end.
# Runs on the repository that contains the current directory, so a copy of this script can gate
# another checkout. Every step runs, even after a failure; the output of a failed step follows its
# line. Exit status: 0 when no step failed (skipped steps do not fail), 1 otherwise.
set -u
cd "$(git rev-parse --show-toplevel)" || exit 1
trials=${1:-}
logs=$(mktemp -d)
trap 'rm -rf "$logs"' EXIT
export RUSTYZOO_USER_DIR=$logs/user # tests never touch the real user folder
export RUSTYZOO_ENGLISH_DATES=1 # the same dates (and frames) whatever the system's locale
ok=0 failed=0 skipped=0

run() { # run <name> <command...>: log in $logs/<name>, status and seconds in $logs/<name>.st
  name=$1
  shift
  start=$SECONDS
  "$@" >"$logs/$name" 2>&1
  echo "$? $((SECONDS - start))" >"$logs/$name.st"
}

report() { # report <name>: the summary line, the tail of the log of a failed step
  read -r status secs <"$logs/$1.st"
  if [ "$status" = 0 ]; then
    printf 'ok      %-11s %ss\n' "$1" "$secs"
    ok=$((ok + 1))
  else
    printf 'FAILED  %-11s %ss\n' "$1" "$secs"
    tail -n 40 "$logs/$1" | sed -E 's/\x1b(\[[0-9;]*[A-Za-z]|\(B)|\x0f//g; s/^/        /'
    failed=$((failed + 1))
  fi
}

step() { run "$@"; report "$1"; [ "$(cut -d' ' -f1 "$logs/$1.st")" = 0 ]; } # step <name> <command...>

skip() { # skip <name> <reason>
  printf 'skipped %-11s %s\n' "$1" "$2"
  skipped=$((skipped + 1))
}

trials_step() {
  ci/check-trials.sh "${CARGO_TARGET_DIR:-target}/release/rustyzoo" "$trials"
}

play_step() { # play_step <game folder>
  ci/check-playable.sh "${CARGO_TARGET_DIR:-target}/release/rustyzoo" "$1" ci/playable-full.txt
}

play_trials_step() {
  bin=${CARGO_TARGET_DIR:-target}/release/rustyzoo
  status=0
  ci/check-playable.sh "$bin" "$trials/base-trial" ci/playable.txt || status=1
  ci/check-playable.sh "$bin" "$trials/expansion-trial" ci/playable-tank.txt || status=1
  return $status
}

step fmt cargo fmt --check
step clippy cargo clippy --all-targets -- -D warnings
step test cargo test
if [ -n "${ZT_DATA_DIR:-}" ]; then step full cargo test -- --ignored; else skip full "ZT_DATA_DIR is not set"; fi
if [ -n "${ZT_DATA_DIR_FR:-}" ]; then
  step full-fr env ZT_DATA_DIR="$ZT_DATA_DIR_FR" cargo test -- --ignored
else skip full-fr "ZT_DATA_DIR_FR is not set"; fi
# The four long steps only share the release binary: build it once, then run them side by side.
long=()
if [ -n "${ZT_DATA_DIR:-}" ]; then long+=(play); else skip play "ZT_DATA_DIR is not set"; fi
if [ -n "${ZT_DATA_DIR_FR:-}" ]; then long+=(play-fr); else skip play-fr "ZT_DATA_DIR_FR is not set"; fi
if [ -n "$trials" ]; then long+=(trials play-trials); else
  skip trials "no trials dir given"
  skip play-trials "no trials dir given"
fi
if [ ${#long[@]} != 0 ] && step build cargo build --release; then
  for name in "${long[@]}"; do
    case $name in
    play) run play play_step "$ZT_DATA_DIR" & ;;
    play-fr) run play-fr play_step "$ZT_DATA_DIR_FR" & ;;
    trials) run trials trials_step & ;;
    play-trials) run play-trials play_trials_step & ;;
    esac
  done
  wait
  for name in "${long[@]}"; do report "$name"; done
fi

echo "gate: $ok ok, $failed failed, $skipped skipped"
[ "$failed" = 0 ]
