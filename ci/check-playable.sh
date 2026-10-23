#!/usr/bin/env bash
# Plays a game with a script of commands, headless, twice at the same time, and checks the log.
# Usage: ci/check-playable.sh <rustyzoo binary> <game folder> <script>
#
# The script is a `run` script (see `rustyzoo help`) whose first comment lines say how it is run
# and what it builds:
#   #run <map> <flags>    the words after `run`, such as `maps/default.zoo --cash 75000 --seed 5
#                         --passes 22300 --clock 200` (the run adds --headless and --script)
#   #exhibit X0,Y0,X1,Y1  the tiles of the land exhibit with the animals (none for a tank)
#   #months N             the month changes (`sim month` lines) the run must show: 12 for the thirteenth
#                         month, 3 for the fourth
#   #expect N <words>     what the script builds and hires: at least N lines of the log start (after the
#                         pass and the time) with the words, each an extended regular expression that
#                         must match a whole word (`unit add [0-9]+ keeper`); N 0 means no such line.
#                         The checks fail when the game did not build it; they never skip for it
#   #pending <check> N    leaves the check out until the issue N is fixed
#
# Both logs must have the same state hashes (the `sim hash` line of each month change) and be the
# same, byte for byte (the lines carry the units, guests, money, rating and the hash of every frame).
# The line `ok: <script>: ... last frame F, last state S` gives the last hashes, which CI compares
# between systems. The checks below read the first log; a line says `ok`, `skipped` (the game has
# nothing for it), `pending` or FAILED. Exit status: 1 if one failed.
set -u
export SDL_AUDIO_DRIVER=dummy # never a real audio device
export RUSTYZOO_ENGLISH_DATES=1 # the same dates (and frames) whatever the system's locale
bin=$1
data=$2
script=$3

# The clock at which an animal of the shortest-lived species may die of old age, in ms. An animal
# that is gone later must have posted `animal dying ID` before (animals-needs 10.2).
OLD_AGE_MS=4000000
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
name=$(basename "$script" .txt)
failed=0

ok() { echo "ok: $name: $*"; }
skip() { echo "skipped: $name: $*"; }
fail() { echo "FAILED: $name: $*" >&2; failed=1; }
# pending <check>: whether the script leaves the check out (it prints the line).
pending() {
  issue=$(sed -n "s/^#pending $1 \([0-9][0-9]*\).*/\1/p" "$script" | head -1)
  [ -n "$issue" ] || return 1
  echo "pending: $name: $1, waits for issue $issue"
}
header() { sed -n "s/^#$1 //p" "$script" | head -1 | tr -d '\r'; }

words=$(header run)
if [ -z "$words" ]; then
  fail "no #run line in $script"
  exit 1
fi
for r in a b; do
  # shellcheck disable=SC2086 # the words are a list of flags
  (RUSTYZOO_USER_DIR=$tmp/user-$r "$bin" --data "$data" run $words --headless --script "$script" >"$tmp/$r.log" 2>"$tmp/$r.err"; echo $? >"$tmp/$r.status") &
done
wait
for r in a b; do
  [ "$(cat "$tmp/$r.status")" = 0 ] || { fail "the run $r ended with status $(cat "$tmp/$r.status"): $(tail -n 3 "$tmp/$r.err")"; exit 1; }
done
tr -d '\r' <"$tmp/a.log" >"$tmp/log"

# The same game twice: the same state at each month change, then the same log.
hashes() { awk '$4 == "sim" && $5 == "hash" { print $1, $2, $6 }' "$tmp/$1.log" | tr -d '\r'; }
hashes a >"$tmp/a.hash"
hashes b >"$tmp/b.hash"
if [ ! -s "$tmp/a.hash" ]; then
  fail "no state hash: the run shows no month change"
elif ! cmp -s "$tmp/a.hash" "$tmp/b.hash"; then
  fail "the state hashes of the two runs differ from the month change $(cmp "$tmp/a.hash" "$tmp/b.hash" | sed 's/.*line //') on: $(diff "$tmp/a.hash" "$tmp/b.hash" | sed -n 's/^< //p' | head -1)"
