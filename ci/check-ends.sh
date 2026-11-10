#!/usr/bin/env bash
# Plays a scenario to its win and another to its loss, headless, by script, and checks how each ends.
# Usage: ci/check-ends.sh <rustyzoo binary> <game folder>
#
# The two scenarios are made here (a few lines each) and put in a copy of the install's settings,
# ahead of the install's own folders, so that they run on the map of the install in any edition:
#   win   starts with 100000 of cash and wins when the cash reaches 150000: the script donates 10000
#         five times (the `$` key), and the scenario has no time limit
#   lose  has a time limit of one month and a goal no zoo can meet there (100000 guests)
# Both are in the scenario list, so the result is stored. The game of an install that ships the third
# scenario (a limit of 6 months, goals a zoo of its first day cannot meet) also plays it to its loss.
# What is checked is a property of the end, not a step number: the scenario ends once, the way it
# should, after the donation or the month change that causes it and within a few passes of it, the
# end dialog comes in the same pass with the text of that result and a question is asked, the result
# is stored for the list entry and in the user's settings, and No on the dialog ends the game and
# shows the scenario screen. A line says `ok` or FAILED; the exit status is 1 if one failed.
set -u
bin=$1
data=$2
export SDL_AUDIO_DRIVER=dummy # never a real audio device
export RUSTYZOO_ENGLISH_DATES=1 # the same dates whatever the system's locale
failed=0

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
ok() { echo "ok: $*"; }
fail() { echo "FAILED: $*" >&2; failed=1; }
rz() { "$bin" --data "$data" "$@" 2>/dev/null; }

# The copy of the install: its settings with the folder `mod` first on the resource path (the other
# entries made absolute), its libraries and the map the scenarios start (a game opens its map from the
# install folder).
ov=$tmp/install
mkdir -p "$ov/mod/scenario/ci" "$ov/maps"
ini=$(find "$data/" -maxdepth 1 -iname zoo.ini | head -1)
[ -n "$ini" ] || ini=$(find "$data/" -maxdepth 1 -iname zoodemo.ini | head -1)
if [ -z "$ini" ]; then
  fail "no zoo.ini in the install"
  exit 1
fi
find "$data/" -maxdepth 1 -iname '*.dll' -exec cp {} "$ov/" \;
awk -v mod="$ov/mod" -v data="$data" 'BEGIN { FS = "=" } /^\[/ { sec = tolower($0) }
  sec ~ /^\[resource\]/ && tolower($1) == "path" && !done {
    sub(/\r$/, "", $2); n = split($2, e, ";"); out = mod
    for (i = 1; i <= n; i++) { if (e[i] == "") continue; p = e[i]; if (p !~ /^([\/\\]|[A-Za-z]:)/) p = data "/" p; out = out ";" p }
    print "path=" out; done = 1; next }
  { print }' "$ini" >"$ov/$(basename "$ini")"
map=maps/default.zoo
rz cat $map >"$ov/$map" || true
if [ ! -s "$ov/$map" ]; then
  map=maps/sm_aqua.zoo
  rz cat $map >"$ov/$map" || true
fi
[ -s "$ov/$map" ] || { fail "no map to start the scenarios on"; exit 1; }
cat >"$ov/mod/scenario/ci/win.scn" <<EOF
[desc]
name=16022
winpicture=ui/scenario/win/win
winstory=SCENARIO_WIN_STORY_SCN01
losepicture=ui/scenario/lose/lose
losestory=SCENARIO_LOSE_STORY_SCN01
[start]
savegame=$map
setcash=100000
reset=1
[goals]
goal=rich
[rich]
rulea=1
ruleb=7
value=150000
text=17501
EOF
cat >"$ov/mod/scenario/ci/lose.scn" <<EOF
[desc]
name=16022
winpicture=ui/scenario/win/win
winstory=SCENARIO_WIN_STORY_SCN01
losepicture=ui/scenario/lose/lose
losestory=SCENARIO_LOSE_STORY_SCN01
[start]
savegame=$map
setcash=100000
reset=1
[duration]
nummonths=1
[goals]
goal=crowd
[crowd]
rulea=1
ruleb=3
value=100000
text=17501
EOF
printf '[zz]\nname=16022\nscenario=scenario/ci/win.scn\nunlocks=zz\n[zy]\nname=16022\nscenario=scenario/ci/lose.scn\nunlocks=zy\n' >"$ov/mod/scenarzz.cfg"

# play <install> <scenario file> <passes> <script lines...>: the log in $tmp/log, the user's settings
# in $tmp/user/zoo.ini. The scenario starts at pass 2.
play() {
  inst=$1 file=$2 passes=$3
  shift 3
  rm -rf "$tmp/user"
  mkdir -p "$tmp/user"
  printf '[UI]\nplayMovie=0\n' >"$tmp/user/zoo.ini"
  { echo "2 scenario $file"; printf '%s\n' "$@"; } >"$tmp/script"
  RUSTYZOO_USER_DIR=$tmp/user "$bin" --data "$inst" run --headless --passes "$passes" --clock 200 --seed 1 \
    --script "$tmp/script" >"$tmp/log" 2>"$tmp/err"
  status=$?
  tr -d '\r' <"$tmp/log" >"$tmp/log.txt"
  [ "$status" = 0 ] || fail "$file: the run ended with status $status"
  return 0
}

