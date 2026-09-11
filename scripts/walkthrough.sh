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
APP="$REPO/target/debug/atlas"
FRAMES="$REPO/shots/walkthrough"
FONT="/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"
SIZE=1600x1000
SECONDS_PER_FRAME="${SECONDS_PER_FRAME:-4}"

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
  "$SHOT" --out "$raw" --size "$SIZE" --timeout 180 "$@" > "$FRAMES/frame-$n.log" 2>&1
  # The caption goes through a text file: no filter-graph escaping of commas, colons or quotes.
  printf '%s' "$caption" > "$FRAMES/caption-$n.txt"
  ffmpeg -hide_banner -loglevel error -y -i "$raw" -vf "drawbox=y=ih-72:h=72:color=black@0.72:t=fill,drawtext=fontfile=$FONT:textfile=$FRAMES/caption-$n.txt:fontcolor=white:fontsize=22:x=32:y=h-48" "$out"
  printf "file '%s'\nduration %s\n" "$out" "$SECONDS_PER_FRAME" >> "$FRAMES/list.txt"
}

frame "1 · A new, empty household: name, currency (USD by default), the first person — real data starts here (M12)" -- "$APP" --theme light --size $SIZE --new
frame "2 · Everything is entered by hand; an account carries all of §7: owners, liquidity, minimums, visibility, calculation access" --step click:1504,69 --step wait:700 -- "$APP" --theme light --size $SIZE --new --screen accounts
frame "3 · The sample household: current money, reserved money and free cash are three different numbers (§6)" -- "$APP" --theme light --size $SIZE
frame "4 · Why is this number this number? Every figure opens its §2.1 chain with money class, certainty and result strength" --step click:1400,283 --step wait:600 -- "$APP" --theme light --size $SIZE
frame "5 · Accounts: the full §7 property table and the balance definitions per account" -- "$APP" --theme light --size $SIZE --screen accounts
frame "6 · Liquidity & reservations (§17): earmarks reduce spendability without leaving the ledger; runway against hard floors (M13)" -- "$APP" --theme light --size $SIZE --screen liquidity
frame "7 · Timeline (§9): every occurrence with its clocks, certainty and status; scenario overlays" -- "$APP" --theme light --size $SIZE --screen timeline
frame "8 · Projections (§11): a chronological path per boundary, intraday ordering, taxes and fees posted once" -- "$APP" --theme light --size $SIZE --screen projections
frame "9 · Assumptions (§10): the register, derived assumptions, one-at-a-time breakpoints and the conditional statement" -- "$APP" --theme light --size $SIZE --screen assumptions
frame "10 · Taxes (§12): effective-dated packs, events with cash dates, attribution, E05 with-vs-without" -- "$APP" --theme light --size $SIZE --screen taxes
frame "11 · Rules (§14): deterministic rules with a conflict inspector, fee events, funding order and simulation" -- "$APP" --theme light --size $SIZE --screen rules
frame "12 · Scenarios (§18): overlays over the baseline, composition, side-by-side metrics and the F139 attribution" -- "$APP" --theme light --size $SIZE --screen scenarios
frame "13 · Decisions (§19): step 1 — what you buy, when, the reserve to keep, the objective" -- "$APP" --theme light --size $SIZE --screen decisions
frame "14 · Step 2 — the down payment and where it comes from, in funding order, with floors and company routes" --step click:1546,674 --step wait:400 -- "$APP" --theme light --size $SIZE --screen decisions
frame "15 · The result: baseline vs decision vs reserve, §19.1 metrics, §13.5 funding strategies, the E03 grid, goals, the §26 contract" --step click:1542,202 --step wait:2500 -- "$APP" --theme light --size $SIZE --screen decisions
frame "16 · Privacy (§7): versioned, effective-dated policies, purpose grants, fail-closed checks and the audit log" -- "$APP" --theme light --size $SIZE --screen privacy
frame "17 · Viewing as Person B: private objects are not listed; denials name the kind, never the object (V077)" -- "$APP" --theme light --size $SIZE --screen privacy --viewer b
frame "18 · Person B's chain for the same number: the lone restricted term is suppressed, never exposed as a difference (§7.6)" --step click:1400,283 --step wait:600 -- "$APP" --theme light --size $SIZE --viewer b
# The concat demuxer needs the last file repeated without a duration.
printf "file '%s'\n" "$FRAMES/frame-$(printf %02d $n).png" >> "$FRAMES/list.txt"
ffmpeg -hide_banner -loglevel error -y -f concat -safe 0 -i "$FRAMES/list.txt" -vf "fps=25,format=yuv420p" -c:v libx264 -preset medium -crf 20 "$REPO/shots/walkthrough.mp4"
ls -la "$REPO/shots/walkthrough.mp4"
