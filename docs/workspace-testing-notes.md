# Workspace rebuild — testing notes for the final review pass

These notes accumulate while the "everything is a pane" rebuild is implemented, one task at a
time. The independent tester at the end of the run takes this file as its brief: every section
lists what a task claims, how to verify it, and what looked fragile while it was built. Add a
section per task; never delete one.

Conventions for the tester:

- Work in the worktree `/home/coder/project/atlas/.claude/worktrees/everything-is-a-pane`
  (branch `devbench/atlas-financer-panes`). Build with the shared target dir and sysroot
  (`CARGO_TARGET_DIR=/home/coder/project/atlas/target`,
  `PKG_CONFIG_PATH=/home/coder/project/atlas/.sysroot/lib/pkgconfig`,
  `LIBRARY_PATH=/home/coder/project/atlas/.sysroot/lib`); the pure model crate can use
  `CARGO_TARGET_DIR=<worktree>/target-ws`. One cargo build at a time; 2 GiB box.
- Write review tests in new files (`crates/atlas-workspace/tests/review.rs`,
  `crates/atlas-app/tests/review.rs`) so they stay separate from the implementers' tests.
- Report defects with severity (blocker / should-fix / nit), file + function + input + expected
  vs actual. Small, obvious defects may be fixed directly with a regression test; anything
  bigger is reported.

---

## 1. Workspace data model — `crates/atlas-workspace` (commit 437fb7e)

**Claims.** Pure-data tree of splits (proportional weights summing to 1) and stacks (tabs);
transactional ops (`insert`, `remove`, `move_pane`, `stack`, `unstack`, `resize`,
`set_active`) that leave the layout byte-identical on any `Err`; one central `normalize`
(idempotent fixpoint: drop empty stacks, collapse one-child splits, flatten same-axis nesting
with weight scaling, renormalise weights, repair active pane); `validate()` with readable
`Violation`s; `LayoutHistory` snapshots; `ClosedPanes` + `reopen`; `persist` (schema version,
migration chain, `load` never fails — `Fresh`/`Loaded`/`Recovered`, `save_atomic` with `.bak`
and `.tmp` + rename, view-state scrubbing); `presets`; `SavedLayouts` store; `resolver`;
`focus` geometry; `JobBoard`; `grid` renderer. 79 tests.

**Verify.**

1. Layout algebra: default share 0.35 clamped 0.15..=0.85; sibling docking takes the share
   from the target child's weight only (`[B|C]` + A right of C → `wB, wC·(1−s), wC·s`);
   docking beside a same-axis split wraps then flattens (children scale by `1−s`, ratios kept);
   `remove` leaves unrelated subtrees byte-identical (ids and weights); `move_pane` to the
   current position → `OpError::NoOp`, layout unchanged; `SplitLimits` (`min_share` 0.08,
   `max_depth` 12) rejects with `TooSmall`/`TooDeep`.
2. Invariants: build bad trees by hand through JSON and check `validate()` catches each —
   duplicate pane ids, pane in two stacks/windows, definition without a tree entry and vice
   versa, empty stack, one-child split, bad weights (len mismatch, ≤ 0, sum ≠ 1), duplicate
   node ids, active pane not in its stack, same-axis nesting, missing `active_window`.
3. Pictures through ops only (`grid::render_numbered`, 3×3 unless noted): `123/123/124`,
   `123/144/144`, `23/23/44` (2 cols); presets Focus `111/111/111`, Compare `112/112/112`,
   Main+Inspector `1112/1112/1112` (4 cols), Analysis `112/113/113`, Review `122/133/133`.
4. Determinism: the same op sequence from `WorkspaceLayout::new` twice → identical JSON.
5. JSON: camelCase keys (`schemaVersion`, `workspaceId`, `activePaneId`, `viewState`); a
   hand-written layout in the plan's shape loads; a tree without node `id`s or `weights` loads
   and normalises.
