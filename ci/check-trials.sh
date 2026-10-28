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
export RUSTYZOO_ENGLISH_DATES=1 # the same dates (and frames) whatever the system's locale

# --reset-fences is for a map whose scenario file has `resetfences` above 0 (scn19 of the expansion trial):
# its start makes every fence piece healthy, which draws from the shared random stream. The other maps
# keep the stored pieces, as `sim` does without the flag.
# The base trial's update screen (ui/update.lyt) names ui/sharedui/updatbck.tga, a file that neither
# trial ships (the trials keep only the folder of its animation): the picture is left out, as the
# game does, and that one warning is dropped here. Every other warning is printed.
missing_update_picture='rustyzoo: warning: picture ui/sharedui/updatbck.tga: no such member: ui/sharedui/updatbck.tga'
rz() { "$bin" --data "$dir/$trial" "$@" 2> >(grep -v -x -F "$missing_update_picture" >&2); }

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
check base-trial '502@112,84,131,disabled,10.10.00.0004,873b7d1f9db25ad7,exit@3,131' 'startup 620 | paste -sd, -'
check expansion-trial '505@112,84,253,disabled,12.05.00.0004,1548dd4c32e1e7a4,exit@3,253' 'startup 600 | paste -sd, -'
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
# what is left, and the pile goes. Seed 2 is the first (from 1) where it reaches the pile in the run: with seed 1
# it is still on its way at step 600 and then eats from another pile.
check base-trial 'keeperfood;30,10,0' 'printf "0 food 45,38 carnchow 50\n" >"$tmp/food"; rz sim maps/scn01a.zoo --ticks 600 --seed 2 --script "$tmp/food" --log 2>/dev/null | awk "/ unit goal 27645 keeperfood /{g=\"keeperfood\"} / animal eat 27645 /{l=l (l?\",\":\"\") \$(NF-1)} END{print g \";\" l}"'
# An animal placed (habitat rating 9.4, animals 10): ci/reactions.txt adopts a gazelle and a lion into a
# pen. The lion's cat family is at -20 in the gazelle's compatible animals, so the gazelle (15672)
# dislikes the newcomer at pass 131, and the frown plays once.
check base-trial 'dislikes15672@131;frown' 'rz run maps/default.zoo --headless --cash 75000 --seed 5 --passes 140 --clock 200 --script ci/reactions.txt 2>/dev/null | awk "/ animal dislikes /{r=\$6 \"@\" \$2} / animal sound /{s=\$NF} END{print \"dislikes\" r \";\" s}"'
# The same pen: a second lion is adopted and the first one sold, which the gazelle likes (the compatible animals
# values negated for a removal); then the lion is picked up and put down elsewhere, which counts as a placement.
check base-trial 'likes15672@146;smile' 'd=$(mktemp -d); head -n "$(grep -n "^131 up" ci/reactions.txt | cut -d: -f1)" ci/reactions.txt >"$d/s"; printf "134 mouse 448 304\n135 down\n136 up\n138 key escape\n140 key delete\n142 mouse 512 268\n143 down\n144 up\n146 key y\n" >>"$d/s"; rz run maps/default.zoo --headless --cash 75000 --seed 5 --passes 148 --clock 200 --script "$d/s" 2>/dev/null | awk "\$2>143 && / animal (likes|dislikes) /{r=r \$5 \$6 \"@\" \$2} \$2>143 && / animal sound /{s=s \$NF} END{print r \";\" s}"; rm -r "$d"'
check base-trial 'dislikes15672@150;frown' 'd=$(mktemp -d); head -n "$(grep -n "^133 key escape" ci/reactions.txt | cut -d: -f1)" ci/reactions.txt >"$d/s"; printf "136 mouse 512 268\n137 down\n138 up\n141 mouse 512 268\n142 down\n143 up\n146 mouse 448 240\n149 down\n150 up\n" >>"$d/s"; rz run maps/default.zoo --headless --cash 75000 --seed 5 --passes 152 --clock 200 --script "$d/s" 2>/dev/null | awk "\$2>143 && / animal (likes|dislikes) /{r=r \$5 \$6 \"@\" \$2} \$2>143 && / animal sound /{s=s \$NF} END{print r \";\" s}"; rm -r "$d"'
# Each fence piece of a drag runs the habitat pair (habitat rating 9.4), on the zoo grounds too: the
# pair marks the grounds for revision, so their next update draws and the state hash of the first month
# change moves (without the pair it would be 2cf67ce9093b3d71). A drag the cash cannot pay for (100 for
# pieces of 150) is refused and runs no pair: the hash is that of the same game with no drag at all.
check base-trial d4bed89505f9479f 'printf "3 mouse 25 34\n4 down\n5 up\n7 mouse 183 74\n8 down\n9 up\n11 mouse 310 267\n12 down\n13 mouse 470 187\n14 up\n16 key escape\n" >"$tmp/fence"; rz run maps/default.zoo --headless --cash 75000 --seed 5 --passes 1472 --clock 200 --script "$tmp/fence" 2>/dev/null | awk "/ sim hash /{print \$NF; exit}"'
check base-trial c3806921661f5f9c 'rz run maps/default.zoo --headless --cash 100 --seed 5 --passes 1472 --clock 200 --script "$tmp/fence" 2>/dev/null | awk "/ sim hash /{print \$NF; exit}"'
# A play call queues behind the playing sequence, the base loop included: the lion shows its first
# animation other than the idle one at pass 14 (the lion 27645, happiness 75, chooses bFriendly; the lion
# 27646 is tired, so its ambient choice runs the sleep check instead of a row, behaviours 6.4: it starts
# bSleep at pass 25 and shows its first other animation at pass 28); a sound named without .wav is a
# file of the type's folder.
check base-trial 14,28 'rz run maps/scn01a.zoo --headless --bare --passes 40 --clock 67 --seed 1 2>/dev/null | awk "/ unit anim 27646 / && \$NF != \"idle\" {print \$2}" | head -2 | paste -sd, -'
check base-trial bFriendly,14 'rz run maps/scn01a.zoo --headless --bare --passes 40 --clock 67 --seed 1 2>/dev/null | awk "/ unit set 27645 / && s == \"\" {s=\$NF} / unit anim 27645 / && \$NF != \"idle\" && a == \"\" {a=\$2} END {print s \",\" a}"'
# The lion's first sound comes after pass 1500 (a failed shelter walk resets the whole goal, which shifted
# the draws of the run), so this run is 3000 passes long.
check base-trial animals/lion/cat_sgr.wav 'rz run maps/scn01a.zoo --headless --bare --passes 3000 --clock 67 --seed 3 2>/dev/null | grep " unit sound " | awk "\$NF ~ /^animals/ {print \$NF}" | sort -u'

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

