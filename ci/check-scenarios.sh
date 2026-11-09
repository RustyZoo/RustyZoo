#!/usr/bin/env bash
# Starts every scenario and every other map of an install, headless, and plays one month of each.
# Usage: ci/check-scenarios.sh <rustyzoo binary> <game folder> [jobs]
#
# The scenarios are those of the lists the scenario screen shows (`scenar*.cfg`), each started from
# the menu as its Play button does (the script command `scenario FILE`); the maps are the `.zoo`
# files that no listed scenario starts, each played as a saved game. A tutorial's popups are
# answered with Enter. A game passes when the run ends
# with status 0, the start was not refused (no `menu message`), the scenario began (`sim scenario
# start`), a month change came and the scenario did not end in that month. A line says `ok` or
# FAILED; the exit status is 1 if one failed. The games run `jobs` at a time (default 4, or the
# environment variable SCENARIO_JOBS). The lines name files of the install and print no game text.
set -u
bin=$1
data=$2
jobs=${3:-${SCENARIO_JOBS:-4}}
export SDL_AUDIO_DRIVER=dummy # never a real audio device
export RUSTYZOO_ENGLISH_DATES=1 # the same dates whatever the system's locale
# The passes of a month: 31 days of 12 minutes of game at 200 ms is 1860 passes, and a popup holds the game.
passes=2000

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
rz() { "$bin" --data "$data" "$@" 2>/dev/null | tr -d '\r'; }
lower() { tr 'A-Z' 'a-z'; }

# Scenario files: the `scenario=` lines of the lists (a commented line starts with ;), and the map
# each starts (`savegame=`). The maps that are left over are played alone.
for cfg in $(rz ls 'scenar*.cfg' | grep -v /); do
  rz cat "$cfg" | sed -n 's/^scenario=[[:space:]]*//p' | sed 's/[[:space:]]*$//'
done | sort -u >"$tmp/scenarios"
: >"$tmp/started"
while read -r scn; do
  rz cat "$scn" | awk '{ l = tolower($0) } l ~ /^savegame=/ { sub(/^[^=]*=[ \t]*/, ""); sub(/[ \t]*$/, ""); print tolower($0); exit }' >>"$tmp/started"
done <"$tmp/scenarios"
rz ls 'maps/*.zoo' | while read -r map; do
  grep -qx -F "$(echo "$map" | lower)" "$tmp/started" || echo "$map"
done >"$tmp/maps"

# One game: kind and file. Writes $tmp/result/<n> with the status line.
cat >"$tmp/one.sh" <<'EOF'
#!/usr/bin/env bash
bin=$1 data=$2 tmp=$3 kind=$4 file=$5 n=$6
user=$tmp/user-$n
mkdir -p "$user"
printf '[UI]\nplayMovie=0\n' >"$user/zoo.ini"
words="--headless --passes $PASSES --clock 200 --seed 1"
start=$SECONDS
if [ "$kind" = scenario ]; then
  # A tutorial waits on its popups, which Enter answers (it does nothing else in a game).
  { printf '2 scenario %s\n' "$file"; for p in $(seq 5 5 "$PASSES"); do echo "$p key enter"; done; } >"$user/script"
  RUSTYZOO_USER_DIR=$user "$bin" --data "$data" run $words --script "$user/script" >"$user/log" 2>"$user/err"
else
  RUSTYZOO_USER_DIR=$user "$bin" --data "$data" run "$file" $words --bare >"$user/log" 2>"$user/err"
fi
status=$?
why=""
[ "$status" = 0 ] || why="$why exit $status;"
grep -q ' menu message ' "$user/log" && why="$why start refused;"
months=$(grep -c ' sim month ' "$user/log")
[ "$months" -gt 0 ] || why="$why no month change;"
if [ "$kind" = scenario ]; then
  grep -q ' sim scenario start ' "$user/log" || why="$why scenario not started;"
  ! grep -q ' sim scenario end ' "$user/log" || why="$why ended within the month;"
fi
if [ -n "$why" ]; then
  echo "FAILED: $kind $file:$why" >"$tmp/result/$n"
else
  echo "ok: $kind $file: $months month changes, $(wc -l <"$user/log" | tr -d ' ') lines, $((SECONDS - start)) s" >"$tmp/result/$n"
fi
rm -rf "$user"
EOF
chmod +x "$tmp/one.sh"
mkdir "$tmp/result"
export PASSES=$passes
{
  sed 's/^/scenario /' "$tmp/scenarios"
  sed 's/^/map /' "$tmp/maps"
} | awk '{ print $1; print substr($0, index($0, " ") + 1); print NR }' |
  tr '\n' '\0' | xargs -0 -n 3 -P "$jobs" "$tmp/one.sh" "$bin" "$data" "$tmp"

total=$(($(wc -l <"$tmp/scenarios") + $(wc -l <"$tmp/maps")))
# shellcheck disable=SC2012 # the names are numbers
count=$(ls "$tmp/result" | wc -l | tr -d ' ')
cat "$tmp"/result/* | sort
if [ "$count" != "$total" ] || [ "$total" = 0 ]; then
  echo "FAILED: $count games ran of $total" >&2
  exit 1
fi
! cat "$tmp"/result/* | grep -q '^FAILED' || exit 1
echo "ok: $(wc -l <"$tmp/scenarios" | tr -d ' ') scenarios and $(wc -l <"$tmp/maps" | tr -d ' ') maps played one month in $SECONDS s"
