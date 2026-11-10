#!/usr/bin/env bash
# Fails if a non-test .rs file under src/ has over 900 lines and is not in ci/file-size-allow.txt,
# or if a listed file grew past its ceiling (the "path lines" in the list) or is no longer over
# 900 lines (remove it from the list). Test files (tests.rs, tests/, *_tests.rs) are exempt.
# Usage: ci/check-file-size.sh (from the repository root)
set -u
cd "$(git rev-parse --show-toplevel)" || exit 1
fail=0
while read -r f n; do
  [ "$n" -gt 900 ] || continue
  case $f in */tests.rs | */tests/* | *_tests.rs) continue ;; esac
  ceil=$(awk -v f="$f" '$1 == f { print $2 }' ci/file-size-allow.txt)
  [ -n "$ceil" ] || { echo "$f: $n lines, over 900 (split it, or list it in ci/file-size-allow.txt)"; fail=1; }
  [ -z "$ceil" ] || [ "$n" -le "$ceil" ] || { echo "$f: $n lines, over its ceiling $ceil"; fail=1; }
done < <(git ls-files 'src/*.rs' | xargs wc -l | grep -v ' total$' | awk '{ print $2, $1 }')
while read -r f ceil; do
  [ -n "$f" ] || continue
  n=$(wc -l < "$f" 2>/dev/null || echo 0)
  [ "$n" -gt 900 ] || { echo "$f: $n lines, no longer over 900 (remove it from ci/file-size-allow.txt)"; fail=1; }
done < ci/file-size-allow.txt
exit $fail
