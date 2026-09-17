//! `WorkspaceView`: the content column of the window, as a workspace of panes.
//!
//! Two representations of the same layout live here, with one rule between
//! them: the model ([`WorkspaceLayout`]) is the truth about structure,
//! history and (later) persistence; gpui-kit's [`DockArea`] is the live
//! projection the person sees and drags. Every command applies to the model
//! first and then to the area; every edit the engine makes on its own — a
//! divider dragged, a tab chosen, a tab's close button — is mirrored back
//! into the model from `area.dump(cx)`, and the model is validated before it
//! is accepted (an invalid mirror is reported and the previous model kept).
//!
//! | model change | how the area follows |
//! |---|---|
//! | a pane opened (`open`, `split_*`), a layout installed (undo, redo, a new household) | [`WorkspaceView::rebuild_area`]: `set_center` with the model's tree, slot sizes = weights × the area's extent; pane entities survive, so their scroll and history do |
//! | a pane closed | `DockArea::remove_panel` — the engine collapses the emptied group exactly as the model did |
//! | the active pane changed | the pane's tab is selected in its group (`TabGroup::select_tab`); the pane is focused; the launcher follows through `AtlasApp::set_route_for_chrome` |
//! | a split resized from the model | `rebuild_area` (the engine has no public "set these sizes") |
//!
//! | engine change | how the model follows |
//! |---|---|
//! | a pane dropped on a tab group (`TabGroupEvent::Drop`, heard through [`WorkspaceView::pane_joined_group`]) | [`WorkspaceView::dock_pane`]: the drop as a [`DockTarget`] → `WorkspaceLayout::move_pane` (transactional: the minimum-size rule, "nothing changed") → `rebuild_area`; a refused drop puts the area back and says why in a toast |
//! | `DockEvent::LayoutChanged` | [`WorkspaceView::mirror_from_area`]: dump → [`LayoutNode`] → `WindowLayout::replace_root` → validate; a newly displayed tab becomes the active pane; a divider drag is "Resize split" (consecutive drags of one divider share the entry); an echo of the workspace's own edit changes nothing; a rearrangement the drop path did not see is checked against the limits and taken as "Move pane" |
//! | a pane told `on_removed` | [`WorkspaceView::pane_left`]: closed in the model too, unless the model already closed it |
//!
//! Dragging is the engine's: it starts a drag from a tab or a single pane's
//! title (gpui's own 2 px threshold keeps a click from becoming one), draws
//! the drop indicator over the centre and the four edge zones, and commits
//! nothing until the drop. `Escape` during a drag ends it before any of that
//! happens (a keystroke interceptor installed in [`WorkspaceView::new`]).
//! The broader targets — beside a whole group, beside a run of siblings, along
//! a window edge — are the workspace's own: while a pane is dragged it draws
//! the bands of [`super::dock_targets`] over the pane under the pointer, with
//! the rectangle the pane would take, and a drop on a band goes through
//! [`WorkspaceView::dock_pane`] like any other. `Space` cycles the levels a
//! side offers when there are more than fit.
//!
//! A pane's definition in the model follows what the pane shows: every
//! in-pane navigation ([`WorkspaceView::navigate_active`], Back, a viewer
//! reset) ends in [`WorkspaceView::sync_pane_definition`], which writes the
//! pane's current route and Back history into the model. That is what a saved
//! layout reopens on and what the resolver's "is this already open?" reads.
//! A definition this build cannot show is kept and shown as a placeholder
//! (see [`super::pane`]) rather than dropped.
//!
//! The workspace is kept: every change marks the household's session dirty
//! and, a moment after the last change, the model is written to the session
//! file ([`super::session`]); when the household opens again the session is
//! read back and installed, windows included, unless the launch asked for a
//! particular screen. A session that cannot be read starts the workspace
//! fresh and says so.
//!
//! There is one workspace and any number of windows. This view is the main
//! window's content column and the owner of everything shared — the model,
//! the pane views, history, the closed-pane stack — and every other window
//! is a [`super::floating::FloatingView`] with a dock area of its own over
//! the model's tree for that window. Each area is rebuilt from its window's
//! tree and mirrored back into it; a pane moves between windows by a model
//! operation followed by a rebuild of both areas, and the pane entity itself
//! never changes. A floating window closes when its last pane leaves; closing
//! it from its own close button sends its panes back to the main window.
//!
//! The workspace follows the app rather than being told: it observes
//! [`AtlasApp`] and starts a fresh layout, scoped to the household, the
//! moment a household is open **and** someone has said who is looking; it
//! empties itself when the household closes and resets every pane to its
//! destination's home screen when the viewer changes.
//!
//! One rule keeps gpui happy: nothing here calls into the app while the app is
//! being updated. The two methods the app calls — [`WorkspaceView::navigate_active`]
//! and [`WorkspaceView::back_active`] — touch only panes; everything the panes
//! and the engine report arrives deferred (see [`super::pane`]).

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use atlas_core::authz::Viewer;
use atlas_workspace::resolver::{self, Intent, Resolution};
use atlas_workspace::{Axis, ClosedPane, ClosedPanes, DockTarget, LayoutHistory, LayoutNode, NodeId, OpError, PaneDefinition, PaneId, Scope, Side, SplitLimits, WindowId, WorkspaceLayout, ops};
use gpui_kit::assets::IconName;
use gpui_kit::component::dock::{DockArea, DockEvent, DockLayout, InsertTarget, PanelId, TabGroup, TabGroupEvent, panel_handle};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::notification::{Notification, NotificationType};
use gpui_kit::component::{
    ActiveTheme as _, Icon, Placement, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    v_flex,
};
use gpui_kit::*;

use std::cell::{Cell, RefCell};

use super::commands;
use atlas_workspace::focus::{self, Direction};
use super::dock_targets::{self, DragInFlight, Dragged, Field, Outcome, Zone, ZoneKind};
use super::floating::FloatingView;
use super::session::{self, SessionStore};
use super::skin::WorkspaceSkin;
use atlas_workspace::persist::LoadOutcome;
use atlas_workspace::{WindowFrame, WindowLayout, WindowRole};
use super::kinds;
use super::launcher::LaunchDrag;
use super::mirror;
use super::pane::{PaneBounds, PaneView};
use gpui_kit::component::dock::{AnyDrag, DragPanel, DropTarget};
use crate::alerting::{self, Level};
use crate::app::AtlasApp;
use crate::launch::Launch;
use crate::nav::{Destination, Route};

/// Weights that agree within this much are the same layout: the engine
/// measures slots in whole pixels, and a few pixels of a window are noise.
const WEIGHT_TOLERANCE: f64 = 0.005;

/// The share a dropped pane takes of the slot it lands beside: half, which
/// is what the engine's drop indicator shows while the drag is in flight.
const DROP_SHARE: f64 = 0.5;

/// The history label of a pane moved by dragging (or by any rearrangement the
/// engine made on its own).
const MOVE_LABEL: &str = "Move pane";

/// The history label of a divider dragged; consecutive drags of the same
/// divider share one entry.
const RESIZE_LABEL: &str = "Resize split";

/// What the person reads when a drop would split a pane below the minimum size.
pub const REFUSED_SPLIT_MESSAGE: &str = "Not enough room to split here. Drop onto the pane's tabs instead.";

/// The element id of the refusal toast's text, for tests.
pub const REFUSED_SPLIT_TOAST_ID: &str = "workspace-drop-refused";

/// What the workspace watches on the app: whether there is a household to
/// show panes for, which one, and who is looking.
#[derive(Clone, Debug, PartialEq, Eq)]
struct HouseholdSnapshot {
    /// A household is open and the viewer is chosen: panes may show it.
    usable: bool,
    /// Which household: bumped when one replaces another, not when it is saved
    /// under a new name — saving must not reset the layout.
    generation: u64,
    viewer: Viewer,
}

impl HouseholdSnapshot {
    fn of(app: &AtlasApp) -> Self {
        HouseholdSnapshot { usable: app.is_opened() && !app.viewer_pending(), generation: app.household_generation(), viewer: app.viewer() }
    }
}

/// The panes `--open` asked for, opened with the first usable household.
struct LaunchPanes {
    extra: Vec<Route>,
    stacked: Vec<(usize, Route)>,
}

/// The content column: one window's workspace of panes.
pub struct WorkspaceView {
    pub(crate) app: Entity<AtlasApp>,
    window: AnyWindowHandle,
    pub(crate) layout: WorkspaceLayout,
    panes: HashMap<PaneId, Entity<PaneView>>,
    area: Entity<DockArea>,
    skin: Rc<WorkspaceSkin>,
    history: LayoutHistory,
    closed: ClosedPanes,
    seen: HouseholdSnapshot,
    launch_panes: Option<LaunchPanes>,
    focus_handle: FocusHandle,
    /// Renders since creation — how tests see that a frame reused the cache.
    renders: u64,
    /// The tab groups whose drops are listened to, by the group entity's id.
    /// Groups are the engine's and come and go with every rebuild; a pane
    /// reports the group it joined and the entry is made then, and entries
    /// whose group is gone are dropped the next time one is made.
    group_watches: HashMap<EntityId, (WeakEntity<TabGroup>, Subscription)>,
    /// The split whose divider the last history entry recorded, while no
    /// other change has happened since: another drag of the same divider
    /// then joins that entry instead of adding one.
    last_resized_split: Option<NodeId>,
    /// Where every drawn pane is, written by the panes each frame and read by
    /// the drag-target overlay.
    pane_bounds: PaneBounds,
    /// Where this view was drawn, so window coordinates can be made relative.
    root_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// The pane drag in flight, while one is: what the overlay follows.
    drag: Option<DragInFlight>,
    /// The floating windows, by the model's id of each.
    floating: HashMap<WindowId, FloatingWindow>,
    /// Panes a rebuild took out of one window's area because the model moved
    /// them elsewhere: the engine's "removed" notice for them is not a close.
    expected_removals: HashSet<PaneId>,
    /// The household's session file and the autosave policy over it.
    session: SessionStore,
    /// Where the app keeps its files; `None` keeps nothing.
    data_dir: Option<std::path::PathBuf>,
    /// The launch named a screen: the session is not restored over it.
    explicit_screen: bool,
    /// The saved layouts and templates, and which one is on show.
    pub(crate) layouts: super::layouts::LayoutStore,
    _subscriptions: Vec<Subscription>,
}

/// One floating window: the gpui window, its root view and its dock area.
struct FloatingWindow {
    handle: AnyWindowHandle,
    view: Entity<FloatingView>,
    area: Entity<DockArea>,
    skin: Rc<WorkspaceSkin>,
    _subscriptions: Vec<Subscription>,
}

/// The size a detached pane's window opens at.
const FLOATING_WINDOW_SIZE: Size<Pixels> = size(px(960.), px(720.));

impl WorkspaceView {
    /// The workspace for `app`'s window. Starts empty, and with the household
    /// the launch opened (if it is usable already) the moment it exists.
    pub fn new(app: Entity<AtlasApp>, launch: &Launch, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (area, skin) = WorkspaceSkin::dock_area("atlas-workspace", None, window, cx);
        let area_events = cx.subscribe_in(&area, window, |this, _, event: &DockEvent, window, cx| match event {
            DockEvent::LayoutChanged => this.mirror_from_area(&WindowId::main(), window, cx),
            DockEvent::DragDrop { item, target } => this.item_dropped_on_dock(item, target, window, cx),
        });
        let app_changes = cx.observe_in(&app, window, |this, _, window, cx| this.follow_household(window, cx));
        // Escape while a pane (or a divider) is being dragged ends the drag
        // with nothing changed. An interceptor rather than a binding: gpui
        // dispatches key bindings before key-down listeners, so a control
        // inside the pane that binds Escape (an input) would otherwise take
        // the key first; the interceptor runs before either and only ever
        // acts while a drag is in flight.
        // `Space` while dragging shows the next levels of docking targets on
        // every side of the pane under the pointer.
        let this_for_keys = cx.weak_entity();
        let escape_cancels_drag = cx.intercept_keystrokes(move |event, window, cx| {
            if !cx.has_active_drag() || event.keystroke.modifiers.number_of_modifiers() != 0 {
                return;
            }
            match event.keystroke.key.as_str() {
                "escape" => {
                    cx.stop_active_drag(window);
                    cx.stop_propagation();
                    let _ = this_for_keys.update(cx, |this, cx| this.end_drag_overlay(cx));
                    log::info!("workspace: drag cancelled with Escape; nothing changed");
                }
                "space" => {
                    let _ = this_for_keys.update(cx, |this, cx| {
                        if let Some(drag) = this.drag.as_mut() {
                            drag.level_offset += 1;
                            log::info!("workspace: docking levels cycled to offset {}", drag.level_offset);
                            cx.notify();
                        }
                    });
                    // After a key press gpui counts the pointer as away until
                    // the mouse moves again (hover is for the mouse only), and
                    // a release before that would miss its target. Telling the
                    // window the pointer is still where it is puts it back —
                    // now, not later: the release may be the very next event.
                    // (A nested dispatch resets propagation, so the key is
                    // stopped after it.)
                    let still_here = MouseMoveEvent { position: window.mouse_position(), pressed_button: Some(MouseButton::Left), modifiers: window.modifiers() };
                    window.dispatch_event(PlatformInput::MouseMove(still_here), cx);
                    cx.stop_propagation();
                }
                // The panes are out of use while a tab is held: no other
                // key reaches them.
                _ => cx.stop_propagation(),
            }
        });
        let mut this = WorkspaceView {
            window: window.window_handle(),
            app,
            layout: WorkspaceLayout::new("Main"),
            panes: HashMap::new(),
            area,
            skin,
            history: LayoutHistory::default(),
            closed: ClosedPanes::default(),
            seen: HouseholdSnapshot { usable: false, generation: 0, viewer: Viewer::person(atlas_core::ids::PersonId::new(0)) },
            launch_panes: Some(LaunchPanes { extra: launch.extra.clone(), stacked: launch.stacked.clone() }),
            focus_handle: cx.focus_handle(),
            renders: 0,
            group_watches: HashMap::new(),
            last_resized_split: None,
            pane_bounds: Rc::new(RefCell::new(HashMap::new())),
            root_bounds: Rc::new(Cell::new(Bounds::default())),
            drag: None,
            floating: HashMap::new(),
            expected_removals: HashSet::new(),
            session: SessionStore::for_household(None, "none"),
            data_dir: launch.data_dir.clone(),
            explicit_screen: launch.explicit_screen,
            layouts: super::layouts::LayoutStore::new(launch.data_dir.as_deref()),
            _subscriptions: vec![area_events, app_changes, escape_cancels_drag],
        };
        this.follow_household(window, cx);
        this
    }