6. Randomized: a second generator/seed set (e.g. 5 seeds × 1500 ops over every op incl.
   Beside-an-ancestor-split, WindowEdge, detach, cross-window move, undo/redo, reopen): after
   each op `validate()` empty, `normalize` unchanged, every `Err` leaves JSON identical, pane
   count == definitions count.
7. Persistence: corrupt file → `Recovered` with a `<path>.corrupt-*` copy holding the original
   bytes and `.bak` tried first; newer `schemaVersion` → reason names the version; `.bak` after
   the second save; nested directory created; a pre-existing garbage `.tmp` does not break a
   save; `scrub_view_state` drops keys containing token/secret/password/authorization/
   signedUrl/apiKey/credential/cookie and values > 16 KiB.
8. History: undo/redo of a move restores the exact JSON; `push` clears redo; limit evicts the
   oldest; `NoOp` results must not be pushed by callers (check the runtime honours this).
9. Closed panes: reopen → old stack if it exists, else beside the old neighbour on the recorded
   side, else the fallback; cap 20.
10. Saved layouts: save/list/get/rename/duplicate/delete; `save_as` → `NameTaken`;
    `WrongHousehold`; templates load anywhere; no `.tmp` left behind.
11. Resolver: exact kind+resource match (by JSON value; `None` ≠ `Some`) → `Focus`, prefer the
    active window; else `Create` in the active stack; empty window → `WindowEdge`;
    `NewInstance` always creates; `OpenRight/OpenBelow` → `Beside` the active stack;
    `OpenNewWindow` → `CreateWindow`.
12. Focus: in `123/123/124`: 2→Right = 3, 4→Left = 2, 3→Down = 4, 1→Left = None;
    `move_direction_target` at an edge → `WindowEdge`.
13. Jobs: start→progress→complete; fail(retryable)→retry = new Queued job, attempts+1, same
    title/source/associated; cancel refuses finished/non-cancellable; prune keeps newest;
    `percent` None without total.
14. Floating windows: detach → Floating window; its last pane leaving removes it; the main
    window's last pane leaving leaves `root: None` and the window present.
15. Code quality: no `unwrap()`/`expect()` in `src/` without a justifying comment; no
    milestone/plan codes in ordinary comments; `cargo doc` and `clippy -D warnings` clean;
    `#[serde(default)]` on optional fields so older files load.

**Known / fragile.** A flat `1|2|3` has no 2–3 sub-group to dock beneath (same-axis
flattening) — an explicit sibling-range target is planned for the ancestor-docking task; check
that it exists by the end and that `123/123/144` is reachable through it. `load()` tries
`<path>.bak` before falling back to a fresh default (a deliberate extension). `resize` returns
`Err(NoOp)` for unchanged weights.

---

## 2. Single-window workspace runtime — `crates/atlas-app/src/workspace/` (commits f5ce6bf, 1561f6e, fc95d07)

**Claims.** `WorkspaceView` is the window's content column once a household is usable; it owns
the `WorkspaceLayout` model, one `PaneView` entity per pane, gpui-kit's `DockArea` + `DockSkin`,
`LayoutHistory` and `ClosedPanes`. Every new pane, undo/redo, household start/clear and
`resize_split` rebuild the area from the model (`set_center`, slot px = weight × extent; pane
entities survive so scroll and in-pane history survive). Close and tab activation use native
engine calls. Every `DockEvent::LayoutChanged` is mirrored back (`dump` →
`mirror::layout_node_from_state` → `WindowLayout::replace_root` → `validate()`; a failure keeps
the previous model and reports through `alerting`). Echoes of our own edits (same structure,
weights within 0.005) push no history; genuine engine edits push "Rearrange panes"/"Resize
panes". `AtlasApp::navigate`/`go_back` act on the active pane; `route()` answers with the active
pane's route. `--open <slug>` / `--open +<slug>` open extra panes/tabs at launch.

**Verify.**

1. `tests/workspace.rs` (12) and `tests/ui.rs` (34) and `tests/copy.rs` pass; `mirror` and
   `kinds` unit tests pass (`cargo test -p atlas-app`).
