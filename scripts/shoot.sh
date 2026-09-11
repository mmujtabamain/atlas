#!/usr/bin/env bash
# Headless screenshots of every Atlas Financer screen (Linux DevBench box).
#
# Usage: scripts/shoot.sh                 # every scenario, light + dark
#        scripts/shoot.sh household-dark  # one scenario by name
#
# Needs the gpui-shot helper from ../gpui-lab (built with `cargo build -p gpui-shot`
# there) and the vendored sysroot it points the Vulkan loader at. Output lands in
# shots/<name>.png with the app log next to it; share PNGs with `devbench media put`.
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LAB="${GPUI_LAB:-$REPO/../gpui-lab}"
SHOT="$LAB/target/debug/gpui-shot"
APP="$REPO/target/debug/atlas"
ONLY="${1:-}"
SIZE="${SIZE:-1600x1000}"

[ -x "$SHOT" ] || { echo "gpui-shot not built: (cd $LAB && cargo build -p gpui-shot)"; exit 1; }
(cd "$REPO" && source ~/.cargo/env 2>/dev/null; cargo build -p atlas-app)
mkdir -p "$REPO/shots"

scenario() { # scenario <name> [gpui-shot args...] -- [atlas args...]
  local name="$1"; shift
  if [ -n "$ONLY" ] && [ "$ONLY" != "$name" ]; then return; fi
  echo "== $name"
  "$SHOT" --out "$REPO/shots/$name.png" --size "$SIZE" --timeout 120 "$@"
}

for theme in light dark; do
  for screen in household people companies accounts liquidity timeline projections assumptions taxes rules scenarios decisions privacy settings; do
    scenario "$screen-$theme" -- "$APP" --theme "$theme" --size "$SIZE" --screen "$screen"
  done
done
# The explain sheet, opened by clicking "Why?" next to Free current cash.
scenario explain-free-cash-light --step click:1400,283 --step wait:600 -- "$APP" --theme light --size "$SIZE"
scenario explain-free-cash-dark  --step click:1400,283 --step wait:600 -- "$APP" --theme dark  --size "$SIZE"
# Person B: private objects appear only as authorized aggregates (§7.5).
scenario household-person-b-light -- "$APP" --theme light --size "$SIZE" --viewer b
scenario explain-free-cash-person-b --step click:1400,283 --step wait:600 -- "$APP" --theme light --size "$SIZE" --viewer b
ls -la "$REPO"/shots/*.png