# Play on the freeform screen (its button at y 405, Play at 831,657) starts the map of the list with the
# cash of the screen: the map and the cash the game starts with. With --seed, the game started there
# is a test run too: the seconds of the seedings, at the start of the program and of the map.
freeform_play() { # freeform_play [flags]: the log
  rm -rf "$tmp/user"
  mkdir -p "$tmp/user"
  printf '[UI]\nplayMovie=0\n' >"$tmp/user/zoo.ini"
  printf '1 mouse 512 405\n2 down\n3 up\n4 mouse 831 657\n5 down\n6 up\n' >"$tmp/play"
  rz run --headless --passes 8 --clock 100 --script "$tmp/play" "$@" 2>/dev/null
}
check base-trial 'freeformcash75000maps/default.zoo' "freeform_play | sed -n 's/^pass [0-9]* t=[0-9]* menu game start //p'"
check expansion-trial 'freeformcash75000maps/sm_aqua.zoo' "freeform_play | sed -n 's/^pass [0-9]* t=[0-9]* menu game start //p'"
check base-trial '5,6' "freeform_play --seed 5 | sed -n 's/^pass [0-9]* t=[0-9]* sim seed //p' | paste -sd, -"

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
check expansion-trial "building/building/bounce,35,46,3,valid;building/building/bounce,37,48,3,valid;7c2f217b5d63a073" "pending_object maps/scn19.zoo '${open}7 mouse 700 420\n'"

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
check expansion-trial "8,49997,8aa6d1214c34d01f" "path_drag maps/scn19.zoo '632 384'"
check expansion-trial "8,49997,007c329247e16031" "path_drag maps/scn19.zoo '632 416'"

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
check expansion-trial "brush3,40,51;amount-270;tooltips3;1984f837ede6f594" "paint_text maps/scn19.zoo '$paint'"

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
check expansion-trial "6,49327,9affdcaafa6f83f6" "fence_drag maps/scn19.zoo '878 332'"

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
exhibit_close() { # exhibit_close <map> [key] [grep pattern]: the key is the answer to the question of a tank, Enter by default
  d=$(mktemp -d)
  printf "1 mouse 25 35\n2 down\n3 up\n4 mouse 130 75\n5 down\n6 up\n10 mouse 613 324\n11 down\n12 mouse 677 292\n13 up\n14 mouse 677 292\n15 mouse 695 292\n16 down\n17 mouse 759 324\n18 up\n19 mouse 759 324\n20 mouse 759 332\n21 down\n22 mouse 695 364\n23 up\n24 mouse 695 364\n25 mouse 677 364\n26 down\n27 mouse 613 332\n28 up\n29 mouse 613 332\n32 char L\n33 char i\n34 char o\n35 char n\n36 key ${2:-enter}\n" >"$d/script"
  rz run "$1" --headless --passes 41 --clock 20 --cash 50000 --script "$d/script" 2>/dev/null |
    grep "${3:-t=[0-9]* exhibit \| tool tank }" | cut -d' ' -f5- | tr ' ' _ | paste -sd';' -
  rm -rf "$d"
}
check base-trial 'new_1_"Exhibit_1"_tiles_6_gate_none;name_1_"Lion";gate_1_18,43_side_2' "exhibit_close maps/default.zoo"
# The exhibit sound plays once when the exhibit is complete (sound 7), before its name is answered.
check base-trial 'exhibit_1' "exhibit_close maps/default.zoo enter ' sound exhibit '"
check expansion-trial 'exhibit_2' "exhibit_close maps/scn19.zoo enter ' sound exhibit '"
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
check expansion-trial "16,50233,16f941c0362857c2" "bulldozer_area maps/scn19.zoo"

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
check expansion-trial "building/building/bounce,35,46,4;message10555;49977;938c4b7d2c172942" "building_place maps/scn19.zoo"
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
check base-trial 'exhibit_1_"Lion";status_"Jan_7,_Year_1"_upkeep_$0.00_$0.00_$0.00_staff_No;donations_stars_0_$0.00_$0.00_$0.00;animals_0_thoughts_0' "panel_gate maps/default.zoo"
# The exhibit list button, the first row, the Animals tab and its first row open the animal panel,
# and its recommendations button the plaque of that animal: prints the animals of the exhibit, the
# lines in red (important) and the others, read from the animal's stored bits: the game runs to just past the first
# rating of the exhibits (pass 102, which settles the bits) and is then paused, so the animal checks' bits are in too
# (scn01's giraffe: bits 24 and 27 are important, the "few animals" bit of its social check is an other line).
panel_plaque() { # panel_plaque <map>
  d=$(mktemp -d)
  printf '110 pause\n111 mouse 965 750\n112 down\n113 up\n114 mouse 900 455\n115 down\n116 up\n118 mouse 1004 668\n119 down\n120 up\n121 mouse 880 610\n122 down\n123 up\n125 mouse 992 256\n126 down\n127 up\n' >"$d/script"
  rz run "$1" --headless --passes 132 --clock 67 --seed 1 --script "$d/script" 2>/dev/null |
    awk '/ panel animals /{a=$6} / panel line! /{i++} / panel line "/{o++} END{print a "," i+0 "," o+0}'
  rm -rf "$d"
}
check base-trial "1,2,4" "panel_plaque maps/scn01.zoo"
check expansion-trial "2,2,2" "panel_plaque maps/scn19.zoo"
check expansion-trial "1,3,2" "panel_plaque maps/scn27.zoo"
check expansion-trial "1,0,1" "panel_plaque maps/scn28b.zoo"

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
# park (the arrival date the map stores is before the date it starts on: a short time),
# the favourite and the thoughts.
guest_panel() { # guest_panel <map>
  d=$(mktemp -d)
  printf '1 pause\n2 mouse 652 598\n3 down\n4 up\n6 mouse 1005 667\n7 down\n8 up\n10 mouse 400 100\n11 down\n12 up\n' >"$d/script"
  rz run "$1" --headless --seed 3 --passes 14 --clock 67 --script "$d/script" 2>/dev/null |
    grep ' panel guest ' | cut -d' ' -f4- | sed 's/panel guest [0-9]* /panel guest /' |
    while IFS= read -r l; do # the thoughts are printed as their count and a checksum, not as text
      case $l in
        *' thoughts ['*)
          t=${l#*' thoughts '}
          l=${l%%' thoughts ['*}
          n=$(printf '%s' "$t" | grep -o '", "' | wc -l | tr -d ' ')
          [ "$t" = '[]' ] || n=$((n + 1))
          printf '%s thoughts %s %08x\n' "$l" "$n" "$(printf '%s' "$t" | cksum | cut -d' ' -f1)" ;;
        *) printf '%s\n' "$l" ;;
      esac
    done | tr ' ' _ | paste -sd';' -
  rm -rf "${d:?}"
}
check expansion-trial 'panel_guest_"Guest_58"_happiness_70_drink_48_food_48_restroom_58_energy_64_time_"A_short_time"_favourite_"Polar_Bear"_thoughts_5_b193c13a;panel_guest_none' "guest_panel maps/scn19.zoo"

