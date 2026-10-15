#!/usr/bin/env bash
# Fails if simulation code walks a HashMap or HashSet (the order differs at every run, so the
# state hash would differ): a map is only looked up, anything walked is a Vec in a fixed order.
# It finds the names declared with a hash type in the simulation modules below and then the
# iterations over those names (`.iter()`, `.keys()`, `.values()`, `.drain()`, `for _ in name`...).
# A walk whose order provably does not matter carries `// order-free: <why>` on its line.
# Usage: ci/check-hash-iteration.sh (from the repository root)
set -u
cd "$(git rev-parse --show-toplevel)" || exit 1
# The simulation modules (files and folders under src/); tests are not simulation code.
modules="sim sim.rs unit unit.rs behaviour behaviour.rs path path.rs exhibit exhibit.rs world world.rs
  money money.rs entity.rs ground.rs terrain terrain.rs place place.rs status status.rs zoo zoo.rs"
files=()
for m in $modules; do
  [ -e "src/$m" ] && files+=($(find "src/$m" -name '*.rs' ! -name 'tests.rs' ! -path '*/tests/*'))
done
fail=0
for f in "${files[@]}"; do
  # Only the code above the unit tests module of the file.
  code=$(awk '/^(#\[cfg\(test\)\]|mod tests)/ {exit} {print}' "$f")
  hash='(std::collections::)?Hash(Map|Set)'
  names=$( { grep -oP "\b\w+(?=\s*:\s*&?(mut )?$hash\b)" <<<"$code"
             grep -oP "let (mut )?\K\w+(?=[^=\n;]*=[^;]*$hash::)" <<<"$code"; } | sort -u | paste -sd'|')
  [ -n "$names" ] || continue
  walk="(\b($names)\b\s*(\)\s*)?\.(iter|iter_mut|into_iter|keys|values|values_mut|drain|retain|into_keys|into_values)\(|\bin\s+&?(mut\s+)?(self\.)?($names)\b\s*\{)"
  hits=$(grep -nP "$walk" <<<"$code" | grep -v '// order-free:')
  if [ -n "$hits" ]; then
    echo "$f: iteration over a hash map or set (use a Vec in a fixed order):"
    echo "$hits" | sed 's/^/  /'
    fail=1
  fi
done
exit $fail