elif cmp -s "$tmp/a.log" "$tmp/b.log"; then
  last=$(awk '$4 == "draw" && $5 != "-" { h = $5 } END { print h }' "$tmp/log")
  ok "two runs of $SECONDS s, the same $(wc -l <"$tmp/log" | tr -d ' ') lines, last frame $last, last state $(tail -n 1 "$tmp/a.hash" | cut -d' ' -f3)"
else
  fail "the two runs differ: first line $(cmp "$tmp/a.log" "$tmp/b.log" | sed 's/.*line //')"
fi

# Counts of the log: <awk condition> -> number of lines.
count() { awk "$1 { n++ } END { print n + 0 }" "$tmp/log"; }

# What the script builds and hires, as its header declares it.
while read -r n words; do
  [ -n "$n" ] || continue
  case $n in *[!0-9]*) fail "builds: '$n $words' does not start with a number"; continue ;; esac
  got=$(awk -v w="$words" 'BEGIN { nw = split(w, p, " ") }
    { for (i = 1; i <= nw; i++) if ($(3 + i) !~ ("^(" p[i] ")$")) next; c++ }
    END { print c + 0 }' "$tmp/log")
  if [ "$n" = 0 ]; then
    [ "$got" = 0 ] && ok "builds: no '$words'" || fail "builds: $got lines '$words', the script has none"
  elif [ "$got" -ge "$n" ]; then
    ok "builds: $got lines '$words' (at least $n)"
  else
    fail "builds: $got lines '$words', at least $n needed"
  fi
done <<EXPECT
$(sed -n 's/^#expect //p' "$script" | tr -d '\r')
EXPECT
# want <words>: the number the script expects for these words (0 when it names none).
want() { tr -d '\r' <"$script" | awk -v w="$1" '/^#expect [0-9]+ / { n = $2; sub(/^#expect [0-9]+ /, ""); if ($0 == w) { print n; exit } }'; }

# The run reached the month the script names.
months=$(header months)
case $months in
  ""|*[!0-9]*) fail "months: no '#months N' line in the header" ;;
  *) shown=$(count '$4 == "sim" && $5 == "month"')
     if [ "$shown" -ge "$months" ]; then ok "months: $shown month changes (at least $months)"
     else fail "months: $shown month changes, at least $months needed"; fi ;;
esac

# Guests created and gone.
made=$(count '$4 == "guest" && $5 == "add"')
gone=$(count '$4 == "guest" && $5 == "leave"')
if [ "$made" -gt 0 ] && [ "$gone" -gt 0 ] && [ "$gone" -le "$made" ]; then
  ok "guests: $made created, $gone gone"
else
  fail "guests: $made created, $gone gone"
fi

# A tour group formed: a guest asked to follow a tour guide (when one was hired).
guides=$(want 'unit add [0-9]+ guide')
followers=$(count '$4 == "guest" && $5 == "follow" && $7 != 0')
if [ "${guides:-0}" = 0 ]; then
  skip "tour group: the script hires no tour guide"
elif pending tour; then :
elif [ "$followers" -gt 0 ]; then
  ok "tour group: $followers requests to follow a guide"
else
  fail "tour group: a guide was hired and no guest followed"
fi

# The Finances table, as the log shows it last: row R of the column C (0 is the current month, 1 to
# 3 the months before) is the element 3930 + 15 C + R (rows: 0 admissions, 1 admissions income, 4 upkeep,
# 6 concessions, 7 wages).
cell() { awk -v id="$1" '$4 == "money" && $5 == "cell" && $6 == id { t = $8; for (i = 9; i <= NF; i++) t = t " " $i } END { print t }' "$tmp/log"; }
# row <R>: the texts of the four columns, such as "$154 $2,926 $3,850 $3,927".
row() { for c in 0 1 2 3; do printf '%s ' "$(cell $((3930 + 15 * c + $1)))"; done; }
# nonzero <R>: whether a column of the row holds an amount other than 0 (or blank).
nonzero() { row "$1" | tr ' ' '\n' | grep -v '^$' | grep -qv '^[$(]*0[)]*$'; }
if [ -z "$(cell 3930)" ]; then
  fail "the Finances table was never shown (no money cell line)"