# The building panel (information panels 4.1, guest buildings 7.2, 10.3): the game paused, the Objects
# panel's second item (a burger stand, 3.50 within 1.00 and 6.50) is placed and clicked, then the up
# button is pressed 14 times and the down button 30 times. Prints the number of changes the price
# logged and the last price of each run (the bounds hold: 12 steps up to 6.50, 22 down to 1.00) and
# the last `panel building` line (spaces as underscores, the id left out).
building_panel() { # building_panel <map>
  d=$(mktemp -d)
  printf '2 pause\n75 mouse 25 129\n76 down\n77 up\n79 mouse 140 120\n80 down\n81 up\n82 mouse 864 256\n84 down\n85 up\n86 key escape\n90 mouse 864 256\n91 down\n92 up\n94 mouse 956 572\n' >"$d/script"
  for i in $(seq 0 13); do printf '%d down\n%d up\n' $((95 + 2 * i)) $((96 + 2 * i)) >>"$d/script"; done
  printf '125 mouse 956 586\n' >>"$d/script"
  for i in $(seq 0 29); do printf '%d down\n%d up\n' $((126 + 2 * i)) $((127 + 2 * i)) >>"$d/script"; done
  rz run "$1" --headless --cash 75000 --seed 5 --passes 190 --clock 200 --script "$d/script" 2>/dev/null |
    awk '/ building price /{ if ($2 < 125) { u++; up = $7 } else { d++; down = $7 } }
         / panel building /{ l = $0; sub(/^.* panel building [0-9]+ /, "", l) }
         END{ printf "%s:%s;%s:%s;%s\n", u, up, d, down, l }' | tr ' ' _
  rm -rf "${d:?}"
}
check base-trial '12:6.50;22:1.00;"Burger_Stand_1"_price_$1.00_income_$0_upkeep_$0_profit_$0_month_$0_$0_$0_months_1_guests_last_0_this_0_total_0_sells_["burger"]' "building_panel maps/default.zoo"

# The panel of a building that charges nothing: the third item (a family restroom) is placed and clicked.
building_plain() { # building_plain <map>
  d=$(mktemp -d)
  printf '2 pause\n75 mouse 25 129\n76 down\n77 up\n79 mouse 140 160\n80 down\n81 up\n82 mouse 864 256\n84 down\n85 up\n86 key escape\n90 mouse 864 256\n91 down\n92 up\n' >"$d/script"
  rz run "$1" --headless --cash 75000 --seed 5 --passes 100 --clock 200 --script "$d/script" 2>/dev/null |
    sed -n 's/^.* panel building [0-9]* //p' | tr ' ' _
  rm -rf "${d:?}"
}
check base-trial '"Family_Restroom_1"_guests_last_0_this_0_total_0_months_1' "building_plain maps/default.zoo"

# A building is renamed in its panel (guest buildings 10.3.1): the item at `<x y>` is placed and clicked, a click on
# the name field gives it the focus, "Zoo" is typed and Enter ends the editing; `<more>` (script lines) edits the
# field again. Prints the names the panel logged, the last one the name it shows back after the last editing.
building_rename() { # building_rename <map> <x y of the item> [<more script lines>]
  d=$(mktemp -d)
  printf '2 pause\n75 mouse 25 129\n76 down\n77 up\n79 mouse %s\n80 down\n81 up\n82 mouse 864 256\n84 down\n85 up\n86 key escape\n90 mouse 864 256\n91 down\n92 up\n94 mouse 940 555\n95 down\n96 up\n97 char Z\n98 char o\n99 char o\n100 key enter\n%b' "$2" "${3-}" >"$d/script"
  rz run "$1" --headless --cash 75000 --seed 5 --passes 120 --clock 200 --script "$d/script" 2>/dev/null |
    sed -n 's/^.* panel building [0-9]* \("[^"]*"\).*/\1/p' | tr ' ' _ | paste -sd';' -
  rm -rf "${d:?}"
}
check base-trial '"Burger_Stand_1";"Zoo"' "building_rename maps/default.zoo '140 120'"
check base-trial '"Family_Restroom_1";"Zoo"' "building_rename maps/default.zoo '140 160'"
# A text of spaces only, and an empty one, leave the name (the test of the unit panels); other text replaces it.
again='101 mouse 940 555\n102 down\n103 up\n104 key backspace\n105 key backspace\n106 key backspace\n107 key backspace\n'
spaces='108 char 32\n109 key enter\n'
empty='108 key enter\n'
zed='108 char Z\n109 char e\n110 char d\n111 key enter\n'
check base-trial '"Burger_Stand_1";"Zoo";"Zoo"' "building_rename maps/default.zoo '140 120' '$again$spaces'"
check base-trial '"Burger_Stand_1";"Zoo";"Zoo"' "building_rename maps/default.zoo '140 120' '$again$empty'"
check base-trial '"Burger_Stand_1";"Zoo";"Zed"' "building_rename maps/default.zoo '140 120' '$again$zed'"

# A grandstand of the expansion trial (`cHideCostChange` 1; scn28 has some) is clicked: its panel hides the up and
# down buttons and disables the price field. Prints the name, the price and the word the line has for it.
building_fixed() { # building_fixed <map> <x y of the stand>
  d=$(mktemp -d)
  printf '1 pause\n3 mouse %s\n4 down\n5 up\n' "$2" >"$d/script"
  rz run "$1" --headless --passes 10 --clock 200 --script "$d/script" 2>/dev/null |
    sed -n 's/^.* panel building [0-9]* \("[^"]*"\) price \([^ ]*\) \(fixed\) .*/\1 \2 \3/p' | head -1 | tr ' ' _
  rm -rf "${d:?}"
}
check expansion-trial '"Show_Grandstand_7"_$5.00_fixed' "building_fixed maps/scn28.zoo '600 60'"

# The same panels in a running game (the game unpaused at pass 93; a game day is 60 passes of 200 ms), over
# the month change (guest buildings 10.3). The map starts on the date it stores, 7 January 2001, so the month
# changes at pass 1561. building_running <map> <x y of the item> <awk pattern>
# prints the pass of the first `panel building` line that matches: the charging panel is refreshed every 5000 ms
# of the clock (a refresh every 25 passes from pass 93), so it shows the upkeep of the month (the burger stand's
# 50) at pass 1568, and the second month at pass 1968 (the stand is built at pass 85 and is 31 days old at
# pass 1953); the panel of a building that charges nothing is filled when shown and never by the clock.
building_running() { # building_running <map> <x y> <awk pattern>
  d=$(mktemp -d)
  printf '2 pause\n75 mouse 25 129\n76 down\n77 up\n79 mouse %s\n80 down\n81 up\n82 mouse 864 256\n84 down\n85 up\n86 key escape\n90 mouse 864 256\n91 down\n92 up\n93 pause\n' "$2" >"$d/script"
  rz run "$1" --headless --cash 75000 --seed 5 --passes 2100 --clock 200 --script "$d/script" 2>/dev/null |
    awk "/ panel building .*$3/{ print \$2; exit }" | tr -d '\n'
  rm -rf "${d:?}"
}
check base-trial 1968 "building_running maps/default.zoo '140 120' 'months 2 '"
check base-trial 1568 "building_running maps/default.zoo '140 120' 'upkeep .50 '"
check base-trial '' "building_running maps/default.zoo '140 160' 'months 2'"

