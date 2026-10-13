#!/usr/bin/env bash
# Runs the asset commands on the trials extracted by ci/fetch-trials.sh and compares the output
# with the expected numbers. Usage: ci/check-trials.sh <rustyzoo binary> <trials dir>
#
# To cover a new command, add a line to the list below:
#   check <trial folder> <expected output> '<shell command, rz runs rustyzoo on the folder>'
set -u
bin=$1
dir=$2
failed=0

rz() { "$bin" --data "$dir/$trial" "$@"; }

check() {
  trial=$1 expected=$2 command=$3
  got=$(eval "$command" | tr -d ' \r') # wc pads its numbers on macOS
  if [ "$got" = "$expected" ]; then
    echo "ok: $trial: $command = $got"
  else
    echo "FAILED: $trial: $command = $got, expected $expected" >&2
    failed=1
  fi
}

check base-trial 8853 'rz ls | wc -l'
check base-trial 25627 'rz cat animals/lion.ai | wc -c'
check expansion-trial 10843 'rz ls | wc -l'
check expansion-trial 24871 'rz cat animals/allo.ai | wc -c'

exit $failed