# check_end <name> <won|lost> <list key> <pass the cause came at> <install> <scenario file>: the properties
# of the end in $tmp/log.txt.
check_end() {
  name=$1 result=$2 key=$3 cause=$4 inst=$5 file=$6
  [ "$result" = won ] && want=1 || want=2
  ends=$(grep -c ' sim scenario end ' "$tmp/log.txt")
  got=$(sed -n 's/^pass \([0-9]*\) .* sim scenario end \(.*\)$/\1 \2/p' "$tmp/log.txt" | head -1)
  pass=${got% *}
  if [ "$ends" != 1 ] || [ "${got#* }" != "$result" ]; then
    fail "$name: the scenario ended $ends times, first: '${got:-never}', expected once, $result"
    return
  fi
  if [ "$pass" -lt "$cause" ] || [ "$pass" -gt $((cause + 10)) ]; then
    fail "$name: ended at pass $pass, the cause came at pass $cause (expected within 10 passes after)"
    return
  fi
  ok "$name: $result once, at pass $pass, $((pass - cause)) after the cause"
  # The dialog in the same pass: 16100 + 4 c + (lost ? 1 : 0), and a question asked then.
  text=$(sed -n "s/^pass $pass .* menu scenario dialog \([0-9]*\)\$/\1/p" "$tmp/log.txt" | head -1)
  if [ -n "$text" ] && [ $(((text - 16100) % 4)) = $((want - 1)) ]; then
    ok "$name: the end dialog $text in the same pass (the text of a $result scenario)"
  else
    fail "$name: no end dialog for a $result scenario in pass $pass (text '${text:-none}')"
  fi
  grep -q "^pass $((pass + 1)) .* ui ask 45200\|^pass $pass .* ui ask 45200" "$tmp/log.txt" ||
    fail "$name: the end dialog was not asked"
  # The result: the list entry, then the user's settings.
  grep -q " menu scenario result $key $want\$" "$tmp/log.txt" && ok "$name: result $want for the list entry" ||
    fail "$name: no result $want for the entry"
  sed -n '/^\[scenario\]/,/^\[user\]/p' "$tmp/user/zoo.ini" | grep -q "^$key=$want\$" && ok "$name: result $want in the settings" ||
    fail "$name: no result $want for $key in the user's settings"
  # No on the dialog stops the game and shows the scenario screen with the entries.
  stop=$(grep -c ' menu game stop$' "$tmp/log.txt")
  shown=$(grep -c ' menu scenario select ' "$tmp/log.txt")
  [ "$stop" = 1 ] && [ "$shown" -ge 1 ] && ok "$name: No ends the game and shows the scenario screen" ||
    fail "$name: after No: $stop game stops, $shown scenario screens"
  # The scenario screen shows the picture and story of that result (win*/lose* of the file's [desc]), not
  # the ones of a game not yet played: the last selection says so.
  [ "$result" = won ] && pre=win || pre=lose
  pic=$("$bin" --data "$inst" cat "$file" 2>/dev/null | tr -d '\r' |
    awk -F= -v k="${pre}picture" 'tolower($1) == "[desc]" { d = 1 } d && tolower($1) == k { print $2; exit }')
  sel=$(grep ' menu scenario select ' "$tmp/log.txt" | tail -1)
  case "$sel" in
  *"select $key "*" picture ${pic:-none} story shown "*) ok "$name: the screen shows the ${pic:-none} picture and a story" ;;
  *) fail "$name: the screen after the end shows '${sel#*select }', expected picture '${pic:-none}' and a story" ;;
  esac
}

# The win: five donations at passes 21 to 25; the cash passes the goal with the fifth.
play "$ov" scenario/ci/win.scn 60 "21 char 36" "22 char 36" "23 char 36" "24 char 36" "25 char 36" "40 key n"
cause=$(awk '$4 == "money" && $5 == "cash" && $6 >= 150000 { print $2; exit }' "$tmp/log.txt")
[ -n "$cause" ] || fail "win: the cash never reached the goal"
check_end win won zz "${cause:-0}" "$ov" scenario/ci/win.scn

# The loss at the end of the month: the calendar reaches the second month, and the goal is not met.
play "$ov" scenario/ci/lose.scn 2100 "2050 key n"
cause=$(awk '$4 == "sim" && $5 == "month" { print $2; exit }' "$tmp/log.txt")
[ -n "$cause" ] || fail "lose: no month change"
check_end lose lost zy "${cause:-0}" "$ov" scenario/ci/lose.scn

# The third scenario of a full game, played as shipped: lost when the sixth month ends.
if rz ls 'scenario/scn03/scn03.scn' | grep -qi scn03; then
  play "$data" scenario/scn03/scn03.scn 11200 "11150 key n"
  cause=$(awk '$4 == "sim" && $5 == "month" && $6 == 7 { print $2; exit }' "$tmp/log.txt")
  [ -n "$cause" ] || fail "scn03: no seventh month"
  key=$(sed -n 's/^.* menu scenario result \([a-z]*\) 2$/\1/p' "$tmp/log.txt" | head -1)
  check_end scn03 lost "${key:-none}" "${cause:-0}" "$data" scenario/scn03/scn03.scn
fi
exit $failed
