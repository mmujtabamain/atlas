# Frame performance: what was measured, what was changed, the rules that follow

Status: 2026-09-12, after three rounds. Numbers are per frame while scrolling the
screen, `cargo run` (debug profile), from `logs.log` — see README "Logs and the
frame meter". "Box" is the Linux DevBench box (CPU rendering, slow); the
customer's Mac is roughly 2× faster per frame.

## 1. The report and what the log said

Reported: single-digit fps. The first `logs.log` from the Mac (Household screen,
wheel-scrolling, debug build) showed:

| measure | value |
|---|---|
| engine (`refresh_derived`, all screen models) | 20 ms once at startup, then idle for the whole session |
| `build` — time in `AtlasApp::render` (our element tree) | 0.5–0.7 ms |
| `draw≈` — build + gpui layout + paint | **35 ms every frame** |
| gpui's own `Window::draw` histogram | p50 35.7 ms |
| present interval while scrolling | 36 ms ⇒ 27 fps |

So the frame cost was inside gpui's layout/paint of the tree, not in our code
and not in the engine. A per-phase probe (`perf.rs`, `PhaseProbe`) then split
the draw: on the box, Household was **layout 6 · taffy 45 · prepaint 8 ·
paint 4 ms**. The flexbox solve dominated.

A counting build of gpui (vendored `gpui-pre` with counters in
`taffy.rs`/`line_layout.rs`, wired in with a temporary `[patch.crates-io]`)
gave the mechanism:

| screen | taffy nodes | measure callbacks / frame | per node |
|---|---|---|---|
| Settings | 161 | 306 | 1.9 |
| Household | 679 | 19,586 | 29 |
| Privacy | 679 | 31,688 | 47 |
| Timeline | 1,804 | 7,489 | 4.2 |

Text shaping was not the problem (1 cache miss per frame, thousands of hits):
taffy was asking the same text leaves for their size up to 47 times per frame.

## 2. Why taffy re-measures

A flex item whose width is `auto` is sized from its content. For a flex
*container* (a `Tag`, a nested `h_flex`/`v_flex`, a card) that means running
the flex algorithm on its subtree, which measures every text leaf below it;
and it happens once per sizing pass of every ancestor that is itself
content-sized. The cost therefore compounds with nesting. Concretely, bisected
with the counting build (all counts per frame):

- A `flex_wrap` row of auto-width cards (`div().min_w_48()`): the Household
  money row alone cost 5,200; with cards at a fixed 16 rem, 460.
- A sentence in a `flex_1().min_w_0()` cell of a wrap row next to a tag: the
  Household assumptions block (8 rows) cost 11,400; as plain full-width rows
  with the tag beneath, 985.
- Three `Tag`s in an auto-width `h_flex` row inside a policy row: 8,300 for
  four rows; the same row with `w_full()` 1,300; a plain block `div` instead
  of a flex container costs 16 per element.
- The main content column with `flex_1().min_w_0()`: 8,300 on the same four
  rows; with a pixel width, 2,600; both fixes together, 516.
- gpui-component's scrollbar wrapper (`overflow_y_scrollbar`) doubles the
  count versus gpui's bare `overflow_y_scroll` (8,300 vs 4,100); kept, the
  scrollbar is wanted.

## 3. Rules for layout code in this app

