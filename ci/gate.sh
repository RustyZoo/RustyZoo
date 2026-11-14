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
#   dung     the first dung on maps/med_kids.zoo of $ZT_DATA_DIR, seeds 1 to 12: every seed has one by
#            step 2000 and their median comes between steps 600 and 650, where the original's two runs
#            put it (612 and 616); skipped when unset
#   scen     ci/check-scenarios.sh and ci/check-ends.sh on $ZT_DATA_DIR (every scenario and map plays a
#            month, a scenario is won and one lost; about eight minutes); skipped when unset
#   scen-fr  the same on $ZT_DATA_DIR_FR; skipped when unset
#   scen-trials  the same on both trials; skipped without the argument
#   speed    maps/med_kids.zoo of $ZT_DATA_DIR (seed 1) played 30000 steps by `sim` (about 160
#            guests), then 1000 passes of `run --headless` from there: prints the time of a step
#            and of a pass against the 67 ms of a step, without a pin; runs alone after the others;
#            skipped when unset
#   trials   ci/check-trials.sh on the trials dir (see ci/fetch-trials.sh);
#            skipped without the argument
#   play-trials  as CI does: ci/playable.txt on base-trial and ci/playable-tank.txt on expansion-trial
#            (a trial folder missing from the trials dir fails the step); skipped without the argument
#
# The release binary is built once (a failure shows as step "build"), then play, play-fr, trials,
# play-trials and the scen steps run at the same time, each with its own log; their lines are printed together at the end.
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
  rm -f "$logs/$name.st"
  "$@" >"$logs/$name" 2>&1
  echo "$? $((SECONDS - start))" >"$logs/$name.st"
}

report() { # report <name>: the summary line, the tail of the log of a failed step
  status=1 secs=0 # a step that died before writing its status file counts as failed
  read -r status secs <"$logs/$1.st" 2>/dev/null || { status=1; secs=0; }
  if [ "$status" = 0 ]; then
    printf 'ok      %-11s %ss\n' "$1" "$secs"
    [ "$1" = speed ] && sed 's/^/        /' "$logs/$1"
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

dung_step() { # the step of the first `animal dung` line of each seed, as `seed:step`, played side by side
  all=$(for seed in $(seq 12); do
    "${CARGO_TARGET_DIR:-target}/release/rustyzoo" --data "$ZT_DATA_DIR" sim maps/med_kids.zoo --seed "$seed" --ticks 2000 --log |
      awk -v s="$seed" '$3 == "animal" && $4 == "dung" { print s ":" $2; f = 1; exit } END { if (!f) print s ":none" }' &
  done; wait)
  echo "first dung by seed: $(sort -n <<<"$all" | tr '\n' ' ')"
  ! grep -q none <<<"$all" || return 1
  median=$(cut -d: -f2 <<<"$all" | sort -n | awk 'NR == 6 { a = $1 } NR == 7 { print int((a + $1) / 2) }')
  echo "median $median"
  [ "$median" -ge 600 ] && [ "$median" -le 650 ]
}

scen_step() { # scen_step <game folder>...
  bin=${CARGO_TARGET_DIR:-target}/release/rustyzoo
  status=0
  for dir in "$@"; do
    ci/check-scenarios.sh "$bin" "$dir" || status=1
    ci/check-ends.sh "$bin" "$dir" || status=1
  done
  return $status
}

ms_since() { echo $(( ($(date +%s%N) - $1) / 1000000 )); } # ms_since <date +%s%N>

speed_step() { # the timings of the speed step; fails only when a run fails
  bin=${CARGO_TARGET_DIR:-target}/release/rustyzoo
  zoo=$logs/speed.zoo
  echo "30000 save $zoo" >"$logs/speed.sim"
  echo "1 load $zoo" >"$logs/speed.run"
  start=$(date +%s%N)
  "$bin" --data "$ZT_DATA_DIR" sim maps/med_kids.zoo --ticks 30001 --script "$logs/speed.sim" >/dev/null || return 1
  sim=$(ms_since "$start")
  guests=$(printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
    "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"load\",\"arguments\":{\"file\":\"$zoo\"}}}" \
    '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"state","arguments":{}}}' |
    "$bin" --data "$ZT_DATA_DIR" mcp | grep -o 'guests [0-9]*' | tail -1 | cut -d' ' -f2)
  # The load (of the map, then of the zoo at pass 1) is timed alone and taken off.
  runs=()
  for passes in 2 1002; do
    start=$(date +%s%N)
    "$bin" --data "$ZT_DATA_DIR" run maps/med_kids.zoo --headless --passes $passes --clock 67 --seed 1 \
      --script "$logs/speed.run" >/dev/null || return 1
    runs+=("$(ms_since "$start")")
  done
  awk -v sim="$sim" -v run=$((runs[1] - runs[0])) -v g="${guests:-?}" 'BEGIN {
    printf "med_kids: sim %.1f ms a step (30000 steps); run with %s guests %.1f ms a pass (1000 passes); a step is 67 ms\n", sim / 30000, g, run / 1000 }'
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
# The long steps only share the release binary: build it once, then run them side by side.
long=()
if [ -n "${ZT_DATA_DIR:-}" ]; then long+=(play); else skip play "ZT_DATA_DIR is not set"; fi
if [ -n "${ZT_DATA_DIR_FR:-}" ]; then long+=(play-fr); else skip play-fr "ZT_DATA_DIR_FR is not set"; fi
if [ -n "${ZT_DATA_DIR:-}" ]; then long+=(scen); else skip scen "ZT_DATA_DIR is not set"; fi
if [ -n "${ZT_DATA_DIR:-}" ]; then long+=(dung); else skip dung "ZT_DATA_DIR is not set"; fi
if [ -n "${ZT_DATA_DIR_FR:-}" ]; then long+=(scen-fr); else skip scen-fr "ZT_DATA_DIR_FR is not set"; fi
if [ -n "$trials" ]; then long+=(trials play-trials scen-trials); else
  skip trials "no trials dir given"
  skip play-trials "no trials dir given"
  skip scen-trials "no trials dir given"
fi
if [ ${#long[@]} != 0 ] && step build cargo build --release; then
  for name in "${long[@]}"; do
    case $name in
    play) run play play_step "$ZT_DATA_DIR" & ;;
    play-fr) run play-fr play_step "$ZT_DATA_DIR_FR" & ;;
    trials) run trials trials_step & ;;
    play-trials) run play-trials play_trials_step & ;;
    dung) run dung dung_step & ;;
    scen) run scen scen_step "$ZT_DATA_DIR" & ;;
    scen-fr) run scen-fr scen_step "$ZT_DATA_DIR_FR" & ;;
    scen-trials) run scen-trials scen_step "$trials/base-trial" "$trials/expansion-trial" & ;;
    esac
  done
  wait
  for name in "${long[@]}"; do report "$name"; done
  # Alone, so that the other steps do not slow it down.
  if [ -n "${ZT_DATA_DIR:-}" ]; then step speed speed_step; fi
fi
[ -n "${ZT_DATA_DIR:-}" ] || skip speed "ZT_DATA_DIR is not set"

echo "gate: $ok ok, $failed failed, $skipped skipped"
[ "$failed" = 0 ]
