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
export SDL_AUDIO_DRIVER=dummy # check-music opens the audio device: never a real one

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
export RUSTYZOO_USER_DIR=$tmp/user # never the real user folder

rz() { "$bin" --data "$dir/$trial" "$@"; }

# Plays a map headless (--bare: the map view alone, without the main panel that a game shows) with
# scrolls, a rotation and the zoom in and out (-2, 0 and 2), the game paused so that the units
# stand still, and compares every painted frame with the window `render-map --view` draws for the
# camera the log names, at its zoom, byte for byte (as PNG files).
same_frames() {
  d=$(mktemp -d)
  printf '0 pause\n1 scroll 1 1\n2 rotate cw\n3 scroll -1 0\n4 zoom in\n5 scroll 1 0\n6 zoom out\n7 zoom out\n8 scroll 0 1\n' >"$d/script"
  rz run "$1" --headless --bare --passes 9 --clock 20 --script "$d/script" --png-every 1 --out "$d" |
    sed -n 's/^pass \([0-9]*\) .* draw [0-9a-f]\{16\} view=\(-*[0-9]*\),\(-*[0-9]*\) rotation=\([0-3]\) zoom=\(-*[0-9]*\).*/\1 \2 \3 \4 \5/p' |
    while read -r pass x y rotation zoom; do
      rz render-map "$1" "$d/ref.png" --rotation "$rotation" --zoom "$zoom" --view "$x" "$y" 1024 768 >/dev/null || { echo failed; continue; }
      if cmp -s "$d/frame-$(printf %06d "$pass").png" "$d/ref.png"; then echo same; else echo different; fi
    done | sort | uniq -c
  rm -rf "${d:?}"
}

# Plays a map in a window of the dummy video driver (nothing is shown) for 30 passes of the real
# clock, with commands from a script, and writes a log. Prints the game size and the factor, the
# window size, the size in pixels and the output size the program reports, whether at least four
# frames were painted, and whether the log replayed headless gives the same lines, so the same
# frame hashes.
windowed() {
  d=$(mktemp -d)
  printf '4 scroll 1 1\n12 rotate cw\n20 scroll -1 0\n' >"$d/script"
  SDL_VIDEO_DRIVER=dummy rz run "$1" --passes 30 --script "$d/script" --log "$d/log" |
    sed -n -e 's/^game \([0-9x]*\) factor \([0-9]*\)$/\1,\2/p' \
      -e 's/^window \([0-9x]*\) pixels \([0-9x]*\) output \([0-9x]*\)$/\1,\2,\3/p' | tr '\n' ,
  awk '/ draw [0-9a-f]{16} /{n++} END{printf (n >= 4 ? "frames," : "few,")}' "$d/log"
  if rz run "$1" --headless --replay "$d/log" | cmp -s - "$d/log"; then echo same; else echo different; fi
  rm -rf "${d:?}"
}

# Switches the dummy window to full screen and back by script, with a zoom in during each. The map
# is 800 pixels wide, so full screen refuses the zoom in at zoom 0 (camera 8.3) and the window
# takes it. Prints the log lines of the switches, the zoom of the last draw, and whether the log
# replayed headless gives the same lines.
fullscreen() {
  d=$(mktemp -d)
  printf '3 fullscreen on\n8 zoom in\n14 fullscreen off\n18 zoom in\n' >"$d/script"
  SDL_VIDEO_DRIVER=dummy rz run "$1" --passes 30 --script "$d/script" --log "$d/log" >/dev/null
  sed -n -E 's/^pass [0-9]+ t=[0-9]+ fullscreen (on|off)$/\1/p' "$d/log" | tr '\n' ,
  awk '/ draw /{z=$NF} END{printf "%s,", z}' "$d/log"
  if rz run "$1" --headless --replay "$d/log" | cmp -s - "$d/log"; then echo same; else echo different; fi
  rm -rf "${d:?}"
}

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
check base-trial 5,3,0,2 "rz check-exhibits '*.zoo' | tail -1 | grep -o '[0-9]*' | paste -sd, -"
check base-trial 3 "rz check-exhibits '*.zoo' | grep -c 'stored first tiles in distinct exhibits'"
# 3 maps, 5 species rated in their exhibits, none out of range
check base-trial 3,5,0 "rz check-rating '*.zoo' | tail -1 | grep -o '[0-9]*' | paste -sd, -"
check base-trial 5,3,2,0,13750,1371 "rz check-maps | tail -1 | grep -o '[0-9]*' | paste -sd, -"
# Whole maps drawn in memory: maps drawn, skipped, failed; tiles drawn from the cache, rendered at
# run time (the cache lacks them), undrawn; cliff pieces drawn, missing; objects drawn, with a
# missing type or animation, not placed.
check base-trial 3,2,0,13731,19,0,71,0,1371,0,0 "rz check-render | tail -1 | grep -o '[0-9]*' | paste -sd, -"
# One map at the three zoom levels (-2, 0, 2): the size of the picture of the whole map, in the
# view space of that zoom.
check base-trial 2404x1233,4800x2459,9604x4922 "for z in -2 0 2; do rz render-map maps/default.zoo $tmp/z.png --zoom \$z | grep -o '[0-9]*x[0-9]* pixels'; done | tr -d ' ' | sed 's/pixels//' | paste -sd, -"
check expansion-trial 2404x1204,4800x2401,9604x4806 "for z in -2 0 2; do rz render-map maps/sm_aqua.zoo $tmp/z.png --rotation 1 --zoom \$z | grep -o '[0-9]*x[0-9]* pixels'; done | tr -d ' ' | sed 's/pixels//' | paste -sd, -"
# Root bitmaps (grid lines, highlight, cliff pieces) converted in memory and compared with the
# shipped copies: bitmaps, identical, differing, without a shipped copy, failed.
check base-trial 41,41,0,0,0 "rz check-bitmaps | tail -1 | grep -o '[0-9]*' | paste -sd, -"
# Behaviour sets: files with sets or tables, sets, calls, ambient tables, calls of unknown names, calls with
# defective arguments (the numbers of the specification's survey).
check base-trial 8,377,1285,9,5,1 "rz check-behaviours 2>/dev/null | tail -1 | grep -o '[0-9]*' | paste -sd, -"
# Entity types: the total, then per class (food, paths, fences, buildings, animals, guests, scenery,
# staff, ambient, rubble, tank walls, filters), and the types whose base animation member is missing.
check base-trial 51,6,1,6,17,4,2,13,2,0,0,0,0 "rz types | tail -1 | grep -o '[0-9]*' | paste -sd, -"
check base-trial 1 "rz types | awk -F'\\t' 'NF>6 && \$7==\"no\"{n++} END{print n+0}'"
# Ground pictures of the terrain cache rendered again: pictures, covered pixels, pictures with
# differing covered pixels, differing pixels, other cuts, without palette, failed (the numbers before
# the semicolon); then the colour difference. The shipped plain pictures are lit and the rendered
# ones are not, so they differ by about 11 levels; the expansion trial has no cache.
check base-trial 2907,3126249,0,0,0,0,0 "rz check-ground | tail -1 | cut -d';' -f1 | grep -o '[0-9]*' | paste -sd, -"
check base-trial R11.10G10.61B7.62,95thpercentile25,max214 "rz check-ground | tail -1 | sed 's/.*mean //'"
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
check expansion-trial 5,5,0,0 "rz check-exhibits '*.zoo' | tail -1 | grep -o '[0-9]*' | paste -sd, -"
check expansion-trial 5 "rz check-exhibits '*.zoo' | grep -c 'stored first tiles in distinct exhibits'"
check expansion-trial 5,5,0 "rz check-rating '*.zoo' | tail -1 | grep -o '[0-9]*' | paste -sd, -"
check expansion-trial 5,5,0,0,28125,4160 "rz check-maps | tail -1 | grep -o '[0-9]*' | paste -sd, -"
# The expansion trial ships no ground cache, so every tile is rendered at run time. It ships no
# converted cliff pieces either (only their bitmaps), so they are converted in memory: all 2434
# are drawn.
check expansion-trial 5,0,0,0,28125,0,2434,0,3421,290,449 "rz check-render | tail -1 | grep -o '[0-9]*' | paste -sd, -"
# Behaviour sets: files with sets or tables, sets, calls, ambient tables, calls of unknown names, calls with
# defective arguments (the numbers of the specification's survey).
check expansion-trial 13,987,2728,25,5,1 "rz check-behaviours 2>/dev/null | tail -1 | grep -o '[0-9]*' | paste -sd, -"
check expansion-trial 150,14,8,17,26,8,2,47,6,0,7,14,1 "rz types | tail -1 | grep -o '[0-9]*' | paste -sd, -"
# The expansion trial ships none of the 41 copies: all convert.
check expansion-trial 41,0,0,41,0 "rz check-bitmaps | tail -1 | grep -o '[0-9]*' | paste -sd, -"
check expansion-trial 0 "rz types | awk -F'\\t' 'NF>6 && \$7==\"no\"{n++} END{print n+0}'"
check expansion-trial 0,0,0,0,0,0,0 "rz check-ground | tail -1 | cut -d';' -f1 | grep -o '[0-9]*' | paste -sd, -"
# The interface tree of ui/gamescrn.lyt: elements, layouts, duplicate ids, images, animations and
# pictures missing. The base trial has no expansion data (mask 0); the expansion trial has both.
check base-trial 902,75,0,24,174,1 "rz check-layouts 2>/dev/null | tail -1 | grep -o '[0-9]*' | paste -sd, -"
check expansion-trial 1119,99,0,3,244,0 "rz check-layouts 2>/dev/null | tail -1 | grep -o '[0-9]*' | paste -sd, -"
check expansion-trial 1013,86,0,3,224,0 "rz check-layouts --expansions 1 2>/dev/null | tail -1 | grep -o '[0-9]*' | paste -sd, -"
# The tree resolved for two window sizes (id:x,y,width,height in drawing order): the panel beside
# the left bar, the strip between two anchors that grows with the window, the main menu background
# centred, and the first button of the menu.
rects() { rz dump-layout --size "$1" 2>/dev/null | awk -F'\t' '$1==3||$1==1036||$1==7100||$1==7111{printf "%s:%s,%s,%s,%s;", $1,$5,$6,$7,$8}'; echo; }
check base-trial 902 "rz dump-layout 2>/dev/null | wc -l"
check base-trial '3:17,6,179,440;1036:293,733,54,35;7100:112,84,800,600;7111:337,359,350,25;' 'rects 1024x768'
check base-trial '3:17,6,179,440;1036:235,565,58,35;7100:0,0,800,600;7111:225,275,350,25;' 'rects 800x600'
check expansion-trial '3:17,6,179,440;1036:304,733,43,35;7100:112,84,800,600;7111:337,344,350,25;' 'rects 1024x768'
check expansion-trial '3:17,6,179,440;1036:235,565,69,35;7100:0,0,800,600;7111:225,260,350,25;' 'rects 800x600'

