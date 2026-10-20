#!/usr/bin/env bash
# Prints the state hash of 1000 steps of every map of the two trials, twice: one `<trial>/<map>=<hash>`
# line per map for steps of 67 ms, and one `<trial>/<map>@dt1000=<hash>` line for steps of 1000 ms,
# which cross two month changes (`failed` for a map that does not run). The simulation is
# deterministic, so the lines are the same on every system; the CI job `hashes` compares them and
# fails on a `failed`. Usage: ci/sim-hash.sh <rustyzoo binary> <trials dir>
set -u
bin=$1
dir=$2
export SDL_AUDIO_DRIVER=dummy
RUSTYZOO_USER_DIR=$(mktemp -d)
export RUSTYZOO_USER_DIR
trap 'rm -rf "$RUSTYZOO_USER_DIR"' EXIT

for trial in base-trial expansion-trial; do
  for map in $("$bin" --data "$dir/$trial" ls 'maps/*.zoo' | tr -d '\r'); do
    hash=$("$bin" --data "$dir/$trial" sim "$map" --ticks 1000 --hash 2>/dev/null | tr -d '\r')
    echo "$trial/$map=${hash:-failed}"
    hash=$("$bin" --data "$dir/$trial" sim "$map" --ticks 1000 --dt 1000 --hash 2>/dev/null | tr -d '\r')
    echo "$trial/$map@dt1000=${hash:-failed}"
  done
done
