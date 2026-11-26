#!/bin/bash
# Builds RustyZoo.app from the universal binary. Usage: bundle.sh <binary> <version> <min-macos> <out-dir>
# Needs iconutil and codesign (macOS). The icon files are copied, never resampled.
set -euo pipefail
bin=$1 version=$2 min=$3 out=$4
here=$(cd "$(dirname "$0")" && pwd)
png=$here/../icon/png
app=$out/RustyZoo.app
rm -rf "$app" "$out/rustyzoo.iconset"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$out/rustyzoo.iconset"
cp "$bin" "$app/Contents/MacOS/rustyzoo"
sed -e "s/@VERSION@/$version/g" -e "s/@MIN_MACOS@/$min/g" "$here/Info.plist" > "$app/Contents/Info.plist"
while read -r name size; do
  cp "$png/rustyzoo-$size.png" "$out/rustyzoo.iconset/$name.png"
done <<'LIST'
icon_16x16 16
icon_16x16@2x 32
icon_32x32 32
icon_32x32@2x 64
icon_128x128 128
icon_128x128@2x 256
icon_256x256 256
icon_256x256@2x 512
icon_512x512 512
icon_512x512@2x 1024
LIST
iconutil -c icns -o "$app/Contents/Resources/rustyzoo.icns" "$out/rustyzoo.iconset"
plutil -lint "$app/Contents/Info.plist"
# Ad-hoc seal of the whole bundle (Info.plist and Resources included), so a downloaded copy takes
# the normal "unverified developer" path. #66 replaces `-` with the Developer ID identity and adds
# the hardened runtime and notarisation.
codesign --force --deep -s - "$app"
codesign --verify --deep --strict "$app"