# The interface painted headless: the main menu with a hovered button, the size of the picture, and
# the exit status for a layout that does not exist (1) and a size of 0 (2, a usage error).
for trial in base-trial expansion-trial; do
  check $trial 800x600 "rz render-ui $tmp/ui.png --size 800x600 --show 26 --hover 7111 2>/dev/null | sed 's/.*: //'"
  check $trial 1 "rz render-ui $tmp/ui.png --show 99999 >/dev/null 2>&1; echo \$?"
  check $trial 2 "rz render-ui $tmp/ui.png --size 0x600 >/dev/null 2>&1; echo \$?"
done

# The main menu hovered, pressed, dragged off and back and released: what the interface did, from
# the log. The menu is shown over the map; the first button is at (512, 365) at 1024x768.
menu_click() {
  d=$(mktemp -d)
  printf '1 mouse 512 365\n2 down\n3 mouse 5 5\n4 mouse 512 365\n5 up\n' >"$d/script"
  rz run "$1" --headless --passes 8 --clock 20 --show 26 --script "$d/script" 2>/dev/null |
    grep ' ui ' | cut -d' ' -f4- | tr '\n' ';'
  echo
  rm -rf "${d:?}"
}
check base-trial 'uihover7111;uisounduiclick;uipress7111;uitooltip51239527428;uirelease7111inside;uiselect7111;uishow32;uihover50004;' "menu_click maps/default.zoo"
check expansion-trial 'uihover7111;uisounduiclick;uipress7111;uitooltip51239537628;uirelease7111inside;uiselect7111;uishow32;uihover50004;' "menu_click maps/scn19.zoo"

# Start up to the main menu, headless, without a map: the picture drawn (resource number and
# corner), the loading steps (also written to [UI] progresscalls of the user folder), Continue
# disabled, the version label, the hash of the first frame, and a click on Exit at y ending the run
# at its pass. The numbers depend on the data of each trial.
startup() {
  rm -rf "$tmp/user"
  mkdir -p "$tmp/user"
  printf '[UI]\nplayMovie=0\n' >"$tmp/user/zoo.ini" # the movies are checked below
  printf '1 mouse 512 %s\n2 down\n3 up\n' "$1" >"$tmp/exit"
  rz run --headless --passes 6 --clock 20 --script "$tmp/exit" 2>/dev/null |
    awk '/^startup picture/{p=$3"@"$5} /^startup steps/{s=$3} /menu continue/{c=$6} /menu version/{v=$6} / draw [0-9a-f]{16}/{if(!h)h=$5} /menu exit/{e=$2} END{print p","s","c","v","h",exit@"e}'
  sed -n 's/^progresscalls=//p' "$tmp/user/zoo.ini"
}
check base-trial '502@112,84,131,disabled,10.10.00.0004,a4fa517cc1900e76,exit@3,131' 'startup 620 | paste -sd, -'
check expansion-trial '505@112,84,253,disabled,12.05.00.0004,6f73263a788c8fdd,exit@3,253' 'startup 600 | paste -sd, -'
# Continue is enabled by a last file that exists (named in the tooltip); a name that is gone
# disables it and is written back empty; the settings of the game itself are never written.
last() {
  rm -rf "$tmp/user"
  mkdir -p "$tmp/user"
  printf '[UI]\nplayMovie=0\n[user]\nlastfile=%s\n' "$1" >"$tmp/user/zoo.ini"
  rz run --headless --passes 1 --clock 20 2>/dev/null | sed -n 's/^pass 0 t=0 menu continue //p'
  sed -n 's/^lastfile=//p' "$tmp/user/zoo.ini"
}
check base-trial 'enableddefault.zoo,maps\default.zoo' 'last "maps\\default.zoo" | tr -d " \n" | sed "s/maps/,maps/"'
check base-trial 'disabled,' 'last "maps\\gone.zoo" | paste -sd, -'

# A seeded run of a map for 160 passes of 67 ms (about 11 s of simulated time), twice: the two logs
# are the same, the units log the sets they start and an animal plays an animation other than the
# idle one (`unit set`, `unit anim`), and the frame hash changes at (nearly) every draw while the
# camera stands still.
animating() {
  d=$(mktemp -d)
  for run in a b; do
    rz run "$1" --headless --bare --passes 160 --clock 67 --seed 1 >"$d/$run" 2>/dev/null
  done
  if cmp -s "$d/a" "$d/b"; then printf same,; else printf different,; fi
  if grep ' unit set ' "$d/a" | grep -qv ' bZooRoutine$' && grep ' unit anim ' "$d/a" | grep -qv ' idle$'; then printf animated,; else printf still,; fi
  awk '/ draw /{h[$5]} END{print (length(h) >= 100 ? "changing" : "steady")}' "$d/a"
  rm -rf "${d:?}"
}
check base-trial same,animated,changing 'animating maps/scn01a.zoo'
# The sim command runs the sets too.
check base-trial yes 'rz sim maps/scn01a.zoo --ticks 160 --dt 67 --seed 1 --log 2>/dev/null | grep -q " unit set " && echo yes'
# A hungry lion (27645) goes to the pile of 50 units that the script puts down, takes bites of 20, 20 and
# what is left, and the pile goes.
check base-trial 'keeperfood;30,10,0' 'printf "0 food 45,38 carnchow 50\n" >"$tmp/food"; rz sim maps/scn01a.zoo --ticks 400 --seed 1 --script "$tmp/food" --log 2>/dev/null | awk "/ unit goal 27645 keeperfood /{g=\"keeperfood\"} / animal eat 27645 /{l=l (l?\",\":\"\") \$(NF-1)} END{print g \";\" l}"'
# A play call queues behind the playing sequence, the base loop included: the lion shows its first
# animation other than the idle one at pass 14 (the lion 27645, happiness 75, chooses bFriendly; the lion
# 27646 is tired, so its ambient choice runs the sleep check instead of a row, behaviours 6.4: it starts
# bSleep at pass 25 and shows its first other animation at pass 28); a sound named without .wav is a
# file of the type's folder.
check base-trial 28 'rz run maps/scn01a.zoo --headless --bare --passes 40 --clock 67 --seed 1 2>/dev/null | awk "/ unit anim 27646 / && \$NF != \"idle\" {print \$2}" | head -2 | paste -sd, -'
check base-trial bFriendly,14 'rz run maps/scn01a.zoo --headless --bare --passes 40 --clock 67 --seed 1 2>/dev/null | awk "/ unit set 27645 / && s == \"\" {s=\$NF} / unit anim 27645 / && \$NF != \"idle\" && a == \"\" {a=\$2} END {print s \",\" a}"'
check base-trial animals/lion/cat_sgr.wav 'rz run maps/scn01a.zoo --headless --bare --passes 1500 --clock 67 --seed 3 2>/dev/null | grep " unit sound " | awk "\$NF ~ /^animals/ {print \$NF}" | sort -u'