# A building deleted while a guest uses it (guest buildings 11.1): `sim` runs the map; a second run has the
# bulldozer delete the building one step after the first guest entered it. The user is pushed out (one
# `building push` line; none in the plain run) and its walk out of the building (the goal `leavebuilding`,
# which the plain run shows) never comes. Prints, for the plain run and the deleting one, the `building push`
# lines and the walk-out goals of that guest, then the building and the guest. Seed 1 is the first (from 1)
# whose first user has no other walk-out in the run. Once keepers keep the exhibits their records assign, seed 4,
# the former seed, shows a walk-out in the deleting run (a longer run only adds more); seeds searched from 1.
pushed_out() { # pushed_out <map>
  d=$(mktemp -d)
  rz sim "$1" --ticks 400 --seed 1 --log 2>/dev/null >"$d/plain"
  read -r step b g <<<"$(awk '$3 == "building" && $4 == "use" { print $2, $5, $6; exit }' "$d/plain")"
  printf '%s bulldoze %s\n' $((step + 1)) "$b" >"$d/script"
  rz sim "$1" --ticks 400 --seed 1 --script "$d/script" --log 2>/dev/null >"$d/log"
  for f in plain log; do
    awk -v g="$g" '$3 == "building" && $4 == "push" { p++ }
      $3 == "unit" && $4 == "goal" && $5 == g && $6 == "leavebuilding" { l++ }
      END { printf "%d/%d,", p, l }' "$d/$f"
  done
  echo "$b,$g"
  rm -rf "${d:?}"
}
check expansion-trial 0/1,1/0,29780,40284 "pushed_out maps/scn19.zoo"

# The panel of a building of a saved zoo shows the figures its record stores (zoo format 6.9, guest
# buildings 4): the game paused, the view scrolled <presses> times by <dx> <dy> to the building, a
# click on it at <x> <y>. Prints the income, upkeep, profit, per month, months and guests of the panel.
loaded_building() { # loaded_building <map> <presses> <dx> <dy> <x> <y>
  d=$(mktemp -d)
  { echo '2 pause'; for i in $(seq 1 "$2"); do echo "$((2 + i)) scroll $3 $4"; done
    echo "20 mouse $5 $6"; echo '21 down'; echo '22 up'; } >"$d/script"
  rz run "$1" --headless --passes 30 --clock 67 --script "$d/script" 2>/dev/null |
    sed -n 's/.* panel building [0-9]* .* \(income .*\) sells.*/\1/p' | head -1 | tr ' ' _
  rm -rf "${d:?}"
}
check base-trial 'income_$182_upkeep_$100_profit_$82_month_$182_$100_$82_months_1_guests_last_15_this_12_total_27' "loaded_building maps/scn01a.zoo 8 -1 0 500 440"
check expansion-trial 'income_$354_upkeep_$300_profit_$54_month_$50_$42_$7_months_7_guests_last_10_this_0_total_59' "loaded_building maps/scn19.zoo 5 1 0 880 320"

# A building sold from its panel while a guest uses it (guest buildings 11.1, placing 7): the first
# guest to use the burger stand of scn19 (type brontbrg) is inside from pass 1084 (the stand is at the pixel the
# first lines click; the panel's sell button is the one at 987 578), the question is answered Yes at
# pass 1098 and the guest is pushed out. Prints the `building push` lines (building/guest), the refund
# (0.8 of 250) and the cash the sale left, and the walk-out goals the guest took after it (none).
sold_with_user() { # sold_with_user <map>
  d=$(mktemp -d)
  printf '2 mouse 875 230\n3 down\n4 up\n1094 mouse 987 578\n1095 down\n1096 up\n1098 key y\n' >"$d/script"
  rz run "$1" --headless --passes 1134 --clock 67 --seed 1 --script "$d/script" 2>/dev/null |
    awk '$4 == "building" && $5 == "push" { p = p $6 "/" $7 ";" }
      $4 == "tool" && $5 == "bulldoze" && $6 == "ask" { q = $7 }
      $4 == "world" && $5 == "remove" { r = $NF }
      $4 == "unit" && $5 == "goal" && $6 == 40287 && $7 == "leavebuilding" && r != "" { l++ }
      END { printf "%s%s;%s;%d\n", p, q, r, l }'
  rm -rf "${d:?}"
}
check expansion-trial "31076/40287;Sell;200;0" "sold_with_user maps/scn19.zoo"

# A building picked up from its panel and put down elsewhere (unit placing 8.7, guest buildings 11.1): the
# gift stand of scn19 (type giftstnd, cMoveable 1, id 29780 at tile 31,41, at the pixel 300 375) is selected, its
# user (with seed 4, the first from 1 whose pick-up pushes a user out: 1, 2 and 3 push none) pushed out when the
# pick-up button (871 573) is pressed at pass 1150, and it is put down at the pixel 520 310 (tile 37,42). Prints
# the carry, the push and the drop lines, then what the panel shows once it is down: the stand keeps its figures
# (the $430 its saved record stores and the $10 of each of two users, the one that left before and the one pushed out).
building_moved() { # building_moved <map>
  d=$(mktemp -d)
  printf '2 mouse 300 375\n3 down\n4 up\n1149 mouse 871 573\n1150 down\n1151 up\n1153 mouse 500 300\n1155 mouse 520 310\n1156 down\n1157 up\n1159 mouse 871 573\n' >"$d/script"
  rz run "$1" --headless --passes 1169 --clock 67 --seed 4 --script "$d/script" 2>/dev/null |
    awk '$4 == "world" && ($5 == "carry" || $5 == "drop") { w = w $5 "," $6 "," $7 ";" }
      $4 == "building" && $5 == "push" && $6 == 29780 { w = w "push;" }
      $4 == "panel" && $5 == "building" && $6 == 29780 { for (i = 7; i < NF; i++) if ($i == "income") l = "income " $(i + 1) }
      END { print w l }' | tr ' ' _
  rm -rf "${d:?}"
}
check expansion-trial 'carry,29780,;drop,29780,37,42;income_$440' "building_moved maps/scn19.zoo"

# A guest picked up by hand while it uses a building (guest buildings 11.2): the first guest to use the burger
# stand of scn19 (seed 1) is inside at pass 1088. It is selected by a click (pass 1088), picked up by a second
# click (1091) and put back by Escape (1093); it leaves the stand as a normal leave, so the stand's panel read at
# pass 1098 shows the price factor booked as income ($847 before, $854 after; $847 when it stays listed).
guest_picked() { # guest_picked <map>
  d=$(mktemp -d)
  printf '1086 mouse 830 230\n1087 down\n1088 up\n1090 down\n1091 up\n1093 key escape\n1096 mouse 875 230\n1097 down\n1098 up\n' >"$d/script"
  rz run "$1" --headless --passes 1102 --clock 67 --seed 1 --script "$d/script" 2>/dev/null |
    awk '$4 == "world" && ($5 == "carry" || $5 == "putback") { w = w $5 "," $6 ";" }
      $4 == "panel" && $5 == "building" && $6 == 31076 { for (i = 7; i < NF; i++) if ($i == "income") l = "income_" $(i + 1) }
      END { print w l }'
  rm -rf "${d:?}"
}
check expansion-trial 'carry,31554;putback,31554;income_$889' "guest_picked maps/scn19.zoo"

