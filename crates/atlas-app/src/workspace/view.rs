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
//! | `DockEvent::LayoutChanged` | [`WorkspaceView::mirror_from_area`]: dump → [`LayoutNode`] → `WindowLayout::replace_root` → validate; a newly displayed tab becomes the active pane; an echo of the workspace's own edit changes nothing |
//! | a pane told `on_removed` | [`WorkspaceView::pane_left`]: closed in the model too, unless the model already closed it |
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
use atlas_workspace::{Axis, ClosedPane, ClosedPanes, DockTarget, LayoutHistory, LayoutNode, NodeId, OpError, PaneDefinition, PaneId, Scope, Side, WindowId, WorkspaceLayout};
use gpui_kit::assets::IconName;
use gpui_kit::component::dock::{DockArea, DockLayout, DockSkin, PanelId, DockEvent, panel_handle};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    v_flex,
};
use gpui_kit::*;

use super::commands::{self, Back, ClosePane, FocusNextPane, SplitBelow, SplitRight};
use super::kinds;
use super::mirror;
use super::pane::PaneView;
use crate::alerting::{self, Level};
use crate::app::AtlasApp;
use crate::launch::Launch;
use crate::nav::{Destination, Route};

/// Weights that agree within this much are the same layout: the engine
/// measures slots in whole pixels, and a few pixels of a window are noise.
const WEIGHT_TOLERANCE: f64 = 0.005;

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
            _subscriptions: vec![area_events, app_changes],
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
        for view in self.panes.values() {
            view.update(cx, |pane, cx| {
                let route = pane.route();
                let home = route.destination().map(Destination::home).unwrap_or(route);
                pane.reset_to(home, cx);
            });
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
        let view = cx.new(|cx| PaneView::new(pane.clone(), route, app, workspace, cx));
        self.panes.insert(pane.clone(), view);
        self.history.push(format!("Open {}", route.title()), before);
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
        self.history.push(format!("Close {title}"), before);
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
        self.history.push("Resize panes", before);
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

    /// Installs a whole layout (undo, redo, later a saved layout): pane views
    /// are created for panes the layout has and this view does not, dropped
    /// for the reverse, then the area is rebuilt.
    fn install_layout(&mut self, layout: WorkspaceLayout, window: &mut Window, cx: &mut Context<Self>) {
        self.layout = layout;
        self.sync_pane_entities(cx);
        self.rebuild_area(window, cx);
        self.focus_active(window, cx);
        self.sync_chrome(cx);
    }

    /// One pane view per pane definition, no more and no fewer. A definition
    /// this build cannot show (an unknown kind) is closed in the model.
    fn sync_pane_entities(&mut self, cx: &mut Context<Self>) {
        let wanted: HashSet<PaneId> = self.layout.panes.keys().cloned().collect();
        self.panes.retain(|pane, _| wanted.contains(pane));
        let missing: Vec<(PaneId, PaneDefinition)> = self.layout.panes.iter().filter(|(pane, _)| !self.panes.contains_key(*pane)).map(|(pane, definition)| (pane.clone(), definition.clone())).collect();
        for (pane, definition) in missing {
            match kinds::route_of(&definition) {
                Some(route) => {
                    let workspace = cx.weak_entity();
                    let app = self.app.clone();
                    let view = cx.new(|cx| PaneView::new(pane.clone(), route, app, workspace, cx));
                    self.panes.insert(pane, view);
                }
                None => {
                    log::error!("workspace: pane {pane} shows {:?}, which this build cannot open; closing it", definition.kind);
                    let _ = self.layout.close_pane(&pane);
                }
            }
        }
    }

    // ----- in-pane navigation (called by the app) -----------------------------------

    /// The active pane shows `route`, remembering the current one for Back.
    /// With no pane open, one is opened — once this update is over, since
    /// the app that calls this holds no window.
    pub fn navigate_active(&mut self, route: Route, cx: &mut Context<Self>) {
        match self.active_pane().and_then(|active| self.panes.get(&active).cloned()) {
            Some(view) => view.update(cx, |pane, cx| pane.show(route, cx)),
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
        let view = self.active_pane().and_then(|active| self.panes.get(&active).cloned())?;
        Some(view.update(cx, |pane, cx| pane.back(cx)))
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
        self.sync_chrome(cx);
    }

    /// The engine removed `pane` (its tab's close button, the skin's Close):
    /// the model follows, unless the model closed it first (the workspace's
    /// own `close_pane`, a rebuild that no longer includes it).
    pub(crate) fn pane_left(&mut self, pane: &PaneId, window: &mut Window, cx: &mut Context<Self>) {
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
                self.history.push(format!("Close {title}"), before);
            }
            Err(err) => log::warn!("workspace: pane {pane} left the dock but could not be closed in the model: {err}"),
        }
        self.panes.remove(pane);
        self.apply_active_flags(cx);
        self.focus_active(window, cx);
        self.sync_chrome(cx);
        cx.notify();
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
        let label = if structure_changed {
            Some("Rearrange panes")
        } else if weights_changed {
            Some("Resize panes")
        } else {
            None
        };
        log::info!(
            "workspace: mirrored the dock's layout (structure changed: {structure_changed}, weights changed: {weights_changed}, active changed: {active_changed}); grid:\n{}",
            atlas_workspace::grid::render_window(new_root, 12, 4, |pane| pane.minted_counter().map(|n| char::from_digit((n % 36) as u32, 36).unwrap_or('?')).unwrap_or('?'))
        );
        let previous = std::mem::replace(&mut self.layout, candidate);
        if let Some(label) = label {
            self.history.push(label, previous);
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

impl Render for WorkspaceView {
    /// The dock area while the main window has panes, the empty state
    /// otherwise. The shell embeds this view cached at the content column's
    /// size; the panes are cached views of their own inside the dock.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders += 1;
        let has_panes = self.layout.main_window().is_some_and(|main| !main.is_empty());
        let root = div()
            .id("workspace")
            .key_context(commands::KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .size_full()
            .on_action(cx.listener(|this, _: &SplitRight, window, cx| this.command_split(Side::Right, window, cx)))
            .on_action(cx.listener(|this, _: &SplitBelow, window, cx| this.command_split(Side::Bottom, window, cx)))
            .on_action(cx.listener(|this, _: &ClosePane, window, cx| this.command_close(window, cx)))
            .on_action(cx.listener(|this, _: &FocusNextPane, window, cx| this.command_focus_next(window, cx)))
            .on_action(cx.listener(|this, _: &Back, window, cx| this.command_back(window, cx)));
        if has_panes { root.child(self.area.clone()) } else { root.child(self.render_empty(cx)) }
    }
}