    // ----- readers -------------------------------------------------------------------

    /// The window this workspace is the content of.
    pub fn window_handle(&self) -> AnyWindowHandle {
        self.window
    }

    /// How many times the workspace has been rendered.
    pub fn renders(&self) -> u64 {
        self.renders
    }

    /// The layout model: the truth about structure.
    pub fn layout(&self) -> &WorkspaceLayout {
        &self.layout
    }

    /// The main window's dock area.
    pub fn area(&self) -> &Entity<DockArea> {
        &self.area
    }

    /// The dock area of any window of the workspace.
    fn area_of(&self, window: &WindowId) -> Option<Entity<DockArea>> {
        if *window == WindowId::main() { Some(self.area.clone()) } else { self.floating.get(window).map(|floating| floating.area.clone()) }
    }

    /// The gpui window behind a model window.
    pub fn window_handle_of(&self, window: &WindowId) -> Option<AnyWindowHandle> {
        if *window == WindowId::main() { Some(self.window) } else { self.floating.get(window).map(|floating| floating.handle) }
    }

    /// The root view of a floating window.
    pub fn floating_view(&self, window: &WindowId) -> Option<Entity<FloatingView>> {
        self.floating.get(window).map(|floating| floating.view.clone())
    }

    /// The model ids of the floating windows open right now.
    pub fn floating_windows(&self) -> Vec<WindowId> {
        let mut ids: Vec<WindowId> = self.floating.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// The window a pane is in, as the model has it.
    fn window_of_pane(&self, pane: &PaneId) -> WindowId {
        self.layout.window_id_of(pane).unwrap_or_else(WindowId::main)
    }

    /// Runs `f` with the gpui window behind `id`: the current one when that
    /// is it, else the other window brought in by its handle. Nothing happens
    /// for a window that does not exist.
    fn in_window(&self, id: &WindowId, current: &mut Window, cx: &mut App, f: impl FnOnce(&mut Window, &mut App)) {
        let Some(handle) = self.window_handle_of(id) else {
            return;
        };
        if handle == current.window_handle() {
            f(current, cx);
        } else if let Err(err) = handle.update(cx, |_, window, cx| f(window, cx)) {
            log::warn!("workspace: window {id} could not be updated: {err}");
        }
    }

    /// The skin the main window's dock area wears.
    pub fn skin(&self) -> &Rc<WorkspaceSkin> {
        &self.skin
    }

    /// Tells every window's skin whether a tab is held: the gaps widen and
    /// the cards pull in while one is.
    fn set_skins_held(&self, held: bool, cx: &mut App) {
        self.skin.set_held(held, cx);
        for floating in self.floating.values() {
            floating.skin.set_held(held, cx);
        }
    }

    /// The undo history of layout changes.
    pub fn history(&self) -> &LayoutHistory {
        &self.history
    }

    /// Replaces the minimum-size rule drops and splits are checked against.
    pub fn set_split_limits(&mut self, limits: SplitLimits) {
        log::info!("workspace: split limits set to {limits:?}");
        self.layout.set_limits(limits);
    }

    /// Records one undoable step. Any step but a divider drag ends the run of
    /// divider drags that share an entry. Every step is a change to keep.
    pub(crate) fn record(&mut self, label: impl Into<String>, before: WorkspaceLayout, cx: &mut Context<Self>) {
        self.last_resized_split = None;
        let label = label.into();
        self.history.push(label.clone(), before);
        self.touch(label, cx);
    }

    /// The household's session on disk, and how it is written.
    pub fn session(&self) -> &SessionStore {
        &self.session
    }

    /// Notes a change to the workspace; the session is written once the
    /// debounce has lapsed, by a task started for the first change.
    fn touch(&mut self, reason: impl Into<String>, cx: &mut Context<Self>) {
        self.session.mark_dirty(reason);
        if self.session.flush_scheduled || !self.session.persists() {
            return;
        }
        // One write, a little after the first of a burst of changes; the
        // changes that follow within the wait join it.
        self.session.flush_scheduled = true;
        let debounce = session::DEBOUNCE;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(debounce).await;
            let _ = this.update(cx, |workspace, cx| workspace.flush_session(cx));
        })
        .detach();
    }

    /// Writes the session now if anything changed, window frames included.
    /// Called when the debounce lapses, when the household closes and when
    /// the window closes; safe to call at any time.
    pub fn flush_session(&mut self, cx: &mut Context<Self>) {
        self.session.flush_scheduled = false;
        if !self.session.is_dirty() {
            return;
        }
        self.note_window_frames(cx);
        if let Err(err) = self.session.write(&self.layout) {
            let path = self.session.path().map(|path| path.display().to_string()).unwrap_or_default();
            alerting::report(Level::Warning, format!("workspace session could not be written to {path}: {err}"));
        }
    }

    /// Records where every window is, so a restored session opens them there.
    fn note_window_frames(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<WindowId> = self.layout.windows.iter().map(|candidate| candidate.id.clone()).collect();
        for id in ids {
            let Some(handle) = self.window_handle_of(&id) else {
                continue;
            };
            let bounds = handle.update(cx, |_, window, _| window.bounds()).ok();
            if let Some(bounds) = bounds {
                let frame = WindowFrame::new(f64::from(f32::from(bounds.origin.x)), f64::from(f32::from(bounds.origin.y)), f64::from(f32::from(bounds.size.width)), f64::from(f32::from(bounds.size.height)));
                let _ = self.layout.set_frame(&id, Some(frame));
            }
        }
    }

    /// The panes closed recently, newest last.
    pub fn closed(&self) -> &ClosedPanes {
        &self.closed
    }

    /// How many panes are open.
    pub fn pane_count(&self) -> usize {
        self.panes.len()
    }

    /// The pane commands act on.
    pub fn active_pane(&self) -> Option<PaneId> {
        self.layout.active_pane()
    }

    /// The route the active pane shows.
    pub fn active_route(&self, cx: &App) -> Option<Route> {
        let active = self.active_pane()?;
        self.pane_route(&active, cx)
    }

    /// The view of a pane.
    pub fn pane(&self, pane: &PaneId) -> Option<&Entity<PaneView>> {
        self.panes.get(pane)
    }

    /// The route a pane shows.
    pub fn pane_route(&self, pane: &PaneId, cx: &App) -> Option<Route> {
        self.panes.get(pane).map(|view| view.read(cx).route())
    }

    /// The panes of the main window in reading order (left to right, top to bottom).
    pub fn panes_in_order(&self) -> Vec<PaneId> {
        self.layout.main_window().map(|window| window.panes()).unwrap_or_default()
    }

    // ----- following the app ---------------------------------------------------------

    /// Reacts to what changed on the app: a household that became usable gets
    /// a fresh workspace, a closed one empties it, a new viewer resets the panes.
    fn follow_household(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let snapshot = HouseholdSnapshot::of(self.app.read(cx));
        if snapshot == self.seen {
            return;
        }
        let previous = std::mem::replace(&mut self.seen, snapshot.clone());
        if snapshot.usable && (!previous.usable || snapshot.generation != previous.generation) {
            let identity = self.app.read(cx).household_identity();
            self.start_household(&identity, window, cx);
        } else if !snapshot.usable && previous.usable {
            self.clear_for_closed_household(window, cx);
        } else if snapshot.usable && snapshot.viewer != previous.viewer {
            self.reset_panes_for_viewer(cx);
        }
    }

    /// A fresh layout for the household on show, with the launch route as its
    /// first pane and the `--open` panes beside it.
    fn start_household(&mut self, identity: &str, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("workspace: household {identity:?} is usable; starting its workspace");
        self.flush_session(cx);
        self.layout = WorkspaceLayout::new("Main").with_scope(Scope::household(identity));
        self.history = LayoutHistory::default();
        self.closed = ClosedPanes::default();
        self.panes.clear();
        self.session = SessionStore::for_household(self.data_dir.as_deref(), identity);
        if self.restore_session(window, cx) {
            self.launch_panes = None;
            return;
        }
        // The first household opens what `--screen` asked for; a household
        // opened later starts at Today. Either way it is a request to the
        // resolver, not a route the app owns.
        let first = if self.launch_panes.is_some() { self.app.read(cx).launch_route() } else { Route::Today };
        let first = match first {
            Route::Welcome => Route::Today,
            route => route,
        };
        if let Err(err) = self.open(first, Intent::Open, window, cx) {
            log::warn!("workspace: could not open the first pane ({}): {err}", first.slug());
            self.rebuild_area(window, cx);
        }
        let Some(launch) = self.launch_panes.take() else {
            return;
        };
        for route in launch.extra {
            // At the window's right edge, taking an equal share: `--open a
            // --open b` gives three equal columns, not a column and its halves.
            let columns = self.panes_in_order().len() + 1;
            let target = DockTarget::WindowEdge { side: Side::Right, share: Some(1.0 / columns as f64) };
            let result = self.create_pane(route, kinds::definition_of(route), &WindowId::main(), target, window, cx);
            if let Err(err) = result {
                log::warn!("workspace: --open {} refused: {err}", route.slug());
            }
        }
        for (onto, route) in launch.stacked {
            let order = self.panes_in_order();
            match order.get(onto) {
                Some(pane) => {
                    if let Err(err) = self.stack_onto(pane, route, window, cx) {
                        log::warn!("workspace: --open +{} refused: {err}", route.slug());
                    }
                }
                None => log::warn!("workspace: --open +{} names pane {onto}, which does not exist", route.slug()),
            }
        }
    }

    /// Installs the household's saved session, if there is one to install.
    /// Returns false when the workspace should start fresh: no session, the
    /// launch named a screen, or the session could not be read (then the
    /// person is told, and the file was copied aside for diagnosis).
    fn restore_session(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.explicit_screen {
            log::info!("workspace: the launch named a screen; the saved session is not restored");
            return false;
        }
        match self.session.load() {
            LoadOutcome::Fresh => false,
            LoadOutcome::Loaded(mut layout) => {
                layout.set_limits(*self.layout.limits());
                log::info!("workspace: session restored from {} ({} panes, {} windows)", self.session.path().map(|path| path.display().to_string()).unwrap_or_default(), layout.panes.len(), layout.windows.len());
                self.install_layout(layout, window, cx);
                true
            }
            LoadOutcome::Recovered { diagnostics, reason, .. } => {
                let message = format!("Your last workspace could not be restored ({reason}). A copy of the file was kept at {}.", diagnostics.display());
                log::warn!("workspace: {message}");
                alerting::report(Level::Warning, format!("workspace session recovery: {reason}; copy at {}", diagnostics.display()));
                // Told once the window is whole: at start-up this runs while
                // the window is still being built, before it can show a toast.
                let _ = window;
                let handle = self.window;
                cx.defer(move |cx| {
                    let _ = handle.update(cx, |_, window, cx| window.push_notification(Notification::warning(message), cx));
                });
                false
            }
        }
    }

    /// Everything goes: the household is closed, Welcome takes the column.
    fn clear_for_closed_household(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("workspace: household closed; removing every pane ({} open)", self.panes.len());
        self.flush_session(cx);
        self.session = SessionStore::for_household(None, "none");
        self.layout = WorkspaceLayout::new("Main");
        self.history = LayoutHistory::default();
        self.closed = ClosedPanes::default();
        self.panes.clear();
        self.rebuild_area(window, cx);
    }

    /// Another person is looking: the records behind the routes on show, and
    /// behind Back, may be hidden from them. Every pane returns to the home
    /// screen of its destination, as the single content view used to.
    fn reset_panes_for_viewer(&mut self, cx: &mut Context<Self>) {
        log::info!("workspace: viewer changed; every pane returns to its destination's home screen");
        let panes: Vec<PaneId> = self.panes.keys().cloned().collect();
        for pane in panes {
            if let Some(view) = self.panes.get(&pane).cloned() {
                view.update(cx, |pane, cx| {
                    let route = pane.route();
                    let home = route.destination().map(Destination::home).unwrap_or(route);
                    pane.reset_to(home, cx);
                });
            }
            self.sync_pane_definition(&pane, cx);
        }
        self.sync_chrome(cx);
    }

    // ----- opening and arranging -----------------------------------------------------

    /// Opens `route` the way `intent` asks: focusing a pane that already shows
    /// it, or creating one where the resolver puts it. Floating windows are not
    /// available yet, so `OpenNewWindow` opens in the active stack instead.
    pub fn open(&mut self, route: Route, intent: Intent, window: &mut Window, cx: &mut Context<Self>) -> Result<PaneId, OpError> {
        let definition = kinds::definition_of(route);
        // A screen that lives in a window of its own: the one already open,
        // else a new floating window, whatever the intent.
        if kinds::opens_in_own_window(route) {
            let existing = self.layout.find_panes(&definition.kind, definition.resource.as_ref()).into_iter().next();
            let result = match existing {
                Some(pane) => {
                    log::info!("workspace: open {} ({intent:?}) → already open as {pane}; focusing it", route.slug());
                    self.set_active_pane(&pane, window, cx).map(|()| pane)
                }
                None => {
                    log::info!("workspace: open {} ({intent:?}) → a window of its own", route.slug());
                    self.open_in_new_window(route, None, window, cx)
                }
            };
            self.report(&result, window, cx);
            return result;
        }
        // "Here" is the window the request came from, whichever window
        // holds the active pane.
        let here = self.window_id_of_handle(window.window_handle()).unwrap_or_else(WindowId::main);
        let resolution = resolver::resolve_in(&self.layout, &here, &definition.kind, definition.resource.as_ref(), intent);
        log::info!("workspace: open {} ({intent:?}) from {here} → {resolution:?}", route.slug());
        let result = match resolution {
            Resolution::Focus(pane) => self.set_active_pane(&pane, window, cx).map(|()| pane),
            Resolution::Create { window: target_window, target } => self.create_pane(route, definition, &target_window, target, window, cx),
            Resolution::CreateWindow => self.open_in_new_window(route, None, window, cx),
        };
        self.report(&result, window, cx);
        result
    }

    /// Opens `route` in a new pane beside the active pane's stack.
    pub fn split_active(&mut self, side: Side, route: Route, window: &mut Window, cx: &mut Context<Self>) -> Result<PaneId, OpError> {
        match self.active_pane() {
            Some(active) => self.split_beside(&active, side, route, window, cx),
            None => self.open(route, Intent::Open, window, cx),
        }
    }

    /// Opens `route` in a new pane on `side` of the stack that holds `pane`.
    pub fn split_beside(&mut self, pane: &PaneId, side: Side, route: Route, window: &mut Window, cx: &mut Context<Self>) -> Result<PaneId, OpError> {
        let stack = self.layout.stack_of(pane).ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
        let target = DockTarget::beside(stack, side);
        let in_window = self.window_of_pane(pane);
        let result = self.create_pane(route, kinds::definition_of(route), &in_window, target, window, cx);
        self.report(&result, window, cx);
        result
    }

    /// Opens `route` as a new tab of the stack that holds `pane`.
    pub fn stack_onto(&mut self, pane: &PaneId, route: Route, window: &mut Window, cx: &mut Context<Self>) -> Result<PaneId, OpError> {
        let stack = self.layout.stack_of(pane).ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
        let in_window = self.window_of_pane(pane);
        let result = self.create_pane(route, kinds::definition_of(route), &in_window, DockTarget::tab(stack), window, cx);
        self.report(&result, window, cx);
        result
    }

    /// The one way a pane comes into being: the model first, then its view,
    /// then the area rebuilt from the model.
    fn create_pane(&mut self, route: Route, definition: PaneDefinition, target_window: &WindowId, target: DockTarget, window: &mut Window, cx: &mut Context<Self>) -> Result<PaneId, OpError> {
        let before = self.layout.clone();
        let pane = self.layout.open_pane(target_window, definition, target.clone())?;
        let workspace = cx.weak_entity();
        let app = self.app.clone();
        let bounds = self.pane_bounds.clone();
        let view = cx.new(|cx| PaneView::new(pane.clone(), route, app, workspace, bounds, cx));
        self.panes.insert(pane.clone(), view);
        self.record(format!("Open {}", route.title()), before, cx);
        log::info!("workspace: opened pane {pane} ({}) at {target:?}; {} panes", route.slug(), self.panes.len());
        self.rebuild_area(window, cx);
        self.focus_active(window, cx);
        self.sync_chrome(cx);
        Ok(pane)
    }

    /// Closes a pane: remembered for reopening, removed from the model, then
    /// from the area, whose engine collapses the emptied group as the model did.
    pub fn close_pane(&mut self, pane: &PaneId, window: &mut Window, cx: &mut Context<Self>) -> Result<(), OpError> {
        let before = self.layout.clone();
        let closed = self.layout.close_pane_detailed(pane)?;
        let title = kinds::route_of(&closed.definition).map(Route::title).unwrap_or("pane");
        log::info!("workspace: closed pane {pane} ({}); {} panes left", closed.definition.kind, self.layout.panes.len());
        let window_id = closed.window.clone();
        self.closed.record(closed);
        self.record(format!("Close {title}"), before, cx);
        if let Some(view) = self.panes.remove(pane) {
            if let Some(area) = self.area_of(&window_id) {
                self.in_window(&window_id, window, cx, |window, cx| area.update(cx, |area, cx| area.remove_panel(view, window, cx)));
            }
        }
        // The window the pane left may have been the last thing in a floating
        // window, which the model has then dropped.
        self.close_vanished_windows(window, cx);
        self.apply_active_flags(cx);
        self.focus_active(window, cx);
        self.sync_chrome(cx);
        cx.notify();
        Ok(())
    }

    /// Closes the active pane.
    pub fn close_active(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<(), OpError> {
        let active = self.active_pane().ok_or(OpError::EmptyWindow(WindowId::main()))?;
        self.close_pane(&active, window, cx)
    }

    /// Makes `pane` the one commands act on: active in the model, its tab
    /// selected in its group, focused, and the launcher following its route.
    pub fn set_active_pane(&mut self, pane: &PaneId, window: &mut Window, cx: &mut Context<Self>) -> Result<(), OpError> {
        let changed = self.layout.active_pane().as_ref() != Some(pane);
        self.layout.set_active_pane(pane)?;
        if changed {
            let slug = self.pane_route(pane, cx).map(Route::slug).unwrap_or("?");
            log::info!("workspace: active pane {pane} ({slug})");
            self.touch(format!("active pane {pane}"), cx);
        }
        self.select_tab_in_area(pane, window, cx);
        self.apply_active_flags(cx);
        if changed {
            self.focus_active(window, cx);
        }
        self.sync_chrome(cx);
        cx.notify();
        Ok(())
    }

    /// The next pane in reading order becomes the active one (wrapping).
    pub fn focus_next_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<(), OpError> {
        let order = self.panes_in_order();
        if order.is_empty() {
            return Err(OpError::EmptyWindow(WindowId::main()));
        }
        let current = self.active_pane().and_then(|active| order.iter().position(|pane| *pane == active)).unwrap_or(0);
        let next = order[(current + 1) % order.len()].clone();
        self.set_active_pane(&next, window, cx)
    }

    /// The previous pane in reading order becomes the active one (wrapping).
    pub fn focus_previous_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<(), OpError> {
        let order = self.panes_in_order();
        if order.is_empty() {
            return Err(OpError::EmptyWindow(WindowId::main()));
        }
        let current = self.active_pane().and_then(|active| order.iter().position(|pane| *pane == active)).unwrap_or(0);
        let previous = order[(current + order.len() - 1) % order.len()].clone();
        self.set_active_pane(&previous, window, cx)
    }

    /// The pane in `direction` from the active one (the neighbour sharing the
    /// longest edge, in the same window) becomes the active one. `NoOp`
    /// when there is none that way.
    pub fn focus_direction(&mut self, direction: Direction, window: &mut Window, cx: &mut Context<Self>) -> Result<(), OpError> {
        let active = self.active_pane().ok_or(OpError::EmptyWindow(WindowId::main()))?;
        let in_window = self.window_of_pane(&active);
        let root = self.layout.window(&in_window).and_then(|layout| layout.root.as_ref()).ok_or_else(|| OpError::UnknownWindow(in_window.clone()))?;
        match focus::neighbour(root, &active, direction) {
            Some(next) => {
                log::info!("workspace: focus {direction:?} from {active} → {next}");
                self.set_active_pane(&next, window, cx)
            }
            None => {
                log::info!("workspace: no pane {direction:?} of {active}");
                Err(OpError::NoOp)
            }
        }
    }

    /// Moves the active pane one step in `direction`: beside its neighbour
    /// there, over it when it already sits beside it, or to the window edge
    /// when it has no neighbour that way. One history step, like a drop.
    pub fn move_active(&mut self, direction: Direction, window: &mut Window, cx: &mut Context<Self>) -> Result<(), OpError> {
        let active = self.active_pane().ok_or(OpError::EmptyWindow(WindowId::main()))?;
        let in_window = self.window_of_pane(&active);
        let root = self.layout.window(&in_window).and_then(|layout| layout.root.as_ref()).ok_or_else(|| OpError::UnknownWindow(in_window.clone()))?;
        let target = focus::move_direction_target(root, &active, direction).ok_or(OpError::NoOp)?;
        log::info!("workspace: move {active} {direction:?} → {target:?}");
        self.dock_pane(&active, &in_window, target, true, window, cx);
        Ok(())
    }

    /// Opens the active pane again — same screen, same state — to its right.
    pub fn duplicate_active(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<PaneId, OpError> {
        let active = self.active_pane().ok_or(OpError::EmptyWindow(WindowId::main()))?;
        self.duplicate_pane(&active, window, cx)
    }

    /// Opens `pane` again beside it (to the right), with the screen it shows
    /// and the state of that screen — its scroll, its filters — copied. Unlike
    /// a split, which opens the screen afresh.
    pub fn duplicate_pane(&mut self, pane: &PaneId, window: &mut Window, cx: &mut Context<Self>) -> Result<PaneId, OpError> {
        let stack = self.layout.stack_of(pane).ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
        self.sync_pane_definition(pane, cx);
        let before = self.layout.clone();
        let title = self.pane_route(pane, cx).map(Route::title).unwrap_or("pane");
        let result = self.layout.duplicate_pane(pane, DockTarget::beside(stack, Side::Right));
        match &result {
            Ok(copy) => {
                log::info!("workspace: duplicated pane {pane} as {copy}");
                self.record(format!("Duplicate {title}"), before, cx);
                self.sync_pane_entities(cx);
                self.rebuild_area(window, cx);
                self.focus_active(window, cx);
                self.sync_chrome(cx);
            }
            Err(error) => log::warn!("workspace: duplicating pane {pane} refused: {error}"),
        }
        self.report(&result, window, cx);
        result
    }

    /// Puts the most recently closed pane back where it was.
    pub fn reopen_last_closed(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<PaneId, OpError> {
        let closed = self.closed.pop().ok_or(OpError::NoOp)?;
        self.reopen_closed(closed, window, cx)
    }

    /// Puts back the remembered pane at `index` (oldest first) — a pick from
    /// the list of recently closed panes.
    pub fn reopen_closed_at(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> Result<PaneId, OpError> {
        let closed = self.closed.take(index).ok_or(OpError::NoOp)?;
        self.reopen_closed(closed, window, cx)
    }

    /// The remembered closed panes a menu can offer, newest first: the index
    /// [`reopen_closed_at`](Self::reopen_closed_at) takes, and a title.
    pub fn recently_closed(&self, cx: &App) -> Vec<(usize, SharedString)> {
        let household = self.app.read(cx).household().clone();
        let entries: Vec<&ClosedPane> = self.closed.iter().collect();
        entries
            .into_iter()
            .enumerate()
            .rev()
            .map(|(index, closed)| {
                let title = kinds::route_of(&closed.definition).map(|route| kinds::title_of(route, &household)).unwrap_or_else(|| kinds::label_of_unknown_kind(&closed.definition.kind).into());
                (index, title)
            })
            .collect()
    }

    fn reopen_closed(&mut self, closed: ClosedPane, window: &mut Window, cx: &mut Context<Self>) -> Result<PaneId, OpError> {
        let before = self.layout.clone();
        let title = kinds::route_of(&closed.definition).map(Route::title).unwrap_or("pane");
        let fallback = self.layout.active_window().map(WindowLayout::default_target).unwrap_or_else(|| DockTarget::edge(Side::Right));
        let result = self.layout.reopen(closed.clone(), fallback);
        match &result {
            Ok(pane) => {
                let _ = self.layout.set_active_pane(pane);
                self.record(format!("Reopen {title}"), before, cx);
                log::info!("workspace: reopened {pane} ({}); {} panes", closed.definition.kind, self.layout.panes.len());
                self.sync_pane_entities(cx);
                self.rebuild_area(window, cx);
                self.focus_active(window, cx);
                self.sync_chrome(cx);
            }
            Err(error) => {
                log::warn!("workspace: reopening {} refused: {error}", closed.definition.kind);
                // Refused, not lost: it stays the next one to reopen.
                self.closed.record(closed);
            }
        }
        self.report(&result, window, cx);
        result
    }

    /// The active pane fills its window, or comes back to its place. A view
    /// state of the window, not of the layout: any layout change ends it.
    pub fn toggle_zoom(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<(), OpError> {
        let active = self.active_pane().ok_or(OpError::EmptyWindow(WindowId::main()))?;
        let group = self.panes.get(&active).and_then(|view| view.read(cx).group().cloned()).and_then(|group| group.upgrade()).ok_or_else(|| OpError::UnknownPane(active.clone()))?;
        let in_window = self.window_of_pane(&active);
        let zoomed = group.read(cx).is_zoomed();
        log::info!("workspace: pane {active} {}", if zoomed { "back from zoom" } else { "zoomed" });
        self.in_window(&in_window, window, cx, |window, cx| group.update(cx, |group, cx| group.toggle_zoom(window, cx)));
        cx.notify();
        Ok(())
    }

    /// True while a pane of `in_window` is zoomed.
    pub fn is_zoomed(&self, in_window: &WindowId, cx: &App) -> bool {
        self.area_of(in_window).is_some_and(|area| area.read(cx).is_zoomed())
    }

    /// Replaces a split's weights, in the model and then on screen.
    pub fn resize_split(&mut self, split: &NodeId, weights: &[f64], window: &mut Window, cx: &mut Context<Self>) -> Result<(), OpError> {
        let before = self.layout.clone();
        self.layout.resize(split, weights)?;
        self.record(RESIZE_LABEL, before, cx);
        log::info!("workspace: split {split} resized to {weights:?}");
        self.rebuild_area(window, cx);
        Ok(())
    }

    /// Steps the layout back one change. Returns false when there is none.
    pub fn undo(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some((layout, label)) = self.history.undo(&self.layout) else {
            return false;
        };
        log::info!("workspace: undo {label:?}");
        self.install_layout(layout, window, cx);
        true
    }

    /// Steps the layout forward again after an undo. Returns false when there is none.
    pub fn redo(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some((layout, label)) = self.history.redo(&self.layout) else {
            return false;
        };
        log::info!("workspace: redo {label:?}");
        self.install_layout(layout, window, cx);
        true
    }

    /// Installs a whole layout — undo, redo, a saved or restored layout — as
    /// the workspace's own: pane views are created for panes the layout has
    /// and this view does not, dropped for the reverse, then the area is
    /// rebuilt. The layout is recorded as one undoable step under `label`.
    pub fn load_layout(&mut self, layout: WorkspaceLayout, label: impl Into<String>, window: &mut Window, cx: &mut Context<Self>) {
        let before = self.layout.clone();
        self.install_layout(layout, window, cx);
        self.record(label, before, cx);
    }

    /// Installs a whole layout without recording history (undo and redo do
    /// their own bookkeeping).
    fn install_layout(&mut self, layout: WorkspaceLayout, window: &mut Window, cx: &mut Context<Self>) {
        self.layout = layout;
        self.sync_pane_entities(cx);
        self.rebuild_area(window, cx);
        self.focus_active(window, cx);
        self.sync_chrome(cx);
    }

    /// One pane view per pane definition, no more and no fewer. A pane the
    /// layout has and this view does not is rebuilt from its definition, Back
    /// history included; a definition of a kind this build does not know gets
    /// a placeholder pane so the layout loads whole and the person decides.
    pub(crate) fn sync_pane_entities(&mut self, cx: &mut Context<Self>) {
        // A view stays only while it shows what the model says the pane is:
        // a layout that was loaded may name another screen for the same id.
        let layout = &self.layout;
        self.panes.retain(|pane, view| {
            let Some(definition) = layout.pane(pane) else {
                return false;
            };
            let view = view.read(cx);
            match (view.placeholder(), kinds::route_of(definition)) {
                (None, Some(route)) => view.route() == route,
                (Some(super::pane::Placeholder::Unsupported { kind }), None) => *kind == definition.kind,
                _ => false,
            }
        });
        let missing: Vec<(PaneId, PaneDefinition)> = self.layout.panes.iter().filter(|(pane, _)| !self.panes.contains_key(*pane)).map(|(pane, definition)| (pane.clone(), definition.clone())).collect();
        for (pane, definition) in missing {
            let workspace = cx.weak_entity();
            let app = self.app.clone();
            let bounds = self.pane_bounds.clone();
            let view = match kinds::route_of(&definition) {
                Some(route) => {
                    let view_state = definition.view_state.clone();
                    cx.new(|cx| {
                        let mut view = PaneView::new(pane.clone(), route, app, workspace, bounds, cx);
                        view.restore_view_state(&view_state);
                        view
                    })
                }
                None => {
                    let kind = definition.kind.clone();
                    cx.new(|cx| PaneView::unsupported(pane.clone(), kind, app, workspace, bounds, cx))
                }
            };
            self.panes.insert(pane, view);
        }
    }

    /// Writes what `pane` shows — its route and Back history — into the
    /// model, so the layout that is saved and the resolver's answers follow
    /// the pane. A placeholder pane keeps the definition it could not show.
    fn sync_pane_definition(&mut self, pane: &PaneId, cx: &mut Context<Self>) {
        let Some(view) = self.panes.get(pane) else {
            return;
        };
        let view = view.read(cx);
        if view.placeholder().is_some() {
            return;
        }
        let definition = kinds::definition_of(view.route()).with_view_state(view.view_state());
        if self.layout.pane(pane) == Some(&definition) {
            return;
        }
        let slug = view.route().slug();
        if let Err(err) = self.layout.replace_pane(pane, definition) {
            log::warn!("workspace: pane {pane} shows {slug} but its definition could not be updated: {err}");
            return;
        }
        self.touch(format!("pane {pane} shows {slug}"), cx);
    }

    /// Replaces what `pane` shows — a placeholder, or a screen — with
    /// `route`, in the pane and in the model.
    pub(crate) fn replace_pane_content(&mut self, pane: &PaneId, route: Route, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.panes.get(pane).cloned() else {
            return;
        };
        let before = self.layout.clone();
        view.update(cx, |view, cx| view.become_screen(route, cx));
        self.sync_pane_definition(pane, cx);
        self.record(format!("Replace pane with {}", route.title()), before, cx);
        log::info!("workspace: pane {pane} replaced with {}", route.slug());
        let _ = self.set_active_pane(pane, window, cx);
        self.sync_chrome(cx);
        cx.notify();
    }

    // ----- in-pane navigation (called by the app) -----------------------------------

    /// The active pane shows `route`, remembering the current one for Back.
    /// With no pane open, one is opened — once this update is over, since
    /// the app that calls this holds no window.
    pub fn navigate_active(&mut self, route: Route, cx: &mut Context<Self>) {
        match self.active_pane().and_then(|active| self.panes.get(&active).cloned()) {
            Some(view) => {
                view.update(cx, |pane, cx| pane.show(route, cx));
                if let Some(active) = self.active_pane() {
                    self.sync_pane_definition(&active, cx);
                }
            }
            None => {
                log::info!("workspace: no pane to navigate; opening {} in a new one", route.slug());
                let this = cx.weak_entity();
                let window = self.window;
                cx.defer(move |cx| {
                    let _ = window.update(cx, |_, window, cx| {
                        let _ = this.update(cx, |this, cx| {
                            let _ = this.open(route, Intent::Open, window, cx);
                        });
                    });
                });
            }
        }
    }

    /// The active pane returns to the route it showed before (or its parent).
    /// Returns the route now on show, or `None` when there is no pane.
    pub fn back_active(&mut self, cx: &mut Context<Self>) -> Option<Route> {
        let active = self.active_pane()?;
        let view = self.panes.get(&active).cloned()?;
        let route = view.update(cx, |pane, cx| pane.back(cx));
        self.sync_pane_definition(&active, cx);
        Some(route)
    }

    // ----- what the panes and the engine report -------------------------------------

    /// A press inside `pane`: it becomes the active pane. Nothing to do when
    /// it already is, so a click in the pane one is working in changes no focus.
    pub(crate) fn set_active_pane_from_pointer(&mut self, pane: &PaneId, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_pane().as_ref() == Some(pane) {
            return;
        }
        if let Err(err) = self.set_active_pane(pane, window, cx) {
            log::warn!("workspace: press in pane {pane} could not activate it: {err}");
        }
    }

    /// `Split right` / `Split below` from a pane's title-bar menu: the pane's
    /// screen again, beside that pane.
    pub(crate) fn split_pane_from_menu(&mut self, pane: &PaneId, side: Side, window: &mut Window, cx: &mut Context<Self>) {
        let Some(route) = self.pane_route(pane, cx) else {
            return;
        };
        let _ = self.set_active_pane(pane, window, cx);
        let _ = self.split_beside(pane, side, route, window, cx);
    }

    /// `Back` from a pane's title-bar menu.
    pub(crate) fn back_pane_from_menu(&mut self, pane: &PaneId, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.set_active_pane(pane, window, cx);
        if let Some(view) = self.panes.get(pane).cloned() {
            view.update(cx, |pane, cx| {
                pane.back(cx);
            });
        }
        self.sync_pane_definition(pane, cx);
        self.sync_chrome(cx);
    }

    /// The engine removed `pane` (its tab's close button, the skin's Close):
    /// the model follows, unless the model closed it first (the workspace's
    /// own `close_pane`, a rebuild that no longer includes it). `view` is the
    /// pane view that left: a notice from a view this workspace has since
    /// replaced (a loaded layout gave the pane another definition) is stale
    /// and changes nothing.
    pub(crate) fn pane_left(&mut self, pane: &PaneId, view: EntityId, window: &mut Window, cx: &mut Context<Self>) {
        match self.panes.get(pane) {
            Some(current) if current.entity_id() != view => {
                log::debug!("workspace: pane {pane}'s previous view left the dock; its current view stays");
                return;
            }
            _ => {}
        }
        if self.expected_removals.remove(pane) {
            log::debug!("workspace: pane {pane} left one window's dock for another; not a close");
            return;
        }
        if self.layout.pane(pane).is_none() {
            self.panes.remove(pane);
            return;
        }
        let before = self.layout.clone();
        match self.layout.close_pane_detailed(pane) {
            Ok(closed) => {
                let title = kinds::route_of(&closed.definition).map(Route::title).unwrap_or("pane");
                log::info!("workspace: pane {pane} ({}) was closed from its tab; {} panes left", closed.definition.kind, self.layout.panes.len());
                self.closed.record(closed);
                self.record(format!("Close {title}"), before, cx);
            }
            Err(err) => log::warn!("workspace: pane {pane} left the dock but could not be closed in the model: {err}"),
        }
        self.panes.remove(pane);
        self.apply_active_flags(cx);
        self.focus_active(window, cx);
        self.sync_chrome(cx);
        cx.notify();
    }

    // ----- drag and drop -----------------------------------------------------------------

    /// The engine put a pane into `group`: from now on the workspace hears
    /// every drop on that group. The engine resolves a drop — which pane, from
    /// which group, onto which group, as a tab at which index or on which
    /// side — and applies it to its own tree; the workspace applies the same
    /// drop to the model, whose transactional [`WorkspaceLayout::move_pane`]
    /// decides whether it is allowed and what the weights become, and then
    /// rebuilds the area from the model. Whatever the engine did to its tree
    /// in between is never drawn: both happen inside one event flush.
    pub(crate) fn pane_joined_group(&mut self, group: WeakEntity<TabGroup>, window: &mut Window, cx: &mut Context<Self>) {
        // Groups the engine has dropped since take their subscriptions with them.
        self.group_watches.retain(|_, (group, _)| group.upgrade().is_some());
        let Some(strong) = group.upgrade() else {
            return;
        };
        if self.group_watches.contains_key(&strong.entity_id()) {
            return;
        }
        let subscription = cx.subscribe_in(&strong, window, |this, _, event: &TabGroupEvent, window, cx| {
            if let TabGroupEvent::Drop { panel, source, target } = event {
                this.pane_dropped(*panel, *source, *target, window, cx);
            }
        });
        self.group_watches.insert(strong.entity_id(), (group, subscription));
    }

    /// A pane was dropped somewhere in the area. Resolves the engine's ids to
    /// the model's — the dragged panel to its pane, the target group to the
    /// stack of a pane it displays — and moves the pane in the model.
    fn pane_dropped(&mut self, panel: PanelId, source: gpui_kit::component::dock::NodeId, target: InsertTarget, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pane) = self.pane_of_panel(panel) else {
            log::warn!("workspace: a panel that is not one of the workspace's panes was dropped on the dock; ignoring it");
            return;
        };
        let source_stack = self.layout.stack_of(&pane);
        let (target_group, placement, index, activate) = match target {
            InsertTarget::Tabs { node, ix, activate } => (node, None, ix, activate),
            InsertTarget::Split { node, placement, .. } => (node, Some(placement), None, true),
            InsertTarget::Tile { .. } => {
                log::warn!("workspace: pane {pane} was dropped on a tiles canvas, which the workspace does not use; ignoring it");
                return;
            }
        };
        // The dragged pane may already sit in the target group (the engine
        // has applied its move by now), so the stack is read off another pane.
        let Some(target_stack) = self.stack_of_group(target_group, &pane, cx) else {
            log::warn!("workspace: pane {pane} was dropped on a group that displays no other pane; leaving the layout to the mirror");
            return;
        };
        let dock_target = match placement {
            None => DockTarget::Stack { node: target_stack.clone(), index },
            Some(placement) => DockTarget::Beside { node: target_stack.clone(), side: side_of(placement), share: Some(DROP_SHARE) },
        };
        let target_window = self.layout.window_of_node(&target_stack).map(|window| window.id.clone()).unwrap_or_else(WindowId::main);
        let same_group = source == target_group;
        log::info!(
            "workspace: pane {pane} dropped from stack {} onto stack {target_stack}{} as {}",
            source_stack.as_ref().map(ToString::to_string).unwrap_or_else(|| "?".into()),
            if same_group { " (its own)" } else { "" },
            match placement {
                None => format!("a tab at {index:?}"),
                Some(placement) => format!("a split on the {placement}"),
            }
        );
        self.dock_pane(&pane, &target_window, dock_target, activate, window, cx);
    }

    /// Moves `pane` to `target` in the model and shows the result — the one
    /// way a drop becomes a layout change. `activate` false keeps the tab the
    /// target stack was displaying (a drop past the last tab lands in the
    /// background). A refused move leaves the model as it was and puts the
    /// area back to it; a move that changes nothing does the same, because the
    /// engine may have redistributed sizes while deciding nothing changed.
    pub fn dock_pane(&mut self, pane: &PaneId, in_window: &WindowId, target: DockTarget, activate: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.end_drag_overlay(cx);
        let before = self.layout.clone();
        let displayed_before = self.layout.window(in_window).and_then(|target_window| target_window.root.as_ref()).and_then(|root| target.node().and_then(|node| root.find(node))).and_then(|stack| stack.active_pane().cloned());
        match self.layout.move_pane(pane, in_window, target.clone()) {
            Ok(()) => {
                if !activate
                    && let Some(displayed) = displayed_before
                    && displayed != *pane
                {
                    let _ = self.layout.set_active_pane(&displayed);
                }
                self.record(MOVE_LABEL, before, cx);
                log::info!(
                    "workspace: moved pane {pane} to {target:?} in {in_window}; grid:\n{}",
                    atlas_workspace::grid::render_window(self.layout.window(in_window).and_then(|target_window| target_window.root.as_ref()), 12, 4, |pane| pane.minted_counter().map(|n| char::from_digit((n % 36) as u32, 36).unwrap_or('?')).unwrap_or('?'))
                );
                self.rebuild_area(window, cx);
                self.focus_active(window, cx);
                self.sync_chrome(cx);
            }
            Err(OpError::NoOp) => {
                log::info!("workspace: pane {pane} dropped back where it was; nothing changed");
                self.rebuild_area(window, cx);
            }
            Err(err @ (OpError::TooSmall { .. } | OpError::TooDeep { .. })) => {
                log::warn!("workspace: drop of pane {pane} at {target:?} refused: {err}");
                self.rebuild_area(window, cx);
                self.focus_active(window, cx);
                window.push_notification(refused_split_toast(), cx);
            }
            Err(err) => {
                log::warn!("workspace: drop of pane {pane} at {target:?} refused: {err}");
                self.rebuild_area(window, cx);
                self.report(&Err::<(), _>(err), window, cx);
            }
        }
    }

    // ----- the drag-target overlay -------------------------------------------------------

    /// The pointer moved while a pane is dragged: the overlay follows it.
    /// `window` is the window the drag started in, whose coordinates
    /// `pointer` is in — it keeps the pointer even past its own edge.
    pub(crate) fn follow_drag(&mut self, panel: PanelId, pointer: Point<Pixels>, window: &Window, cx: &mut Context<Self>) {
        let Some(pane) = self.pane_of_panel(panel) else {
            return;
        };
        self.follow_dragged(Dragged::Pane(pane), pointer, window, cx);
    }

    /// The pointer moved while a screen is dragged off the launcher.
    pub(crate) fn follow_launch_drag(&mut self, item: &AnyDrag, pointer: Point<Pixels>, window: &Window, cx: &mut Context<Self>) {
        let Some(launch) = item.value().downcast_ref::<LaunchDrag>() else {
            return;
        };
        self.follow_dragged(Dragged::New(launch.route), pointer, window, cx);
    }

    fn follow_dragged(&mut self, dragged: Dragged, pointer: Point<Pixels>, window: &Window, cx: &mut Context<Self>) {
        let level_offset = self.drag.as_ref().filter(|drag| drag.dragged == dragged).map(|drag| drag.level_offset).unwrap_or(0);
        let elsewhere = match self.window_id_of_handle(window.window_handle()) {
            Some(source) => self.window_under(&source, window.bounds().origin + pointer, cx),
            None => None,
        };
        let next = DragInFlight { dragged, pointer, level_offset, elsewhere };
        if self.drag.as_ref() != Some(&next) {
            self.drag = Some(next);
            self.set_skins_held(true, cx);
            cx.notify();
        }
    }

    /// The drag in flight, if any: what the overlay follows.
    pub fn drag_in_flight(&self) -> Option<&DragInFlight> {
        self.drag.as_ref()
    }

    /// The model's id of the window behind `handle`.
    fn window_id_of_handle(&self, handle: AnyWindowHandle) -> Option<WindowId> {
        if handle == self.window {
            return Some(WindowId::main());
        }
        self.floating.iter().find(|(_, floating)| floating.handle == handle).map(|(id, _)| id.clone())
    }

    /// The window of the workspace, other than `from`, under the screen
    /// point, and the displayed pane there (none over its chrome). The last
    /// opened floating window is taken first: it is the one on top.
    fn window_under(&self, from: &WindowId, screen: Point<Pixels>, cx: &mut App) -> Option<(WindowId, Option<PaneId>)> {
        let mut candidates: Vec<(WindowId, AnyWindowHandle)> = self.floating.iter().map(|(id, floating)| (id.clone(), floating.handle)).collect();
        candidates.sort_by(|a, b| b.0.to_string().cmp(&a.0.to_string()));
        candidates.push((WindowId::main(), self.window));
        for (id, handle) in candidates {
            if id == *from {
                continue;
            }
            let Ok(bounds) = handle.update(cx, |_, window, _| window.bounds()) else {
                continue;
            };
            if !bounds.contains(&screen) {
                continue;
            }
            let local = screen - bounds.origin;
            let root = self.layout.window(&id).and_then(|layout| layout.root.as_ref());
            let pane = dock_targets::hovered_pane(root, &self.pane_bounds.borrow(), local);
            return Some((id, pane));
        }
        None
    }

    /// A screen from the launcher was dropped on one of the dock's tab groups:
    /// the engine says which group and which zone; a new pane opens there.
    fn item_dropped_on_dock(&mut self, item: &AnyDrag, target: &DropTarget, window: &mut Window, cx: &mut Context<Self>) {
        self.end_drag_overlay(cx);
        let Some(launch) = item.value().downcast_ref::<LaunchDrag>() else {
            log::info!("workspace: something that is not a screen was dropped on the dock; ignoring it");
            return;
        };
        let DropTarget::Group { node, placement } = target else {
            log::info!("workspace: screen {} dropped on a canvas, which the workspace does not use; ignoring it", launch.route.slug());
            return;
        };
        let Some(stack) = self.stack_of_group_any(*node, cx) else {
            log::warn!("workspace: screen {} dropped on a group with no pane of this workspace; ignoring it", launch.route.slug());
            return;
        };
        let target_window = self.layout.window_of_node(&stack).map(|target| target.id.clone()).unwrap_or_else(WindowId::main);
        let dock_target = match placement {
            None => DockTarget::tab(stack),
            Some(placement) => DockTarget::Beside { node: stack, side: side_of(*placement), share: Some(DROP_SHARE) },
        };
        log::info!("workspace: screen {} dropped from the launcher at {dock_target:?} in {target_window}", launch.route.slug());
        let result = self.create_pane(launch.route, kinds::definition_of(launch.route), &target_window, dock_target, window, cx);
        self.report(&result, window, cx);
    }

    /// Opens a new pane for a screen dropped on a drop zone.
    fn open_on_zone(&mut self, route: Route, in_window: &WindowId, zone: &Zone, window: &mut Window, cx: &mut Context<Self>) {
        self.end_drag_overlay(cx);
        if !zone.accepts_drops() {
            log::info!("workspace: screen {} dropped on a zone that refuses it ({}); nothing changed", route.slug(), zone.label);
            return;
        }
        log::info!("workspace: screen {} dropped on zone {} ({}) in {in_window}", route.slug(), zone.element_id(), zone.label);
        let result = self.create_pane(route, kinds::definition_of(route), in_window, zone.target.clone(), window, cx);
        self.report(&result, window, cx);
    }

    // ----- windows -------------------------------------------------------------------------

    /// Opens `route` in a new floating window: a pane of its own, placed at
    /// `at` (a screen position) or beside the main window.
    pub fn open_in_new_window(&mut self, route: Route, at: Option<Point<Pixels>>, window: &mut Window, cx: &mut Context<Self>) -> Result<PaneId, OpError> {
        let before = self.layout.clone();
        let frame = self.floating_frame(at, window);
        let mut fresh = WindowLayout::new(self.layout.ids.mint_window(), WindowRole::Floating);
        fresh.frame = Some(frame);
        let window_id = fresh.id.clone();
        self.layout.windows.push(fresh);
        match self.layout.open_pane(&window_id, kinds::definition_of(route), DockTarget::edge(Side::Right)) {
            Ok(pane) => {
                let workspace = cx.weak_entity();
                let app = self.app.clone();
                let bounds = self.pane_bounds.clone();
                let view = cx.new(|cx| PaneView::new(pane.clone(), route, app, workspace, bounds, cx));
                self.panes.insert(pane.clone(), view);
                self.record(format!("Open {} in a new window", route.title()), before, cx);
                log::info!("workspace: opened pane {pane} ({}) in new window {window_id}", route.slug());
                self.open_floating_window(&window_id, frame, cx);
                self.rebuild_area(window, cx);
                self.focus_active(window, cx);
                self.sync_chrome(cx);
                Ok(pane)
            }
            Err(err) => {
                self.layout = before;
                Err(err)
            }
        }
    }

    /// Moves `pane` into a floating window of its own, at `at` (a screen
    /// position) or beside the main window. One undoable step.
    pub fn detach_pane(&mut self, pane: &PaneId, at: Option<Point<Pixels>>, window: &mut Window, cx: &mut Context<Self>) -> Result<WindowId, OpError> {
        let before = self.layout.clone();
        let frame = self.floating_frame(at, window);
        let window_id = self.layout.detach_pane(pane, frame)?;
        let title = self.pane_route(pane, cx).map(Route::title).unwrap_or("pane");
        self.record(format!("Move {title} to a new window"), before, cx);
        log::info!("workspace: pane {pane} detached into window {window_id} at {frame:?}");
        self.open_floating_window(&window_id, frame, cx);
        self.rebuild_area(window, cx);
        self.focus_active(window, cx);
        self.sync_chrome(cx);
        Ok(window_id)
    }

    /// Moves `pane` into `target_window`, as a tab of its active stack (or
    /// alone, when that window is empty). A floating window left empty closes.
    pub fn move_pane_to_window(&mut self, pane: &PaneId, target_window: &WindowId, window: &mut Window, cx: &mut Context<Self>) -> Result<(), OpError> {
        let target = self.layout.window(target_window).map(|target| target.default_target()).ok_or_else(|| OpError::UnknownWindow(target_window.clone()))?;
        let before = self.layout.clone();
        self.layout.move_pane(pane, target_window, target)?;
        let title = self.pane_route(pane, cx).map(Route::title).unwrap_or("pane");
        self.record(format!("Move {title} to another window"), before, cx);
        log::info!("workspace: pane {pane} moved to window {target_window}");
        self.rebuild_area(window, cx);
        self.focus_active(window, cx);
        self.sync_chrome(cx);
        Ok(())
    }

    /// Every pane of a floating window goes back to the main window and the
    /// window closes — the floating window's own button, and its close box.
    pub fn gather_window(&mut self, window_id: &WindowId, window: &mut Window, cx: &mut Context<Self>) {
        let panes = self.layout.panes_in(window_id);
        if panes.is_empty() || *window_id == WindowId::main() {
            return;
        }
        let before = self.layout.clone();
        for pane in &panes {
            let Some(target) = self.layout.main_window().map(|main| main.default_target()) else {
                break;
            };
            if let Err(err) = self.layout.move_pane(pane, &WindowId::main(), target) {
                log::warn!("workspace: pane {pane} could not be moved back to the main window: {err}");
            }
        }
        self.record("Move panes back to the main window", before, cx);
        log::info!("workspace: window {window_id} gathered into the main window ({} panes)", panes.len());
        self.rebuild_area(window, cx);
        self.focus_active(window, cx);
        self.sync_chrome(cx);
    }

    /// A pane drag ended outside the window it was in: the pane opens a
    /// window of its own where the pointer was let go.
    pub(crate) fn drag_released_outside(&mut self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(DragInFlight { dragged, .. }) = self.drag.clone() else {
            self.end_drag_overlay(cx);
            return;
        };
        let viewport = Bounds::new(Point::default(), window.viewport_size());
        if viewport.contains(&position) {
            // Released over the window's own chrome (title bar, launcher):
            // not a request for a new window.
            self.end_drag_overlay(cx);
            return;
        }
        self.end_drag_overlay(cx);
        let screen = window.bounds().origin + position;
        let source = self.window_id_of_handle(window.window_handle()).unwrap_or_else(WindowId::main);
        // Over another window of the workspace: the pane goes there, as a tab
        // of the pane under the pointer, or of that window's active stack.
        if let Some((target_window, under)) = self.window_under(&source, screen, cx) {
            let target = under.and_then(|pane| self.layout.stack_of(&pane)).map(DockTarget::tab).or_else(|| self.layout.window(&target_window).map(WindowLayout::default_target));
            let Some(target) = target else {
                return;
            };
            match dragged {
                Dragged::Pane(pane) => {
                    log::info!("workspace: pane {pane} released over {target_window} at {screen:?}; moving it there");
                    self.dock_pane(&pane, &target_window, target, true, window, cx);
                }
                Dragged::New(route) => {
                    log::info!("workspace: screen {} released over {target_window} at {screen:?}; opening it there", route.slug());
                    let result = self.create_pane(route, kinds::definition_of(route), &target_window, target, window, cx);
                    self.report(&result, window, cx);
                }
            }
            return;
        }
        match dragged {
            Dragged::Pane(pane) => {
                log::info!("workspace: pane {pane} released outside every window at {screen:?}; opening a window for it");
                if let Err(err) = self.detach_pane(&pane, Some(screen), window, cx) {
                    self.report(&Err::<(), _>(err), window, cx);
                }
            }
            Dragged::New(route) => {
                log::info!("workspace: screen {} released outside every window at {screen:?}; opening a window for it", route.slug());
                let result = self.open_in_new_window(route, Some(screen), window, cx);
                self.report(&result, window, cx);
            }
        }
    }

    /// Where a new floating window goes: at `at` (its top-left corner, on
    /// screen), else offset from the current window; clamped onto a display.
    pub(crate) fn floating_frame(&self, at: Option<Point<Pixels>>, window: &Window) -> WindowFrame {
        let current = window.bounds();
        let origin = at.unwrap_or(current.origin + point(px(80.), px(80.)));
        WindowFrame::new(f64::from(f32::from(origin.x)), f64::from(f32::from(origin.y)), f64::from(f32::from(FLOATING_WINDOW_SIZE.width)), f64::from(f32::from(FLOATING_WINDOW_SIZE.height)))
    }

    /// The displays as frames, for keeping windows on screen.
    fn display_frames(cx: &App) -> Vec<WindowFrame> {
        cx.displays().iter().map(|display| {
            let bounds = display.bounds();
            WindowFrame::new(f64::from(f32::from(bounds.origin.x)), f64::from(f32::from(bounds.origin.y)), f64::from(f32::from(bounds.size.width)), f64::from(f32::from(bounds.size.height)))
        }).collect()
    }

    /// Opens the gpui window for the model's floating window `id` — once
    /// this update is over. Opening a window draws its first frame at once,
    /// and that frame reads the workspace, which is being updated right now;
    /// so the window is opened from the app, then registered here, then given
    /// its area's contents.
    pub(crate) fn open_floating_window(&mut self, id: &WindowId, frame: WindowFrame, cx: &mut Context<Self>) {
        if self.floating.contains_key(id) {
            return;
        }
        let fallback = WindowFrame::new(80.0, 80.0, f64::from(f32::from(FLOATING_WINDOW_SIZE.width)), f64::from(f32::from(FLOATING_WINDOW_SIZE.height)));
        let frame = frame.clamped_to_displays(&Self::display_frames(cx), fallback);
        let _ = self.layout.set_frame(id, Some(frame));
        let workspace = cx.entity();
        let window_id = id.clone();
        cx.defer(move |cx| {
            if workspace.read(cx).floating.contains_key(&window_id) {
                return;
            }
            let bounds = Bounds::new(point(px(frame.x as f32), px(frame.y as f32)), size(px(frame.width as f32), px(frame.height as f32)));
            let mut options = WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), ..gpui_kit::component::TitleBar::window_options() };
            if let Some(titlebar) = options.titlebar.as_mut() {
                titlebar.title = Some("Atlas Financer — floating window".into());
            }
            // The build closure runs before `open_window` returns, so the
            // view and area it makes can be handed out through this slot.
            let mut opened: Option<(Entity<FloatingView>, Entity<DockArea>, Rc<WorkspaceSkin>)> = None;
            let for_build = workspace.clone();
            let id_for_build = window_id.clone();
            let result = cx.open_window(options, |window, cx| {
                let (area, skin) = WorkspaceSkin::dock_area(SharedString::from(format!("atlas-workspace-{id_for_build}")), None, window, cx);
                let view = cx.new(|cx| FloatingView::new(for_build.clone(), id_for_build.clone(), area.clone(), cx));
                opened = Some((view.clone(), area, skin));
                // The close box sends the panes home rather than losing them.
                let closing = for_build.clone();
                let closing_id = id_for_build.clone();
                window.on_window_should_close(cx, move |window, cx| {
                    closing.update(cx, |workspace, cx| workspace.floating_window_closed(&closing_id, window, cx));
                    true
                });
                cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
            });
            match (result, opened) {
                (Ok(handle), Some((view, area, skin))) => workspace.update(cx, |workspace, cx| workspace.register_floating_window(&window_id, frame, handle.into(), view, area, skin, cx)),
                (Err(err), _) => log::error!("workspace: floating window {window_id} could not be opened: {err}"),
                (Ok(_), None) => log::error!("workspace: floating window {window_id} opened without a view"),
            }
        });
    }

    /// Takes a freshly opened floating window into the workspace: its area is
    /// watched and filled from the model's tree for that window.
    fn register_floating_window(&mut self, id: &WindowId, frame: WindowFrame, handle: AnyWindowHandle, view: Entity<FloatingView>, area: Entity<DockArea>, skin: Rc<WorkspaceSkin>, cx: &mut Context<Self>) {
        let id_for_events = id.clone();
        // The area's events arrive after its update has finished, so its
        // window is free to be brought in by its handle. Subscribed at the
        // app's level, not the workspace's: a subscription made through this
        // context would run with the workspace already leased, and the
        // handler needs the floating window's context to reach it.
        let app: &mut App = &mut *cx;
        let events = app.subscribe(&area, move |_, event: &DockEvent, cx| {
            let id = id_for_events.clone();
            let Some(workspace) = workspace_of(cx) else {
                return;
            };
            let result = match event {
                DockEvent::LayoutChanged => handle.update(cx, |_, window, cx| workspace.update(cx, |workspace, cx| workspace.mirror_from_area(&id, window, cx))),
                DockEvent::DragDrop { item, target } => handle.update(cx, |_, window, cx| workspace.update(cx, |workspace, cx| workspace.item_dropped_on_dock(item, target, window, cx))),
            };
            if let Err(err) = result {
                log::warn!("workspace: floating window {id} could not take a dock event: {err}");
            }
        });
        self.floating.insert(id.clone(), FloatingWindow { handle, view, area, skin, _subscriptions: vec![events] });
        log::info!("workspace: floating window {id} opened at {frame:?}");
        if self.layout.window(id).is_none() {
            // The model dropped the window before it could open (its pane
            // moved on already): close it again.
            self.floating.remove(id);
            let _ = handle.update(cx, |_, window, _| window.remove_window());
            return;
        }
        self.rebuild_window(id, cx);
        self.focus_active_in(id, cx);
    }

    /// Rebuilds one window's area from the model, through the window's own handle.
    fn rebuild_window(&mut self, id: &WindowId, cx: &mut Context<Self>) {
        let (Some(area), Some(handle)) = (self.area_of(id), self.window_handle_of(id)) else {
            return;
        };
        let root = self.layout.window(id).and_then(|candidate| candidate.root.clone());
        self.note_expected_removals(&area, root.as_ref(), cx);
        let extent = self.layout.window(id).and_then(|candidate| candidate.frame).map(|frame| size(px(frame.width as f32), px(frame.height as f32))).unwrap_or(FLOATING_WINDOW_SIZE);
        let layout = match &root {
            Some(node) => self.dock_layout_of(node, extent, cx),
            None => DockLayout::h_split(),
        };
        if let Err(err) = handle.update(cx, |_, window, cx| area.update(cx, |area, cx| area.set_center(layout, window, cx))) {
            log::warn!("workspace: window {id} could not be rebuilt: {err}");
        }
        self.apply_active_flags(cx);
        cx.notify();
    }

    /// Focuses the active pane if it lives in window `id`, through the window's handle.
    fn focus_active_in(&self, id: &WindowId, cx: &mut Context<Self>) {
        let Some(active) = self.active_pane() else {
            return;
        };
        if self.window_of_pane(&active) != *id {
            return;
        }
        let (Some(view), Some(handle)) = (self.panes.get(&active), self.window_handle_of(id)) else {
            return;
        };
        let focus = view.read(cx).focus_handle(cx);
        let _ = handle.update(cx, |_, window, cx| window.focus(&focus, cx));
    }

    /// A floating window's close box was used: its panes go back to the main
    /// window and the window is forgotten.
    pub fn floating_window_closed(&mut self, id: &WindowId, window: &mut Window, cx: &mut Context<Self>) {
        self.gather_window(id, window, cx);
        // gather_window rebuilt the areas; if the model still lists the
        // window (it had no panes), drop it here.
        if let Some(index) = self.layout.windows.iter().position(|candidate| candidate.id == *id && candidate.role == WindowRole::Floating) {
            self.layout.windows.remove(index);
        }
        self.floating.remove(id);
        cx.notify();
    }

    /// Closes the gpui windows of floating windows the model no longer has,
    /// and opens windows for floating windows the model has and this view
    /// does not (a restored layout).
    fn close_vanished_windows(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let live: HashSet<WindowId> = self.layout.windows.iter().map(|candidate| candidate.id.clone()).collect();
        let vanished: Vec<WindowId> = self.floating.keys().filter(|id| !live.contains(*id)).cloned().collect();
        for id in vanished {
            if let Some(floating) = self.floating.remove(&id) {
                log::info!("workspace: floating window {id} has no panes left; closing it");
                if floating.handle == window.window_handle() {
                    window.remove_window();
                } else if let Err(err) = floating.handle.update(cx, |_, window, _| window.remove_window()) {
                    log::warn!("workspace: floating window {id} could not be closed: {err}");
                }
            }
        }
        let missing: Vec<(WindowId, WindowFrame)> = self
            .layout
            .windows
            .iter()
            .filter(|candidate| candidate.role == WindowRole::Floating && !self.floating.contains_key(&candidate.id))
            .map(|candidate| (candidate.id.clone(), candidate.frame.unwrap_or(WindowFrame::new(80.0, 80.0, 960.0, 720.0))))
            .collect();
        for (id, frame) in missing {
            self.open_floating_window(&id, frame, cx);
        }
    }

    /// The model stack shown by the engine group `node`, from any pane it displays.
    fn stack_of_group_any(&self, node: gpui_kit::component::dock::NodeId, cx: &App) -> Option<NodeId> {
        self.panes.iter().find_map(|(pane, view)| {
            let group = view.read(cx).group()?.upgrade()?;
            (group.read(cx).node() == node).then(|| self.layout.stack_of(pane)).flatten()
        })
    }

    /// Everything back to one pane: the active screen (or Today) alone in the
    /// window, as one undoable step. Application data is untouched.
    pub fn reset_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.layouts.current = None;
        let route = self.active_route(cx).unwrap_or(Route::Today);
        let before = self.layout.clone();
        // The panes the reset drops can be put back one by one.
        let active = self.active_pane();
        for pane in self.panes_in_order() {
            if Some(&pane) == active.as_ref() {
                continue;
            }
            self.sync_pane_definition(&pane, cx);
            if let Some(definition) = self.layout.pane(&pane).cloned() {
                self.closed.record(ClosedPane { definition, window: WindowId::main(), stack: None, index: 0, neighbour: None, neighbour_side: None, closed_at: chrono::Utc::now() });
            }
        }
        let mut fresh = WorkspaceLayout::new("Main").with_scope(before.scope.clone());
        fresh.set_limits(*before.limits());
        self.layout = fresh;
        match self.layout.open_pane(&WindowId::main(), kinds::definition_of(route), DockTarget::edge(Side::Right)) {
            Ok(pane) => {
                let workspace = cx.weak_entity();
                let app = self.app.clone();
                let bounds = self.pane_bounds.clone();
                let view = cx.new(|cx| PaneView::new(pane.clone(), route, app, workspace, bounds, cx));
                self.panes.clear();
                self.panes.insert(pane, view);
            }
            Err(err) => log::warn!("workspace: reset could not open {}: {err}", route.slug()),
        }
        self.record("Reset layout", before, cx);
        log::info!("workspace: layout reset to a single {} pane", route.slug());
        self.rebuild_area(window, cx);
        self.focus_active(window, cx);
        self.sync_chrome(cx);
    }

    /// The drag is over (dropped, cancelled, released elsewhere): no overlay.
    pub(crate) fn end_drag_overlay(&mut self, cx: &mut Context<Self>) {
        if self.drag.take().is_some() {
            self.set_skins_held(false, cx);
            self.skin.set_zone_hovered(false);
            for floating in self.floating.values() {
                floating.skin.set_zone_hovered(false);
            }
            cx.notify();
        }
    }

    /// A screen from the launcher was dropped on one of the overlay's zones.
    fn drop_item_on_zone(&mut self, item: &AnyDrag, in_window: &WindowId, zone: &Zone, window: &mut Window, cx: &mut Context<Self>) {
        match item.value().downcast_ref::<LaunchDrag>() {
            Some(launch) => self.open_on_zone(launch.route, in_window, zone, window, cx),
            None => self.end_drag_overlay(cx),
        }
    }

    /// A pane was dropped on one of the overlay's zones.
    fn drop_on_zone(&mut self, panel: PanelId, in_window: &WindowId, zone: &Zone, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pane) = self.pane_of_panel(panel) else {
            log::warn!("workspace: a panel that is not one of the workspace's panes was dropped on a drop zone; ignoring it");
            self.end_drag_overlay(cx);
            return;
        };
        if !zone.accepts_drops() {
            log::info!("workspace: pane {pane} dropped on a zone that refuses it ({}); nothing changed", zone.label);
            self.end_drag_overlay(cx);
            return;
        }
        log::info!("workspace: pane {pane} dropped on zone {} ({}) in {in_window}", zone.element_id(), zone.label);
        self.dock_pane(&pane, in_window, zone.target.clone(), true, window, cx);
    }

    /// The skin of `in_window`'s area.
    fn skin_of(&self, in_window: &WindowId) -> Option<&Rc<WorkspaceSkin>> {
        if *in_window == WindowId::main() { Some(&self.skin) } else { self.floating.get(in_window).map(|floating| &floating.skin) }
    }

    /// The bands and the preview for the drag in flight, over the area. Only
    /// drawn while a pane drag is active; the panes' own drop zones stay the
    /// engine's, underneath.
    fn render_drag_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let root = self.root_bounds.get();
        self.render_drag_overlay_for(&WindowId::main(), root.origin, root.size, cx)
    }

    /// The bands and the preview for the drag in flight, over the area of
    /// `in_window`, placed relative to that window's `origin` (where its pane
    /// area is drawn) and kept within `root_size`. Drawn only while a drag is
    /// in flight; the panes' own drop zones stay the engine's, underneath.
    pub(crate) fn render_drag_overlay_for(&self, in_window: &WindowId, origin: Point<Pixels>, root_size: Size<Pixels>, cx: &mut Context<Self>) -> Option<AnyElement> {
        let drag = self.drag.clone()?;
        if !cx.has_active_drag() {
            return None;
        }
        let drag = &drag;
        let area_entity = self.area_of(in_window)?;
        // The pointer is in another window of the workspace: that window
        // shows where the pane would land; this one shows nothing.
        if let Some((over, under)) = &drag.elsewhere {
            return if over == in_window { self.render_elsewhere_target(over, under.as_ref(), origin, cx) } else { None };
        }
        let skin = self.skin_of(in_window)?;
        // The zones lie where the cards are heading, not where the spring has
        // them this frame: a target that moved under the pointer would be
        // missed, and the strips need their room from the first frame.
        let field = Field { area: area_entity.read(cx).bounds(), inset: skin.target_inset(), gap: skin.target_gap() };
        let zones = dock_targets::zones_for(&self.layout, in_window, field, drag);
        if zones.is_empty() {
            return None;
        }
        let theme = cx.theme();
        let primary = theme.primary;
        let muted = theme.muted_foreground;
        let popover = theme.popover;
        let popover_foreground = theme.popover_foreground;
        let hovered = zones.iter().find(|zone| zone.hovered).cloned();
        skin.set_zone_hovered(hovered.is_some());
        let mut overlay = div().id("dock-targets").test_support().absolute().inset_0();
        for zone in zones {
            let relative = Bounds::new(zone.bounds.origin - origin, zone.bounds.size);
            let accepts = zone.accepts_drops();
            let fill = match (&zone.outcome, zone.hovered) {
                (Outcome::Refused(_), _) => muted.opacity(0.15),
                (_, true) => primary.opacity(0.4),
                (_, false) => primary.opacity(0.14),
            };
            let border = if accepts { primary.opacity(0.55) } else { muted.opacity(0.5) };
            let element_id = zone.element_id();
            let zone_for_drop = zone.clone();
            // The label doubles as the accessible name, so a screen reader
            // says what the zone does.
            let spoken = match &zone.outcome {
                Outcome::Refused(_) => format!("{} (not enough room)", zone.label),
                _ => zone.label.clone(),
            };
            let radius = match zone.kind {
                ZoneKind::Gap { .. } => px(6.),
                ZoneKind::Edge { .. } => px(5.),
            };
            let mut strip = div()
                .id(element_id)
                .test_support()
                .aria_label(spoken)
                .absolute()
                .left(relative.origin.x)
                .top(relative.origin.y)
                .w(relative.size.width)
                .h(relative.size.height)
                .bg(fill)
                .border_1()
                .border_color(border)
                .rounded(radius);
            // A cycled level's highlight: the span of what it docks beside.
            if zone.hovered && zone.drawn != zone.bounds {
                let span = Bounds::new(zone.drawn.origin - zone.bounds.origin, zone.drawn.size);
                strip = strip.child(div().absolute().left(span.origin.x).top(span.origin.y).w(span.size.width).h(span.size.height).bg(primary.opacity(0.5)).rounded(radius));
            }
            // A refused zone takes the drop as well — and then does nothing —
            // so the engine underneath does not treat it as a drop on the
            // pane's own zone. Panes and launcher screens both land here.
            let zone_for_item = zone.clone();
            let window_for_drop = in_window.clone();
            let window_for_item = in_window.clone();
            strip = strip
                .on_drop(cx.listener(move |this, dropped: &DragPanel, window, cx| {
                    cx.stop_propagation();
                    this.drop_on_zone(dropped.panel(), &window_for_drop, &zone_for_drop, window, cx);
                }))
                .on_drop(cx.listener(move |this, dropped: &AnyDrag, window, cx| {
                    cx.stop_propagation();
                    this.drop_item_on_zone(dropped, &window_for_item, &zone_for_item, window, cx);
                }));
            overlay = overlay.child(strip);
        }
        if let Some(zone) = hovered {
            // The rectangle the pane would take, and what will happen.
            if let Outcome::Allowed { preview } = &zone.outcome {
                let placed = field.place(*preview);
                let rect = Bounds::new(placed.origin - origin, placed.size);
                overlay = overlay.child(
                    div()
                        .id("dock-preview")
                        .test_support()
                        .absolute()
                        .left(rect.origin.x)
                        .top(rect.origin.y)
                        .w(rect.size.width)
                        .h(rect.size.height)
                        .bg(primary.opacity(0.12))
                        .border_1()
                        .border_color(primary)
                        .rounded(px(3.)),
                );
            }
            let label = match &zone.outcome {
                Outcome::Allowed { .. } => zone.label.clone(),
                Outcome::Unchanged => "Already here: dropping changes nothing".to_string(),
                Outcome::Refused(_) => "Not enough room here".to_string(),
            };
            // Beside the pointer, or above it when the pointer is near the
            // bottom (the bands people aim for are at the edges).
            let anchor = drag.pointer - origin;
            let room_below = root_size.height - anchor.y;
            let label_top = if room_below < px(48.) { anchor.y - px(32.) } else { anchor.y + px(16.) };
            let label_left = (anchor.x + px(16.)).min(root_size.width - px(260.)).max(px(0.));
            overlay = overlay.child(
                div()
                    .id("dock-band-label")
                    .test_support()
                    .aria_label(label.clone())
                    .absolute()
                    .left(label_left)
                    .top(label_top)
                    .px_2()
                    .py_1()
                    .rounded(px(4.))
                    .bg(popover)
                    .text_color(popover_foreground)
                    .text_xs()
                    .whitespace_nowrap()
                    .child(label),
            );
        }
        Some(overlay.into_any_element())
    }

    /// What a drag from another window shows in this one: the pane under the
    /// pointer lit up (the drop adds a tab to it), or the whole area when the
    /// pointer is over chrome (the drop goes to the active stack).
    fn render_elsewhere_target(&self, in_window: &WindowId, under: Option<&PaneId>, origin: Point<Pixels>, cx: &mut Context<Self>) -> Option<AnyElement> {
        let drag = self.drag.as_ref()?;
        let area = self.area_of(in_window)?.read(cx).bounds();
        let target = under.and_then(|pane| self.pane_bounds.borrow().get(pane).copied()).unwrap_or(area);
        let relative = Bounds::new(target.origin - origin, target.size);
        let primary = cx.theme().primary;
        let (popover, popover_foreground) = (cx.theme().popover, cx.theme().popover_foreground);
        let label = match (&drag.dragged, under) {
            (Dragged::Pane(_), Some(_)) => "Move here, as a tab",
            (Dragged::Pane(_), None) => "Move into this window",
            (Dragged::New(_), Some(_)) => "Open here, as a tab",
            (Dragged::New(_), None) => "Open in this window",
        };
        Some(
            div()
                .id("dock-targets")
                .test_support()
                .absolute()
                .inset_0()
                .child(
                    div()
                        .id("dock-elsewhere")
                        .test_support()
                        .aria_label(label)
                        .absolute()
                        .left(relative.origin.x)
                        .top(relative.origin.y)
                        .w(relative.size.width)
                        .h(relative.size.height)
                        .bg(primary.opacity(0.14))
                        .border_2()
                        .border_color(primary.opacity(0.7))
                        .rounded(px(8.))
                        .child(div().absolute().left(px(12.)).top(px(12.)).px_2().py_1().rounded(px(4.)).bg(popover).text_color(popover_foreground).text_xs().whitespace_nowrap().child(label)),
                )
                .into_any_element(),
        )
    }

    /// The pane behind an engine panel id.
    fn pane_of_panel(&self, panel: PanelId) -> Option<PaneId> {
        self.panes.iter().find(|(_, view)| PanelId::from(view.entity_id()) == panel).map(|(pane, _)| pane.clone())
    }

    /// The model stack shown by the engine group `node`: the stack of a pane
    /// (other than `except`) whose group that is.
    fn stack_of_group(&self, node: gpui_kit::component::dock::NodeId, except: &PaneId, cx: &App) -> Option<NodeId> {
        self.panes.iter().filter(|(pane, _)| *pane != except).find_map(|(pane, view)| {
            let group = view.read(cx).group()?.upgrade()?;
            (group.read(cx).node() == node).then(|| self.layout.stack_of(pane)).flatten()
        })
    }

    /// Reads the engine's layout back into the model after an edit the engine
    /// made on its own. An echo of the workspace's own edit changes nothing;
    /// a mirror that would leave the model invalid is reported and dropped.
    pub fn mirror_from_area(&mut self, in_window: &WindowId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(area) = self.area_of(in_window) else {
            return;
        };
        let state = area.read(cx).dump(cx);
        let main = in_window.clone();
        let previous_root = self.layout.window(&main).and_then(|window| window.root.clone());
        let mut candidate = self.layout.clone();
        let root = match mirror::layout_node_from_state(&state.center, previous_root.as_ref(), &mut candidate.ids) {
            Ok(root) => root,
            Err(err) => {
                self.mirror_failed(&format!("could not read the dock's layout back: {err}"));
                return;
            }
        };
        // Panes the engine no longer shows left the dock (a tab's close
        // button); their definitions go with them. Only this window's panes
        // can have left through this window's area: the definitions map
        // holds every window's. Panes it shows that the model never opened
        // cannot be mirrored at all.
        let present: HashSet<PaneId> = root.as_ref().map(LayoutNode::panes).unwrap_or_default().into_iter().collect();
        if let Some(unknown) = present.iter().find(|pane| !candidate.panes.contains_key(*pane)) {
            self.mirror_failed(&format!("the dock shows pane {unknown}, which the model never opened"));
            return;
        }
        let in_this_window: Vec<PaneId> = previous_root.as_ref().map(LayoutNode::panes).unwrap_or_default();
        let departed: Vec<(PaneId, PaneDefinition)> = in_this_window.iter().filter(|pane| !present.contains(*pane)).filter_map(|pane| candidate.panes.get(pane).map(|definition| (pane.clone(), definition.clone()))).collect();
        for (pane, _) in &departed {
            candidate.panes.remove(pane);
        }
        let displayed_before = mirror::displayed_panes(previous_root.as_ref());
        let Some(window_layout) = candidate.window_mut(&main) else {
            return;
        };
        window_layout.replace_root(root);
        let displayed_after = mirror::displayed_panes(window_layout.root.as_ref());
        // A tab the person chose becomes the active pane (a rebuild echo
        // displays nothing new; a closed tab's neighbour taking over does).
        let newly_displayed: Vec<PaneId> = displayed_after.difference(&displayed_before).cloned().collect();
        if let [pane] = newly_displayed.as_slice()
            && candidate.active_pane().as_ref() != Some(pane)
        {
            let _ = candidate.set_active_pane(pane);
        }
        let violations = candidate.validate();
        if !violations.is_empty() {
            let listed: Vec<String> = violations.iter().map(ToString::to_string).collect();
            self.mirror_failed(&format!("the mirrored layout breaks {} invariant(s): {}", listed.len(), listed.join("; ")));
            return;
        }
        let current_root = self.layout.window(&main).and_then(|window| window.root.as_ref());
        let new_root = candidate.window(&main).and_then(|window| window.root.as_ref());
        let structure_changed = mirror::structure_differs(current_root, new_root) || !departed.is_empty();
        let weights_changed = !mirror::trees_match(current_root, new_root, WEIGHT_TOLERANCE);
        let active_changed = candidate.active_pane() != self.layout.active_pane();
        if !structure_changed && !weights_changed && !active_changed {
            return;
        }
        // A rearrangement the engine made without a drop the workspace saw
        // (drops are applied to the model first, see `dock_pane`) is judged
        // by the model's minimum-size rule like any other split.
        if structure_changed
            && let Err(err) = ops::check_limits(current_root, new_root, self.layout.limits())
        {
            log::warn!("workspace: the dock's rearrangement is refused ({err}); putting the area back");
            self.rebuild_area(window, cx);
            window.push_notification(refused_split_toast(), cx);
            return;
        }
        // Divider drags of one split, one after another, are one undo step:
        // the entry keeps the layout from before the first drag.
        let resized = if structure_changed { Vec::new() } else { mirror::resized_splits(current_root, new_root, WEIGHT_TOLERANCE) };
        let joins_previous_resize = !structure_changed && weights_changed && matches!((&resized[..], &self.last_resized_split), ([split], Some(last)) if split == last) && self.history.undo_label() == Some(RESIZE_LABEL);
        log::info!(
            "workspace: mirrored the dock's layout (structure changed: {structure_changed}, weights changed: {weights_changed}, active changed: {active_changed}, resized splits: {resized:?}); grid:\n{}",
            atlas_workspace::grid::render_window(new_root, 12, 4, |pane| pane.minted_counter().map(|n| char::from_digit((n % 36) as u32, 36).unwrap_or('?')).unwrap_or('?'))
        );
        let previous = std::mem::replace(&mut self.layout, candidate);
        if structure_changed {
            self.record(MOVE_LABEL, previous, cx);
        } else if weights_changed && !joins_previous_resize {
            self.record(RESIZE_LABEL, previous, cx);
            self.last_resized_split = match resized.as_slice() {
                [split] => Some(split.clone()),
                _ => None,
            };
        } else {
            self.touch("dock mirrored", cx);
        }
        for (pane, definition) in departed {
            log::info!("workspace: pane {pane} ({}) was closed from its tab", definition.kind);
            self.panes.remove(&pane);
            self.closed.record(ClosedPane { definition, window: main.clone(), stack: None, index: 0, neighbour: None, neighbour_side: None, closed_at: chrono::Utc::now() });
        }
        self.apply_active_flags(cx);
        if active_changed {
            self.focus_active(window, cx);
        }
        self.sync_chrome(cx);
        cx.notify();
    }

    /// A mirror that cannot be accepted: the previous model stays, and the
    /// team hears about it, because the screen and the model now disagree.
    fn mirror_failed(&self, reason: &str) {
        alerting::report(Level::Error, format!("workspace: {reason}; keeping the previous layout model"));
    }

    // ----- the area ----------------------------------------------------------------------

    /// Rebuilds every window's dock area from the model: every stack a tab
    /// group of the existing pane views, every split's slots sized from its
    /// weights. The pane views survive, so their scroll and history do too.
    /// Floating windows the model dropped are closed; ones it has and this
    /// view does not are opened.
    pub fn rebuild_area(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A layout changing under a drag (an undo, a loaded layout, a reset)
        // ends the drag: the pane in hand may not exist any more, and the
        // engine would be handed a drop for a panel it no longer has.
        if self.drag.is_some() {
            log::info!("workspace: the layout changed while a drag was in flight; the drag ends with nothing dropped");
            cx.stop_active_drag(window);
            self.end_drag_overlay(cx);
        }
        self.close_vanished_windows(window, cx);
        let ids: Vec<WindowId> = self.layout.windows.iter().map(|candidate| candidate.id.clone()).collect();
        for id in ids {
            let Some(area) = self.area_of(&id) else {
                continue;
            };
            let root = self.layout.window(&id).and_then(|candidate| candidate.root.clone());
            self.note_expected_removals(&area, root.as_ref(), cx);
            let extent = self.area_extent(&area, &id, window, cx);
            let layout = match &root {
                Some(node) => self.dock_layout_of(node, extent, cx),
                None => DockLayout::h_split(),
            };
            self.in_window(&id, window, cx, |window, cx| area.update(cx, |area, cx| area.set_center(layout, window, cx)));
        }
        self.apply_active_flags(cx);
        cx.notify();
    }

    /// Panes `area` shows that `root` (the tree it is about to be rebuilt
    /// from) no longer has, although the model still has them: they moved to
    /// another window, and the engine's "removed" notice for each is expected.
    fn note_expected_removals(&mut self, area: &Entity<DockArea>, root: Option<&LayoutNode>, cx: &App) {
        let keeps: HashSet<PaneId> = root.map(LayoutNode::panes).unwrap_or_default().into_iter().collect();
        let shown: Vec<PanelId> = area.read(cx).layout(gpui_kit::component::dock::DockPlacement::Center).map(|tree| tree.panels().collect()).unwrap_or_default();
        for panel in shown {
            if let Some(pane) = self.pane_of_panel(panel)
                && !keeps.contains(&pane)
                && self.layout.pane(&pane).is_some()
            {
                self.expected_removals.insert(pane);
            }
        }
    }

    /// An area's size — measured, or its window's size before the first
    /// frame. Slot sizes are shares of this, so only the ratios matter.
    fn area_extent(&self, area: &Entity<DockArea>, id: &WindowId, window: &Window, cx: &App) -> Size<Pixels> {
        let measured = area.read(cx).bounds().size;
        if measured.width > px(0.) && measured.height > px(0.) {
            return measured;
        }
        let viewport = if *id == WindowId::main() {
            window.viewport_size()
        } else {
            self.layout.window(id).and_then(|candidate| candidate.frame).map(|frame| size(px(frame.width as f32), px(frame.height as f32))).unwrap_or(FLOATING_WINDOW_SIZE)
        };
        size(viewport.width.max(px(1.)), viewport.height.max(px(1.)))
    }

    /// The engine's description of a model subtree over `extent`.
    fn dock_layout_of(&self, node: &LayoutNode, extent: Size<Pixels>, cx: &App) -> DockLayout {
        match node {
            LayoutNode::Stack { panes, active_pane_id, .. } => {
                let mut tabs = DockLayout::tabs();
                let mut shown = Vec::new();
                for pane in panes {
                    match self.panes.get(pane) {
                        Some(view) => {
                            tabs = tabs.panel_view(panel_handle(view.clone()), cx);
                            shown.push(pane.clone());
                        }
                        None => log::error!("workspace: pane {pane} is in the layout but has no view; it is left out of the dock"),
                    }
                }
                let active = active_pane_id.as_ref().and_then(|active| shown.iter().position(|pane| pane == active)).unwrap_or(0);
                tabs.active_index(active)
            }
            LayoutNode::Split { axis, children, weights, .. } => {
                let shares = atlas_workspace::layout::effective_weights(weights, children.len());
                let along = match axis {
                    Axis::Horizontal => extent.width,
                    Axis::Vertical => extent.height,
                };
                let mut split = match axis {
                    Axis::Horizontal => DockLayout::h_split(),
                    Axis::Vertical => DockLayout::v_split(),
                };
                for (child, share) in children.iter().zip(shares) {
                    let slot = along * share as f32;
                    let child_extent = match axis {
                        Axis::Horizontal => size(slot, extent.height),
                        Axis::Vertical => size(extent.width, slot),
                    };
                    split = split.child(self.dock_layout_of(child, child_extent, cx), Some(slot));
                }
                split
            }
        }
    }

    /// Selects `pane`'s tab in its group when another tab is displayed.
    fn select_tab_in_area(&self, pane: &PaneId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.panes.get(pane) else {
            return;
        };
        if view.read(cx).is_displayed() {
            return;
        }
        let Some(group) = view.read(cx).group().and_then(WeakEntity::upgrade) else {
            return;
        };
        let panel = PanelId::from(view.entity_id());
        let index = group.read(cx).panels().iter().position(|candidate| candidate.panel_id(cx) == panel);
        if let Some(index) = index {
            let in_window = self.window_of_pane(pane);
            self.in_window(&in_window, window, cx, |window, cx| group.update(cx, |group, cx| group.select_tab(index, window, cx)));
        }
    }

    /// Tells every pane whether it is the active one, and that the layout
    /// changed (what a pane may show depends on what other windows show).
    fn apply_active_flags(&self, cx: &mut Context<Self>) {
        let active = self.layout.active_pane();
        for (pane, view) in &self.panes {
            let is_active = active.as_ref() == Some(pane);
            view.update(cx, |pane, cx| {
                pane.set_workspace_active(is_active, cx);
                cx.notify();
            });
        }
    }

    /// The window that already shows a pane of the same screen family as
    /// `pane`, when that window takes precedence — the main window over any
    /// floating one, an earlier floating window over a later one. A screen
    /// family's controls (its search field, its filters, its tables) are one
    /// set on the app, and one set can be drawn in one window at a time;
    /// the pane that loses shows a placeholder pointing at the winner.
    ///
    /// `route` is what `pane` shows; the pane passes it in because it asks
    /// while it is being rendered, when it cannot be read.
    pub fn shown_elsewhere(&self, pane: &PaneId, route: Route, cx: &App) -> Option<(WindowId, PaneId)> {
        let family = route.destination()?;
        let mine = self.window_of_pane(pane);
        let precedence = |window: &WindowId| if *window == WindowId::main() { (0, String::new()) } else { (1, window.to_string()) };
        let mut winner: Option<(WindowId, PaneId)> = None;
        for window in &self.layout.windows {
            if window.id == mine || precedence(&window.id) >= precedence(&mine) {
                continue;
            }
            for other in mirror::displayed_panes(window.root.as_ref()) {
                let Some(other_route) = self.pane_route(&other, cx) else {
                    continue;
                };
                if other_route.destination() == Some(family) {
                    winner = Some((window.id.clone(), other));
                }
            }
        }
        winner
    }

    /// Brings the window and pane that already show a screen to the front.
    pub(crate) fn show_pane_in_its_window(&mut self, pane: &PaneId, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.set_active_pane(pane, window, cx);
        let in_window = self.window_of_pane(pane);
        self.in_window(&in_window, window, cx, |window, _| window.activate_window());
    }

    /// Gives the active pane keyboard focus, in whichever window it is, so
    /// the workspace commands reach it.
    pub(crate) fn focus_active(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(active) = self.active_pane() else {
            // No pane, but the workspace's keys — redo, reopen — still have
            // to reach it: the workspace itself takes the focus.
            if window.window_handle() == self.window {
                window.focus(&self.focus_handle, cx);
            }
            return;
        };
        if let Some(view) = self.panes.get(&active) {
            let handle = view.read(cx).focus_handle(cx);
            let in_window = self.window_of_pane(&active);
            self.in_window(&in_window, window, cx, |window, cx| window.focus(&handle, cx));
        }
    }

    /// The launcher highlight and the frame label follow the active pane.
    /// Never called while the app is being updated (see the module docs).
    pub(crate) fn sync_chrome(&self, cx: &mut Context<Self>) {
        if let Some(route) = self.active_route(cx) {
            self.app.update(cx, |app, cx| app.set_route_for_chrome(route, cx));
        }
    }

    /// Tells the person about a refused command; a no-op is only logged.
    pub(crate) fn report<T>(&self, result: &Result<T, OpError>, window: &mut Window, cx: &mut Context<Self>) {
        match result {
            Ok(_) | Err(OpError::NoOp) => {}
            Err(err) => {
                log::warn!("workspace: command refused: {err}");
                window.push_notification(Notification::warning(err.to_string()), cx);
            }
        }
    }

    // ----- commands ------------------------------------------------------------------------

    pub(crate) fn command_split(&mut self, side: Side, window: &mut Window, cx: &mut Context<Self>) {
        let Some(route) = self.active_route(cx) else {
            log::warn!("workspace: split asked with no pane open");
            return;
        };
        let _ = self.split_active(side, route, window, cx);
    }

    pub(crate) fn command_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.close_active(window, cx);
        self.report(&result, window, cx);
    }

    pub(crate) fn command_focus_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.focus_next_pane(window, cx);
        self.report(&result, window, cx);
    }

    pub(crate) fn command_focus_previous(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.focus_previous_pane(window, cx);
        self.report(&result, window, cx);
    }

    /// No neighbour that way is not an error worth a toast.
    pub(crate) fn command_focus_direction(&mut self, direction: Direction, window: &mut Window, cx: &mut Context<Self>) {
        match self.focus_direction(direction, window, cx) {
            Ok(()) | Err(OpError::NoOp) => {}
            other => self.report(&other, window, cx),
        }
    }

    pub(crate) fn command_move_direction(&mut self, direction: Direction, window: &mut Window, cx: &mut Context<Self>) {
        match self.move_active(direction, window, cx) {
            Ok(()) | Err(OpError::NoOp) => {}
            other => self.report(&other, window, cx),
        }
    }

    pub(crate) fn command_duplicate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.duplicate_active(window, cx);
    }

    pub(crate) fn command_reopen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(OpError::NoOp) = self.reopen_last_closed(window, cx) {
            window.push_notification("No closed pane to reopen.", cx);
        }
    }

    pub(crate) fn command_zoom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.toggle_zoom(window, cx);
        self.report(&result, window, cx);
    }

    pub(crate) fn command_back(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.back_active(cx).is_some() {
            self.sync_chrome(cx);
        }
    }

    /// The active pane goes to a window of its own.
    pub(crate) fn command_detach(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(active) = self.active_pane() else {
            log::warn!("workspace: detach asked with no pane open");
            return;
        };
        let result = self.detach_pane(&active, None, window, cx);
        self.report(&result, window, cx);
    }

    /// `Move to new window` / `Move to the main window` from a pane's menu.
    pub(crate) fn move_pane_from_menu(&mut self, pane: &PaneId, target: Option<WindowId>, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.set_active_pane(pane, window, cx);
        let result = match target {
            None => self.detach_pane(pane, None, window, cx).map(|_| ()),
            Some(target) => self.move_pane_to_window(pane, &target, window, cx),
        };
        self.report(&result, window, cx);
    }

    // ----- rendering -------------------------------------------------------------------------

    /// The empty workspace: what the column shows when every pane is closed.
    fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let workspace = cx.weak_entity();
        v_flex()
            .id("workspace-empty")
            .test_support()
            .size_full()
            .items_center()
            .justify_center()
            .gap_4()
            .child(Icon::new(IconName::LayoutGrid).large().text_color(muted))
            .child(div().text_lg().font_weight(FontWeight::MEDIUM).child("Open a pane"))
            .child(div().text_sm().text_color(muted).child("Every screen opens in a pane of this window. Choose the first one."))
            .child(Button::new("workspace-add-pane").primary().icon(IconName::Plus).label("Add pane").dropdown_menu(move |menu, _, _| {
                let destinations = Destination::GROUPS.iter().flat_map(|group| group.iter()).chain(std::iter::once(&Destination::Settings));
                destinations.fold(menu, |menu, destination| {
                    let destination = *destination;
                    let workspace = workspace.clone();
                    menu.item(PopupMenuItem::new(destination.label()).icon(destination.icon()).on_click(move |_, window, cx| {
                        let _ = workspace.update(cx, |workspace, cx| {
                            let _ = workspace.open(destination.home(), Intent::Open, window, cx);
                        });
                    }))
                })
            }))
    }
}

