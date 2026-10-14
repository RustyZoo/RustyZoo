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

check base-trial 8895 'rz ls | wc -l'
check base-trial 102706 'rz cat maps/default.zoo | wc -c'
check base-trial 25627 'rz cat animals/lion.ai | wc -c'
check base-trial 1884,4750,30967,38199,2,1,0,0 "rz check-config | tail -1 | grep -o '[0-9]*' | paste -sd, -"
check base-trial 2948,24609,0,36,0,0 "rz check-sprites | tail -1 | grep -o '[0-9]*' | paste -sd, -"
# Sets, sprite file references, missing files, mirrored views, failures, warnings; sets by file
# count; boxes equal to the extents of their frames, larger, different.
check base-trial 1412,2950,0,3459,0,0,1,966,3,1,4,259,5,185,20,1,1402,1,9 "rz check-anis | tail -1 | grep -o '[0-9]*' | paste -sd, -"
# Libraries, string numbers and TEXT entries of the merged view of the lang*.dll.
check base-trial 1,3161,57 "rz strings | awk '/^LIB/{l++} /^TEXT/{t++} /^[0-9]/{s++} END{print l\",\"s\",\"t}'"
check base-trial 5,3,2,0,13750,1371 "rz check-maps | tail -1 | grep -o '[0-9]*' | paste -sd, -"
# Entity types: the total, then per class (food, paths, fences, buildings, animals, guests, scenery,
# staff, ambient, rubble, tank walls, filters), and the types whose base animation member is missing.
check base-trial 51,6,1,6,17,4,2,13,2,0,0,0,0 "rz types | tail -1 | grep -o '[0-9]*' | paste -sd, -"
check base-trial 1 "rz types | awk -F'\\t' 'NF>6 && \$7==\"no\"{n++} END{print n+0}'"
check expansion-trial 10877 'rz ls | wc -l'
check expansion-trial 24871 'rz cat animals/allo.ai | wc -c'
check expansion-trial 2756,7591,48464,62849,2,0,0,0 "rz check-config | tail -1 | grep -o '[0-9]*' | paste -sd, -"
# Four sprites of the expansion trial have a defective palette (three are missing, one is
# truncated): they load, with a warning, as in the original.
check expansion-trial 6545,82588,12191,140,0,4 "rz check-sprites | tail -1 | grep -o '[0-9]*' | paste -sd, -"
# The four animation sets of the expansion trial that use the four sprites above load with a
# warning; their boxes equal the extents of their frames (2097 became 2101).
check expansion-trial 2119,6556,0,4065,0,4,1,852,3,1,4,764,5,493,20,9,2101,9,9 "rz check-anis | tail -1 | grep -o '[0-9]*' | paste -sd, -"
check expansion-trial 3,3924,187 "rz strings | awk '/^LIB/{l++} /^TEXT/{t++} /^[0-9]/{s++} END{print l\",\"s\",\"t}'"
check expansion-trial 5,5,0,0,28125,4160 "rz check-maps | tail -1 | grep -o '[0-9]*' | paste -sd, -"
check expansion-trial 150,14,8,17,26,8,2,47,6,0,7,14,1 "rz types | tail -1 | grep -o '[0-9]*' | paste -sd, -"
check expansion-trial 0 "rz types | awk -F'\\t' 'NF>6 && \$7==\"no\"{n++} END{print n+0}'"

exit $failed