# The scenario screen from the menu button at y: the number of entries, the state of each, and the
# first entry selected (its state and whether Play is on). A click on the icon of the first row and a
# double click ask to play it: the scenario file the request names. (A transparent list is hit
# only where it draws something.)
scenario_screen() { # scenario_screen <script after the click on the button>
  rm -rf "$tmp/user"
  mkdir -p "$tmp/user"
  printf '[UI]\nplayMovie=0\n' >"$tmp/user/zoo.ini"
  printf '1 mouse 512 365\n2 down\n3 up\n%b\n' "$1" >"$tmp/scn"
  rz run --headless --passes 12 --clock 100 --script "$tmp/scn" 2>/dev/null |
    awk '/ menu scenarios /{n=$6} / menu scenario [a-z][a-z] /{s=s","$7} / menu scenario select /{if(!p)p=$8":"$NF} / menu play scenario /{y=$NF} END{print n s","p","y}'
}
check base-trial '2,playable,playable,playable:enabled,' 'scenario_screen ""'
check base-trial '2,playable,playable,playable:enabled,scenario/scn01a/scn01a.scn' 'scenario_screen "4 mouse 146 438\n5 down\n6 up\n7 double\n8 up"'
check expansion-trial '4,playable,playable,playable,playable,playable:enabled,' 'scenario_screen ""'

# The freeform screen from its menu button: the number of maps, then the starting cash and its
# difficulty. (The spinners of the full game are not in the trials' layout; the unit tests cover
# them.)
freeform_screen() {
  rm -rf "$tmp/user"
  mkdir -p "$tmp/user"
  printf '[UI]\nplayMovie=0\n' >"$tmp/user/zoo.ini"
  printf '1 mouse 512 405\n2 down\n3 up\n' >"$tmp/free"
  rz run --headless --passes 5 --clock 100 --script "$tmp/free" 2>/dev/null |
    awk '/ menu freeform /{n=$NF} / menu cash /{c=$6":"$7} END{print n","c}'
}
check base-trial '1,75000:middle' 'freeform_screen'
check expansion-trial '1,75000:middle' 'freeform_screen'

# From the menu into a game and back: Continue (at y) opens `[user] lastfile` as a saved game,
# Escape asks (leave), No shows the menu again (the music starts again), Continue once more, then
# closing the window asks (exit) and No ends the run. The lines of the game, the steps of the
# capped size (one after each start) and the restarts of the music, from the log.
game_round() { # game_round <y of Continue> <last file>
  rm -rf "$tmp/user"
  mkdir -p "$tmp/user"
  printf '[UI]\nplayMovie=0\n[user]\nlastfile=%s\n' "$2" >"$tmp/user/zoo.ini"
  printf '1 mouse 512 %s\n2 down\n3 up\n10 key escape\n12 key n\n20 mouse 512 %s\n21 down\n22 up\n30 close\n32 key n\n' "$1" "$1" >"$tmp/round"
  rz run --headless --passes 40 --clock 100 --script "$tmp/round" 2>/dev/null |
    awk '/ menu game /{g=g (g?";":"") $6" "$7} / step 200 /{c++} / menu music restart/{m++} END{print g","c","m}'
}
# --bare is for a run of a map.
check base-trial 2 'rz run --headless --bare --passes 1 --clock 1 >/dev/null 2>&1; echo $?'
check base-trial 0 'rz run maps/default.zoo --headless --bare --passes 1 --clock 1 >/dev/null 2>&1; echo $?'
check base-trial 'startcontinue;askleave;stop;startcontinue;askexit;exit,2,1' 'game_round 489 "maps\\default.zoo"'
check expansion-trial 'startcontinue;askleave;stop;startcontinue;askexit;exit,2,1' 'game_round 474 "maps\\scn19.zoo"'
# Play on a scenario and on a freeform map opens the map their file names, and empties the last
# file (so Continue is off when the menu comes back): the start lines and the last file left.
play_map() { # play_map <y of the menu button>
  rm -rf "$tmp/user"
  mkdir -p "$tmp/user"
  printf '[UI]\nplayMovie=0\n[user]\nlastfile=%s\n' 'maps\\default.zoo' >"$tmp/user/zoo.ini"
  printf '1 mouse 512 %s\n2 down\n3 up\n5 mouse 790 654\n6 down\n7 up\n' "$1" >"$tmp/play"
  rz run --headless --passes 10 --clock 100 --script "$tmp/play" 2>/dev/null | awk '/ menu game start /{print $7" "$8" "$NF}'
  sed -n 's/^lastfile=//p' "$tmp/user/zoo.ini"
}
check base-trial 'scenarioaamaps/scn01a.zoo,' 'play_map 365 | paste -sd, -'
check base-trial 'freeformcashmaps/default.zoo,' 'play_map 405 | paste -sd, -'
check expansion-trial 'freeformcashmaps/sm_aqua.zoo,' 'play_map 405 | paste -sd, -'
# A run of a map ends in the same state as a Continue: Escape asks, and No ends the run, there being
# no menu to go back to.
check base-trial 'askleave;exit' 'printf "3 key escape\n5 key n\n" > "$tmp/esc"; rz run maps/default.zoo --headless --passes 8 --clock 100 --script "$tmp/esc" 2>/dev/null | awk "/ menu game /{g=g (g?\";\":\"\") \$6\" \"\$7} END{print g}"'
# The same in a window of the dummy video driver: the log replays headless to the same lines.
windowed_game() { # windowed_game <y of Continue> <last file>
  d=$(mktemp -d)
  printf '[UI]\nplayMovie=0\n[user]\nlastfile=%s\n' "$2" >"$tmp/user/zoo.ini"
  printf '3 mouse 512 %s\n5 down\n6 up\n12 key escape\n14 key n\n24 mouse 512 %s\n25 down\n26 up\n32 close\n34 key n\n' "$1" "$1" >"$d/script"
  SDL_VIDEO_DRIVER=dummy rz run --size 1024x768 --passes 60 --script "$d/script" --log "$d/log" >/dev/null 2>&1
  grep -c 'menu game start' "$d/log"
  if rz run --headless --replay "$d/log" 2>/dev/null | cmp -s - "$d/log"; then echo same; else echo different; fi
  rm -rf "${d:?}"
}
check base-trial 2,same 'windowed_game 489 "maps\\default.zoo" | paste -sd, -'
check expansion-trial 2,same 'windowed_game 474 "maps\\scn19.zoo" | paste -sd, -'

# The same in a window of the dummy video driver (the start-up picture is presented as the bar
# moves): a click on Exit at y ends the run, and its log replays headless to the same lines.
windowed_menu() {
  d=$(mktemp -d)
  printf '3 mouse 512 %s\n5 down\n6 up\n' "$1" >"$d/script"
  SDL_VIDEO_DRIVER=dummy rz run --size 1024x768 --passes 40 --script "$d/script" --log "$d/log" >/dev/null 2>&1
  grep -c 'menu exit' "$d/log"
  if rz run --headless --replay "$d/log" 2>/dev/null | cmp -s - "$d/log"; then echo same; else echo different; fi
  rm -rf "${d:?}"
}
check base-trial 1,same 'windowed_menu 620 | paste -sd, -'
check expansion-trial 1,same 'windowed_menu 600 | paste -sd, -'