/// The workspace, from a floating window's event: the app keeps its handle.
fn workspace_of(cx: &App) -> Option<Entity<WorkspaceView>> {
    cx.try_global::<crate::actions::AppHandle>().and_then(|handle| handle.0.upgrade()).and_then(|app| app.read(cx).workspace_handle()).and_then(|workspace| workspace.upgrade())
}

/// The model's side for the engine's placement of a drop zone.
fn side_of(placement: Placement) -> Side {
    match placement {
        Placement::Left => Side::Left,
        Placement::Right => Side::Right,
        Placement::Top => Side::Top,
        Placement::Bottom => Side::Bottom,
    }
}

/// The toast shown when a drop would split a pane below the minimum size.
/// The text is its own observed element so a test can read it.
fn refused_split_toast() -> Notification {
    Notification::new().with_type(NotificationType::Warning).content(|_, _, _| {
        div().id(REFUSED_SPLIT_TOAST_ID).test_support().text_sm().aria_label(REFUSED_SPLIT_MESSAGE).child(REFUSED_SPLIT_MESSAGE).into_any_element()
    })
}

impl Render for WorkspaceView {
    /// The dock area while the main window has panes, the empty state
    /// otherwise. The shell embeds this view cached at the content column's
    /// size; the panes are cached views of their own inside the dock.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders += 1;
        let has_panes = self.layout.main_window().is_some_and(|main| !main.is_empty());
        // If the drag ended anywhere the overlay did not see (a release over
        // the launcher, a drop the engine took), the next frame clears it.
        if self.drag.is_some() && !cx.has_active_drag() {
            self.drag = None;
        }
        let recorded = self.root_bounds.clone();
        let recorder = canvas(
            move |bounds, _, _| {
                recorded.set(bounds);
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        let overlay = self.render_drag_overlay(cx);
        let root = div()
            .id("workspace")
            .key_context(commands::KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .relative()
            .size_full()
            .on_drag_move(cx.listener(|this, event: &DragMoveEvent<DragPanel>, window, cx| {
                let panel = event.drag(cx).panel();
                this.follow_drag(panel, event.event.position, window, cx);
            }))
            .on_drag_move(cx.listener(|this, event: &DragMoveEvent<AnyDrag>, window, cx| {
                let item = event.drag(cx).clone();
                this.follow_launch_drag(&item, event.event.position, window, cx);
            }))
            .capture_any_mouse_up(cx.listener(|this, _, _, cx| this.end_drag_overlay(cx)))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, event: &MouseUpEvent, window, cx| this.drag_released_outside(event.position, window, cx)))
            .child(recorder);
        let root = commands::attach(root, cx.entity());
        if has_panes { root.child(self.area.clone()).children(overlay) } else { root.child(self.render_empty(cx)) }
    }
}
