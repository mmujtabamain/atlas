# UI responsiveness: what still makes the app slow, and the plan

Status: 2026-09-12 — audit, plan, and (§5) what each step delivered. Written
after the layout rounds in `docs/perf.md`. Those rounds
fixed *how expensive one frame's flexbox solve is*. This document is about the
other half of the problem: **how much work the app repeats on every frame and
every edit that it could do once and keep**. The app has no caching layer of
any kind today — not for rendered subtrees, not for derived data, not for the
strings it paints.

Baseline (Linux box, debug profile, `scripts/perf-screens.sh after3`, per frame
while scrolling): Household 42 ms · Timeline 91 ms · Taxes 92 ms · Rules 54 ms ·
Scenarios 51 ms · Settings 18 ms. The Mac is ~2× faster; the release profile
another ~2×. A frame must cost < 16 ms to hold 60 fps.

## 1. What the code does per frame that it does not need to

| # | Finding | Where | Cost class |
|---|---|---|---|
| 1 | **One monolithic view.** `AtlasApp` is the only `Render` entity, so *any* event — a hover over a sidebar item, a tooltip, a keystroke in an input, a toast — calls `cx.notify(AtlasApp)` and gpui rebuilds, lays out and paints the whole tree: title bar + sidebar + the whole screen + status bar. gpui reuses a `.cached()` sub-view's layout and paint when its entity was not notified, but nothing here is a sub-view. | `app.rs` `Render for AtlasApp` | every event → full frame |
| 2 | **Screen models are deep-cloned every frame.** `render_section` calls `self.cloned(model)` on `TimelineModel` (≈1,000 `Occurrence`s), `ProjectionModel` (chart points + provenance chains), `TaxModel`, `RulesModel`, `ScenariosModel`, `PrivacyModel`, `EntityModels` … only so the screen function can take `&mut Context`. | `app.rs::render_section` | allocation per frame, grows with data |
| 3 | **Provenance chains deep-cloned per figure per frame.** `ExplainedFigure::figure()` clones `Calc<Money>` (a `ProvNode` tree) and then clones the tree *again* into the `Why?` click closure, and walks it in `disclosure()`. Household has 6+ figures, Liquidity/Projections more. | `widgets/figure.rs` | tree clones per frame |
| 4 | **Engine work inside render.** The Household accounts table calls `account_liquidity()` (engine, builds provenance nodes) for every account row on every frame; Household/Timeline/Assumptions/Liquidity/Projections/Privacy/entities call `disclosure_for()` (policy scan) per row per frame. | `screens/household.rs::render_accounts` and others | engine per frame |
| 5 | **O(n) lookups and `format!` per cell per frame.** `household.account(id)`, `series_by_id`, `entity_name`, `scenario(id)` are linear scans over `Vec`s; each timeline row does 3–5 of them and 6+ `format!`s. 1,000 rows × 5 scans × 50 accounts = 250k comparisons per frame before a single pixel. | `model.rs` lookups, every screen | O(rows × objects) per frame |
| 6 | **No virtualisation.** Timeline paints every occurrence (1,804 taffy nodes), Taxes 902, Rules 726. gpui-kit ships a virtualised `Table` (delegate-driven) and gpui has `uniform_list`; neither is used. | `screens/timeline.rs`, `taxes.rs`, `rules.rs` | nodes ∝ rows |
| 7 | **Per-frame log line with a synchronous, unbuffered file write** on the UI thread (`perf: frame #…` at debug level → `logs.log`), plus `chrono::Local::now()` and ~10 `format!`s per frame. `perf::timed` formats its label even when the line is filtered out. | `perf.rs::finish`, `logging.rs::log` | syscall per frame |
| 8 | **Shell strings rebuilt per frame**: `viewer_name()` (clone) ×3, status-bar counts, `file.path().display()`, section labels. Small on their own; matters because the shell is redrawn on every event (#1). | `render_title_bar`, `render_sidebar`, `render_status_bar` | allocation per frame |

## 2. What the code does per edit that it does not need to

| # | Finding | Where | Cost class |
|---|---|---|---|
| 9 | **Every derived model is recomputed after any edit.** `refresh_derived()` runs all eleven screen models (overview, entities, liquidity, timeline, projection, assumptions, taxes, rules, scenarios, decision, privacy) even though one section is visible. Each of them expands the series and runs its own forecast: 20 ms on the sample household, and it grows with the audit log, the series and the horizon — on a real household it is a visible hitch after every dialog. | `app.rs::refresh_derived_models` | 11 × engine per edit |
| 10 | **No shared expansion.** `expand_all()`/`forecast()` are called independently by the overview, timeline, projection, taxes, rules and scenarios for the same `(as_of, through, scenario)`. | `atlas-core` callers | duplicate engine runs |
| 11 | **Every form is rebuilt.** `rebuild_forms()` recreates ~60 `InputState`/`SelectState`/`DatePickerState` entities on each household change. | `app.rs::rebuild_forms` | entity churn per edit |
| 12 | **Save and load block the UI thread.** `HouseholdFile::save` copies the backup, opens SQLite, deletes and rewrites 16 tables and commits — synchronously inside the click handler. `load_sample`/open likewise. | `lifecycle.rs`, `atlas-store` | hitch per save |

## 3. The plan — one technique per finding, in the order they will be done

Each step is its own commit, measured before/after with `scripts/perf-screens.sh`
and covered by tests in `crates/atlas-app/tests/ui.rs` (behaviour must not change).

1. **Stop cloning per frame** (#2, #3). Screen models become `Rc<Model>` fields
   (`Rc::clone` is a pointer copy); provenance nodes inside `Calc<Money>` become
   `Rc<ProvNode>` so `Figure` and the `Why?` closure share them; `disclosure()`
   is computed once in `ExplainedFigure::new`.
2. **Precompute row view-models; index the household** (#4, #5). Every table
   row's strings (`account_text`, `subtitle`, `signed`, `detail`, disclosure,
   liquidity figures) are built once in the model's `compute()`; render only
   pastes `SharedString`s. `Household` gets id→index maps (rebuilt on mutation)
   so the lookups that remain are O(1). `account_liquidity` leaves the render path.
3. **Split the shell into cached views** (#1, #8). New entities `TitleBarView`,
   `SidebarView`, `StatusBarView` (each `cx.observe`s the app and is embedded
   with `Entity::cached(style)`); `AtlasApp` becomes the content view, embedded
   cached at its pixel size (viewport − sidebar − bars). A hover in the sidebar
   then re-renders ~40 nodes, not 1,800; the status-bar counter refreshes on a
   1 s timer instead of every frame. The frame meter moves to the new root.
4. **Lazy derived models** (#9). `refresh_derived()` marks every model stale
   and computes only the visible section's; a stale model is computed on first
   navigation to its screen. Models are memoised on `(household revision,
   viewer, screen params)` so switching back is free.
5. **Virtualise the long tables** (#6). Timeline occurrences/series/actuals,
   Taxes assessments and Rules decisions move to gpui-kit's delegate `Table`
   (virtualised rows, own scroll region, column sizing) or `uniform_list`;
   only the visible rows exist as elements.
6. **Throttle the perf log** (#7). Per-frame lines only for slow frames and
   when `ATLAS_PERF_TRACE=1`; the 1 s summary stays; the file sink is buffered
   and flushed on summary, warning and exit.
7. **Background save/load** (#12). `save` clones the household into
   `cx.background_spawn`, the status bar shows "saving…", failures still alert.
8. **Shared expansion cache** (#10) and **form reuse** (#11) — after the above,
   if the per-edit numbers still warrant it.

## 4. How success is measured

- `scripts/perf-screens.sh <label>` before and after each step (draw≈ per
  screen while scrolling) — the numbers go into the commit message.
- A new frame-log figure: `content(cached)` when the content view was reused.
- `refresh_derived` time in `logs.log` after an edit (step 4).
- The UI test suite stays green; new tests cover: cached shell re-render
  behaviour, lazy model invalidation, virtualised table row identity, and the
  background save round-trip.

## 5. What was done, and what it measured

Box = the Linux DevBench box, debug profile, CPU renderer; the Mac is ~2×
faster per frame and the release profile another ~2×. Scroll frames from
`scripts/perf-screens.sh` (single samples, ±5 ms noise); the rest from
`logs.log`.

| step | commit | what changed | measured |
|---|---|---|---|
| 1 | `850ecd5` | `Calc` shares its `ProvNode` through an `Arc`; figures prebuild their explain content; screen models passed by reference | scroll frames unchanged (the clones were in `build`); per-frame allocation no longer grows with the household |
| 3 | `11e8c31` | `Shell` root view; sidebar and content are cached views (`Entity::cached`); sidebar re-renders only when its snapshot changes; status bar shows gpui's own counter | a frame that does not touch the screen (sidebar hover, typing in a dialog, toast): **5–11 ms** instead of the full 10–64 ms; Timeline hover 64 → 10.7 ms |
| 5 | `350daf8` | `widgets::grid`: gpui-kit `DataTable` (virtualised) fed from rows the models format once; occurrences, tax events, fee postings, actuals | scroll frame: **Timeline 64 → 28 ms, Taxes 38 → 26 ms**; row count no longer matters |
| 4 | `b1c1b21` | `derived::Lazy<M>`: models dropped on change, computed on first use | an edit runs one model (**0.3 ms** on Rules) instead of all eleven (**~26 ms**); start-up computes the launch screen's model only |
| 6 | `6864b23` | per-frame log line → trace; slow frames keep their line; summary unchanged | measured first: the line cost < 0.1 ms; it was ~40 MB/h of log while scrolling |
| 7 | `b77c6ce` | save / open in `cx.background_spawn`; `dirty` survives an edit that lands mid-save | the SQLite rewrite no longer freezes the window; `perf: save … took Nms off the UI thread` in the log |
| 2 | `aa62af3` | Household people/companies/accounts rows and the actuals precomputed in the models | no screen runs engine code (`account_liquidity`, `disclosure_for`, `policy_for`) or household lookups while rendering |
| 8 | — | not done, by measurement | `rebuild_forms` is 1–2 ms on the box; with lazy models an edit runs one expansion, and the biggest remaining model (assumptions, 13 ms) is intrinsic sensitivity work, not duplication |

What is left is the cost of rendering a screen that *did* change (a scroll
tick, a hover inside it): gpui's layout/prepaint/paint of the remaining
nodes. The next levers there are the ones `docs/perf.md` §3b names — fewer and
shallower nodes on the heavy screens (Household 665, Taxes' pack list, Rules'
register), and the release profile for the customer's `cargo run` if the
debug numbers still feel slow on the Mac.
