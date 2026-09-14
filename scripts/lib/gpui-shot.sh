#!/usr/bin/env bash
# Shared by scripts/shoot.sh, scripts/walkthrough.sh and scripts/perf-screens.sh:
# where the app and the gpui-shot helper live, and how to get both built. Source
# it after setting REPO (the checkout root); nothing here runs on its own.
#
#   TARGET   the cargo target directory ($CARGO_TARGET_DIR, else <repo>/target)
#   APP      target/debug/atlas — the product binary
#   SHOT     target/debug/gpui-shot — the headless screenshot helper from tools/gpui-shot
#            (override with GPUI_SHOT=/path/to/gpui-shot to use a prebuilt one)
#
# Both are built by `cargo build -p atlas-app -p gpui-shot` in this workspace, so a
# screenshot run needs nothing outside the checkout. On the Linux DevBench box the
# build links against the vendored `.sysroot` (scripts/setup-linux-sysroot.sh); the
# helper finds the same directory at run time to hand the app its Vulkan driver.

TARGET="${CARGO_TARGET_DIR:-$REPO/target}"
APP="$TARGET/debug/atlas"
SHOT="${GPUI_SHOT:-$TARGET/debug/gpui-shot}"

# Fails early, with the command to run, when the Linux box has no vendored sysroot
# yet: without it the app does not link and gpui-shot has no Vulkan driver to offer.
require_linux_sysroot() {
  [ "$(uname -s)" = Linux ] || return 0
  [ -e "$REPO/.sysroot/vulkan/lvp_icd.json" ] && return 0
  echo "no vendored sysroot at $REPO/.sysroot — run scripts/setup-linux-sysroot.sh once (no root needed)" >&2
  return 1
}

# Builds the app and, unless GPUI_SHOT points at a prebuilt helper, gpui-shot too.
build_app_and_gpui_shot() {
  require_linux_sysroot
  local packages=(-p atlas-app)
  [ -n "${GPUI_SHOT:-}" ] || packages+=(-p gpui-shot)
  (cd "$REPO" && source ~/.cargo/env 2>/dev/null; cargo build "${packages[@]}")
  [ -x "$SHOT" ] || { echo "gpui-shot is not at $SHOT (cargo build -p gpui-shot, or set GPUI_SHOT)" >&2; return 1; }
}