2. Re-entrancy: pane/engine callbacks are deferred through the window handle; the workspace never
   calls the app while the app is updating. Try: navigate from a breadcrumb inside a pane, close
   the active pane from its tab, switch households while several panes are open — no panics
   ("already being updated"), no stale panes.
3. Household switch: new/open/sample start a fresh layout (one pane on the launch route);
   *Save as…* keeps the layout (`household_generation` unchanged). Closing the household shows the
   Welcome screen again, and reopening the sample gives a fresh single pane.
4. Two panes on the same register (Accounts | Accounts): they share selection/filter state
   (documented limitation) but have independent scroll and history.
5. Mirror correctness: build 0.65/0.35, render a frame, drag the divider, check the model
   weights follow within 0.02; undo restores the grid.
6. Menu items act on the pane whose menu was used (not the active pane).
7. Perf: the workspace view is the cached content view (`shell_reuses_cached_views_between_frames`);
   a hover in one pane must not re-render the sidebar.
8. Narrow panes: screens squash (per-character wrapping, rows below the fold) — fixed in task 3
   by a minimum content width with horizontal scroll; confirm the fix and that no screen is
   clipped at the default `--open accounts --open rules` widths.

**Known / fragile.** Engine node ids are never stored; everything maps by pane id. `PanelRegistry`
and `DockArea::load` are unused (restore goes model → `rebuild_area`). Pane in-pane histories
live only in `PaneView` (not in the model, so not persisted). `Resolution::CreateWindow` falls
back to the active stack until floating windows exist.

---

## 3. Transactional drag and dock — (commits a62fcc0, 506a189, 73183a7)

**Claims.** Tab/title drags are the dock engine's; the workspace subscribes to every tab group's
`TabGroupEvent::Drop` (`pane_joined_group`) and applies the drop to the **model first**
(`dock_pane` → `WorkspaceLayout::move_pane` with share 0.5), then `rebuild_area`. `Err(NoOp)`
(dropped back where it was) records no history; `TooSmall`/`TooDeep` refuse with the toast
`REFUSED_SPLIT_MESSAGE` (`workspace-drop-refused`) and put the area back. `Escape` during any drag
ends it through a keystroke interceptor with nothing changed. `mirror_from_area` classifies
engine edits: structural → "Move pane" (after `ops::check_limits`), weights only → "Resize
split" coalesced per split, active tab only → no history. Model rule (`slot_handover`): a pane
alone in its stack moved beside a node inside a former sibling hands its freed weight to that
sibling, so the other siblings keep their exact widths. `PaneView` body: `MIN_CONTENT_WIDTH`
1080 px with sideways scroll. `gpui-shot --step drag:X1,Y1,X2,Y2` / `release`.

**Verify.**

1. `tests/workspace.rs` drag tests (centre → tabs; edge → split + undo exact; below → wraps in a
   column and leaves the rest alone; Escape → byte-identical; drop back → nothing recorded;
   divider run → one undo step; refused drop → toast, JSON identical; narrow pane → min width
   and sideways scroll).
2. Try drags the tests do not: a tab out of a 3-tab stack onto its own stack's edge (should
   split, leaving 2 tabs); the last tab of a stack onto a far pane (source stack collapses,
   target divided); a drop on the same edge the pane already sits on (NoOp, no history).
3. Confirm no double handling: after a drop, pane count is unchanged and the grid matches the
   model (the engine's own move and the model's rebuild happen inside one event flush).
4. The interceptor: Escape inside a text input while *not* dragging must still reach the input.
5. `check_limits` on an engine rearrangement: tighten limits, drag to split a narrow pane → toast
   and the previous model kept.
6. Sideways scroll: `pane-body` scrolls horizontally when a pane is narrower than 1080 px; a wide
   pane fills its width; the vertical scroll still works inside; wheel scrolls vertically.