# The intro movies, headless on the passes of the scripted clock: the expansion trial has one (the
# base trial's first movie is not in its folder and is skipped with a warning): file, frames,
# duration in ms, the hash of the first picture, pictures shown, the hash of the 41st (the movie
# stretched over the whole window), the pass the movie ends on and the
# pass of the first menu (the pass after it). A key (or a click, or `skip`) at pass 12 stops it and
# the menu follows.
intro() {
  rm -rf "$tmp/user"
  printf '12 %s\n' "${1:-pause}" >"$tmp/skip"
  rz run --headless --passes 400 --clock 20 --script "$tmp/skip" 2>/dev/null |
    awk '/^startup movie/{m=$3","$4","$6} / movie [A-Z.]* frame [0-9]/{n++; if(!h)h=$NF} / movie [A-Z.]* frame 40 /{f=$NF} / movie .* (end|skipped)$/{e=$2"@"$NF} /menu continue/{c=$2} END{print m","h","n","f","e","c}'
}
check expansion-trial 'BFLOGO.AVI,82,6833,face601842022325,3,,12@skipped,13' 'intro "key enter"'
check expansion-trial 'BFLOGO.AVI,82,6833,face601842022325,3,,12@skipped,13' 'intro down'
check expansion-trial 'BFLOGO.AVI,82,6833,face601842022325,82,21628c137396ef8d,342@end,343' 'intro "mouse 1 1"'
check base-trial ',,,,,0' 'intro "key enter"'
# The same in a window of the dummy video driver: a script that skips ends the movie, the log tells
# it and replays headless to the same lines.
windowed_intro() {
  d=$(mktemp -d)
  printf '10 skip\n' >"$d/script"
  SDL_VIDEO_DRIVER=dummy rz run --size 1024x768 --passes 40 --script "$d/script" --log "$d/log" >/dev/null 2>&1
  grep -c 'movie BFLOGO.AVI skipped' "$d/log"
  if rz run --headless --replay "$d/log" 2>/dev/null | cmp -s - "$d/log"; then echo same; else echo different; fi
  rm -rf "${d:?}"
}
check expansion-trial 1,same 'windowed_intro | paste -sd, -'

# The credits on the interface clock (a pass every 66 ms, the interface update's period): a click on
# the credits button at y shows the first page at pass 3. The update of that pass sets the deadline
# (8 s per page later) and the second page comes at the first update at or after it, pass 125; that
# update sets the deadline of the third page (pass 247), and so on. After the last of the 16 pages the
# credits end. A click on Exit (here at pass 131) closes them at once. The base trial has no credits
# pages.
credits() { # credits <y of the button> <passes> <pass of the click on Exit>
  rm -rf "$tmp/user"
  mkdir -p "$tmp/user"
  printf '[UI]\nplayMovie=0\n' >"$tmp/user/zoo.ini"
  printf '1 mouse 512 %s\n2 down\n3 up\n%s mouse 512 664\n%s down\n%s up\n' "$1" "$3" "$3" "$3" >"$tmp/credits"
  rz run --headless --passes "$2" --clock 66 --script "$tmp/credits" 2>/dev/null |
    awk '/menu credits page/{n++; if(n==1)f=$2; if(n==2)p=$2; if(n==3)q=$2} /menu credits (end|closed)/{e=$2"@"$NF} END{print n","f","p","q","e}'
}
check expansion-trial '16,3,125,247,1955@end' 'credits 559 2200 9999'
check expansion-trial '2,3,125,,131@closed' 'credits 559 300 131'
check base-trial ',,,,' 'credits 574 300 131'

# The freeform map list filled with made-up rows: the track pages, an arrow steps, the wheel
# scrolls two rows, a row is selected, a key moves the selection (the list runs its select action
# on keys). Positions and rows, from the log.
list_use() {
  d=$(mktemp -d)
  printf '1 mouse 474 600\n2 down\n3 up\n4 mouse 474 400\n5 down\n6 up\n7 mouse 474 140\n8 down\n9 up\n10 mouse 132 146\n11 wheel 1\n12 down\n13 up\n14 key down\n' >"$d/script"
  rz run "$1" --headless --passes 16 --clock 100 --show 39 --rows 11504 40 --script "$d/script" 2>/dev/null |
    grep ' ui \(scroll\|row\|select\|focus\)' | cut -d' ' -f5- | tr '\n' ';'
  echo
  rm -rf "${d:?}"
}
list='scroll115059;scroll115058;scroll115056;focus11504;row115047;select115047;row115048;select115048;'
check base-trial "$list" "list_use maps/default.zoo"
check expansion-trial "$list" "list_use maps/scn19.zoo"

# A text field typed into: the click that focuses it selects its text, which the first digit
# replaces; a letter in a digits-only field is refused, Left moves the caret to the start where the
# next digit goes, Enter ends the edit.
field_use() {
  d=$(mktemp -d)
  printf '2 mouse 418 652\n3 down\n4 up\n5 char 55\n6 char 120\n7 key left\n8 key left shift\n9 char 57\n10 key enter\n' >"$d/script"
  rz run "$1" --headless --passes 12 --clock 100 --show 39 --text 11510 1234 --script "$d/script" 2>/dev/null |
    grep ' ui \(focus\|text\|unfocus\|key\)' | cut -d' ' -f5- | tr '\n' ';'
  echo
  rm -rf "${d:?}"
}
check expansion-trial 'focus11510;text115107;text1151097;unfocus11510;key39enter;' "field_use maps/scn19.zoo"

# The tooltip of the first main menu button: shown 30 pixels under the pointer after the update,
# gone after its 3 seconds. The size follows the text of the install.
tooltip_use() {
  d=$(mktemp -d)
  printf '1 mouse 512 365\n5 mouse 513 366\n' >"$d/script"
  rz run "$1" --headless --passes 40 --clock 100 --show 26 --script "$d/script" 2>/dev/null |
    grep ' ui \(tooltip\|hide\)' | sed 's/^pass \([0-9]*\) .* ui /\1 /' | tr '\n' ';'
  echo
  rm -rf "${d:?}"
}
question_use() {
  d=$(mktemp -d)
  printf '%b\n' "$2" >"$d/script"
  rz run "$1" --headless --passes 8 --clock 100 --ask 45000 "$3" --script "$d/script" 2>/dev/null |
    grep ' ui \(ask\|answer\)\|^pass [0-9]* t=[0-9]* step ' | sed 's/^pass \([0-9]*\) t=[0-9]* \(ui \)\{0,1\}/\1 /' | sed 's/step [0-9]* [-0-9]* [:.0-9]*/step/' | tr '\n' ';'
  echo
  rm -rf "${d:?}"
}
check base-trial '1tooltip51239527428;31hide7003;' "tooltip_use maps/default.zoo"
check expansion-trial '1tooltip51239537628;31hide7003;' "tooltip_use maps/scn19.zoo"

# A question opened over the map: the game stands still until it is answered, by a click on a
# button, by a key, and not by a click outside it.
check base-trial '0ask45000;0steppaused;1steppaused;2steppaused;3steppaused;4answer6;4step;5step;6step;7step;' "question_use maps/default.zoo '2 mouse 395 417\n3 down\n4 up' yes,no,cancel"
check base-trial '0ask45000;0steppaused;1steppaused;2steppaused;3answer7;3step;4step;5step;6step;7step;' "question_use maps/default.zoo '3 key escape' yes,no"
check base-trial '0ask45000;0steppaused;1steppaused;2steppaused;3answer1;3step;4step;5step;6step;7step;' "question_use maps/default.zoo '3 key enter' ok,cancel"
check base-trial '0ask45000;0steppaused;1steppaused;2steppaused;3steppaused;4steppaused;5answer6;5step;6step;7step;' "question_use maps/default.zoo '2 mouse 5 5\n3 down\n4 up\n5 key y' yes,no"

# Loop settings read from the ini file: update period, draw period, cap (ms), then the draws,
# interface updates and steps of a ten second run with a pass at every millisecond.
check base-trial 66,16,200,1,625,151,149 "rz timing | grep -o '[0-9]*' | paste -sd, -"
check expansion-trial 66,16,200,1,625,151,149 "rz timing | grep -o '[0-9]*' | paste -sd, -"

# Headless frames equal the same window drawn by render-map: four frames of a scripted run.
check base-trial 9same "same_frames maps/default.zoo"
check expansion-trial 9same "same_frames maps/scn19.zoo"
# A log replayed gives the same log, so the same frame hashes: its commands and the clock of each
# pass that did something are read back, at the size of the window. The clock here is irregular.
check base-trial same 'f=$(mktemp); g=$(mktemp); printf "1 scroll 1 0\n3 rotate cw\n5 pointer 0 0\n9 pause\n11 quit\n" >"$f"; rz run maps/default.zoo --headless --passes 12 --clock 23 --size 640x480 --script "$f" >"$g"; rz run maps/default.zoo --headless --replay "$g" | cmp -s - "$g" && echo same; rm -f "$f" "$g"'
# A run on the wall clock (no --seed) tells the seconds that seeded its generator, and its replay uses
# them: the animals draw, and the replay gives the same log.
check base-trial same 'g=$(mktemp); rz run maps/scn01a.zoo --headless --passes 600 --clock 67 >"$g"; grep -q "sim seed" "$g" && rz run maps/scn01a.zoo --headless --replay "$g" | cmp -s - "$g" && echo same; rm -f "$g"'

