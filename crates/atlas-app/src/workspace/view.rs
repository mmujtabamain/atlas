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
//! | the active pane changed | the pane's tab is selected in its group (`TabGroup::select_tab`); the pane is focused; the sidebar follows through `AtlasApp::set_route_for_chrome` |
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
use gpui_kit::component::dock::{DockArea, DockEvent, DockLayout, DockSkin, InsertTarget, PanelId, TabGroup, TabGroupEvent, panel_handle};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::notification::{Notification, NotificationType};
use gpui_kit::component::{
    ActiveTheme as _, Icon, Placement, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    v_flex,
};
use gpui_kit::*;

use std::cell::{Cell, RefCell};

use super::commands::{self, Back, ClosePane, FocusNextPane, SplitBelow, SplitRight};
use super::dock_targets::{self, Band, DragInFlight, Outcome};
use super::kinds;
use super::mirror;
use super::pane::{PaneBounds, PaneView};
use gpui_kit::component::dock::DragPanel;
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
    app: Entity<AtlasApp>,
    window: AnyWindowHandle,
    layout: WorkspaceLayout,
    panes: HashMap<PaneId, Entity<PaneView>>,
    area: Entity<DockArea>,
    skin: Rc<DockSkin>,
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
    _subscriptions: Vec<Subscription>,
}

