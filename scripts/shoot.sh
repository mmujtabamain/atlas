#!/usr/bin/env bash
# Headless screenshots of every Atlas Financer screen (Linux DevBench box).
#
# Usage: scripts/shoot.sh              # every scenario, light + dark
#        scripts/shoot.sh today-dark   # one scenario by name
#
# Everything it needs is in this repo: the gpui-shot helper (`tools/gpui-shot`, built
# here alongside the app) and the vendored sysroot it points the Vulkan loader at
# (`.sysroot`, from `scripts/setup-linux-sysroot.sh`, once per box). Output lands in
# shots/<name>.png with the app log next to it; share PNGs with `devbench media put`.
# Long pages are photographed with a taller window (gpui-shot cannot scroll).
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$REPO/scripts/lib/gpui-shot.sh"
ONLY="${1:-}"
WIDTH=1600

build_app_and_gpui_shot
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
    today) echo 2000 ;;
    accounts) echo 1400 ;; account) echo 1400 ;; earmarks) echo 1800 ;; funding) echo 1000 ;;
    upcoming) echo 2200 ;; series) echo 1400 ;; actuals) echo 1000 ;;
    people) echo 900 ;; person) echo 1600 ;; companies) echo 900 ;; company) echo 1600 ;;
    forecast) echo 2400 ;; assumptions) echo 1600 ;; derive) echo 1600 ;; sensitivity) echo 1600 ;;
    purchase) echo 1400 ;; scenarios) echo 1400 ;; compare) echo 2200 ;; extraction) echo 1000 ;;
    rules) echo 1400 ;; rule-activity) echo 1400 ;; taxes) echo 1800 ;; tax-packs) echo 1800 ;;
    policies) echo 1800 ;; grants) echo 900 ;; audit) echo 900 ;;
    settings) echo 1200 ;; welcome) echo 900 ;;
    *) echo 1200 ;;
  esac
}

# Every workspace screen the sidebar and its tabs reach, for the sample's owner.
SCREENS="today accounts earmarks funding upcoming series actuals people companies forecast assumptions derive sensitivity purchase scenarios compare extraction rules rule-activity taxes tax-packs policies grants audit settings"
for theme in light dark; do
  for screen in $SCREENS; do
    h=$(height_of "$screen")
    scenario "$screen-$theme" "$h" -- "$APP" --theme "$theme" --size "${WIDTH}x${h}" --sample --viewer a --screen "$screen"
  done
done

# The first experience: Welcome, and the "Who is looking?" chooser over it.
scenario welcome-light 900 -- "$APP" --theme light --size ${WIDTH}x900
scenario welcome-dark 900 -- "$APP" --theme dark --size ${WIDTH}x900
scenario viewer-gate-light 900 --step wait:800 -- "$APP" --theme light --size ${WIDTH}x900 --sample

# The calculation sheet, opened from Today's leading figure.
scenario explain-free-cash-light 1000 --step click:560,200 --step wait:800 -- "$APP" --theme light --size ${WIDTH}x1000 --sample --viewer a --screen today
scenario explain-free-cash-dark 1000 --step click:560,200 --step wait:800 -- "$APP" --theme dark --size ${WIDTH}x1000 --sample --viewer a --screen today

# Person B: a private account is an authorized aggregate, a company is a
# planning-safe summary, a private scenario is a count.
scenario today-person-b-light 2000 -- "$APP" --theme light --size ${WIDTH}x2000 --sample --viewer b --screen today
scenario company-person-b-light 900 -- "$APP" --theme light --size ${WIDTH}x900 --sample --viewer b --screen company
scenario scenarios-person-b-light 1400 -- "$APP" --theme light --size ${WIDTH}x1400 --sample --viewer b --screen scenarios
scenario policies-person-b-dark 1800 -- "$APP" --theme dark --size ${WIDTH}x1800 --sample --viewer b --screen policies

# Details reached from a register: an account, a person, a company, a series, a rule.
scenario account-detail-light 1500 -- "$APP" --theme light --size ${WIDTH}x1500 --sample --viewer a --screen account
scenario person-detail-light 1600 -- "$APP" --theme light --size ${WIDTH}x1600 --sample --viewer a --screen person

# The purchase builder's steps and its result.
scenario purchase-step-2-light 1500 --step click:1546,807 --step wait:800 -- "$APP" --theme light --size ${WIDTH}x1500 --sample --viewer a --screen purchase
scenario purchase-result-light 2000 --step click:508,192 --step wait:3000 -- "$APP" --theme light --size ${WIDTH}x2000 --sample --viewer a --screen purchase-result
scenario purchase-result-dark 2000 --step click:508,192 --step wait:3000 -- "$APP" --theme dark --size ${WIDTH}x2000 --sample --viewer a --screen purchase-result

# The create-rule flow and the forecast record sheet.
scenario create-rule-light 1200 -- "$APP" --theme light --size ${WIDTH}x1200 --sample --viewer a --screen create-rule
scenario forecast-record-light 1400 --step click:1496,70 --step wait:900 -- "$APP" --theme light --size ${WIDTH}x1400 --sample --viewer a --screen forecast

# An empty household: the checklist, and the first entry dialog.
scenario empty-today-light 1200 -- "$APP" --theme light --size ${WIDTH}x1200 --new
scenario empty-accounts-light 1000 -- "$APP" --theme light --size ${WIDTH}x1000 --new --screen accounts
ls -la "$REPO"/shots/*.png | wc -l