# The dummy video driver has no pointer: a scripted pointer inside the map, away from the edge
# bands, leaves the view where the map stores it for every pass. Prints the distinct views drawn.
windowed_view() {
  d=$(mktemp -d)
  printf '1 pointer 500 400\n' >"$d/script"
  SDL_VIDEO_DRIVER=dummy rz run "$1" --passes 8 --script "$d/script" --log "$d/log" >/dev/null 2>&1
  sed -n 's/.* draw [0-9a-f]\{16\} view=\([-0-9]*,[-0-9]*\) .*/\1/p' "$d/log" | sort -u | paste -sd' ' -
  rm -rf "${d:?}"
}

# The game size of the ini file (800x600 in both trials) at factor 1, on the dummy video driver.
check base-trial 800x600,1,800x600,800x600,800x600,frames,same "windowed maps/default.zoo"
check expansion-trial 800x600,1,800x600,800x600,800x600,frames,same "windowed maps/scn19.zoo"
# Its view stays at the one the map stores (the pointer of the dummy driver is not read).
check base-trial 1376,48 "windowed_view maps/default.zoo"
# Full screen on and off, and the zoom rule of an 800 pixel wide full screen.
check base-trial on,off,zoom=2,same "fullscreen maps/default.zoo"
check expansion-trial on,off,zoom=2,same "fullscreen maps/scn19.zoo"
# The timing mode (the summary of three timers) and the read-back of the presented pixels run on the
# dummy video driver; the values are information, so only the mode is checked: three summary lines
# and the size of the picture read back.
check base-trial 3,800x600 "SDL_VIDEO_DRIVER=dummy rz run maps/default.zoo --timing 300 --shot $tmp/s.png | awk '/^timing /{n++} /^shot /{s=\$2} END{print n\",\"s}'"
check expansion-trial 3,800x600 "SDL_VIDEO_DRIVER=dummy rz run maps/scn19.zoo --timing 300 --shot $tmp/s.png | awk '/^timing /{n++} /^shot /{s=\$2} END{print n\",\"s}'"
# No sound system is silence with a warning, not a failed run; no pass at all is refused.
check base-trial 0 'SDL_VIDEO_DRIVER=dummy SDL_AUDIO_DRIVER=none-such rz run maps/default.zoo --passes 3 >/dev/null 2>&1; echo $?'
check base-trial 1 'SDL_VIDEO_DRIVER=dummy rz run maps/default.zoo --passes 0 >/dev/null 2>&1; echo $?'

# WAV files decoded: files, ok, failed; the counts per format (sample type, rate, channels).
check base-trial 48,48,0 "rz check-wavs | tail -1 | grep -o '[0-9]*' | paste -sd, -"
check base-trial 38,2,1,1,6 "rz check-wavs | grep -v wavs: | cut -d' ' -f1 | paste -sd, -"
check expansion-trial 198,198,0 "rz check-wavs | tail -1 | grep -o '[0-9]*' | paste -sd, -"
check expansion-trial 17,7,162,12 "rz check-wavs | grep -v wavs: | cut -d' ' -f1 | paste -sd, -"
# The intro movie of each trial (Indeo 5 video, no sound): picture size, frames, frames decoded;
# every 40th frame written as a picture; a missing file is an error, not a crash.
check base-trial 800x600,79,79 "rz movie-info demologo.avi | grep -E '^(size|frames|decoded)' | cut -d' ' -f2 | paste -sd, -"
check expansion-trial 800x600,82,82 "rz movie-info bflogo.avi | grep -E '^(size|frames|decoded)' | cut -d' ' -f2 | paste -sd, -"
check base-trial 2,2 "{ rz movie-info demologo.avi --dump 40 $tmp/movie | tail -1 | grep -o '[0-9]*'; ls $tmp/movie | wc -l; } | paste -sd, -"
check base-trial 1 'rz movie-info nope.avi >/dev/null 2>&1; echo $?'

# The menu music queued on the dummy audio device, paused: format, gain, bytes.
check base-trial sounds/mainmenu.wav:S16LE22050Hz1ch,gain0.999,queued704514bytes,paused "rz check-music"
check expansion-trial sounds/mainmenu.wav:U822050Hz2ch,gain0.999,queued940914bytes,paused "rz check-music"

# The menu music fades as a game starts (a run starts its game at once): 5 updates wait, then 90
# steps of 0.33 dB and the music stops at the next one; headless, then in a window (800 passes of
# the real clock outlast the fade of 6.3 s), where the music is really stopped. After 30 passes a
# window still plays it and has queued more of it.
fade='rz run $map --headless --passes 100 --clock 66 | awk "/ music fade/{n++; g=\$NF} / music stopped/{s=\$2} END{print n\"steps,gain\"g\",stopped@\"s}"'
for trial in base-trial expansion-trial; do
  map=maps/default.zoo; [ $trial = expansion-trial ] && map=maps/scn19.zoo
  check $trial 90steps,gain0.104,stopped@95 "$fade"
done
check base-trial musicstopped "SDL_VIDEO_DRIVER=dummy rz run maps/default.zoo --passes 800 | tail -1 | sed 's/^music /music/'"
check base-trial musicgain0.999 "SDL_VIDEO_DRIVER=dummy rz run maps/default.zoo --passes 30 | tail -1 | cut -d' ' -f1-3 | sed 's/ //'"

# Text drawn without game data: a wrapped, centred, shadowed message (smoothed bold glyphs) and a
# smoothed bold one with a 2 pixel shadow have the same pixels, so the same hash, on every system.
check base-trial 286x34,59ed4503fc33a7f2 "rz draw-text $tmp/t.png --font 'Arial Bold' --size 11 --width 286 --justify center --shadow --fore 83 219 83 --back 68 36 20 'Sample message with words enough to fill one whole line and then some more' | tr ' ' ,"
check base-trial 300x21,1eb7b17046b92630 "rz draw-text $tmp/t.png --font 'Arial Bold' --size 14 --width 300 --shadow --fore 255 186 16 --back 20 20 20 'Sample item & more text here' | tr ' ' ,"
# A width of 0, a negative one, a huge one and none at all are refused (exit 1), a word is a usage
# error (exit 2).
for width in "--width 0" "--width -4" "--width 99999999999" ""; do
  check base-trial 1 "rz draw-text $tmp/t.png $width x >/dev/null 2>&1; echo \$?"
done
check base-trial 2 "rz draw-text $tmp/t.png --width wide x >/dev/null 2>&1; echo \$?"

# The pending object: the Objects panel opens with its first item chosen, the pointer moves over the
# map (the log tells the type, the tile, the quadrant and the verdict) and the picture shows the
# tinted object with its cost text; on the base trial a release outside the zoo wall then shows the
# wall message. Prints the pending lines and messages, then the hash of the last picture.
pending_object() { # pending_object <map> <script>
  d=$(mktemp -d)
  printf "$2" >"$d/script"
  rz run "$1" --headless --passes 12 --clock 20 --seed 1 --script "$d/script" 2>/dev/null |
    awk '/ tool pending /{p=p ";" $6 "," $7 "," $9 "," $10} / tool message /{p=p ";message" $6} / draw [0-9a-f]/{h=$5} END{print substr(p, 2) ";" h}'
  rm -rf "$d"
}
open='1 mouse 25 130\n2 down\n3 up\n5 mouse 600 400\n'
check base-trial "building/building/vendfood,16,45,1,valid;building/building/vendfood,17,47,1,valid;b96f4eb79b9f987f" "pending_object maps/default.zoo '${open}7 mouse 700 420\n'"
check base-trial "building/building/vendfood,16,45,1,valid;building/building/vendfood,10,57,3,invalid;message10311;061dde0b578c0cf3" "pending_object maps/default.zoo '${open}7 mouse 800 700\n8 down\n8 up\n'"
check expansion-trial "building/building/bounce,35,46,3,valid;building/building/bounce,37,48,3,valid;2f1604fe9cc2157b" "pending_object maps/scn19.zoo '${open}7 mouse 700 420\n'"

# A path drag: the path tab of the construction panel, a press, a move along one axis and one to the far
# end of an L of 8 tiles, the release and one more move. Prints the pieces placed (the `world add`
# and `world retype` lines), the cash at the end (from 50000) and the hash of the last picture, which shows the pieces
# joined. The mirrored drag (the first move along the other axis) lays the mirrored L.
path_drag() { # path_drag <map> <first move>
  d=$(mktemp -d)
  printf "1 mouse 25 35\n2 down\n3 up\n4 mouse 185 100\n5 down\n6 up\n8 mouse 600 400\n9 down\n10 mouse $2\n11 mouse 824 384\n12 up\n13 mouse 824 384\n" >"$d/script"
  rz run "$1" --headless --passes 16 --clock 20 --seed 1 --cash 50000 --script "$d/script" 2>/dev/null |
    awk '/ world (add|retype) /{n++} / money cash /{c=$6} / draw [0-9a-f]/{h=$5} END{print n "," c "," h}'
  rm -rf "$d"
}
check base-trial "8,49800,40076d8b2dbe27f3" "path_drag maps/default.zoo '632 384'"
check base-trial "8,49800,748d30093a68402c" "path_drag maps/default.zoo '632 416'"
check expansion-trial "8,49997,d4e0dc9b3d36f57f" "path_drag maps/scn19.zoo '632 384'"
check expansion-trial "8,49997,b7e5bf4b789a2659" "path_drag maps/scn19.zoo '632 416'"

