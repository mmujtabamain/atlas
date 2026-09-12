#!/usr/bin/env bash
# Records a walkthrough of Atlas Financer as a video (Linux DevBench box):
# a gpui-shot step sequence — one frame per scene, captioned — stitched by ffmpeg.
#
# Usage: scripts/walkthrough.sh            # writes shots/walkthrough.mp4
# Needs ../gpui-lab/target/debug/gpui-shot, ffmpeg with libx264 + drawtext, DejaVu Sans.
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LAB="${GPUI_LAB:-$REPO/../gpui-lab}"
SHOT="$LAB/target/debug/gpui-shot"
APP="${CARGO_TARGET_DIR:-$REPO/target}/debug/atlas"
FRAMES="$REPO/shots/walkthrough"
FONT="/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"
SIZE=1600x1000
SECONDS_PER_FRAME="${SECONDS_PER_FRAME:-4}"
# Frames with a text caret never "settle"; 40 s is enough for every scene to be drawn.
SHOT_TIMEOUT="${SHOT_TIMEOUT:-40}"

[ -x "$SHOT" ] || { echo "gpui-shot not built: (cd $LAB && cargo build -p gpui-shot)"; exit 1; }
(cd "$REPO" && source ~/.cargo/env 2>/dev/null; cargo build -p atlas-app)
rm -rf "$FRAMES"; mkdir -p "$FRAMES"
: > "$FRAMES/list.txt"
n=0

frame() { # frame "<caption>" [gpui-shot args...] -- [atlas args...]
  local caption="$1"; shift
  n=$((n + 1))
  local raw="$FRAMES/raw-$(printf %02d $n).png"
  local out="$FRAMES/frame-$(printf %02d $n).png"
  echo "== frame $n: $caption"
  # Exit 3 = the frame was written but never settled (a dialog's blinking caret keeps
  # it changing); the capture is still fine for a video, so only a real error stops.
  local rc=0
  "$SHOT" --out "$raw" --size "$SIZE" --timeout "$SHOT_TIMEOUT" "$@" > "$FRAMES/frame-$n.log" 2>&1 || rc=$?
  if [ "$rc" -ne 0 ] && [ "$rc" -ne 3 ]; then echo "gpui-shot failed on frame $n (exit $rc): see $FRAMES/frame-$n.log"; exit "$rc"; fi
  # The caption goes through a text file: no filter-graph escaping of commas, colons or quotes.
  printf '%s' "$caption" > "$FRAMES/caption-$n.txt"
  ffmpeg -hide_banner -loglevel error -y -i "$raw" -vf "drawbox=y=ih-72:h=72:color=black@0.72:t=fill,drawtext=fontfile=$FONT:textfile=$FRAMES/caption-$n.txt:fontcolor=white:fontsize=22:x=32:y=h-48" "$out"
  printf "file '%s'\nduration %s\n" "$out" "$SECONDS_PER_FRAME" >> "$FRAMES/list.txt"
}

frame "1 · A new, empty household: name, currency (USD by default) and the first person" -- "$APP" --theme light --size $SIZE --new
frame "2 · Everything is entered by hand; an account carries its owners, liquidity, minimums, visibility and calculation access" --step click:1504,69 --step wait:700 -- "$APP" --theme light --size $SIZE --new --screen accounts
frame "3 · The sample household: settled cash, reserved cash and free cash are three different numbers" -- "$APP" --theme light --size $SIZE
frame "4 · Why is this number this number? Every figure opens its calculation, tagged with money class, certainty and result strength" --step click:1078,306 --step wait:600 -- "$APP" --theme light --size $SIZE
frame "5 · Accounts: every property of an account and its balances, each with its calculation" -- "$APP" --theme light --size $SIZE --screen accounts
frame "6 · Liquidity & reservations: earmarks reduce what is spendable without leaving the bank; runway against hard floors" -- "$APP" --theme light --size $SIZE --screen liquidity
frame "7 · Timeline: every planned movement with its dates, certainty and status; scenario overlays" -- "$APP" --theme light --size $SIZE --screen timeline
frame "8 · Projections: a dated cash path for the household, a person or a company; taxes and fees posted once" -- "$APP" --theme light --size $SIZE --screen projections
frame "9 · Assumptions: the register, assumptions derived from past payments, which one would have to fail, and the conclusion with its conditions" -- "$APP" --theme light --size $SIZE --screen assumptions
frame "10 · Taxes: effective-dated rule packs, tax events with cash dates, who owes what, and all-at-once versus split extraction" -- "$APP" --theme light --size $SIZE --screen taxes
frame "11 · Rules: deterministic rules with a conflict inspector, fee events, funding order and simulation" -- "$APP" --theme light --size $SIZE --screen rules
frame "12 · Scenarios: changes laid over the baseline, combined scenarios, side-by-side metrics and where the difference comes from" -- "$APP" --theme light --size $SIZE --screen scenarios
frame "13 · Decisions: step 1 — what you buy, when, the reserve to keep, what matters most" -- "$APP" --theme light --size $SIZE --screen decisions
frame "14 · Step 2 — the down payment and where it comes from, in funding order, with floors and company routes" --step click:1546,674 --step wait:400 -- "$APP" --theme light --size $SIZE --screen decisions
frame "15 · The result: baseline vs decision vs reserve, affordability, ways to fund the down payment, the month × down-payment grid, goals" --step click:1542,202 --step wait:2500 -- "$APP" --theme light --size $SIZE --screen decisions
frame "16 · Privacy: versioned, effective-dated policies, grants for one purpose, policy checks and the audit log" -- "$APP" --theme light --size $SIZE --screen privacy
frame "17 · Viewing as Person B: private objects are not listed; a denial names the kind, never the object" -- "$APP" --theme light --size $SIZE --screen privacy --viewer b
frame "18 · Person B's calculation for the same number: the lone restricted term is suppressed, never exposed as a difference" --step click:1078,306 --step wait:600 -- "$APP" --theme light --size $SIZE --viewer b
# The concat demuxer needs the last file repeated without a duration.
printf "file '%s'\n" "$FRAMES/frame-$(printf %02d $n).png" >> "$FRAMES/list.txt"
ffmpeg -hide_banner -loglevel error -y -f concat -safe 0 -i "$FRAMES/list.txt" -vf "fps=25,format=yuv420p" -c:v libx264 -preset medium -crf 20 "$REPO/shots/walkthrough.mp4"
ls -la "$REPO/shots/walkthrough.mp4"
