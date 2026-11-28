#!/bin/bash
# Builds RustyZoo.app from the universal binary. Usage: bundle.sh <binary> <version> <min-macos> <out-dir> [identity]
# identity is a codesign identity (Developer ID Application); the default `-` seals ad hoc.
# Needs iconutil and codesign (macOS) and target/THIRD-PARTY-NOTICES.txt (release.yml makes it with cargo-about). The icon files are copied, never resampled.
set -euo pipefail
bin=$1 version=$2 min=$3 out=$4 identity=${5:--}
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
cp "$here/../../target/THIRD-PARTY-NOTICES.txt" "$app/Contents/Resources/"
iconutil -c icns -o "$app/Contents/Resources/rustyzoo.icns" "$out/rustyzoo.iconset"
plutil -lint "$app/Contents/Info.plist"
if [ "$identity" = - ]; then
  # Ad-hoc seal of the whole bundle (Info.plist and Resources included), so a downloaded copy takes
  # the normal "unverified developer" path.
  codesign --force --deep -s - "$app"
else
  # Developer ID: the binary first, then the bundle, both with the hardened runtime and a secure
  # timestamp (notarisation needs both). No --deep: the bundle holds one executable and no entitlements
  # (SDL3 and the engine need no JIT, no unsigned memory and no third-party library).
  codesign --force --options runtime --timestamp -s "$identity" "$app/Contents/MacOS/rustyzoo"
  codesign --force --options runtime --timestamp -s "$identity" "$app"
fi
codesign --verify --deep --strict "$app"