# The paint tab: the building panel, the paint tab, the second terrain type, then the pointer
# over the map; the log tells the brush, the amount next to the pointer and the tooltips (one for
# a new tile, with the help of its terrain type). Prints those, then the hash of the last picture.
paint_text() { # paint_text <map> <script>
  d=$(mktemp -d)
  printf "$2" >"$d/script"
  rz run "$1" --headless --passes 20 --clock 20 --seed 1 --script "$d/script" 2>/dev/null |
    awk '/ terrain brush /{p=p ";brush" $6 "," $7} / terrain amount /{p=p ";amount" $6} / ui tooltip /{t++} / draw [0-9a-f]/{h=$5} END{print substr(p, 2) ";tooltips" t+0 ";" h}'
  rm -rf "$d"
}
paint='1 mouse 16 26\n2 down\n3 up\n5 mouse 180 195\n6 down\n7 up\n9 mouse 80 222\n10 down\n11 up\n13 mouse 975 420\n15 mouse 980 425\n'
check base-trial "brush2,21,51;amount-120;brush2,21,52;tooltips3;a5b3e85a3e274ffa" "paint_text maps/default.zoo '$paint'"
check expansion-trial "brush3,40,51;amount-270;tooltips3;451f077f76bc44c6" "paint_text maps/scn19.zoo '$paint'"

# A fence drag: the second item of the first row of the construction panel's fence tab, a press on
# the map (the side of the tile it is nearest to is kept for the whole run), a move along the first
# leg and one to the far end of an L, the release and one more move. Prints the pieces placed (the
# `world add` lines), the cash at the end (from 50000) and the hash of the last picture.
fence_drag() { # fence_drag <map> <last move>
  d=$(mktemp -d)
  printf "1 mouse 25 35\n2 down\n3 up\n4 mouse 130 75\n5 down\n6 up\n8 mouse 750 340\n9 down\n10 mouse 780 340\n11 mouse $2\n12 up\n13 mouse $2\n" >"$d/script"
  rz run "$1" --headless --passes 16 --clock 20 --seed 1 --cash 50000 --script "$d/script" 2>/dev/null |
    awk '/ world (add|retype) /{n++} / money cash /{c=$6} / draw [0-9a-f]/{h=$5} END{print n "," c "," h}'
  rm -rf "$d"
}
check base-trial "6,48920,ca28e88f552e7b7b" "fence_drag maps/default.zoo '878 332'"
check expansion-trial "6,49327,8e0e430b7a29bdfe" "fence_drag maps/scn19.zoo '878 332'"

# The drag of the original's reference pictures of placing a fence, here on the trial (its fence
# list and scroll differ from the full game's, which the reference pictures show): a press, three
# moves along the legs of an L, the release and one more move. Prints the pieces placed with their
# tiles and sides (the `world add` lines), the cash at the end and the hash of the last picture.
reference_drag() { # reference_drag <map>
  d=$(mktemp -d)
  printf "1 mouse 25 35\n2 down\n3 up\n4 mouse 130 75\n5 down\n6 up\n8 mouse 760 330\n9 down\n10 mouse 792 346\n11 mouse 824 362\n12 mouse 888 362\n13 mouse 920 346\n14 up\n15 mouse 920 346\n" >"$d/script"
  rz run "$1" --headless --passes 17 --clock 20 --cash 50000 --script "$d/script" 2>/dev/null |
    awk '/ world add /{printf "%s,%s,%s;", $7, $8, $9} / money cash /{c=$6} / draw [0-9a-f]/{h=$5} END{print c "," h}'
  rm -rf "$d"
}
check base-trial "21,45,side,0;22,45,side,0;23,45,side,0;23,45,side,2;23,46,side,2;23,47,side,2;23,48,side,2;48740,f363b863bd9f4cdc" "reference_drag maps/default.zoo"

# Closes a square of fences with four drags (the first item of the fence list, one side each), types
# a name into the dialog the new exhibit asks for and answers it with Enter. Prints the `exhibit`
# lines of the log (spaces as underscores, joined by semicolons): the new exhibit without a gate
# while the dialog is open, its name, then its gate.
exhibit_close() { # exhibit_close <map> [key]: the key is the answer to the question of a tank, Enter by default
  d=$(mktemp -d)
  printf "1 mouse 25 35\n2 down\n3 up\n4 mouse 130 75\n5 down\n6 up\n10 mouse 613 324\n11 down\n12 mouse 677 292\n13 up\n14 mouse 677 292\n15 mouse 695 292\n16 down\n17 mouse 759 324\n18 up\n19 mouse 759 324\n20 mouse 759 332\n21 down\n22 mouse 695 364\n23 up\n24 mouse 695 364\n25 mouse 677 364\n26 down\n27 mouse 613 332\n28 up\n29 mouse 613 332\n32 char L\n33 char i\n34 char o\n35 char n\n36 key ${2:-enter}\n" >"$d/script"
  rz run "$1" --headless --passes 41 --clock 20 --cash 50000 --script "$d/script" 2>/dev/null |
    grep ' exhibit \| tool tank ' | cut -d' ' -f5- | tr ' ' _ | paste -sd';' -
  rm -rf "$d"
}
check base-trial 'new_1_"Exhibit_1"_tiles_6_gate_none;name_1_"Lion";gate_1_18,43_side_2' "exhibit_close maps/default.zoo"
# The square of scn19 holds scenery and is made of tank walls: the closing piece makes a tank with its
# values of tanks.cfg and asks about its objects (the keys of the name go to that question, Enter is Yes),
# then the name dialog comes. With `n` the question is answered No: the last drag is undone.
check expansion-trial 'tank_ask;tank_2_height_5_level_0_show_0_salt_1_purity_100_filled_0;tank_answer_yes;tank_2_height_5_level_0_show_0_salt_1_purity_100_filled_1;new_2_"Tank_1"_tiles_9_gate_none' "exhibit_close maps/scn19.zoo"
check expansion-trial 'tank_ask;tank_2_height_5_level_0_show_0_salt_1_purity_100_filled_0;tank_answer_no;removed_2' "exhibit_close maps/scn19.zoo n"
# The water of a new tank (sm_aqua, nothing in the way): empty and filled while the name dialog is open,
# then one elevation step per simulation step up to the height less one (waterOffset -1), 0 to 4.
tank_water() { # tank_water <map>: the levels the log tells
  d=$(mktemp -d)
  printf "1 mouse 25 35\n2 down\n3 up\n4 mouse 130 75\n5 down\n6 up\n10 mouse 613 324\n11 down\n12 mouse 677 292\n13 up\n14 mouse 677 292\n15 mouse 695 292\n16 down\n17 mouse 759 324\n18 up\n19 mouse 759 324\n20 mouse 759 332\n21 down\n22 mouse 695 364\n23 up\n24 mouse 695 364\n25 mouse 677 364\n26 down\n27 mouse 613 332\n28 up\n29 mouse 613 332\n32 key enter\n" >"$d/script"
  rz run "$1" --headless --passes 80 --clock 20 --cash 50000 --script "$d/script" 2>/dev/null |
    awk '/ exhibit tank /{printf "%s%s", n++ ? "," : "", $10} END{print ""}'
  rm -rf "$d"
}
check expansion-trial '0,1,2,3,4' "tank_water maps/sm_aqua.zoo"

# The bulldozer: the path drag of above, Escape drops the path, the Delete key makes the bulldozer
# the tool, and a press with a drag over the rectangle that holds the L removes everything the
# rectangle covers (each piece refunded 80% of its cost). Prints the removals (the `world remove`
# lines), the cash at the end (from 50000) and the hash of the last picture.
bulldozer_area() { # bulldozer_area <map>
  d=$(mktemp -d)
  printf "1 mouse 25 35\n2 down\n3 up\n4 mouse 185 100\n5 down\n6 up\n8 mouse 600 400\n9 down\n10 mouse 632 384\n11 mouse 824 384\n12 up\n13 key escape\n14 key delete\n15 mouse 600 400\n16 down\n17 mouse 700 380\n18 mouse 824 384\n19 up\n20 mouse 640 380\n" >"$d/script"
  rz run "$1" --headless --passes 24 --clock 20 --seed 1 --cash 50000 --script "$d/script" 2>/dev/null |
    awk '/ world remove .* refund /{n++} / money cash /{c=$6} / draw [0-9a-f]/{h=$5} END{print n "," c "," h}'
  rm -rf "$d"
}
check base-trial "8,49960,b656d74441897b68" "bulldozer_area maps/default.zoo"
check expansion-trial "16,50233,8a8b595c73a8ccd5" "bulldozer_area maps/scn19.zoo"