# The same stand picked up by a second click on it while it is selected, put back by Escape and picked
# up again by the same click (it is selected again): the lines they give, with no user in the stand (so
# no push).
building_put_back() { # building_put_back <map>
  d=$(mktemp -d)
  printf '0 pause\n2 mouse 300 375\n3 down\n4 up\n6 down\n7 up\n10 mouse 500 300\n12 mouse 520 310\n13 key escape\n15 mouse 300 375\n16 down\n17 up\n' >"$d/script"
  rz run "$1" --headless --passes 20 --clock 67 --seed 1 --script "$d/script" 2>/dev/null |
    awk '$4 == "world" && ($5 == "carry" || $5 == "putback") { w = w $5 "," $6 "," $7 ";" }
      $4 == "building" && $5 == "push" { w = w "push;" }
      END { print w }'
  rm -rf "${d:?}"
}
check expansion-trial 'carry,29780,;putback,29780,31,41;carry,29780,;' "building_put_back maps/scn19.zoo"

# Runs the game headless with a script given as printf text (a file in a scratch folder: the native Windows
# program cannot open a process substitution) and prints the log. scripted <script> <rz run arguments...>
scripted() {
  d=$(mktemp -d)
  printf "$1" >"$d/script"
  shift
  rz run "$@" --script "$d/script" 2>/dev/null
  rm -rf "${d:?}"
}

# A building put down after a pick-up is a placement: the animals of the exhibit react (habitat rating 9.4,
# unit placing 8.7). The gift stand of scn19 is picked up by a second click and put down in the pen of the two
# bears of the map, which value its habitat key below 0: both dislike it and the frown plays.
check expansion-trial 'dislikes2,likes0;frown' 'scripted "$({ echo "1 pause"; for p in 2 3 4 5 6 7 8 9 10 11; do echo "$p scroll -1 0"; done; printf "12 scroll 0 0\n14 mouse 940 375\n15 down\n16 up\n18 down\n19 up\n22 mouse 500 240\n24 mouse 500 240\n26 down\n27 up\n"; })" maps/scn19.zoo --headless --passes 40 --clock 67 --seed 1 | awk "\$2>=26 && / animal dislikes /{n++} \$2>=26 && / animal likes /{m++} / animal sound /{s=s \$NF} END{print \"dislikes\" n+0 \",likes\" m+0 \";\" s}"'

# The sell button clears the selection before it asks (placing 7.1): after a No the building stays and its
# panel is closed (the last `panel building` line is `none`).
check expansion-trial 'none' 'scripted "2 mouse 875 230\n3 down\n4 up\n20 mouse 987 578\n21 down\n22 up\n24 key n\n" maps/scn19.zoo --headless --passes 40 --clock 67 --seed 1 | awk "\$4 == \"panel\" && \$5 == \"building\" && \$2 >= 21 {l = \$6} END{print l}"'

# An undo while a building is held (placing 8, unit placing 8.7): a gift stand is bought (130), placed, picked up by
# a second click and Backspace pressed: its move is cancelled and it is removed with its whole price (one
# removal told, the cash back to its start).
check expansion-trial '75000;1' 'scripted "2 pause\n3 mouse 25 129\n4 down\n5 up\n6 mouse 60 150\n7 down\n8 up\n10 mouse 600 400\n12 down\n13 up\n14 key escape\n16 mouse 600 400\n17 down\n18 up\n20 down\n21 up\n25 key backspace\n" maps/scn19.zoo --headless --passes 30 --clock 200 --cash 75000 --seed 5 | awk "/ money cash /{c=\$6} / world remove .* undo\$/{n++} END{print c \";\" n+0}"'

# The pick-up and the drop record nothing for the undo: the burger stand is sold, the gift stand picked up and
# put down, then Backspace gives the burger stand back.
check expansion-trial 'carry;drop;1' 'scripted "2 mouse 875 230\n3 down\n4 up\n6 mouse 987 578\n7 down\n8 up\n10 key y\n14 mouse 300 375\n15 down\n16 up\n18 down\n19 up\n22 mouse 500 300\n24 mouse 520 310\n26 down\n27 up\n30 key backspace\n" maps/scn19.zoo --headless --passes 40 --clock 67 --seed 1 | awk "\$4 == \"world\" && \$5 == \"carry\"{w = w \"carry;\"} \$4 == \"world\" && \$5 == \"drop\"{w = w \"drop;\"} / world add .* undo\$/{n++} END{print w n+0}"'

# The same click, printing the name and the price of the panel: a loaded building shows the price its
# record stores (guest buildings 4.1), not the type's default. A grandstand of scn28 stores 3.00 where
# the type's price is 5.00.
loaded_price() { # loaded_price <map> <presses> <dx> <dy> <x> <y>
  d=$(mktemp -d)
  { echo '2 pause'; for i in $(seq 1 "$2"); do echo "$((2 + i)) scroll $3 $4"; done
    echo "20 mouse $5 $6"; echo '21 down'; echo '22 up'; } >"$d/script"
  rz run "$1" --headless --passes 30 --clock 67 --script "$d/script" 2>/dev/null |
    sed -n 's/.* panel building [0-9]* \("[^"]*"\) price \([^ ]*\) .*/\1 \2/p' | head -1 | tr ' ' _
  rm -rf "${d:?}"
}
check expansion-trial '"Show_Grandstand_4"_$3.00' "loaded_price maps/scn28.zoo 10 1 0 472 316"

# The users of a building are back inside after a load (guest buildings 4.1, 6.5): a guest the record
# lists is on the use of its building (task `dobuilding`), so its first goal is to leave, after the
# time it has left; one that is not listed starts with a goal of its own. Prints the step, the guest
# and the first goal of each guest given, in the order of the log. A pair whose guest is not in the
# zoo is dropped. Guest 40253 of scn19 sits on a seat that is visible inside: only its use set releases it,
# and a loaded zoo does not resume the sets, so it has no goal in the run.
restored_users() { # restored_users <map> <guest>...
  m=$1
  shift
  rz sim "$m" --ticks 300 --seed 1 --log 2>/dev/null | awk -v ids="$*" '
    BEGIN { n = split(ids, a, " "); for (i = 1; i <= n; i++) want[a[i]] = 1 }
    $3 == "unit" && $4 == "goal" && ($5 in want) && !seen[$5]++ { printf "%s%s:%s:%s", c++ ? "," : "", $2, $5, $6 }
    END { print "" }'
}
check base-trial "11:29997:leavebuilding" "restored_users maps/scn01a.zoo 29997"
check expansion-trial "" "restored_users maps/scn19.zoo 40253"
check expansion-trial "6:24919:leavebuilding" "restored_users maps/scn27.zoo 24919"

# The same panel after Play on the first scenario, whose start has `reset`: the guest has just
# arrived (zoo format 6.12), where the plain run above shows its stored date. Prints the name and the time.
scenario_guest_panel() {
  d=$(mktemp -d)
  mkdir -p "$RUSTYZOO_USER_DIR"
  printf '[UI]\nplayMovie=0\n' >"$RUSTYZOO_USER_DIR/zoo.ini"
  printf '1 mouse 512 365\n2 down\n3 up\n5 mouse 790 654\n6 down\n7 up\n9 pause\n10 mouse 652 598\n11 down\n12 up\n14 mouse 1005 667\n15 down\n16 up\n' >"$d/script"
  rz run --headless --seed 3 --passes 20 --clock 100 --script "$d/script" 2>/dev/null |
    sed -n 's/.* panel guest [0-9]* \("[^"]*"\) .* time \("[^"]*"\).*/\1 time \2/p' | tr ' ' _ | paste -sd';' -
  rm -rf "${d:?}"
}
check expansion-trial '"Guest_58"_time_"Just_arrived"' "scenario_guest_panel"

