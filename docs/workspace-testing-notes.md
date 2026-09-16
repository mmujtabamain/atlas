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

---

## 7. Background jobs — (commit fe1315c)

**Claims.** `workspace/jobs.rs`: `JobCenter` (an entity owned by `AtlasApp`, `app.jobs()`) over
the model's `JobBoard`; `start(title, source, associated route, cancellable, runner)` returns a
`JobTicket { id, cancelled flag }`; `progress/complete/fail/cancel/retry/clear_finished`;
`fail` reports through `alerting`; every end emits `JobEvent::Finished` which the shell turns
into a toast (summary / "<title> failed: <error>" / cancelled). The shell's `jobs` button
appears only while jobs exist, labelled `N running` / `N failed` / `Done`, and its menu lists
jobs (click → `workspace.open(associated route)`), `Retry: …`, `Cancel: …`, `Clear finished
jobs`. The save (`lifecycle.rs`) is a job with a retry runner; `run_sensitivity` computes
`AssumptionsModel` in `background_spawn`, installs it through `Lazy::set` unless
`app.edits` moved, ignores a second run while one is running, and the Run button reads
"Recomputing…" meanwhile (`sensitivity_running`).

**Verify.**

1. `tests/workspace.rs`: `a_sensitivity_run_outlives_the_pane_that_started_it`,
   `a_second_run_while_one_is_running_is_ignored_and_an_edit_supersedes_the_result`;
   `jobs.rs` unit tests (completion, retry through the runner, cancel flag, clear).
2. Save job: with a real household file (`real_data_new_household_entry_save_and_reopen`
   drives one), the jobs list shows "Save to <path> · done"; make the path unwritable and check
   the failure toast, the alerting report and that *Retry* re-runs the save.
3. Viewer/household change while a sensitivity job runs: the result must not be installed for
   the wrong household (the `edits` guard covers edits; check `replace_household` bumps `edits`
   or otherwise invalidates — if not, this is a gap).
4. Cancel: no current job is cancellable (both are short); the flag path is unit-tested only.
5. The indicator's menu entry is disabled for a job without an associated screen (the save).

**Known / fragile.** `KEEP_FINISHED` is 8; `finished()` prunes, so a job may vanish from the
list right after finishing if many finished ones are kept. Progress is a single "computing"
step for the sensitivity job (no sub-steps). The jobs menu re-reads the centre when opened; it
does not live-update while open.

---

## 8. Floating windows — (commit d3c2c83)

**Claims.** `WorkspaceView` owns the model, pane entities and history for every window;
`floating: HashMap<WindowId, FloatingWindow { handle, view, area }>`. `FloatingView`
(`workspace/floating.rs`) is a floating window's root: slim title bar (`floating-gather` button),
its own `DockArea`, the same drag handlers/actions, Root layers; its render asks the workspace
for the overlay (`render_drag_overlay_for(window, origin, size)`). `rebuild_area` rebuilds every
window's area (`in_window` runs a closure in the right gpui window) and closes/opens windows the
model dropped/added; `rebuild_window` rebuilds one through its handle. `detach_pane`,
`open_in_new_window`, `move_pane_to_window`, `gather_window`, `floating_window_closed` (close
box → panes home), `drag_released_outside` (release outside the viewport → detach at the screen
point), `command_detach` (`cmd/ctrl-shift-n`), pane menu items. Windows open from a deferred
app-level step (`open_floating_window` → `register_floating_window`), because `open_window`
draws the first frame synchronously and that frame reads the workspace; the area subscription
is made at App level for the same reason. `expected_removals` tells a rebuild's removals from
closes (`pane_left`). Frames are clamped with `WindowFrame::clamped_to_displays`.

**Verify.**

1. `tests/workspace.rs`: `a_pane_moves_into_a_window_of_its_own_and_back`,
   `a_screen_opens_in_a_new_window_and_the_close_box_sends_panes_home`,
   `releasing_a_drag_outside_the_window_opens_a_window_for_the_pane`; model unit test
   `a_frame_is_clamped_onto_the_display_it_overlaps_most`.
