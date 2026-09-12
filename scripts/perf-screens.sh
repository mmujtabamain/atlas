#!/usr/bin/env bash
# Per-screen frame cost on the Linux box: scripts/perf-screens.sh <label>
#
# Opens every screen with gpui-shot, scrolls a few times and prints the last
# frame's phase split from logs.log (build / draw≈ / layout / prepaint / paint)
# into shots/perf-screens-<label>.txt. Run it before and after a layout change
# and diff the two files. The `nodes=… measure_calls=…` columns only appear
# with the counting gpui build (see docs/perf.md: a vendored gpui-pre with
# counters, wired in through a temporary `[patch.crates-io]`).
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO/shots" || exit 1
label="${1:-run}"; out="perf-screens-$label.txt"; : > "$out"
STEPS="--step wait:600"; for i in $(seq 1 6); do STEPS="$STEPS --step wheel:900,500,2 --step wait:120"; done; STEPS="$STEPS --step wait:800"
for screen in household people companies accounts liquidity timeline projections assumptions taxes rules scenarios decisions privacy settings; do
  rm -f logs.log
  timeout 200 ../../gpui-lab/target/debug/gpui-shot --out perf-screen.png --size 1600x1000 --timeout 120 $STEPS -- ../target/debug/atlas --theme light --size 1600x1000 --screen $screen >/dev/null 2>&1
  taffy=$(grep "atlas-probe taffy" logs.log | awk -F'measure_calls=' '{split($2,a," "); if (a[1]+0 > 100) print}' | tail -1 | sed -E 's/.*atlas-probe taffy: //; s/ measure_time.*total=/ taffy=/; s/ line_layout.*//')
  frame=$(grep "perf: frame #" logs.log | tail -1 | sed -E 's/.*build=([^ ]+) draw≈([^ ]+) \(layout=([^ ]+) taffy=[^ ]+ prepaint=([^ ]+) paint=([^ ]+)\).*/draw≈\2 layout=\3 prepaint=\4 paint=\5/')
  printf "%-12s %s | %s\n" "$screen" "$taffy" "$frame" | tee -a "$out"
done
