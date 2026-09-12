#!/usr/bin/env bash
# Headless screenshots of every Atlas Financer screen (Linux DevBench box).
#
# Usage: scripts/shoot.sh                 # every scenario, light + dark
#        scripts/shoot.sh household-dark  # one scenario by name
#
# Needs the gpui-shot helper from ../gpui-lab (built with `cargo build -p gpui-shot`
# there) and the vendored sysroot it points the Vulkan loader at. Output lands in
# shots/<name>.png with the app log next to it; share PNGs with `devbench media put`.
# Long pages are photographed with a taller window (gpui-shot cannot scroll).
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LAB="${GPUI_LAB:-$REPO/../gpui-lab}"
SHOT="$LAB/target/debug/gpui-shot"
APP="${CARGO_TARGET_DIR:-$REPO/target}/debug/atlas"
ONLY="${1:-}"
WIDTH=1600

[ -x "$SHOT" ] || { echo "gpui-shot not built: (cd $LAB && cargo build -p gpui-shot)"; exit 1; }
(cd "$REPO" && source ~/.cargo/env 2>/dev/null; cargo build -p atlas-app)
mkdir -p "$REPO/shots"

scenario() { # scenario <name> <height> [gpui-shot args...] -- [atlas args...]
  local name="$1"; local height="$2"; shift 2
  if [ -n "$ONLY" ] && [ "$ONLY" != "$name" ]; then return; fi
  echo "== $name"
  # Exit 3 (frame written but never settled — a dialog's blinking caret) is fine for a still.
  "$SHOT" --out "$REPO/shots/$name.png" --size "${WIDTH}x${height}" --timeout 60 "$@" || [ $? -eq 3 ]
}

# Page heights: enough for the whole screen without scrolling.
height_of() {
  case "$1" in
    household) echo 1800 ;; people) echo 1200 ;; companies) echo 1400 ;; accounts) echo 1600 ;;
    liquidity) echo 1800 ;; timeline) echo 2000 ;; projections) echo 1800 ;; assumptions) echo 2000 ;;
    taxes) echo 2000 ;; rules) echo 2000 ;; scenarios) echo 2400 ;; decisions) echo 1000 ;;
    privacy) echo 2400 ;; settings) echo 1000 ;; *) echo 1000 ;;
  esac
}

for theme in light dark; do
  for screen in household people companies accounts liquidity timeline projections assumptions taxes rules scenarios decisions privacy settings; do
    h=$(height_of "$screen")
    scenario "$screen-$theme" "$h" -- "$APP" --theme "$theme" --size "${WIDTH}x${h}" --screen "$screen"
  done
done
# The explain sheet, opened by clicking "Why?" next to Free current cash.
scenario explain-free-cash-light 1000 --step click:1078,306 --step wait:600 -- "$APP" --theme light --size ${WIDTH}x1000
scenario explain-free-cash-dark  1000 --step click:1078,306 --step wait:600 -- "$APP" --theme dark  --size ${WIDTH}x1000
# Person B: private objects appear only as authorized aggregates — or are suppressed.
scenario household-person-b-light 1800 -- "$APP" --theme light --size ${WIDTH}x1800 --viewer b
scenario explain-free-cash-person-b 1000 --step click:1078,306 --step wait:600 -- "$APP" --theme light --size ${WIDTH}x1000 --viewer b
scenario scenarios-person-b-light 2400 -- "$APP" --theme light --size ${WIDTH}x2400 --viewer b --screen scenarios
scenario privacy-person-b-dark 1800 -- "$APP" --theme dark --size ${WIDTH}x1800 --viewer b --screen privacy
# Dialogs and the decision result.
scenario rule-editor-light 1000 --step click:1518,429 --step wait:700 -- "$APP" --theme light --size ${WIDTH}x1000 --screen rules
scenario scenario-change-dialog-light 1000 --step click:1400,322 --step wait:700 -- "$APP" --theme light --size ${WIDTH}x1000 --screen scenarios
scenario policy-editor-light 1000 --step click:1235,293 --step wait:700 -- "$APP" --theme light --size ${WIDTH}x1000 --screen privacy
scenario decisions-result-light 3400 --step click:1542,202 --step wait:2500 -- "$APP" --theme light --size ${WIDTH}x3400 --screen decisions
scenario decisions-result-dark 3400 --step click:1542,202 --step wait:2500 -- "$APP" --theme dark --size ${WIDTH}x3400 --screen decisions
# An empty household and its first dialog.
scenario empty-household-light 1000 -- "$APP" --theme light --size ${WIDTH}x1000 --new
scenario new-account-dialog-light 1100 --step click:1504,69 --step wait:700 -- "$APP" --theme light --size ${WIDTH}x1100 --new --screen accounts
ls -la "$REPO"/shots/*.png | wc -l