# The panel of a building after Play on the first scenario of the expansion trial (`resetdate`): the
# start resets every building (guest buildings 4.1), so the burger stand that stores income 847 and
# upkeep 250 shows no income, upkeep or guests (the plain load above keeps them).
scenario_building() {
  d=$(mktemp -d)
  mkdir -p "$RUSTYZOO_USER_DIR"
  printf '[UI]\nplayMovie=0\n' >"$RUSTYZOO_USER_DIR/zoo.ini"
  printf '1 mouse 512 365\n2 down\n3 up\n5 mouse 790 654\n6 down\n7 up\n9 pause\n12 mouse 905 185\n13 down\n14 up\n' >"$d/script"
  rz run --headless --seed 3 --passes 25 --clock 100 --size 1024x768 --script "$d/script" 2>/dev/null |
    sed -n 's/.* panel building [0-9]* .* \(income .*\) sells.*/\1/p' | head -1 | tr ' ' _
  rm -rf "${d:?}"
}
check expansion-trial 'income_$0_upkeep_$0_profit_$0_month_$0_$0_$0_months_1_guests_last_0_this_0_total_0' "scenario_building"

# The marine round (tank staff): scn27 has a filled tank with a manta ray, a tank filter and a
# maintenance worker. The script sends the worker away (he would service the filter as soon as it
# is worn), brings a marine specialist and a second manta ray, and a worker back late, after the
# filter broke. Prints the first jobs of the specialist (the ladders, the food in the water), the
# lowest purity while the filter was sound, the purity when it broke and when the worker serviced
# it, the worker's jobs, the number of rays and whether each took at least 10
# bites in the second half of the run (they eat all along, 17 to 20 bites a half; the floor leaves
# room for a change of the draw order). Seed 2 is the first (from 1) where both rays do: with seed 1 one
# ray eats nothing in the second half once the guests walk to a building slot forced, as it did with seed 5 before.
marine_round() { # marine_round <map>
  d=$(mktemp -d)
  printf '0 remove 25163\n0 keeper 20,40\n0 animal mantaray 27,49\n31000 worker 20,40\n' >"$d/script"
  rz sim "$1" --ticks 38000 --seed 2 --script "$d/script" --log 2>/dev/null | awk '
    / unit add [0-9]+ keeper /{k=$5}
    / staff job /{
      if ($5==k) { if ($6=="none") next; if (!done) j=j (j?",":"") $6; if ($6=="ClimbDownOutsideLadder") done=1 }
      else if ($6!="none") w=w (w?",":"") $6
    }
    / world filter /{
      if ($8=="") { if (!broke && (min=="" || $7<min)) min=$7 }
      else if ($8=="worn" && $6==0) { broke=1; at=$7 }
      else if ($8=="serviced" && fixed=="") fixed=$7
    }
    / animal status /{ ray[$5]=1 }
    / animal eat / && $2>=19000 { late[$5]++ }
    END{ m=-1; for (a in ray) { n++; if (m<0 || late[a]+0<m) m=late[a]+0 }
         print j ";held>=" min ";broken:" at ">" fixed ";worker:" w ";rays:" n ";second-half-bites>=10:" (m>=10 ? "yes" : "no(" m ")") }'
  rm -rf "$d"
}
check expansion-trial "GoToOutsideLadderBase,ClimbUpOutsideLadder,ClimbDownInsideLadder,PutFood,PuttingFood,GoToInsideLadderBase,ClimbUpInsideLadder,ClimbDownOutsideLadder;held>=94;broken:99>90;worker:GoToFilter,FixFilter,GoToFilter,FixFilter;rays:2;second-half-bites>=10:yes" "marine_round maps/scn27.zoo"

# The pace of the guests (movement 7.2): the first five guests the map loads walk for 1500 steps;
# prints, for each, the tiles it entered and the last of them. Each guest has its own pace, so the
# pins differ from the same run with `[AI] VariableUnitSpeeds=0` (a user-folder zoo.ini on top of
# the game's), which gives every guest the plain rate.
guest_pace() { # guest_pace <map> [unit speeds setting]
  u=$(mktemp -d)
  [ -n "${2:-}" ] && printf '[AI]\nVariableUnitSpeeds=%s\n' "$2" >"$u/zoo.ini"
  RUSTYZOO_USER_DIR=$u rz sim "$1" --ticks 1500 --seed 1 --log 2>/dev/null | awk '
    / unit add [0-9]+ guest /{ if (n++ < 5) first[$5]=1 }
    / unit tile /{ if ($5 in first) { c[$5]++; l[$5]=$6 } }
    END{ for (g in c) print g ":" c[g] "@" l[g] }' | sort | paste -sd";" -
  rm -rf "$u"
}
check base-trial "29699:21@29,46;29726:12@21,51;29745:19@36,32;29748:23@31,33;29750:21@20,50" "guest_pace maps/scn01a.zoo"
check base-trial "29699:28@29,53;29726:25@22,45;29745:17@36,32;29748:29@29,37;29750:21@20,50" "guest_pace maps/scn01a.zoo 0"

# The water of a tank over time (exhibits and money 3.10): scn28's tank 2 holds water at purity 93
# and loses a point each `waterPurityDecayTime` (30000 ms, 448 steps of 67 ms: the first at step 447);
# the script drains it at step 1000 and fills it at 1100. Prints the step and purity of the first
# decay, the purity before and after the level came back to 0 (the reset to `initialWaterPurity`),
# and the levels of the refill, which leave the purity alone. The fill at 1100 changes only the flag
# of an empty tank, which is not told, so the refill starts at level 1.
tank_decay() { # tank_decay <map> <tank id>
  d=$(mktemp -d)
  printf '1000 drain %s\n1100 fill %s\n' "$2" "$2" >"$d/script"
  rz sim "$1" --ticks 1200 --seed 1 --script "$d/script" --log 2>/dev/null | awk -v t="$2" '
    $3=="world" && $4=="water" && $5==t && $2>0 {
      if (!first) { first=$2 ":" $9 }
      if ($7==0 && !reset) { reset=prev ">" $9 }
      if ($2>=1100) refill=refill (refill ? "," : "") $7 ":" $9
      prev=$9
    }
    END{print first ";" reset ";" refill}'
  rm -rf "$d"
}
check expansion-trial "447:92;99>100;1:100,2:100,3:100,4:100,5:100,6:100,7:100" "tank_decay maps/scn28.zoo 2"