else
  nonzero 0 && ok "admissions in the table: $(row 0)" || fail "admissions in the table: $(row 0)"
  nonzero 1 && ok "admissions income in the table: $(row 1)" || fail "admissions income in the table: $(row 1)"
  nonzero 4 && ok "upkeep in the table: $(row 4)" || fail "upkeep in the table: $(row 4)"
  nonzero 6 && ok "concessions in the table: $(row 6)" || fail "concessions in the table: $(row 6)"
  paid=$(awk '$4 == "sim" && $5 == "wages" { n++; s += $7 } END { print n + 0, s + 0 }' "$tmp/log")
  case $paid in
    "0 "*) fail "wages: none booked at a month change" ;;
    *" 0") ok "wages: ${paid% *} booked, all 0 (the staff of this data cost nothing); table: $(row 7)" ;;
    *) nonzero 7 && ok "wages in the table: $(row 7), ${paid% *} booked" || fail "wages in the table: $(row 7)" ;;
  esac
fi

# Purchases: guests bought at a building.
bought=$(count '$4 == "building" && $5 == "buy"')
if pending purchases; then :
elif [ "$bought" -gt 0 ]; then ok "purchases: $bought at buildings"; else fail "purchases: none"; fi

# A trash can calls a worker at 13 pieces (guest buildings 8, staff 13.2): the script's can has had 13 uses
# (`building use`, each puts one piece in) when the first maintenance worker sets out for it (`staff job
# ID TrashCan`), not before, and a worker comes once it has had 13.
can=$(awk '$4 == "world" && $5 == "add" && $6 ~ /\/trshcan$/ { print $9; exit }' "$tmp/log")
workers=$(want 'unit add [0-9]+ worker')
if [ -z "$can" ] || [ "${workers:-0}" = 0 ]; then
  skip "trash can: the script builds no can or hires no maintenance worker"
else
  uses=$(count '$4 == "building" && $5 == "use" && $6 == '"$can")
  first=$(awk -v can="$can" '$4 == "building" && $5 == "use" && $6 == can { n++ } $4 == "staff" && $5 == "job" && $7 == "TrashCan" { print n + 0; exit }' "$tmp/log")
  if [ "$uses" -lt 13 ]; then skip "trash can: only $uses uses, the can never held 13 pieces"
  elif [ -z "$first" ]; then fail "trash can: $uses uses and no worker went to it"
  elif [ "$first" -lt 13 ]; then fail "trash can: a worker was sent after $first uses, before 13"
  else ok "trash can: $uses uses, the first worker went to it after $first"; fi
fi

# The animals: the adopted ones (a line `unit add ID animal`), none gone, each fed, all in their exhibit.
animals=$(awk '$4 == "unit" && $5 == "add" && $7 == "animal" { print $6 }' "$tmp/log" | tr '\n' ' ')
n=$(echo "$animals" | wc -w | tr -d ' ')
if [ "$n" = 0 ]; then
  fail "animals: none adopted"
