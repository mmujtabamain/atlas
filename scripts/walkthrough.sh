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

frame "1 · Welcome: create a household, open a file, or explore the fictitious sample" -- "$APP" --theme light --size $SIZE
frame "2 · Who is looking? Every figure is projected for one person, chosen before anything is shown" --step wait:800 -- "$APP" --theme light --size $SIZE --sample
frame "3 · Today: what is free now, the hard floor and headroom, and the outlook through the forecast end" -- "$APP" --theme light --size $SIZE --sample --viewer a --screen today
frame "4 · Why is this number this number? Every derived figure opens its calculation, tagged with money class, certainty and result strength" --step click:560,200 --step wait:800 -- "$APP" --theme light --size $SIZE --sample --viewer a --screen today
frame "5 · Accounts: find an account by name, holder or type; settled, reserved and free are three different numbers" -- "$APP" --theme light --size $SIZE --sample --viewer a --screen accounts
frame "6 · An account: its balances, the earmarks and planned movements behind them, and every property" --step click:420,700 --step wait:400 --step click:1420,884 --step wait:700 -- "$APP" --theme light --size $SIZE --sample --viewer a --screen accounts
frame "7 · Earmarks: money set aside without leaving the bank, the hard floor, and the runway against it" -- "$APP" --theme light --size $SIZE --sample --viewer a --screen earmarks
frame "8 · Funding: which accounts the rules allow a purchase to use, in what order, and which account pays each category" -- "$APP" --theme light --size $SIZE --sample --viewer a --screen funding
frame "9 · Upcoming: every planned movement with its four dates, certainty and status; one occurrence can be skipped, moved or re-priced" -- "$APP" --theme light --size $SIZE --sample --viewer a --screen upcoming
frame "10 · Actuals: recording a transaction does not move the statement balance; matching links it to a planned occurrence" -- "$APP" --theme light --size $SIZE --sample --viewer a --screen actuals
frame "11 · People: each person's attributed share, what they hold, earn and owe in tax through the forecast end" --step click:600,262 --step wait:400 --step click:1493,392 --step wait:700 -- "$APP" --theme light --size $SIZE --sample --viewer a --screen people
frame "12 · A company: business cash, committed obligations and the ceiling before extraction costs — lawful extraction stays unresolved" -- "$APP" --theme light --size $SIZE --sample --viewer a --screen company
frame "13 · Forecast: the conditional cash path against the hard floor, with its exact values and per-account paths" -- "$APP" --theme light --size $SIZE --sample --viewer a --screen forecast
frame "14 · Assumptions: what the forecast relies on, with acceptance and freshness; derivation reads a fixed sample of past payments" -- "$APP" --theme light --size $SIZE --sample --viewer a --screen assumptions
frame "15 · Sensitivity: how far one assumption can move before the floor breaks — separate limits, never a joint guarantee" -- "$APP" --theme light --size $SIZE --sample --viewer a --screen sensitivity
frame "16 · Build a purchase: what you buy, when, the reserve to keep, and what matters most" -- "$APP" --theme light --size $SIZE --sample --viewer a --screen purchase
frame "17 · Step 2: the down payment and where it comes from, in funding order, with floors and company routes" --step click:1546,807 --step wait:800 -- "$APP" --theme light --size $SIZE --sample --viewer a --screen purchase
frame "18 · The result: the conservative verdict, the expected paths against your reserve, and the immediate cash after the purchase" --step click:508,192 --step wait:3000 -- "$APP" --theme light --size $SIZE --sample --viewer a --screen purchase-result
frame "19 · Scenarios: changes laid over the baseline, their compatibility, and a comparison whose difference adds up exactly" -- "$APP" --theme light --size $SIZE --sample --viewer a --screen compare
frame "20 · Rules: deterministic rules with priority, versions and a tie-break; a simulation shows the effect without applying it" -- "$APP" --theme light --size $SIZE --sample --viewer a --screen rules
frame "21 · Rule activity: every decision the rules took, how competing candidates were resolved, and the fees they added" -- "$APP" --theme light --size $SIZE --sample --viewer a --screen rule-activity
frame "22 · Taxes: effective-dated packs, tax events on their cash dates, and what falls due after the horizon as a reserve" -- "$APP" --theme light --size $SIZE --sample --viewer a --screen taxes
frame "23 · Sharing: versioned policies, the nine aspects they control, and the safe reason access is limited" -- "$APP" --theme light --size $SIZE --sample --viewer a --screen policies
frame "24 · Viewing as Person B: a private account is an authorized aggregate, never a name or a balance" -- "$APP" --theme light --size $SIZE --sample --viewer b --screen today
frame "25 · Person B's company view: identity and the planning-safe ceiling only; balances, payroll and taxes are not disclosed" -- "$APP" --theme light --size $SIZE --sample --viewer b --screen company
# The concat demuxer needs the last file repeated without a duration.
printf "file '%s'\n" "$FRAMES/frame-$(printf %02d $n).png" >> "$FRAMES/list.txt"
ffmpeg -hide_banner -loglevel error -y -f concat -safe 0 -i "$FRAMES/list.txt" -vf "fps=25,format=yuv420p" -c:v libx264 -preset medium -crf 20 "$REPO/shots/walkthrough.mp4"
ls -la "$REPO/shots/walkthrough.mp4"