2. Inside a floating window: split right/below, tab drag between two panes there, docking bands
   (their coordinates are that window's), a launcher drag *cannot* reach it (no launcher there —
   expected), keyboard commands, a dialog opened by a screen appears in that window.
3. Drop-outside from a *floating* window makes a third window; the emptied second window closes.
4. Undo/redo across windows: undo a detach (window closes), redo (window reopens with the pane).
5. Household switch / close with floating windows open: they must close (the model is replaced —
   check `clear_for_closed_household`/`start_household` call `rebuild_area` so
   `close_vanished_windows` runs).
6. The close box path with an *empty* floating window (should not exist, but if it does, the
   window is dropped from the model and closed).
7. Two floating windows: `Move to the main window` from each; `floating_windows()` order.

**Known / fragile.** Focus after a cross-window move goes to the pane's new window
(`focus_active` → `in_window`), which may raise that window on some platforms. The floating
window has no launcher; opening a screen there goes through the pane menu (Open right/below on a
pane already there) or a drag from the main window's launcher is not possible. Window frames in
the model are updated only when a window is opened (moves/resizes by the person are not tracked
yet — persistence will record them on save).

---

## 9. Layout persistence — (commit cbcd367)

**Claims.** `workspace/session.rs`: `SessionStore::for_household(data_dir, identity)` →
`sessions/<slug>.json`; `load()` (Fresh / Loaded / Recovered, foreign scope → Fresh);
`write()` scrubs then `save_atomic` (+ `.bak`). `WorkspaceView::touch(reason, cx)` marks dirty
and starts one timer (`session::DEBOUNCE` 750 ms) whose end calls `flush_session` — so a burst
of changes is one write, ~750 ms after the *first* change; `record`, `mirror_from_area`,
`sync_pane_definition` and `set_active_pane` all touch. `flush_session` also runs on household
start/close and on the main window's close box; it notes every window's frame first.
`restore_session` skips when `Launch.explicit_screen` (`--screen`/`--open`); `Loaded` →
`install_layout` (placeholders for unknown kinds, floating windows opened by
`close_vanished_windows`); `Recovered` → fresh + deferred warning toast + alerting.

**Verify.**

1. `tests/workspace.rs`: `the_workspace_is_written_after_a_change_and_restored_for_the_household`,
   `a_corrupt_session_starts_fresh_keeps_the_file_and_says_so`,
   `a_launch_that_names_a_screen_does_not_restore_the_session`,
   `floating_windows_come_back_with_the_session`; `session.rs` unit test.
2. A session written by a newer `schemaVersion` → Recovered with the version in the reason.
3. A session whose pane names a deleted record → the unavailable placeholder, rest intact.
4. Household A's session is never applied to household B (scope check) — open two real files.
5. Data dir unwritable → warning through alerting, app keeps running.
6. Kill the app mid-write: `.tmp` may remain; the next start reads the last complete file.
7. The main window's frame is noted but not applied on restore (the window exists before the
   workspace); floating frames are.
8. Debounce semantics: a change 5 s after the previous write → written ~750 ms later; ten
   divider drags in 500 ms → one write.