impl WorkspaceView {
    /// The workspace for `app`'s window. Starts empty, and with the household
    /// the launch opened (if it is usable already) the moment it exists.
    pub fn new(app: Entity<AtlasApp>, launch: &Launch, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (area, skin) = DockSkin::dock_area("atlas-workspace", None, window, cx);
        // There are no side docks to collapse; the affordance would be noise.
        skin.set_toggle_button_visible(false, cx);
        let area_events = cx.subscribe_in(&area, window, |this, _, event: &DockEvent, window, cx| match event {
            DockEvent::LayoutChanged => this.mirror_from_area(window, cx),
            DockEvent::DragDrop { .. } => log::info!("workspace: something was dropped on the dock; dropping into the workspace is not wired yet"),
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
                    cx.stop_propagation();
                    let _ = this_for_keys.update(cx, |this, cx| {
                        if let Some(drag) = this.drag.as_mut() {
                            drag.level_offset += 1;
                            log::info!("workspace: docking levels cycled to offset {}", drag.level_offset);
                            cx.notify();
                        }
                    });
                }
                _ => {}
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

    /// The dock area the panes are shown in.
    pub fn area(&self) -> &Entity<DockArea> {
        &self.area
    }

    /// The skin the dock area wears.
    pub fn skin(&self) -> &Rc<DockSkin> {
        &self.skin
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
    /// divider drags that share an entry.
    fn record(&mut self, label: impl Into<String>, before: WorkspaceLayout) {
        self.last_resized_split = None;
        self.history.push(label, before);
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
        self.layout = WorkspaceLayout::new("Main").with_scope(Scope::household(identity));
        self.history = LayoutHistory::default();
        self.closed = ClosedPanes::default();
        self.panes.clear();
        let first = match self.app.read(cx).route() {
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

    /// Everything goes: the household is closed, Welcome takes the column.
    fn clear_for_closed_household(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("workspace: household closed; removing every pane ({} open)", self.panes.len());
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
        let resolution = resolver::resolve(&self.layout, &definition.kind, definition.resource.as_ref(), intent);
        log::info!("workspace: open {} ({intent:?}) → {resolution:?}", route.slug());
        let result = match resolution {
            Resolution::Focus(pane) => self.set_active_pane(&pane, window, cx).map(|()| pane),
            Resolution::Create { window: target_window, target } => self.create_pane(route, definition, &target_window, target, window, cx),
            Resolution::CreateWindow => {
                log::info!("workspace: floating windows are not available yet; opening {} in the active stack", route.slug());
                let (target_window, target) = self.layout.active_window().map(|window| (window.id.clone(), window.default_target())).unwrap_or((WindowId::main(), DockTarget::edge(Side::Right)));
                self.create_pane(route, definition, &target_window, target, window, cx)
            }
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
        let result = self.create_pane(route, kinds::definition_of(route), &WindowId::main(), target, window, cx);
        self.report(&result, window, cx);
        result
    }

    /// Opens `route` as a new tab of the stack that holds `pane`.
    pub fn stack_onto(&mut self, pane: &PaneId, route: Route, window: &mut Window, cx: &mut Context<Self>) -> Result<PaneId, OpError> {
        let stack = self.layout.stack_of(pane).ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
        let result = self.create_pane(route, kinds::definition_of(route), &WindowId::main(), DockTarget::tab(stack), window, cx);
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
        self.record(format!("Open {}", route.title()), before);
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
        self.closed.record(closed);
        self.record(format!("Close {title}"), before);
        if let Some(view) = self.panes.remove(pane) {
            self.area.update(cx, |area, cx| area.remove_panel(view, window, cx));
        }
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
    /// selected in its group, focused, and the sidebar following its route.
    pub fn set_active_pane(&mut self, pane: &PaneId, window: &mut Window, cx: &mut Context<Self>) -> Result<(), OpError> {
        let changed = self.layout.active_pane().as_ref() != Some(pane);
        self.layout.set_active_pane(pane)?;
        if changed {
            let slug = self.pane_route(pane, cx).map(Route::slug).unwrap_or("?");
            log::info!("workspace: active pane {pane} ({slug})");
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

    /// Replaces a split's weights, in the model and then on screen.
    pub fn resize_split(&mut self, split: &NodeId, weights: &[f64], window: &mut Window, cx: &mut Context<Self>) -> Result<(), OpError> {
        let before = self.layout.clone();
        self.layout.resize(split, weights)?;
        self.record(RESIZE_LABEL, before);
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
        self.record(label, before);
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
    fn sync_pane_entities(&mut self, cx: &mut Context<Self>) {
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
    fn sync_pane_definition(&mut self, pane: &PaneId, cx: &App) {
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
        if let Err(err) = self.layout.replace_pane(pane, definition) {
            log::warn!("workspace: pane {pane} shows {} but its definition could not be updated: {err}", view.route().slug());
        }
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
        self.record(format!("Replace pane with {}", route.title()), before);
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
                self.record(format!("Close {title}"), before);
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
        self.dock_pane(&pane, dock_target, activate, window, cx);
    }

    /// Moves `pane` to `target` in the model and shows the result — the one
    /// way a drop becomes a layout change. `activate` false keeps the tab the
    /// target stack was displaying (a drop past the last tab lands in the
    /// background). A refused move leaves the model as it was and puts the
    /// area back to it; a move that changes nothing does the same, because the
    /// engine may have redistributed sizes while deciding nothing changed.
    pub fn dock_pane(&mut self, pane: &PaneId, target: DockTarget, activate: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.end_drag_overlay(cx);
        let before = self.layout.clone();
        let displayed_before = self.layout.main_window().and_then(|main| main.root.as_ref()).and_then(|root| target.node().and_then(|node| root.find(node))).and_then(|stack| stack.active_pane().cloned());
        match self.layout.move_pane(pane, &WindowId::main(), target.clone()) {
            Ok(()) => {
                if !activate
                    && let Some(displayed) = displayed_before
                    && displayed != *pane
                {
                    let _ = self.layout.set_active_pane(&displayed);
                }
                self.record(MOVE_LABEL, before);
                log::info!(
                    "workspace: moved pane {pane} to {target:?}; grid:\n{}",
                    atlas_workspace::grid::render_window(self.layout.main_window().and_then(|main| main.root.as_ref()), 12, 4, |pane| pane.minted_counter().map(|n| char::from_digit((n % 36) as u32, 36).unwrap_or('?')).unwrap_or('?'))
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
    fn follow_drag(&mut self, panel: PanelId, pointer: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(pane) = self.pane_of_panel(panel) else {
            return;
        };
        let level_offset = self.drag.as_ref().filter(|drag| drag.pane == pane).map(|drag| drag.level_offset).unwrap_or(0);
        let next = DragInFlight { pane, pointer, level_offset };
        if self.drag.as_ref() != Some(&next) {
            self.drag = Some(next);
            cx.notify();
        }
    }

    /// The drag is over (dropped, cancelled, released elsewhere): no overlay.
    fn end_drag_overlay(&mut self, cx: &mut Context<Self>) {
        if self.drag.take().is_some() {
            cx.notify();
        }
    }

    /// A pane was dropped on one of the overlay's bands.
    fn drop_on_band(&mut self, panel: PanelId, band: &Band, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pane) = self.pane_of_panel(panel) else {
            log::warn!("workspace: a panel that is not one of the workspace's panes was dropped on a docking band; ignoring it");
            self.end_drag_overlay(cx);
            return;
        };
        if !band.accepts_drops() {
            log::info!("workspace: pane {pane} dropped on a band that refuses it ({}); nothing changed", band.label);
            self.end_drag_overlay(cx);
            return;
        }
        log::info!("workspace: pane {pane} dropped on band {} ({})", band.element_id(), band.label);
        self.dock_pane(&pane, band.target.clone(), true, window, cx);
    }

    /// The bands and the preview for the drag in flight, over the area. Only
    /// drawn while a pane drag is active; the panes' own drop zones stay the
    /// engine's, underneath.
    fn render_drag_overlay(&self, drag: &DragInFlight, cx: &mut Context<Self>) -> Option<AnyElement> {
        let drawn = self.pane_bounds.borrow();
        let bands = dock_targets::bands_for(&self.layout, &WindowId::main(), &drawn, drag);
        drop(drawn);
        if bands.is_empty() {
            return None;
        }
        let origin = self.root_bounds.get().origin;
        let area = self.area.read(cx).bounds();
        let theme = cx.theme();
        let primary = theme.primary;
        let muted = theme.muted_foreground;
        let popover = theme.popover;
        let popover_foreground = theme.popover_foreground;
        let hovered = bands.iter().find(|band| band.hovered).cloned();
        let mut overlay = div().id("dock-targets").test_support().absolute().inset_0();
        for band in bands {
            let relative = Bounds::new(band.bounds.origin - origin, band.bounds.size);
            let accepts = band.accepts_drops();
            let fill = match (&band.outcome, band.hovered) {
                (Outcome::Refused(_), _) => muted.opacity(0.15),
                (_, true) => primary.opacity(0.45),
                (_, false) => primary.opacity(0.18),
            };
            let border = if accepts { primary.opacity(0.7) } else { muted.opacity(0.5) };
            let element_id = band.element_id();
            let band_for_drop = band.clone();
            // The label doubles as the accessible name, so a screen reader
            // says what the band does.
            let spoken = match &band.outcome {
                Outcome::Refused(_) => format!("{} (not enough room)", band.label),
                _ => band.label.clone(),
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
                .rounded(px(2.));
            // A refused band takes the drop as well — and then does nothing —
            // so the engine underneath does not treat it as a drop on the
            // pane's own edge zone.
            strip = strip.on_drop(cx.listener(move |this, dropped: &DragPanel, window, cx| {
                cx.stop_propagation();
                this.drop_on_band(dropped.panel(), &band_for_drop, window, cx);
            }));
            overlay = overlay.child(strip);
        }
        if let Some(band) = hovered {
            // The rectangle the pane would take, and what will happen.
            if let Outcome::Allowed { preview } = &band.outcome {
                let rect = Bounds::new(
                    point(area.origin.x + area.size.width * preview.x as f32 - origin.x, area.origin.y + area.size.height * preview.y as f32 - origin.y),
                    size(area.size.width * preview.width as f32, area.size.height * preview.height as f32),
                );
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
            let label = match &band.outcome {
                Outcome::Allowed { .. } => band.label.clone(),
                Outcome::Unchanged => "Already here: dropping changes nothing".to_string(),
                Outcome::Refused(_) => "Not enough room here".to_string(),
            };
            // Beside the pointer, or above it when the pointer is near the
            // bottom (the bands people aim for are at the edges).
            let anchor = drag.pointer - origin;
            let room_below = self.root_bounds.get().size.height - anchor.y;
            let label_top = if room_below < px(48.) { anchor.y - px(32.) } else { anchor.y + px(16.) };
            let label_left = (anchor.x + px(16.)).min(self.root_bounds.get().size.width - px(260.)).max(px(0.));
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
    pub fn mirror_from_area(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.area.read(cx).dump(cx);
        let main = WindowId::main();
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
        // button); their definitions go with them. Panes it shows that the
        // model never opened cannot be mirrored at all.
        let present: HashSet<PaneId> = root.as_ref().map(LayoutNode::panes).unwrap_or_default().into_iter().collect();
        if let Some(unknown) = present.iter().find(|pane| !candidate.panes.contains_key(*pane)) {
            self.mirror_failed(&format!("the dock shows pane {unknown}, which the model never opened"));
            return;
        }
        let departed: Vec<(PaneId, PaneDefinition)> = candidate.panes.iter().filter(|(pane, _)| !present.contains(*pane)).map(|(pane, definition)| (pane.clone(), definition.clone())).collect();
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
            self.record(MOVE_LABEL, previous);
        } else if weights_changed && !joins_previous_resize {
            self.record(RESIZE_LABEL, previous);
            self.last_resized_split = match resized.as_slice() {
                [split] => Some(split.clone()),
                _ => None,
            };
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

    /// Rebuilds the dock area from the model's main window: every stack a tab
    /// group of the existing pane views, every split's slots sized from its
    /// weights. The pane views survive, so their scroll and history do too.
    pub fn rebuild_area(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let root = self.layout.main_window().and_then(|main| main.root.clone());
        let layout = match &root {
            Some(node) => {
                let extent = self.area_extent(window, cx);
                self.dock_layout_of(node, extent, cx)
            }
            None => DockLayout::h_split(),
        };
        self.area.update(cx, |area, cx| area.set_center(layout, window, cx));
        self.apply_active_flags(cx);
        cx.notify();
    }

    /// The area's size — measured, or the content column's size before the
    /// first frame. Slot sizes are shares of this, so only the ratios matter.
    fn area_extent(&self, window: &Window, cx: &App) -> Size<Pixels> {
        let measured = self.area.read(cx).bounds().size;
        if measured.width > px(0.) && measured.height > px(0.) {
            return measured;
        }
        let viewport = window.viewport_size();
        let sidebar = crate::shell::sidebar_width(self.app.read(cx).sidebar_collapsed());
        size((viewport.width - sidebar).max(px(1.)), viewport.height.max(px(1.)))
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
            group.update(cx, |group, cx| group.select_tab(index, window, cx));
        }
    }

    /// Tells every pane whether it is the active one.
    fn apply_active_flags(&self, cx: &mut Context<Self>) {
        let active = self.layout.active_pane();
        for (pane, view) in &self.panes {
            let is_active = active.as_ref() == Some(pane);
            view.update(cx, |pane, cx| pane.set_workspace_active(is_active, cx));
        }
    }

    /// Gives the active pane keyboard focus, so the workspace commands reach it.
    fn focus_active(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(view) = self.active_pane().and_then(|active| self.panes.get(&active)) {
            let handle = view.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
        }
    }

    /// The sidebar highlight and the frame label follow the active pane.
    /// Never called while the app is being updated (see the module docs).
    fn sync_chrome(&self, cx: &mut Context<Self>) {
        if let Some(route) = self.active_route(cx) {
            self.app.update(cx, |app, cx| app.set_route_for_chrome(route, cx));
        }
    }

    /// Tells the person about a refused command; a no-op is only logged.
    fn report<T>(&self, result: &Result<T, OpError>, window: &mut Window, cx: &mut Context<Self>) {
        match result {
            Ok(_) | Err(OpError::NoOp) => {}
            Err(err) => {
                log::warn!("workspace: command refused: {err}");
                window.push_notification(Notification::warning(err.to_string()), cx);
            }
        }
    }

    // ----- commands ------------------------------------------------------------------------

    fn command_split(&mut self, side: Side, window: &mut Window, cx: &mut Context<Self>) {
        let Some(route) = self.active_route(cx) else {
            log::warn!("workspace: split asked with no pane open");
            return;
        };
        let _ = self.split_active(side, route, window, cx);
    }

    fn command_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.close_active(window, cx);
        self.report(&result, window, cx);
    }

    fn command_focus_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.focus_next_pane(window, cx);
        self.report(&result, window, cx);
    }

    fn command_back(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.back_active(cx).is_some() {
            self.sync_chrome(cx);
        }
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
        // the sidebar, a drop the engine took), the next frame clears it.
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
        let overlay = self.drag.clone().and_then(|drag| self.render_drag_overlay(&drag, cx));
        let root = div()
            .id("workspace")
            .key_context(commands::KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .relative()
            .size_full()
            .on_drag_move(cx.listener(|this, event: &DragMoveEvent<DragPanel>, _, cx| {
                let panel = event.drag(cx).panel();
                this.follow_drag(panel, event.event.position, cx);
            }))
            .capture_any_mouse_up(cx.listener(|this, _, _, cx| this.end_drag_overlay(cx)))
            .child(recorder)
            .on_action(cx.listener(|this, _: &SplitRight, window, cx| this.command_split(Side::Right, window, cx)))
            .on_action(cx.listener(|this, _: &SplitBelow, window, cx| this.command_split(Side::Bottom, window, cx)))
            .on_action(cx.listener(|this, _: &ClosePane, window, cx| this.command_close(window, cx)))
            .on_action(cx.listener(|this, _: &FocusNextPane, window, cx| this.command_focus_next(window, cx)))
            .on_action(cx.listener(|this, _: &Back, window, cx| this.command_back(window, cx)));
        if has_panes { root.child(self.area.clone()).children(overlay) } else { root.child(self.render_empty(cx)) }
    }
}