# The tank animals' checks (tank fauna 10 to 12).
#
# Breath: a polar bear joins scn28b's tank (the bears of the map have dead counters, the original's
# bug of tank fauna 10.3: they never surface). It gains 10 breath at each run-out of its counter
# under water (30 status ticks, 448 steps of 67 ms), takes the surface task at the second, when it
# has the 20 of its threshold, breathes there, and its next run-out falls at the surface: the check
# never runs again. A run-out at the surface kills the counter for good, which happens to the bear
# at its first run-out under half of the seeds; seed 2 is the first (from 1) where it does not: with the assigned exhibits of the loaded keepers, seed 1 loses it, as 3000 steps or 10000 do not bring the gain back (seeds 1 to 30 tried in order). Prints the steps
# and breath of the gains, the step of the surface task and of `bBreathe`, and the gains after it.
tank_breath() { # tank_breath <map> <x,y>
  d=$(mktemp -d)
  printf '0 animal polarbr %s male\n' "$2" >"$d/script"
  rz sim "$1" --ticks 3000 --seed 2 --script "$d/script" --log 2>/dev/null | awk '
    / unit add [0-9]+ animal /{id=$5}
    $5==id && / animal breath /{ if (done) later++; else gains=gains (gains ? "," : "") $2 ":" $6 }
    $5==id && / unit goal [0-9]+ surfacebreath /{ surface=$2 }
    $5==id && / unit set [0-9]+ bBreathe/{ if (!done) { breathe=$2; done=1 } }
    END{ print gains ";surface@" surface ";breathe@" breathe ";later:" later+0 }'
  rm -rf "$d"
}
check expansion-trial "57:10,505:20;surface@505;breathe@708;later:0" "tank_breath maps/scn28b.zoo 22,23"

# Murky water: scn28's filters are taken away and a manta ray joins tank 2, which holds water of
# purity 93 and loses a point every 448 steps. The ray's happiness is set to 0 every second so that
# the changes of its checks show. Once the purity is below 60 it loses 5 happiness and 5 health at
# each murky check, every 5 status ticks (75 steps); the other checks add to the first two. Prints
# the step the purity went below 60, then the steps of the first five losses of exactly 5 health,
# and the happiness at the last three.
tank_murky() { # tank_murky <map> <x,y> <new animal's id> <filter> <filter> <filter>
  d=$(mktemp -d)
  { printf '0 remove %s\n0 remove %s\n0 remove %s\n0 animal mantaray %s\n' "$4" "$5" "$6" "$2"
    for s in $(seq 10 15 12000); do printf '%s mood %s 0\n' "$s" "$3"; done; } >"$d/script"
  rz sim "$1" --ticks 12000 --seed 1 --script "$d/script" --log 2>/dev/null | awk -v ray="$3" '
    BEGIN{ n=0 }
    / world water 2 /{ if ($9 < 60 && !low) low=$2 }
    $5 == ray && / animal status /{
      if (low && h0 != "" && $9 == health - 5 && n < 5) { steps[n]=$2; hap[n]=$6; n++ }
      health=$9; h0=1 }
    END{ print "below60@" low ";losses@" steps[0] "," steps[1] "," steps[2] "," steps[3] "," steps[4] ";happiness:" hap[2] "," hap[3] "," hap[4] }'
  rm -rf "$d"
}
check expansion-trial "below60@10751;losses@10832,10907,10982,11056,11131;happiness:-5,-5,-5" "tank_murky maps/scn28.zoo 28,47 47192 31192 31194 37416"

# Amphibious neighbours: scn28b's tank 1 and land exhibit 2 are level with the wall between them,
# which is a connector. The polar bear of the tank goes ashore and comes back, and the bear of the
# land exhibit swims into the tank. Prints the exhibits each entered and when (steps of 67 ms). The
# land exhibit counts the tank's water for the bears (habitat rating 5.3), so its bear is content;
# every recomputation ends with the reachability test (5.8), which draws: the tank bear goes ashore
# at step 602 and is home at step 1192 (and again at 4780 and 5729); the land bear swims back ashore
# and into the tank again. The bears' ratings are unchanged; the walrus alone in a tank with no
# land beside it drops from 78 to 28. Its different draws shift the shared random generator, so
# the bears cross at other steps.
tank_cross() { # tank_cross <map> <tank bear> <land bear>
  rz sim "$1" --ticks 14000 --seed 3 --log 2>/dev/null | awk -v a="$2" -v b="$3" '
    / unit enter /{ if ($5 == a) x=x (x ? "," : "") $6 "@" $2; if ($5 == b) y=y (y ? "," : "") $6 "@" $2 }
    END{ print a ":" x ";" b ":" y }'
}
check expansion-trial "93962:2@422,1@3054,2@3615,1@4800,2@6504,1@8382,2@13747;97902:1@1156,2@7855,1@11630" "tank_cross maps/scn28b.zoo 93962 97902"

# The water mode (tank fauna 5, 6.1): on scn19 a walk of animal 51758 ends on land (`bWalk`), then
# the water check gives it the water mode and sends it to a water tile of its exhibit; in the water
# mode its walks end on water tiles too, where it chooses from its water table (`bInWater`). Prints
# each tile a walk ended on with the set chosen there and the change of mode, for 3500 steps. Since
# the loaded keepers keep their assigned exhibits, the animal is in the water mode from step 18 and
# the sequence (a land walk, the mode at step 3003, then `bInWater`) comes once, at the end: 1200
# steps no longer show it, a longer run does, so the seed stays 11 (the guests choose attractions as
# sites and walk to a building slot forced).
water_mode() { # water_mode <map> <animal>
  rz sim "$1" --ticks 3500 --seed 11 --reset-fences --log 2>/dev/null | awk -v a="$2" '
    $5 == a && / animal terrain /{ out = out (out ? ";" : "") "mode" $6 "@" $2 }
    $5 == a && / unit tile /{ t = $6 }
    $5 == a && / unit goal [0-9]+ none /{ ended = t }
    $5 == a && / unit set /{ if (ended != "") { out = out ";" ended ":" $6; ended = "" } }
    END{ print out }'
}
check expansion-trial ";22,21:bWalk;mode1@1212;25,23:bSleep;25,23:bWalk;mode1@2406;21,27:bInWater;23,28:bInWater;27,33:bInWater" "water_mode maps/scn19.zoo 51758"

# Objects in a tank block only their own levels (tank fauna 7.3): piles of food on the floor of
# scn27's tank all along x 30 block its floor level only, and the manta ray, which swims above it,
# crosses the line (a pile stops the 2 by 2 ray from entering its tile at the levels it blocks, so
# with every level blocked it never would). Prints how many times the ray entered a tile of the line
# in 6000 steps, its lowest height there (the floor is at -96) and the first step.
tank_over() { # tank_over <map> <ray>
  d=$(mktemp -d)
  for y in 44 45 46 47 48 49 50 51; do printf '0 food 30,%s herbchow\n' "$y"; done >"$d/script"
  rz sim "$1" --ticks 6000 --seed 1 --script "$d/script" --log 2>/dev/null | awk -v a="$2" '
    $5 == a && / unit tile /{ split($6, p, ","); x = p[1] }
    $5 == a && / unit depth / && x == 30 { n++; if (low == "" || $6 < low) low = $6; if (!first) first = $2 }
    END{ print n + 0 ";" low ";" first }'
  rm -rf "$d"
}
check expansion-trial "19;-65;156" "tank_over maps/scn27.zoo 36659"