1. **The scroll column has a pixel width** (`viewport − sidebar`, the cached
   content view's style in `Shell::render`) and **every screen root is
   `w_full()`**. Do not put `flex_1()` back on the main column.
2. **Cards in a wrap row have a fixed width**: use `widgets::figure::card`
   (16 rem). Never `min_w_*` on a wrap-row child.
3. **Long text never goes in a wrap row and never next to a tag in a
   `flex_1().min_w_0()` cell.** Give the sentence its own full-width row and put
   tags/metadata on the line beneath (Household/Projections assumptions,
   Privacy policies) — or join several short lines into one wrapping sentence
   (Privacy policy terms) instead of a wrap row of chips.
4. **Rows that contain flex containers (tags, nested rows/columns) get
   `w_full()`** when they are direct children of a column. Do **not** put
   `w_full()` on an item that sits inside a row next to siblings (it pushes
   them off-screen; `page_header` is `flex_1`, not `w_full`, for that reason).
5. **Chain blocks and their rows are `w_full()`** (`widgets::explain`).
6. Prefer plain `div()` over `h_flex()`/`v_flex()` for a wrapper that only
   holds one child and needs no alignment: a block is ~100× cheaper to size.

Results of applying 1–5 (box, measure callbacks per frame, before → after):
Household 19,586 → 2,446 · Privacy 31,688 → 1,409 · Scenarios 24,100 → 4,946 ·
Projections 20,234 → 1,611 · Taxes 15,441 → 4,606 · Assumptions 12,462 →
3,365 · Liquidity 8,478 → 1,152 · Rules 7,412 → 3,251 · Companies 9,257 →
1,742 · Accounts 6,071 → 1,679 · People 3,892 → 664 · Timeline 7,489 → 6,478.
Draw time on the box: Household 75 → 42 ms, Privacy 167 → 22 ms.

## 3b. Release build on the box (new layout, `cargo build --release`)

| screen | draw≈ | layout | taffy | prepaint | paint |
|---|---|---|---|---|---|
| Settings | 7 ms | 0.9 | 2.3 | 2.2 | 1.3 |
| Privacy | 13 ms | 1.5 | 6.5 | 2.1 | 2.4 |
| Household | 22 ms | 3.3 | 12.5 | 3.2 | 2.7 |
| Timeline | 40 ms | 7.3 | 21 | 6.3 | 4.9 |

Two things follow. Release is ~3× faster than the debug profile per frame
(Household 42 → 22 ms), so the dev profile is a real factor for the customer's
`cargo run`. And taffy still costs a near-constant 12–19 µs **per node** in
release, whatever the screen — with the re-measurement gone, the remaining
lever for the flexbox solve is fewer and shallower nodes (block `div`s
instead of flex containers for single-child wrappers, no wrapper per table
cell, definite heights where known), not more width fixes.

## 3c. The customer's Mac after round 2, and the dev profile

Mac (M-series, 2× scale), Household, wheel-scrolling, per frame:

| | before | debug after round 2 | release |
|---|---|---|---|
| draw | 35 ms | 15.3 ms | 6.3 ms |
| layout / taffy / prepaint / paint | — | 3.9 / 3.4 / 5.2 / 2.3 | 1.1 / 2.7 / 1.2 / 1.2 |
| frames per second while scrolling | 27 | 55–59 | 60 (display cap) |

Debug was at the 16.6 ms budget, so heavier screens fell to 30 fps through
vsync quantisation. The split showed where: layout and prepaint were 3.5–4×
their release cost while taffy was nearly equal — i.e. component code, not
the flexbox solve. On the box (Household / Timeline draw, ms):

| dev profile | Household | Timeline |
|---|---|---|
| as shipped by gpui-kit's guide (gpui crates at 3, rest at 0) | 43 | 96 |
| every dependency at opt-level 2 | 32 | 69 |
| `atlas-app` at opt-level 1, dependencies as shipped | 40 | 94 |
| **both** (now in `Cargo.toml`) | **26** | **50** |
| release | 22 | 40 |

Either change alone does little because gpui's generic builder code
(`Styled`, `IntoElement`, element wrappers) is monomorphised half in our crate
and half in `gpui-base`/`gpui-component`; both sides must be optimised.
`atlas-core` and `atlas-store` stay at opt-level 0.

About the status-bar counter: it used to lead with "N fps", the number of
frames drawn in the last second. In gpui that is a property of the *input* —
a mouse crossing five buttons draws five frames, so "5 fps" appeared while
each frame cost 6 ms. It now shows gpui's own figures (the profiler's
`Window::draw` p50 and the frame rate from its present-interval histogram,
which gpui records only while the window animates), refreshed once a second;
our meter's numbers stay in the log, where the phase split is the point.

## 4. What remains, in order of expected gain

- ~~**Node count**~~ (Timeline: 1,800 nodes, ~25 per occurrence row) — done
  (`widgets/grid.rs`, perf step 5 in `perf-plan.md`): the occurrences, the
  tax events and the fee postings are gpui-kit `DataTable`s (virtualised,
  own scroll region) fed from rows the models format once. Only the rows in
  view exist as elements, whatever the horizon.
- ~~Per-node cost of the debug profile~~ — done, see §3c.
- **Residual re-measurement** on Scenarios (10 callbacks/node: the chart and
  comparison table), Taxes and Rules (5/node) — same bisection as above.
- ~~**View caching for hover/typing frames**~~ — done (`shell.rs`, perf
  step 3 in `perf-plan.md`): the sidebar and the content are cached views; a
  frame that does not touch the screen costs ~2 ms on the box instead of
  22–60 ms. It does not help wheel scrolling (the content is notified).

## 5. How to measure

- `logs.log`: the once-a-second `perf: summary …` with gpui's histograms, a
  `perf: slow frame …` line per frame over 50 ms (debug), and every frame's
  line at trace — `ATLAS_LOG_FILE_FILTER=info,atlas_app=debug,atlas_app::perf=trace`
  turns those on (the benchmark script sets it).
- `scripts/perf-screens.sh <label>` on the box: every screen, scripted
  scrolling, one line per screen with the phase split, into
  `shots/perf-screens-<label>.txt`.
- The counting gpui build: copy `~/.cargo/registry/src/*/gpui-pre-0.3.4` to
  `gpui-lab/vendor/gpui-pre`, add counters around
  `TaffyLayoutEngine::compute_layout` (nodes, measure callbacks, time) and
  `LineLayoutCache::layout_line` (misses), and add
  `[patch.crates-io] gpui-pre = { path = "../gpui-lab/vendor/gpui-pre" }` to
  the workspace `Cargo.toml` — never commit the patch. Bisect a screen by
  temporarily gating its sections on an environment variable and reading the
  `measure_calls` count, which is deterministic; timings on the box are not.