**Known / fragile.** `DROP_SHARE` is 0.5 (matches the engine's indicator) while command splits use
the model default 0.35. Autoscroll of an overflowing tab bar during a drag is not implemented (the
skin does not expose the tab strip's scroll handle). The gpui-shot drag step holds the button
until `release`; a capture without release photographs the in-flight state.

---

## 4. Ancestor docking — (commits ddad448, 86e373b)

**Claims.** Model: `DockTarget::BesideRange { split, from, to, side, share }` groups children
`from..=to` of a split (perpendicular to `side`) before docking beside the group;
`simplified()` reduces a one-child range to `Beside` the child and a full range to `Beside` the
split; `range_after_removal` re-aims a range whose split loses the moved pane's own slot;
`ops::ancestor_targets(root, pane, side)` lists the levels narrow → broad (stack, sibling runs
growing to the first then to the last child, ancestors, window edge) without duplicates. App:
`dock_targets.rs` computes `Band`s for the pane under the pointer from the panes' recorded
bounds (`PaneBounds`, written by a `canvas` in every pane; only displayed panes are trusted):
the window edge is always the outermost band, at most 3 per side, `Space` cycles the inner
levels; each band's `Outcome` is decided by `move_pane` on a clone (Allowed + preview rect /
Unchanged / Refused); the view draws bands, the hovered band's preview (`dock-preview`) and a
label (`dock-band-label`, aria-labelled), and a drop on a band goes through `dock_pane`. Tabs
now render the pane's `title()` element (`tab_name` is `None`).

**Verify.**

1. `tests/workspace.rs`: `123/123/144` via band depth 1 (Today's weight unchanged), window band →
   `123/123/444` (columns keep their ratio), the group band divides only the group's slot,
   `123/144/144` via drag + resize, preview + Escape leaves JSON identical, Space cycling with the
   window band fixed, refused band takes no drop.
2. Model unit tests in `ops.rs` (`a_range_target_spans_exactly_those_siblings`, refusal
   unchanged, retargeting after removal, `ancestor_targets` lists) and the randomized run
   (targets include deliberately invalid ranges).
3. Try: a range on a *vertical* split (rows) docked left/right; a range in a nested split (not the
   root) — `1 | V[2,3,4]` with 5 dragged to the right band of "3–4"; dropping on a band while the
   dragged pane is the hovered pane itself (its own bands); a drop on a band when the pane is
   alone in its column and the range includes that column (the range shrinks).
4. Double handling: after a band drop, pane count and grid match the model; no engine drop is
   also applied (the band takes `active_drag`, `cx.stop_propagation()`).
5. Stale bounds: hide a pane behind a tab, drag over the stack — bands belong to the displayed
   pane, never to the hidden one.
6. Label positions near the window's bottom/right edge stay on screen.

**Known / fragile.** The engine's own drop indicator (a pane's edge zone) draws under the bands
at the same time; the band wins the drop. Band thickness 14 px × up to 3 levels is a small
target on a trackpad — consider a modifier to widen. `hovered_pane` sorts hits by id when
rectangles overlap (they should not).

---

## 5. Screens as pane types — (commit b03218c)

**Claims.** `kinds.rs` is the registry: `definition_of`/`route_of` (kind = route slug, resource
= record id), `title_of`/`context_of`/`icon_of`, `availability(route, household, viewer)` (missing
or hidden record → reason), `view_state_of`/`history_from` (Back history as definitions),
`label_of_unknown_kind`. `WorkspaceView::sync_pane_definition` rewrites the model's definition
after every in-pane navigation (`navigate_active`, `back_active`, menu Back, viewer reset).
`sync_pane_entities` rebuilds a missing pane view from its definition (history restored), drops a
view whose definition no longer matches, and gives an unknown kind `PaneView::unsupported`.
`PaneView` renders `pane-unsupported` / `pane-unavailable` placeholders with `pane-replace`
(menu of destinations → `replace_pane_content`) and `pane-close`. `load_layout(layout, label)`
installs a layout as one undo step. `pane_left` carries the view's `EntityId` so a replaced
view's removal notice is ignored.

**Verify.**

1. `tests/workspace.rs`: `a_pane_that_drills_into_a_record_is_found_by_the_resolver`,
   `back_history_survives_a_closed_pane_being_restored`,
   `an_unknown_pane_kind_shows_a_placeholder_that_can_be_replaced_or_closed`,
   `a_missing_record_shows_the_unavailable_placeholder`; `kinds` unit tests.
2. Two panes drilled into different accounts: both definitions distinct; `open(Account(x))`
   focuses the right one; a plain `open` with no exact match adds a tab to the active stack.
3. Viewer change: every pane resets to its destination's home and the model definitions follow
   (`reset_panes_for_viewer`); Back history is cleared (it may name hidden records).
4. The placeholder fits a narrow pane (no 1080 px minimum for placeholders); its buttons are
   reachable without sideways scrolling.
5. A placeholder pane is a normal pane otherwise: it can be dragged, split, closed from its tab,
   and appears in the grid.

**Known / fragile.** Scroll positions are not part of the view state. `availability` for rules
checks existence only (rules carry no disclosure). Detail screens also have their own "not
available to this viewer" copy for the in-screen case; the pane-level placeholder takes
precedence because it is checked before the screen renders.

---

## 6. Header and pane launcher — (commits 73c9c6b, de7428f)

**Claims.** `workspace/launcher.rs`: `LauncherConfig { pinned, hidden }` (slugs of home routes)
loaded from `<data_dir>/launcher.json`, repaired on load (unknown slugs dropped, missing screens
appended), saved atomically on every change; `LauncherView` is a cached view (snapshot =
active destination + config) with `launcher-<slug>` buttons inside `launcher-item-<slug>` drag
handles (`AnyDrag(LaunchDrag { route, destination })`), `launcher-add` (+ menu) and
`launcher-more` (… menu: hidden screens, pin, restore). Right-click menu is launcher-owned
(`OpenMenu`, dismissed → dropped) because gpui-kit's `context_menu` leaks its entity through an
`Rc` cycle. `WorkspaceView::item_dropped_on_dock` turns `DockEvent::DragDrop` into a new pane;
the bands accept `AnyDrag` too (`Dragged::New`); `reset_layout` is one undo step. Shell: no
sidebar; title bar leading `app · household ▾ · sample · Layout ▾`, trailing `file state · Save ·
viewer · meanings · theme · settings`; the workspace uses the full width. `Launch.data_dir`
(`--data-dir`, `ATLAS_DATA_DIR`, else the per-user app dir; `None` for launches built in code).

**Verify.**

1. `tests/workspace.rs`: Shift-click instance, menu → Open right, launcher drag onto a pane edge
   and onto the window band, reorder/unpin/pin/restore + file round-trip, Settings + Reset +
   undo; `tests/ui.rs`: `launcher_and_workspace_tabs_navigate`,
   `shell_reuses_cached_views_between_frames` (launcher cached, hover re-renders only it).
2. Semantics: a launcher click never navigates a pane in place (the old sidebar did) — it focuses
   or opens. Check with two panes: click the screen the *inactive* pane shows → it becomes
   active, no new pane.
3. Right-click menu: every item's action, focus returns after dismiss, Escape closes it, no
   leaked `PopupMenu` at test exit (the harness checks).
4. `launcher.json`: corrupt file → default + warning; a slug from a future version → dropped;
   `--data-dir` pointing at a read-only location → save warns, app keeps running.
5. Drag from the launcher onto the *launcher* item itself (no-op), onto the `+`/`…` buttons
   (nothing), release over the title bar (nothing, overlay gone).
6. Narrow window: the strip has no overflow measurement; items past the window's width are
   clipped rather than moved into `…` (known limitation — unpin to make room).

**Known / fragile.** The strip's `…` menu holds only the unpinned screens; automatic overflow by
width is not implemented. Icons on tabs and launcher come from the full Lucide set
(`AllAssets`), so the test-support windows load them too.