else
  lost=0 early="" died="" hungry=""
  for a in $animals; do
    # The time of the clock, in ms, at which the animal left the game, and 1 if a line `animal dying`
    # of it came before; nothing if it did not leave.
    left=$(awk -v id="$a" '$4 == "animal" && $5 == "dying" && $6 == id { d = 1 }
      $4 == "unit" && $5 == "remove" && $6 == id { print substr($3, 3) + 0, d + 0; exit }' "$tmp/log")
    if [ -n "$left" ]; then
      lost=$((lost + 1))
      if [ "${left% *}" -ge "$OLD_AGE_MS" ] && [ "${left#* }" = 1 ]; then died="$died $a"; else early="$early $a"; fi
    fi
    [ "$(count '$4 == "animal" && $5 == "eat" && $6 == '"$a")" -gt 0 ] || hungry="$hungry $a"
  done
  # The shortest life of the species of the data is 4320 s (cTimeDeath of a gazelle, a zebra, a walrus):
  # an animal may be gone after $OLD_AGE_MS ms of the clock only if it died of old age (it posted
  # `animal dying` first); any other removal is a failure.
  if [ -n "$early" ]; then fail "animals: gone, and not of old age after $((OLD_AGE_MS / 1000)) s:$early"
  elif [ "$lost" = 0 ]; then ok "animals: $n alive at the end"
  else ok "animals: $((n - lost)) of $n alive at the end, died of old age:$died"; fi
  if pending fed; then :
  elif [ -z "$hungry" ]; then ok "animals: all $n fed"; else fail "animals: never ate:$hungry"; fi
  # Happiness above the start value of the animal (its first status line; the species differ) at some
  # time of the run; the exhibit of the base trial has no raked dung (its keeper has no cDirt), so it
  # falls later. The young (the ids of the `animal birth FATHER ID ...` lines) are left out: a young
  # born during the run starts at 100 and cannot rise above it.
  if pending happiness; then :; else
    flat="" up=""
    adopted=$(awk '$4 == "animal" && $5 == "birth" { young[$7] } $4 == "unit" && $5 == "add" && $7 == "animal" { all[++n] = $6 }
      END { for (i = 1; i <= n; i++) if (!(all[i] in young)) print all[i] }' "$tmp/log" | tr '\n' ' ')
    for a in $adopted; do
      h=$(awk -v id="$a" '$4 == "animal" && $5 == "status" && $6 == id { if (!seen++) first = $7; if ($7 > best) best = $7 }
        END { print first + 0, best + 0 }' "$tmp/log")
      if [ "${h#* }" -gt "${h% *}" ]; then up="$up $a:${h% *}->${h#* }"; else flat="$flat $a:${h% *}"; fi
    done
    [ -z "$flat" ] && ok "animals: happiness above the start value (start->best):$up" || fail "animals: happiness never above the start value (start):$flat"
  fi
  # Swimming: a tank's animals have a depth.
  tanks=$(want 'exhibit tank [0-9]+')
  if [ "${tanks:-0}" = 0 ]; then
    skip "swimming: the script builds no tank"
  elif pending swim; then :
  else
    still=""
    for a in $animals; do
      [ "$(count '$4 == "unit" && $5 == "depth" && $6 == '"$a")" -gt 0 ] || still="$still $a"
    done
    [ -z "$still" ] && ok "swimming: every animal has a depth" || fail "swimming: no depth for$still"
  fi
  # No loose animal: every tile an animal entered is a tile of the exhibit.
  box=$(header exhibit)
  if [ -n "$box" ]; then
    out=$(awk -v box="$box" -v ids="$animals" '
      BEGIN { split(box, b, ","); n = split(ids, a, " "); for (i = 1; i <= n; i++) mine[a[i]] = 1 }
      $4 == "unit" && $5 == "tile" && ($6 in mine) { split($7, t, ",")
        if (t[1] < b[1] || t[1] > b[3] || t[2] < b[2] || t[2] > b[4]) { bad++; at = $7 } }
      END { print bad + 0, at }' "$tmp/log")
    [ "${out% *}" = 0 ] && ok "no loose animal: all tiles inside $box" || fail "a loose animal: ${out% *} tiles outside $box, last ${out#* }"
  fi
fi

# The rating moved.
rated=$(awk '$4 == "sim" && $5 == "rating" { if (!n++) lo = hi = first = $6; if ($6 < lo) lo = $6; if ($6 > hi) hi = $6; last = $6 }
  END { print n + 0, first, last, lo, hi }' "$tmp/log")
set -- $rated
if [ "$1" -gt 1 ] && [ "$4" != "$5" ]; then ok "rating: $2 at the start, $3 at the end, between $4 and $5"; else fail "rating: did not move ($rated)"; fi

exit $failed
