#!/usr/bin/env bash
# Fails if a table of docs/architecture.md or README.md has two rows with the same first cell, or a
# section has two bullets with the same bold lead (`- **Lead.**`). Both files merge as a union (see
# .gitattributes), so a row that one change edits and another keeps would otherwise be there twice.
# Usage: ci/check-doc-rows.sh (from the repository root)
set -u
cd "$(git rev-parse --show-toplevel)" || exit 1
fail=0
for f in docs/architecture.md README.md; do
  awk -v f="$f" '
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
