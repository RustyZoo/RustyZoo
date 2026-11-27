#!/bin/bash
# Builds RustyZoo-<version>-<arch>.AppImage. Usage: appimage.sh <binary> <version> <out-dir>
# The arch is that of the machine. Needs curl and sha256sum; runs appimagetool without FUSE.
# The binary links only glibc and libgcc_s (SDL is built in; it opens X11, Wayland and the audio
# libraries of the desktop at run time), so nothing is bundled but the binary. The icon files are
# copied, never resampled. appimagetool and the runtime it prepends are pinned releases, checked by sha256.
set -euo pipefail
bin=$1 version=$2 out=$3
here=$(cd "$(dirname "$0")" && pwd)
png=$here/../icon/png
arch=$(uname -m)
tool_version=1.9.1
runtime_version=20251108
case $arch in
  x86_64)
    tool_sha=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
    runtime_sha=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d ;;
  aarch64)
    tool_sha=f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158
    runtime_sha=00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444 ;;
  *) echo "no appimagetool pinned for $arch" >&2; exit 1 ;;
esac
mkdir -p "$out"
tool=$out/appimagetool-$arch.AppImage
curl -fsSL -o "$tool" "https://github.com/AppImage/appimagetool/releases/download/$tool_version/appimagetool-$arch.AppImage"
echo "$tool_sha  $tool" | sha256sum -c -
chmod +x "$tool"
runtime=$out/runtime-$arch
curl -fsSL -o "$runtime" "https://github.com/AppImage/type2-runtime/releases/download/$runtime_version/runtime-$arch"
echo "$runtime_sha  $runtime" | sha256sum -c -

app=$out/RustyZoo.AppDir
rm -rf "$app"
mkdir -p "$app/usr/bin" "$app/usr/share/applications" "$app/usr/share/metainfo"
cp "$bin" "$app/usr/bin/rustyzoo"
cp "$here/rustyzoo.desktop" "$app/usr/share/applications/com.rustyzoo.game.desktop"
sed -e "s/@VERSION@/$version/g" -e "s/@DATE@/$(date -u +%F)/g" \
  "$here/com.rustyzoo.game.appdata.xml" > "$app/usr/share/metainfo/com.rustyzoo.game.appdata.xml"
for size in 16 32 48 64 128 256 512; do
  mkdir -p "$app/usr/share/icons/hicolor/${size}x$size/apps"
  cp "$png/rustyzoo-$size.png" "$app/usr/share/icons/hicolor/${size}x$size/apps/rustyzoo.png"
done
# appimagetool wants these at the root: the desktop file, the icon and AppRun.
cp "$app/usr/share/applications/com.rustyzoo.game.desktop" "$app/"
cp "$png/rustyzoo-256.png" "$app/rustyzoo.png"
# The window's app id is the desktop file's name, so Wayland shows the icon.
cat > "$app/AppRun" <<'RUN'
#!/bin/sh
export SDL_APP_ID=com.rustyzoo.game
exec "$(dirname "$(readlink -f "$0")")/usr/bin/rustyzoo" "$@"
RUN
chmod +x "$app/AppRun"

desktop-file-validate "$app/com.rustyzoo.game.desktop"
# appimagetool validates the metainfo itself (appstreamcli, when present) and fails on an error.
ARCH=$arch "$tool" --appimage-extract-and-run --runtime-file "$runtime" "$app" "$out/RustyZoo-$version-$arch.AppImage"
