#!/usr/bin/env bash
# Fails if a table of docs/architecture.md or README.md has two rows with the same first cell, a
# section has two bullets with the same bold lead (`- **Lead.**`), or a code block has two command
# lines with the same command (the text before the first ` #`; blank and comment-only lines are
# left out). Both files merge as a union (see .gitattributes), so a row, bullet or example line
# that one change edits and another keeps would otherwise be there twice.
# Usage: ci/check-doc-rows.sh (from the repository root)
set -u
cd "$(git rev-parse --show-toplevel)" || exit 1
fail=0
for f in docs/architecture.md README.md; do
  awk -v f="$f" '
    /^```/ { code = !code; delete cmd; next }
    code { k = $0; sub(/ #.*/, "", k); sub(/ +$/, "", k)
           if (k ~ /^ *(#|$)/) next
           if (k in cmd) { printf "%s:%d: command \"%s\" repeats line %d\n", f, NR, k, cmd[k]; bad = 1 }
           else cmd[k] = NR
           next }
    /^\|/ { split($0, c, "|"); k = c[2]; gsub(/^ +| +$/, "", k)
            if (k in row) { printf "%s:%d: row \"%s\" repeats line %d\n", f, NR, k, row[k]; bad = 1 }
            else row[k] = NR; next }
    { delete row }
    /^#/ { delete lead }
    /^- \*\*/ { k = $0; sub(/^- \*\*/, "", k); sub(/\*\*.*/, "", k)
                if (k in lead) { printf "%s:%d: bullet \"%s\" repeats line %d\n", f, NR, k, lead[k]; bad = 1 }
                else lead[k] = NR }
    END { exit bad }' "$f" || fail=1
done
exit $fail
