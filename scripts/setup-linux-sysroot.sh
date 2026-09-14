#!/usr/bin/env bash
# Builds <repo>/.sysroot for a Debian/Ubuntu box WITHOUT root (the Linux DevBench box).
#
# gpui (via gpui-kit) links against a few system libraries that are not
# installed on this box and cannot be installed with apt (no sudo):
#   - libxkbcommon-x11      (runtime lib missing entirely)
#   - dev-style *.so link names for libs whose *.so.N runtime files exist
#   - a Vulkan ICD: gpui renders through blade/Vulkan, the box has no GPU,
#     so we vendor Mesa's lavapipe (CPU Vulkan) and point the loader at it.
#
# Everything is downloaded with `apt-get download` (no root needed) and
# extracted with dpkg-deb into .sysroot/ (gitignored). Nothing outside the repo
# is touched. The build picks it up through .cargo/config.toml (LIBRARY_PATH,
# PKG_CONFIG_PATH, rpath); tools/gpui-shot exports VK_DRIVER_FILES so the Vulkan
# loader finds lavapipe for the app it launches. On a box that has the packages
# installed (or on macOS/Windows) none of this is needed.
#
# Usage: scripts/setup-linux-sysroot.sh        # once per box, seconds
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SYSROOT="$REPO/.sysroot"
LIB="$SYSROOT/lib"
SYS="/usr/lib/x86_64-linux-gnu"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

for tool in apt-get dpkg-deb; do
  command -v "$tool" >/dev/null || { echo "$tool not found: this script only works on a Debian/Ubuntu box"; exit 1; }
done

echo "sysroot: $SYSROOT"
mkdir -p "$LIB" "$SYSROOT/vulkan"

echo "--- downloading .debs (no root required)"
( cd "$TMP" && apt-get download libxkbcommon-x11-0 libxcb-xkb1 mesa-vulkan-drivers \
    libfontconfig-dev libfreetype-dev libxkbcommon-dev libwayland-dev libxcb1-dev libx11-xcb-dev >/dev/null )
for deb in "$TMP"/*.deb; do dpkg-deb -x "$deb" "$TMP/x"; done

echo "--- runtime libs that the box lacks"
cp -a "$TMP/x$SYS"/libxkbcommon-x11.so.0* "$LIB/"
cp -a "$TMP/x$SYS"/libxcb-xkb.so.1* "$LIB/"        # needed by libxkbcommon-x11
cp -a "$TMP/x$SYS"/libvulkan_lvp.so "$LIB/"
ln -sfn libxkbcommon-x11.so.0 "$LIB/libxkbcommon-x11.so"

echo "--- dev link names (-lfoo) for runtime libs the box already has"
link() { # link <dev name> <existing runtime file>
  [ -e "$SYS/$2" ] || { echo "  !! missing $SYS/$2"; return 0; }
  ln -sfn "$SYS/$2" "$LIB/$1"
}
link libxkbcommon.so      libxkbcommon.so.0
link libxcb.so            libxcb.so.1
link libX11.so            libX11.so.6
link libX11-xcb.so        libX11-xcb.so.1
link libwayland-client.so libwayland-client.so.0
link libwayland-cursor.so libwayland-cursor.so.0
link libwayland-egl.so    libwayland-egl.so.1
link libfontconfig.so     libfontconfig.so.1
link libfreetype.so       libfreetype.so.6
link libvulkan.so         libvulkan.so.1

echo "--- pkg-config metadata (.pc) for crates whose build.rs asks pkg-config"
# The .pc files point at the system prefix, which is where the runtime .so.N
# files live; the dev link names come from $LIB via LIBRARY_PATH.
mkdir -p "$LIB/pkgconfig"
find "$TMP/x" -name '*.pc' -exec cp -a {} "$LIB/pkgconfig/" \;
# We only ever link dynamically, so the transitive "Requires:" chains
# (freetype2 -> zlib, bzip2, libpng, brotli ...) are noise we do not want to vendor.
sed -i '/^Requires/d' "$LIB/pkgconfig/"*.pc
ls "$LIB/pkgconfig"

echo "--- Vulkan ICD manifest for lavapipe"
# A relative library_path is resolved against the manifest's own directory by
# the Vulkan loader, so the sysroot can be copied to another checkout as is.
cat > "$SYSROOT/vulkan/lvp_icd.json" <<JSON
{
    "ICD": {
        "api_version": "1.4.305",
        "library_path": "../lib/libvulkan_lvp.so"
    },
    "file_format_version": "1.0.0"
}
JSON

echo "--- done"
ls -la "$LIB"
echo
echo "cargo builds find the libraries through .cargo/config.toml; for a manual run of the app:"
echo "export VK_DRIVER_FILES=$SYSROOT/vulkan/lvp_icd.json   # (VK_ICD_FILENAMES on old loaders)"
echo "export LD_LIBRARY_PATH=$LIB"