**Known / fragile.** Writes happen ~750 ms after the first change, not the last; `Autosave::due`
is not consulted (real-time clocks and the test executor's fake clock disagree). The
`explicit_screen` flag is set by `Launch::parse` only; tests set it directly.

---

## 10. Saved layouts, templates and presets — (commits 87d6bb2, 2d7b534)

**Claims.** Model: `presets.rs` — `Preset::{Focus, Compare, MainInspector, Analysis, Review}`
(`all()`, `label()`, `description()`, `template()`), `LayoutTemplate::arrange(existing, ids,
is_kind)` fills slots by kind first, then by position, and mints new panes for slots nothing
matches; `WorkspaceLayout::apply_template(window, template, is_kind)` (existing panes keep their
ids), `import_window(source, window, frame)` (re-minted ids, the window appended as floating).
`saved.rs` — `SavedLayouts` under `<data dir>/layouts/` (`layout-NNNN.json` /
`template-NNNN.json`, `SavedKind` orders layouts before templates). App: `workspace/layouts.rs`
— `LayoutStore { dir, store, current }`, `WorkspaceView::{saved_layouts, current_layout_name,
can_save_layouts, save_layout, prompt_save_layout_as(as_template), save_layout_as,
save_template, load_saved_layout, open_saved_layout_in_new_window, apply_preset,
apply_template, rename/duplicate/delete_saved_layout, open_manage_layouts}`. The Layout menu
(`layout-menu`, label `Layout: <name>` while a saved layout is on show): Save layout / Save
layout as… / Save as template… / one entry per saved item (Load X, Arrange as X) / Manage
layouts… / the five presets / Undo / Redo / Reset layout; without a data directory the save
entries are absent. Loading and arranging are one history step each (`Load layout X`, `Arrange
as X`); a duplicate name is refused with a toast; `reset_layout` clears the current name.

A screen family's retained controls (search fields, filters, tables) are one set on `AtlasApp`,
and one set cannot be drawn in two gpui windows at once (each window invalidates the other's
frame — an endless ping-pong). `WorkspaceView::shown_elsewhere(pane, route)` therefore gives
the *displayed* pane of a family to one window — the main window over any floating one, an
earlier floating window over a later one — and the losing pane renders `pane-elsewhere`
("<Family> is on show in the main window", `pane-show-elsewhere` → `show_pane_in_its_window`,
`pane-close`). Two panes of one family in *one* window are fine. `apply_active_flags` notifies
every pane on a layout change so the placeholder appears and disappears without other input.

**Verify.**

1. `tests/workspace.rs`: `a_layout_is_saved_loaded_back_and_managed`,
   `a_preset_arranges_the_open_panes_and_a_template_adds_the_screens_it_names`,
   `a_saved_layout_naming_a_deleted_record_loads_with_a_placeholder_in_its_place`,
   `a_screen_shown_in_two_windows_yields_to_the_main_window`; model tests in `presets.rs`.
2. Save as… with an empty name (the dialog refuses), with the name of an existing entry (toast
   "already exists", nothing written), with a name that needs escaping in JSON.
3. Save a layout while a floating window is open → both windows are in the entry; Load it with
   no floating window open → the window opens; Load with a *different* floating window open →
   that one closes (the model is replaced).
4. Save a layout in household A, open household B → the entry is not offered (scope), Manage
   shows only B's.
5. Templates across households: a template saved in A, arranged in B → the same shape, B's
   screens; a slot naming a screen the viewer cannot see (Sharing for a non-owner) → the
   unavailable placeholder.
6. Delete the layout on show → the title button drops the name; Rename it → the button follows.
7. Open a saved layout in a new window whose panes include a family the main window shows →
   the floating pane is the `pane-elsewhere` placeholder; Close pane there; Show it there raises
   the main window and activates its pane; close the main window's pane → the floating one
   shows the screen again.
8. Presets with fewer panes than slots (Review with one pane open) → the missing slots are new
   Today panes? — no: slots without a kind take the panes in order and open nothing; check that
   `Arrange as Review` with one pane gives one pane and a `NoOp`-free history entry.
9. Manage → Duplicate → the copy is "<name> copy"; Duplicate twice → "<name> copy 2".
10. The layouts directory unwritable → the save entries stay, saving reports through alerting.

**Known / fragile.** Menu items are addressed by index in tests (gpui-component gives them no
ids); the reset test counts on the menu shape without saved entries. `open_saved_layout_in_new_window`
opens the saved layout's *main* window as a floating window; its floating windows are not
imported. The one-family-per-window rule is the honest limit of app-owned controls; per-pane
controls would lift it.

---

## 11. Workspace commands — (commit e5d7eb5)

**Claims.** `workspace/commands.rs` declares every action and binds it once under the
`Workspace` key context; `commands::attach(element, workspace)` puts all handlers on a window's
root, used by the main window (`WorkspaceView::render`) and by `FloatingView`. Keys (`cmd` on
macOS, `ctrl` elsewhere): `-\` split right, `-shift-\` split below, `-shift-d` duplicate, `-w`
close, `-shift-t` reopen, `-alt-←↑→↓` focus by direction, `-alt-]`/`[` next/previous,
`-alt-shift-←↑→↓` move by direction, `-shift-enter` zoom, `-[`/`alt-left` back, `-shift-n` float,
`-z`/`-shift-z` (`ctrl-y`) undo/redo. View methods: `focus_direction` (`focus::neighbour`),
`focus_previous_pane`, `move_active` (`focus::move_direction_target` → `dock_pane`, so a move is
one `Move pane` history step, refused like a drop when too small), `duplicate_pane` (model
`duplicate_pane` beside right, definition synced first so the copy has the state; `Duplicate
<title>`), `reopen_last_closed` / `reopen_closed_at(index)` (`ClosedPanes::pop`/`take` →
`WorkspaceLayout::reopen`; a refused reopen is put back on the list; `Reopen <title>`),
`recently_closed(cx)` (newest first, titles), `toggle_zoom` (engine `TabGroup::toggle_zoom`;
no history, no model change), `is_zoomed(window)`. Menus: Layout → `Reopen <last closed>` /
`Zoom the active pane` ↔ `Back from zoom`; launcher `+` → "Recently closed" section (5 newest);
pane menu → Duplicate. `reset_layout` records the panes it drops as closed.

**Verify.**

1. `tests/workspace.rs`: `the_keyboard_moves_the_focus_between_panes_by_direction`,
   `the_keyboard_moves_the_active_pane_and_undo_brings_it_back`,
   `a_closed_pane_is_reopened_where_it_was_by_key_and_from_the_add_menu`,
   `the_active_pane_zooms_to_the_window_and_comes_back`,
   `a_pane_is_duplicated_beside_itself_with_its_state`, and the reset test's recently-closed
   assertion.
2. Keys while a text field has the focus: `ctrl-z` in the Accounts search field must undo the
   text, not the layout (the field's own binding wins); `ctrl-w` there still closes the pane.
3. Keys in a floating window: every command acts on that window's active pane; `ctrl-alt-right`
   never crosses into another window.
4. Focus by direction on a grid with unequal edges (`123 / 123 / 124`): right from 2 → 3, left
   from 4 → 2, down from 3 → 4.
5. Move by direction against the minimum size (`SplitLimits::min_share`): refused with the same
   toast as a drop; move into a tab stack is never refused.
6. Reopen after the old neighbour was itself closed → the pane lands at the active stack's
   default target; reopen when the list is empty → info toast; reopen of a pane whose record was
   deleted meanwhile → the unavailable placeholder.
7. Zoom, then any layout command (split, close, drop, undo): the zoom ends with the rebuild.
   Zoom in a floating window; zoom then close the zoomed pane.
8. Duplicate a placeholder pane (unsupported/unavailable): refused or a second placeholder —
   check which and that nothing panics. Duplicate a pane with Back history: the copy's Back works.
9. Undo/redo keys during a drag in flight: the drag overlay must end first (`end_drag_overlay`
   runs on `dock_pane`; undo does not — check no stale bands remain).

**Known / fragile.** Menu entries are index-addressed in tests: the Layout menu without saved
layouts is presets ×5, ─, Undo, Redo, Reopen, Zoom, ─, Reset (index 11); the `+` menu is 9
destinations, ─, heading, then the closed panes. Zoom is engine state, so it does not survive
a session restore or a layout change. The grid helper's digits are reading-order positions,
not pane numbers.