# The tank filter placed during play (tank staff 7.2, 7.3): scn27's filter is taken away and three
# are put down: one beside the tank, which then serves it and filters (and charges) at its first
# update, step 0, then every `cFilterDelay` (50 s, 747 steps); one beside the tank too (doubled) and
# one on a tank tile (flooded), both removed at their first update. Prints the step, the id and the
# word of each `world filter` line.
tank_filters() { # tank_filters <map> <map's filter>
  d=$(mktemp -d)
  printf '0 remove %s\n0 filter 33,52 0\n0 filter 34,52 0\n0 filter 30,50 0\n' "$2" >"$d/script"
  rz sim "$1" --ticks 1600 --seed 1 --script "$d/script" --log 2>/dev/null |
    awk '/ world filter /{ printf "%s%s:%s%s", n++ ? ";" : "", $2, $5, ($8 == "" ? "" : ":" $8) } END{ print "" }'
  rm -rf "$d"
}
check expansion-trial "0:36661;0:36662:doubled;0:36663:flooded;747:36661;1494:36661" "tank_filters maps/scn27.zoo 25167"

# Breeding (animals-needs 11 to 13). The base trial's animals are all male; three female lions join
# the three males of scn01a's lion exhibit, they pair at once, and a lion gives birth at the first
# reproduction check that rolls it. Prints the number of pairs, the step of the first birth and the
# number of young, when two runs agree. Seed 3 has a birth in the run. The animals rest in their
# shelters, which moved the first birth from step 2214 to 8491, and a hidden animal that chooses its
# ambient set and runs its sleep check as any animal does (the draws they make) to 15835, and the use
# set of the visible-inside rock, whose trial version holds no enter call, to 3109: the run is
# 16000 steps long (the same seed and the same three pairs as before; the staff of a loaded map draw
# at the load, which moved the first birth to 6178 and made it four young). A shelter walk that cannot
# start resets the whole goal (a lion's, at step 1238), which moved the first birth to 30208 and made
# it two young: the run is 34000 steps long. That reset now lowers the need flags of the animal too,
# which brought the first birth back to 5089 (the same two young): the run is 16000 steps long again. The walk
# to a building slot is started forced, which moved the first birth to 16029 (the same two young): the run is
# 20000 steps long. A lion saved asleep in another set now wakes at its first sleep check (the saved flags are
# restored), which moved the first birth to 19229 (four pairs, the same two young). With the dates and the life of
# the saved animals restored too (master), it is 9865 (three pairs, the same two young). Guests that sit on benches and picnic tables
# through their use sets and keep out of the slots of other guest types draw otherwise: it is 1790 (the same).
lion_birth() { # lion_birth <map> <tile> <tile> <tile>
  d=$(mktemp -d)
  printf '0 animal lion %s female\n0 animal lion %s female\n0 animal lion %s female\n' "$2" "$3" "$4" >"$d/script"
  once() { rz sim "$1" --ticks 20000 --seed 3 --script "$d/script" --log 2>/dev/null | awk '/ animal mate /{m++} / animal birth /{n++; if (!s) s=$2} END{print "mates:" m ";birth@" s ";young:" n}'; }
  a=$(once "$1"); b=$(once "$1")
  if [ "$a" = "$b" ]; then echo "$a"; else echo different; fi
  rm -rf "$d"
}
check base-trial "mates:3;birth@1790;young:2" "lion_birth maps/scn01a.zoo 45,37 44,36 46,38"

# The expansion trial's dinosaurs lay eggs. scn19's exhibit is no good for them, so the script
# replaces its polar bears with a pair of velociraptors (the second one female) and sets both to
# the best of spirits every 30 steps, as a good exhibit would. Steps of 200 ms. Prints the step of
# the first clutch, the number of eggs, and the step of the first hatching, which waits for a
# keeper to heal the egg (an untended egg is below its hit points when its time is up).
egg_script() { # egg_script <file> <first polar bear> <second polar bear> <first new id> <last step>
  { printf '0 remove %s\n0 remove %s\n1 animal velocira 23,27 male\n1 animal velocira 30,27 female\n' "$2" "$3"
    for s in $(seq 5 30 "$5"); do printf '%s mood %s 100\n%s mood %s 100\n' "$s" "$4" "$s" "$((${4} + 1))"; done; } >"$1"
}
# A clutch is one roll of 1 percent for each of about 90 rolls in the run, so it depends on the shared
# random stream. Seeds searched from 1: seed 1 gives a clutch and a hatching inside the 7200 steps
# (seed 2 lays no egg at 7200 or at 14400 steps).
egg_clutch() { # egg_clutch <map> <first polar bear> <second polar bear> <first new id>
  d=$(mktemp -d)
  egg_script "$d/script" "$2" "$3" "$4" 7200
  rz sim "$1" --ticks 7200 --dt 200 --seed 1 --reset-fences --script "$d/script" --log 2>/dev/null | awk '/ animal egg /{n++; if (!e) e=$2} / animal hatch /{if (!h) h=$2} END{print "egg@" e ";eggs:" n ";hatch@" h}'
  rm -rf "$d"
}
check expansion-trial "egg@2611;eggs:3;hatch@3288" "egg_clutch maps/scn19.zoo 51757 51758 53931"
# Whatever the stream, the pair finds each other as mates in the first 100 steps (seeds 1 to 4).
egg_mates() { # egg_mates <map> <first polar bear> <second polar bear> <first new id>
  d=$(mktemp -d)
  egg_script "$d/script" "$2" "$3" "$4" 100
  for seed in 1 2 3 4; do
    rz sim "$1" --ticks 100 --dt 200 --seed $seed --script "$d/script" --log 2>/dev/null | grep -c ' animal mate '
  done | paste -sd, -
  rm -rf "$d"
}
check expansion-trial "1,1,1,1" "egg_mates maps/scn19.zoo 51757 51758 53931"

# The simulation run twice in a row prints the same state hash (the CI job `hashes` compares the
# hashes of the three systems).
sim_twice() { # sim_twice <map>
  a=$(rz sim "$1" --ticks 1000 --hash)
  b=$(rz sim "$1" --ticks 1000 --hash)
  if [ -n "$a" ] && [ "$a" = "$b" ]; then echo same; else echo different; fi
}
check base-trial same "sim_twice maps/default.zoo"
check expansion-trial same "sim_twice maps/sm_aqua.zoo"

# A saved game: `save` writes the zoo (the size of the file, as the map's), `load` opens it as Continue
# does, and neither warns. Prints the size, the warnings and the state hash after the load.
save_load() { # save_load <map>
  abs_bin=$(cd "$(dirname "$bin")" && pwd)/$(basename "$bin")
  abs_dir=$(cd "$dir" && pwd)
  d=$(mktemp -d)
  # The names are relative to the folder: a native Windows program does not read a Unix path in a file.
  printf '0 save a.zoo\n1 load a.zoo\n' >"$d/script"
  h=$(cd "$d" && "$abs_bin" --data "$abs_dir/$trial" sim "$1" --ticks 3 --hash --script script 2>err)
  echo "$(wc -c <"$d/a.zoo" | tr -d ' '),$(grep -c warning "$d/err"),$h"
  rm -rf "$d"
}
check base-trial "102822,0,3e576766d5fdeaa7" "save_load maps/default.zoo"
check expansion-trial "109491,0,9aef338e4ed4af50" "save_load maps/sm_aqua.zoo"
# scn19 holds entities of types the trial lacks: the file is shorter than the map.
check expansion-trial "227408,0,d48841c7b8256e64" "save_load maps/scn19.zoo"

exit $failed
