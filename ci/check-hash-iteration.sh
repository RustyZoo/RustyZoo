#!/usr/bin/env bash
# Fails if simulation code walks a HashMap or HashSet (the order differs at every run, so the
# state hash would differ): a map is only looked up, anything walked is a Vec in a fixed order.
# It finds the names declared with a hash type (or an alias of one) in the simulation modules below
# and then the iterations over those names (`.iter()`, `.keys()`, `.values()`, `.drain()`, `for _ in name`...).
# A walk whose order provably does not matter carries `// order-free: <why>` on its line.
# Usage: ci/check-hash-iteration.sh (from the repository root)
set -u
cd "$(git rev-parse --show-toplevel)" || exit 1
# Every module under src/ except those that cannot reach the simulation step (drawing, windows,
# sound, files, command line, menus, startup); tests are not simulation code.
skip=" render render.rs ui ui.rs window app app.rs audio.rs mcp mcp.rs cli menu menu.rs text text.rs textcmd.rs
  vfs vfs.rs probe.rs strings.rs boot.rs main.rs movie.rs music.rs intro intro.rs panel panel.rs input.rs
  cursor.rs tga.rs pe.rs userdir.rs usertext.rs startup startup.rs layout.rs findcmd.rs framelog.rs zoom.rs
  camera camera.rs bitmap bitmap.rs fixtures.rs lib.rs run.rs sprite sprite.rs anim.rs soundscape soundscape.rs "
files=()
for m in $(cd src && ls); do
  case "$skip" in *" $m "*) continue ;; esac
  files+=($(find "src/$m" -name '*.rs' ! -name 'tests.rs' ! -path '*/tests/*'))
done
fail=0
# Type aliases of a hash type (`type Gates = HashMap<..>`) count as hash types in every file.
aliases=$(cat "${files[@]}" | grep -oP "^\s*(pub(\([a-z]+\))? )?type \K\w+(?=(<[^=]*>)?\s*=\s*(std::collections::)?Hash(Map|Set)\b)" | sort -u | paste -sd'|')
hash='(std::collections::)?(Hash(Map|Set)'"${aliases:+|$aliases}"')'
for f in "${files[@]}"; do
  # Only the code above the inline unit tests module (`#[cfg(test)] mod x {`); a `#[cfg(test)]` on a `use`, `fn` or `mod x;` is no end.
  code=$(awk '/^mod tests *\{/ {exit} cfg && /^(pub(\([a-z]+\))? )?mod [a-z_0-9]+ *\{/ {exit} {cfg = /^#\[cfg\(test\)\]/} {print}' "$f")
  # Names declared with a hash type: `name: HashMap<..>`, `let name = HashMap::new()`,
  # `let name = ..collect::<HashSet<..>>()`, `let (mut a, mut b) = (HashSet::new(), ..)`.
  names=$( { grep -oP "\b\w+(?=\s*:\s*&?(mut )?$hash\b)" <<<"$code"
             grep -oP "let (mut )?\K\w+(?=[^=\n;]*=[^;]*(${hash}::|collect::<$hash\b))" <<<"$code"
             grep -P "let \(.*\)\s*=.*$hash" <<<"$code" | grep -oP "let \(\K[^)]*" | tr ',' '\n' | sed 's/mut //; s/ //g'; } |
           grep -vxE '_|' | sort -u | paste -sd'|')
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
