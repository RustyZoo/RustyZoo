#!/usr/bin/env bash
# Downloads the two free official Zoo Tycoon trials from Microsoft, verifies them and extracts
# them with 7-Zip. Nothing is executed. Usage: ci/fetch-trials.sh <dir>
#
#   <dir>/base-trial        Zoo Tycoon trial (zoodemo.ini)
#   <dir>/expansion-trial   Marine Mania trial (zoo.ini)
#
# A folder that already exists is complete (it is moved into place only when extracted), so a
# second run does nothing. Exit status: 0 done, 1 error (a size or checksum mismatch included),
# 3 a download is unavailable (HTTP error, timeout, no network).
set -eu

# folder, URL, size in bytes, sha256. CI keys its cache on the hash of this file.
TRIALS='
base-trial https://download.microsoft.com/download/zootycoon/Trial/2.0/W9X2KMeXP/EN-US/ZooTycoon.exe 27775034 94d2af63c94a1f560acdc60fb6fa525d53594d8cdc2e5f902b6b6234b48283f7
expansion-trial https://download.microsoft.com/download/zootycoon/Trial/1.0b/WIN98XP/EN-US/mardinotrial.exe 87501504 d038b659dad48367033e5a227c1743261697768e0cd53df7f614e7c9ea784460
'

[ $# = 1 ] || { echo "usage: $0 <dir>" >&2; exit 1; }
mkdir -p "$1"
cd "$1" # everything below is relative: no path conversion between bash and native tools on Windows

sha256() {
  if command -v sha256sum >/dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi | cut -d' ' -f1
}

# 7zz (7-Zip 21+), 7z (7-Zip, p7zip), then the Windows install folder, which is not on PATH.
sevenzip() {
  for sz in 7zz 7z "/c/Program Files/7-Zip/7z.exe"; do
    if command -v "$sz" >/dev/null; then
      echo "extracting with $sz: $("$sz" | grep -m1 -i 7-zip)" >&2
      echo "$sz"
      return
    fi
  done
  echo "7-Zip not found (looked for 7zz and 7z)" >&2
  exit 1
}

fetch() {
  name=$1 url=$2 size=$3 sha=$4
  if [ -d "$name" ]; then
    echo "$name: already extracted"
    return
  fi
  file=download/${url##*/}
  if [ ! -f "$file" ]; then
    echo "$name: downloading $url"
    mkdir -p download
    if ! curl -fsSL --retry 2 --connect-timeout 30 --max-time 1200 -o "$file.part" "$url"; then
      rm -f "$file.part"
      echo "$name: download unavailable" >&2
      exit 3
    fi
    mv "$file.part" "$file"
  fi
  if [ "$(wc -c <"$file" | tr -d ' ')" != "$size" ] || [ "$(sha256 "$file")" != "$sha" ]; then
    rm -f "$file"
    echo "$name: $file does not have the expected size $size and sha256 $sha, refusing it" >&2
    exit 1
  fi
  echo "$name: size $size and sha256 $sha verified"

  sz=$(sevenzip)
  rm -rf work
  mkdir -p work
  "$sz" x -y -bd -owork/installer "$file" >/dev/null
  # The installer carries the game as a CAB resource. 7-Zip 26 extracts through it, older
  # versions stop at the CAB file, which then has to be extracted in a second step.
  cab=$(find work/installer -iname '*.cab' | head -n 1)
  if [ -n "$cab" ]; then
    "$sz" x -y -bd -owork/game "$cab" >/dev/null
  else
    mv work/installer work/game
  fi
  [ -f work/game/zoo.ini ] || [ -f work/game/zoodemo.ini ] || { echo "$name: no game ini in $file" >&2; exit 1; }
  mv work/game "$name"
  rm -rf work
  echo "$name: extracted $(find "$name" -type f | wc -l | tr -d ' ') files"
}

while read -r name url size sha; do
  [ -z "$name" ] || fetch "$name" "$url" "$size" "$sha"
done <<<"$TRIALS"