# A building: the Objects panel opens with its first item chosen, a click on the map places it (the
# log tells the add with the quadrants it holds and the cash) and a second click on the same place
# finds them held (message 10555). Prints the adds, the messages, the cash at the end (from 50000)
# and the hash of the last picture.
building_place() { # building_place <map>
  d=$(mktemp -d)
  printf '1 mouse 25 130\n2 down\n3 up\n5 mouse 600 400\n6 down\n7 up\n9 down\n10 up\n11 mouse 700 420\n' >"$d/script"
  rz run "$1" --headless --passes 14 --clock 20 --seed 1 --cash 50000 --script "$d/script" 2>/dev/null |
    awk '/ world add /{p=p ";" $6 "," $7 "," $11} / tool message /{p=p ";message" $6} / money cash /{c=$6} / draw [0-9a-f]/{h=$5} END{print substr(p, 2) ";" c ";" h}'
  rm -rf "$d"
}
check base-trial "building/building/vendfood,16,45,2;message10555;49950;54c1c4d355b2e07d" "building_place maps/default.zoo"
check expansion-trial "building/building/bounce,35,46,4;message10555;49977;a6fd2ba163b33aaa" "building_place maps/scn19.zoo"
# A walkable type (bounce) is placed over guests: units on its tiles never refuse it (placing 5.2).
walkable_place() { # walkable_place <map>
  d=$(mktemp -d)
  printf '1 mouse 25 130\n2 down\n3 up\n5 mouse 600 400\n7 mouse 400 380\n8 down\n9 up\n' >"$d/script"
  rz run "$1" --headless --passes 12 --clock 20 --script "$d/script" 2>/dev/null | awk '/ world add /{print $6 "," $7}'
  rm -rf "$d"
}
check expansion-trial "building/building/bounce,33,42" "walkable_place maps/scn19.zoo"

# The gate tool: the square of above, then the gate button and a click on a piece, picked by the
# pixels of its picture, moves the gate there. Prints the `world ungate`, `world gate` and `exhibit gate` lines
# after the name (spaces as underscores, joined by semicolons): the old gate turns back into a
# fence, the piece becomes the gate and the exhibit follows (paused, so that no guest arriving
# meanwhile takes an entity number).
gate_move() { # gate_move <map> [yes]: with `yes` the square is a tank and its question is answered first
  d=$(mktemp -d)
  s=0
  if [ -n "${2:-}" ]; then s=1; fi
  printf "0 pause\n1 mouse 25 35\n2 down\n3 up\n4 mouse 130 75\n5 down\n6 up\n10 mouse 613 324\n11 down\n12 mouse 677 292\n13 up\n14 mouse 677 292\n15 mouse 695 292\n16 down\n17 mouse 759 324\n18 up\n19 mouse 759 324\n20 mouse 759 332\n21 down\n22 mouse 695 364\n23 up\n24 mouse 695 364\n25 mouse 677 364\n26 down\n27 mouse 613 332\n28 up\n29 mouse 613 332\n" >"$d/script"
  if [ $s = 1 ]; then printf "32 key enter\n" >>"$d/script"; fi
  printf "$((32 + s)) char L\n$((33 + s)) char i\n$((34 + s)) char o\n$((35 + s)) char n\n$((36 + s)) key enter\n$((38 + s)) mouse 110 282\n$((39 + s)) down\n$((40 + s)) up\n$((42 + s)) mouse 727 308\n$((43 + s)) down\n$((44 + s)) up\n" >>"$d/script"
  rz run "$1" --headless --passes $((48 + s)) --clock 20 --cash 50000 --script "$d/script" 2>/dev/null |
    grep ' world ungate \| world gate \| exhibit gate ' | cut -d' ' -f4- | tr ' ' _ | paste -sd';' -
  rm -rf "$d"
}
check base-trial "exhibit_gate_1_18,43_side_2;world_ungate_fences/zoobars/f_18,43_side_2_id_15652;world_gate_fences/zoobars/g_20,45_side_0_id_15649;exhibit_gate_1_20,45_side_0" "gate_move maps/default.zoo"
check expansion-trial "exhibit_gate_2_38,44_side_6;world_ungate_tankwall/tankwal2/f_38,44_side_6_id_53944;world_gate_tankwall/tankwal2/g_40,45_side_2_id_53937;exhibit_gate_2_40,45_side_2" "gate_move maps/scn19.zoo yes"

# Undo (placing 8): the path drag of above, then Backspace takes the pieces back. Prints the removals
# the undo logged, the cash at the end (from 50000) and whether the last picture is the one before
# the drag (the game is paused, so that the units stand still). A bulldozer area drag over some of the pieces then Backspace puts them back: the adds the
# undo logged and the cash (the area drag is paused too, so that no guest pays the admission meanwhile).
undo_path() { # undo_path <map>
  d=$(mktemp -d)
  printf "0 pause\n1 mouse 25 35\n2 down\n3 up\n4 mouse 185 100\n5 down\n6 up\n8 mouse 600 400\n9 down\n10 mouse 632 384\n11 mouse 824 384\n12 up\n13 mouse 185 100\n20 key backspace\n" >"$d/script"
  rz run "$1" --headless --passes 24 --clock 20 --cash 50000 --script "$d/script" 2>/dev/null |
    awk '/ world remove .* undo$/{n++} / money cash /{c=$6} / draw [0-9a-f]/{if ($2 == 6) a=$5; b=$5} END{print n "," c "," (a == b ? "same" : "different")}'
  rm -rf "$d"
}
check base-trial "8,50000,same" "undo_path maps/default.zoo"
check expansion-trial "7,50000,same" "undo_path maps/scn19.zoo"
undo_area() { # undo_area <map>
  d=$(mktemp -d)
  printf "0 pause\n1 mouse 25 35\n2 down\n3 up\n4 mouse 185 100\n5 down\n6 up\n8 mouse 600 400\n9 down\n10 mouse 632 384\n11 mouse 824 384\n12 up\n13 key delete\n14 mouse 580 380\n15 down\n16 mouse 700 360\n17 mouse 800 400\n18 up\n19 key backspace\n" >"$d/script"
  rz run "$1" --headless --passes 24 --clock 20 --cash 50000 --script "$d/script" 2>/dev/null |
    awk '/ world add .* undo$/{n++} / money cash /{c=$6} END{print n "," c}'
  rm -rf "$d"
}
check base-trial "3,49800" "undo_area maps/default.zoo"
check expansion-trial "14,49920" "undo_area maps/scn19.zoo"

# The exhibit panel (information panels 4 and 10): the square of above, named Lion, then Escape drops
# the fence tool and a click on its gate opens the panel on it (the button down, the exhibit current).
# Prints the `panel` lines (spaces as underscores, joined by semicolons): the exhibit, its date
# of construction, upkeep and keeper, the popularity and donations and the number of animals.
panel_gate() { # panel_gate <map>
  d=$(mktemp -d)
  printf "1 mouse 25 35\n2 down\n3 up\n4 mouse 130 75\n5 down\n6 up\n10 mouse 613 324\n11 down\n12 mouse 677 292\n13 up\n14 mouse 677 292\n15 mouse 695 292\n16 down\n17 mouse 759 324\n18 up\n19 mouse 759 324\n20 mouse 759 332\n21 down\n22 mouse 695 364\n23 up\n24 mouse 695 364\n25 mouse 677 364\n26 down\n27 mouse 613 332\n28 up\n29 mouse 613 332\n32 char L\n33 char i\n34 char o\n35 char n\n36 key enter\n38 key escape\n40 mouse 608 296\n41 down\n42 up\n" >"$d/script"
  rz run "$1" --headless --passes 46 --clock 20 --cash 50000 --script "$d/script" 2>/dev/null |
    sed -n 's/^pass [0-9]* t=[0-9]* panel //p' | tr ' ' _ | paste -sd';' -
  rm -rf "$d"
}
check base-trial 'exhibit_1_"Lion";status_"Jan_1,_Year_1"_upkeep_$0.00_$0.00_$0.00_staff_No;donations_stars_0_$0.00_$0.00_$0.00;animals_0_thoughts_0' "panel_gate maps/default.zoo"
# The exhibit list button, the first row, the Animals tab and its first row open the animal panel,
# and its recommendations button the plaque of that animal: prints the animals of the exhibit, the
# lines in red (important) and the others, as the rating of `check-rating` has them (scn01's giraffe: bits 24 and 27 are important).
panel_plaque() { # panel_plaque <map>
  d=$(mktemp -d)
  printf '0 pause\n1 mouse 965 750\n2 down\n3 up\n4 mouse 900 455\n5 down\n6 up\n8 mouse 1004 668\n9 down\n10 up\n11 mouse 880 610\n12 down\n13 up\n15 mouse 992 256\n16 down\n17 up\n' >"$d/script"
  rz run "$1" --headless --passes 22 --clock 67 --script "$d/script" 2>/dev/null |
    awk '/ panel animals /{a=$6} / panel line! /{i++} / panel line "/{o++} END{print a "," i+0 "," o+0}'
  rm -rf "$d"
}
check base-trial "1,2,3" "panel_plaque maps/scn01.zoo"
check expansion-trial "2,2,2" "panel_plaque maps/scn19.zoo"
check expansion-trial "1,2,2" "panel_plaque maps/scn27.zoo"
check expansion-trial "1,0,2" "panel_plaque maps/scn28b.zoo"

