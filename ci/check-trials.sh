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

# Plays a map headless with scrolls, a rotation and the zoom in and out (-2, 0 and 2) and compares
# every painted frame with the window `render-map --view` draws for the camera the log names, at
# its zoom, byte for byte (as PNG files).
same_frames() {
  d=$(mktemp -d)
  printf '1 scroll 1 1\n2 rotate cw\n3 scroll -1 0\n4 zoom in\n5 scroll 1 0\n6 zoom out\n7 zoom out\n8 scroll 0 1\n' >"$d/script"
  rz run "$1" --headless --passes 9 --clock 20 --script "$d/script" --png-every 1 --out "$d" |
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
# Entity types: the total, then per class (food, paths, fences, buildings, animals, guests, scenery,
# staff, ambient, rubble, tank walls, filters), and the types whose base animation member is missing.
check base-trial 51,6,1,6,17,4,2,13,2,0,0,0,0 "rz types | tail -1 | grep -o '[0-9]*' | paste -sd, -"
check base-trial 1 "rz types | awk -F'\\t' 'NF>6 && \$7==\"no\"{n++} END{print n+0}'"
# Ground pictures of the terrain cache rendered again: pictures, covered pixels, pictures with
# differing covered pixels, differing pixels, other cuts, without palette, failed (the numbers before
# the semicolon); then the colour difference. The shipped plain pictures are lit and the rendered
# ones are not, so they differ by about 11 levels; the expansion trial has no cache.
check base-trial 2907,3126249,0,0,0,0,0 "rz check-ground | tail -1 | cut -d';' -f1 | grep -o '[0-9]*' | paste -sd, -"
check base-trial R11.23G10.56B8.03,95thpercentile25,max220 "rz check-ground | tail -1 | sed 's/.*mean //'"
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
check expansion-trial 5,5,0,0,28125,4160 "rz check-maps | tail -1 | grep -o '[0-9]*' | paste -sd, -"
# The expansion trial ships no ground cache, so every tile is rendered at run time. It ships no
# converted cliff pieces either (only their bitmaps), so they are converted in memory: all 2434
# are drawn.
check expansion-trial 5,0,0,0,28125,0,2434,0,3421,290,449 "rz check-render | tail -1 | grep -o '[0-9]*' | paste -sd, -"
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

# The game size of the ini file (800x600 in both trials) at factor 1, on the dummy video driver.
check base-trial 800x600,1,800x600,800x600,800x600,frames,same "windowed maps/default.zoo"
check expansion-trial 800x600,1,800x600,800x600,800x600,frames,same "windowed maps/scn19.zoo"
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

# Text drawn without game data: a wrapped, centred, shadowed message (bilevel glyphs) and a
# smoothed one with a 2 pixel shadow have the same pixels, so the same hash, on every system.
check base-trial 286x34,0da317e3f85dfc38 "rz draw-text $tmp/t.png --font 'Arial Bold' --size 11 --width 286 --justify center --shadow --fore 83 219 83 --back 68 36 20 'Sample message with words enough to fill one whole line and then some more' | tr ' ' ,"
check base-trial 300x21,638219be485aaf40 "rz draw-text $tmp/t.png --font 'Arial Bold' --size 14 --width 300 --shadow --fore 255 186 16 --back 20 20 20 'Sample item & more text here' | tr ' ' ,"
# A width of 0, a negative one, a huge one and none at all are refused (exit 1), a word is a usage
# error (exit 2).
for width in "--width 0" "--width -4" "--width 99999999999" ""; do
  check base-trial 1 "rz draw-text $tmp/t.png $width x >/dev/null 2>&1; echo \$?"
done
check base-trial 2 "rz draw-text $tmp/t.png --width wide x >/dev/null 2>&1; echo \$?"

exit $failed