# The animal panel of the first animal of the first exhibit (the Animals tab's first row): its
# bars, with the suitability the `check-rating` line of its species shows (89 for scn01's giraffe),
# and its time rows.
panel_animal() { # panel_animal <map>
  d=$(mktemp -d)
  printf '0 pause\n1 mouse 965 750\n2 down\n3 up\n4 mouse 900 455\n5 down\n6 up\n8 mouse 1004 668\n9 down\n10 up\n11 mouse 880 610\n12 down\n13 up\n' >"$d/script"
  rz run "$1" --headless --passes 18 --clock 67 --script "$d/script" 2>/dev/null |
    awk '/ panel animal [0-9]/{sub(/^.* panel animal /, ""); gsub(/ /, "_"); print}'
  rm -rf "$d"
}
check base-trial '35179_"Giraffe_2"_happiness_69_hunger_93_health_100_suitability_89_fed_Recently_slept_Recently' "panel_animal maps/scn01.zoo"
check expansion-trial '51757_"Polar_Bear_1"_happiness_100_hunger_60_health_100_suitability_75_fed_Recently_slept_Recently' "panel_animal maps/scn19.zoo"

# The exhibit list button and a row click show the status tab's layout with no tab click: the
# frame is the one that a click on the Donations tab and then on the Status tab leaves (the mouse
# is moved off the tabs, so no tooltip shows). The game is paused, so that the units walking in
# the map part of the frame do not differ between the two runs, and a frame is drawn only when
# something changed: the last one drawn of each run is compared. Prints same or different.
panel_tab() { # panel_tab <map>
  d=$(mktemp -d)
  printf '0 pause\n1 mouse 965 750\n2 down\n3 up\n4 mouse 900 455\n5 down\n6 up\n12 mouse 500 300\n' >"$d/a"
  printf '0 pause\n1 mouse 965 750\n2 down\n3 up\n4 mouse 900 455\n5 down\n6 up\n8 mouse 1005 645\n9 down\n10 up\n12 mouse 1005 614\n13 down\n14 up\n16 mouse 500 300\n' >"$d/b"
  for s in a b; do
    rz run "$1" --headless --passes 24 --clock 67 --script "$d/$s" 2>/dev/null | awk '/^pass [0-9]+ .* draw [0-9a-f]/{h=$5} END{print h}'
  done | sort -u | awk 'END{ if (NR == 1) print "same"; else print "different" }'
  rm -rf "$d"
}
check base-trial "same" "panel_tab maps/scn01.zoo"
check expansion-trial "same" "panel_tab maps/scn19.zoo"

# The staff panel (information panels 9, staff 6): the exhibit list centres the view on the first
# exhibit and closes, a hired zookeeper (seed 3 draws the sex the trials have) is clicked, the assign
# button is pressed and a click on the exhibit assigns it (green under the pointer, none over the
# grounds); the exhibit panel still says no keeper is assigned (the flag follows at the exhibit's
# next rating, which a paused game does not reach). Prints the `panel` and `tool assign` lines
# (spaces as underscores, ids left out, joined by semicolons).
staff_assign() { # staff_assign <map>
  d=$(mktemp -d)
  printf '1 pause\n2 mouse 965 750\n3 down\n4 up\n5 mouse 900 455\n6 down\n7 up\n8 mouse 1008 411\n9 down\n10 up\n11 mouse 28 176\n12 down\n13 up\n14 mouse 300 480\n15 down\n16 up\n17 right\n18 mouse 303 466\n19 down\n20 up\n21 mouse 993 583\n22 down\n23 up\n24 mouse 500 250\n25 mouse 505 255\n26 down\n27 up\n28 mouse 400 600\n29 down\n30 up\n32 mouse 965 750\n33 down\n34 up\n35 mouse 900 455\n36 down\n37 up\n' >"$d/script"
  rz run "$1" --headless --seed 3 --passes 40 --clock 67 --script "$d/script" 2>/dev/null |
    grep ' panel staff \| tool assign \| panel status ' | cut -d' ' -f4- |
    sed -e 's/panel staff [0-9]* /panel staff /' -e 's/tool assign [0-9]* /tool assign /' | tr ' ' _ | paste -sd';' -
  rm -rf "$d"
}
check base-trial 'panel_status_"Jan_1,_Year_1"_upkeep_$0.00_$0.00_$0.00_staff_No;panel_staff_"Zookeeper_1"_salary_$0_duty_""_exhibits_[];tool_assign_over_1_good;tool_assign_1;panel_staff_"Zookeeper_1"_salary_$0_duty_""_exhibits_["Exhibit_1"];tool_assign_over_none;panel_status_"Jan_1,_Year_1"_upkeep_$0.00_$0.00_$0.00_staff_No' "staff_assign maps/scn01.zoo"

# A maintenance worker is hired, clicked and its duties tab opened (the expansion data has the four
# duty kinds, the base data three): a click on the second row switches that kind off.
staff_duties() { # staff_duties <map>
  d=$(mktemp -d)
  printf '1 pause\n2 mouse 28 176\n3 down\n4 up\n5 mouse 80 172\n6 down\n7 up\n8 mouse 600 300\n9 down\n10 up\n11 right\n12 mouse 607 288\n13 down\n14 up\n16 mouse 1005 664\n17 down\n18 up\n24 mouse 880 650\n25 down\n26 up\n' >"$d/script"
  rz run "$1" --headless --seed 3 --passes 30 --clock 67 --script "$d/script" 2>/dev/null |
    sed -n 's/^.* panel staff [0-9]* .* duties /duties_/p' | paste -sd';' -
  rm -rf "$d"
}
check expansion-trial "duties_1111;duties_1011" "staff_duties maps/scn19.zoo"

# The guest panel (information panels 8): the game paused, a click on a guest of the map opens it,
# the second tab is chosen, a click on the ground closes it. Prints the `panel guest` lines (spaces
# as underscores, the id left out, joined by semicolons): the name, the five bars, the time in the
# park (a guest of a stored map has no arrival date), the favourite and the thoughts.
guest_panel() { # guest_panel <map>
  d=$(mktemp -d)
  printf '1 pause\n2 mouse 652 598\n3 down\n4 up\n6 mouse 1005 667\n7 down\n8 up\n10 mouse 400 100\n11 down\n12 up\n' >"$d/script"
  rz run "$1" --headless --seed 3 --passes 14 --clock 67 --script "$d/script" 2>/dev/null |
    grep ' panel guest ' | cut -d' ' -f4- | sed 's/panel guest [0-9]* /panel guest /' | tr ' ' _ | paste -sd';' -
  rm -rf "${d:?}"
}
check expansion-trial 'panel_guest_"Guest_58"_happiness_70_drink_48_food_48_restroom_58_energy_64_time_"Just_arrived"_favourite_"Manta_Ray"_thoughts_[];panel_guest_none' "guest_panel maps/scn19.zoo"

# The simulation run twice in a row prints the same state hash (the CI job `simhash` compares the
# hashes of the three systems).
sim_twice() { # sim_twice <map>
  a=$(rz sim "$1" --ticks 1000 --hash)
  b=$(rz sim "$1" --ticks 1000 --hash)
  if [ -n "$a" ] && [ "$a" = "$b" ]; then echo same; else echo different; fi
}
check base-trial same "sim_twice maps/default.zoo"
check expansion-trial same "sim_twice maps/sm_aqua.zoo"

exit $failed
