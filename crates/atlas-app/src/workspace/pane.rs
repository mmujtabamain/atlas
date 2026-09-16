//! `PaneView`: one pane of the workspace — one screen, in its own scroll
//! region, with its own in-pane history.
//!
//! A pane is a gpui-kit dock panel ([`BasePanel`] for behaviour, [`Panel`]
//! for the title bar) whose content is one of the app's screens, rendered by
//! [`AtlasApp::render_route`] for the pane's route. The screens themselves
//! stay pure functions of the app's state: two panes showing the same screen
//! render the same thing twice, and an edit made in one is visible in both on
//! the next frame, because every pane observes the app and re-renders when it
//! is notified.
//!
//! What the pane owns and what it does not:
//!
//! | owned here | owned elsewhere |
//! |---|---|
//! | the route on show and the routes before it (in-pane Back) | the household, the viewer, every screen model (`AtlasApp`) |
//! | its focus handle, whether it is the active pane, whether its tab is displayed | its place in the layout (`WorkspaceView` and the model) |
//! | its scroll position (the scroll region is keyed by the pane's id) | the tab bar, the close and zoom controls (gpui-kit's dock skin) |
//!
//! The screens were designed for a full-width column, so a pane never squashes
//! its screen: the body gives the screen at least [`MIN_CONTENT_WIDTH`] and
//! scrolls sideways when the pane is narrower than that (a wider pane fills
//! its width as before). The model's `min_share` is the structural minimum of
//! a slot; this is a rendering rule about what is drawn inside it.
//!
//! Dragging a pane is the dock skin's: the title bar (and, in a stack, the
//! tab) is the only drag handle, and the skin starts the drag only when the
//! group may be rearranged. The pane's own title element is what the skin
//! wraps in that handle, so it carries a `pane-title-<n>` id for tests.
//!
//! Every frame the pane records where it was drawn into the workspace's
//! shared [`PaneBounds`] map; the drag-target overlay reads those rectangles
//! to place its bands beside groups and the window (see
//! [`super::dock_targets`]). A hidden tab is not drawn and its entry goes
//! stale, so readers only trust the panes their stacks display.
//!
//! Everything the engine tells the pane — it was displayed, it left the dock,
//! it joined a tab group — is passed to the workspace **deferred**: those
//! callbacks arrive while the dock area, and possibly the workspace that
//! drove the edit, are still being updated, and gpui refuses a nested update
//! of an entity that is already on the stack.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use atlas_workspace::{PaneId, Side};
use gpui_kit::component::dock::{BasePanel, Panel, PanelEvent, PanelInfo, PanelState, TabGroup};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::scroll::{Scrollbar, ScrollableElement as _};
use gpui_kit::component::{ActiveTheme as _, Icon, InteractiveElementExt as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::json;

use super::kinds;
use super::view::WorkspaceView;
use crate::app::AtlasApp;
use crate::nav::Route;

/// The dock's name for every pane, written into persisted layouts. Never
/// change it once layouts have been saved.
pub const PANEL_NAME: &str = "atlas-pane";

/// How many routes a pane remembers for Back.
const HISTORY_LIMIT: usize = 32;

/// The least width a pane gives its screen, the pane's own padding included.
/// A narrower pane scrolls sideways instead of squeezing the screen.
///
/// The screens were laid out for a full-width column, and this is what that
/// layout needs before it breaks: Today's grid of six figures wraps its
/// `text_xl` monospace values at less than about 1060 px (six cells of a
/// nine-digit value plus their gutters), the Accounts register's fixed lanes
/// take 880 px before its flexible lane gets anything, and the Rules
/// register's control row takes 900 px. Below those widths a flexible lane
/// shrinks to nothing and its text wraps one character per line, which makes
/// every row taller than the pane — the rows seem to vanish. A screen
/// redesigned for narrower columns can lower this.
pub const MIN_CONTENT_WIDTH: Pixels = px(1080.);

/// Where every drawn pane is, in window coordinates, as of its last prepaint.
/// Shared between the workspace and its panes.
pub type PaneBounds = Rc<RefCell<HashMap<PaneId, Bounds<Pixels>>>>;

/// One pane: a screen instance inside the workspace.
pub struct PaneView {
    id: PaneId,
    route: Route,
    /// Routes shown before the current one, oldest first (in-pane Back).
    history: Vec<Route>,
    app: Entity<AtlasApp>,
    workspace: WeakEntity<WorkspaceView>,
    focus_handle: FocusHandle,
    /// The pane commands act on; the workspace sets it.
    active: bool,
    /// The pane's tab is the one its group displays; the engine sets it.
    displayed: bool,
    /// The tab group the engine placed the pane in, for selecting its tab.
    group: Option<WeakEntity<TabGroup>>,
    /// The sideways scroll of the body, kept so it survives a rebuild of the area.
    horizontal_scroll: ScrollHandle,
    /// The workspace's map of where every pane is drawn; this pane writes its own entry.
    bounds: PaneBounds,
    _observe_app: Subscription,
}

impl PaneView {
    /// A pane showing `route`. `workspace` is told about activation, Back and
    /// removal; `app` is what the screen renders from.
    pub fn new(id: PaneId, route: Route, app: Entity<AtlasApp>, workspace: WeakEntity<WorkspaceView>, bounds: PaneBounds, cx: &mut Context<Self>) -> Self {
        // Every state change of the app is a possible change of what the
        // screen shows; the pane is a cached view, so it has to ask for a frame.
        let _observe_app = cx.observe(&app, |_, _, cx| cx.notify());
        PaneView { id, route, history: Vec::new(), app, workspace, focus_handle: cx.focus_handle(), active: false, displayed: false, group: None, horizontal_scroll: ScrollHandle::new(), bounds, _observe_app }
    }

    /// The pane's id in the layout model.
    pub fn id(&self) -> &PaneId {
        &self.id
    }

    /// The route on show.
    pub fn route(&self) -> Route {
        self.route
    }

    /// The routes shown before the current one, oldest first.
    pub fn history(&self) -> &[Route] {
        &self.history
    }

    /// True when this is the pane commands act on.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// True when the pane's tab is the one its group displays.
    pub fn is_displayed(&self) -> bool {
        self.displayed
    }

    /// The tab group the engine placed the pane in.
    pub(crate) fn group(&self) -> Option<&WeakEntity<TabGroup>> {
        self.group.as_ref()
    }

    /// The `pane-<n>` element id tests find the pane by.
    pub fn element_id(&self) -> SharedString {
        SharedString::from(format!("pane-{}", self.id_suffix()))
    }

    /// The `pane-title-<n>` element id of the pane's title, which is the drag
    /// handle of a single-pane stack.
    pub fn title_element_id(&self) -> SharedString {
        SharedString::from(format!("pane-title-{}", self.id_suffix()))
    }

    /// The pane's counter when it was minted, else its whole id.
    fn id_suffix(&self) -> String {
        match self.id.minted_counter() {
            Some(counter) => counter.to_string(),
            None => self.id.to_string(),
        }
    }

    /// Shows `route`, remembering the current one for Back. Showing the
    /// route already on show changes nothing.
    pub(crate) fn show(&mut self, route: Route, cx: &mut Context<Self>) {
        if self.route == route {
            return;
        }
        log::info!("pane {}: {} → {}", self.id, self.route.slug(), route.slug());
        self.history.push(self.route);
        if self.history.len() > HISTORY_LIMIT {
            self.history.remove(0);
        }
        self.route = route;
        cx.notify();
    }

    /// Returns to the previous route, or to the current route's parent when
    /// nothing was shown before. Returns the route now on show.
    pub(crate) fn back(&mut self, cx: &mut Context<Self>) -> Route {
        let target = self.history.pop().unwrap_or_else(|| self.route.parent());
        log::info!("pane {}: back {} → {}", self.id, self.route.slug(), target.slug());
        self.route = target;
        cx.notify();
        target
    }

    /// Shows `route` and forgets the history — after a viewer change, when
    /// the routes behind Back may name records the new viewer must not see.
    pub(crate) fn reset_to(&mut self, route: Route, cx: &mut Context<Self>) {
        self.history.clear();
        if self.route != route {
            log::info!("pane {}: reset {} → {}", self.id, self.route.slug(), route.slug());
            self.route = route;
        }
        cx.notify();
    }

    /// The workspace's word on whether this pane is the active one.
    pub(crate) fn set_workspace_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.active != active {
            self.active = active;
            cx.notify();
        }
    }

    /// Runs `f` on the workspace once the current update has finished, with
    /// the workspace's window. See the module docs for why this is deferred.
    fn tell_workspace(&self, cx: &mut App, f: impl FnOnce(&mut WorkspaceView, &mut Window, &mut Context<WorkspaceView>) + 'static) {
        let workspace = self.workspace.clone();
        cx.defer(move |cx| {
            let Some(workspace) = workspace.upgrade() else {
                return;
            };
            let window = workspace.read(cx).window_handle();
            let _ = window.update(cx, |_, window, cx| workspace.update(cx, |workspace, cx| f(workspace, window, cx)));
        });
    }
}

impl BasePanel for PaneView {
    fn panel_name(&self) -> &'static str {
        PANEL_NAME
    }

    fn closable(&self, _: &App) -> bool {
        true
    }

    fn zoomable(&self, _: &App) -> bool {
        true
    }

    fn set_active(&mut self, active: bool, _: &mut Window, cx: &mut Context<Self>) {
        if self.displayed != active {
            log::debug!("pane {}: {}", self.id, if active { "displayed" } else { "hidden behind another tab" });
            self.displayed = active;
            cx.notify();
        }
    }

    fn on_added_to(&mut self, group: WeakEntity<TabGroup>, _: &mut Window, cx: &mut Context<Self>) {
        self.group = Some(group.clone());
        // The workspace listens to every group for drops; it learns of a
        // group through the panes the engine puts in it.
        self.tell_workspace(cx, move |workspace, window, cx| workspace.pane_joined_group(group, window, cx));
    }

    fn on_removed(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        log::debug!("pane {}: left the dock", self.id);
        self.group = None;
        self.displayed = false;
        let id = self.id.clone();
        self.tell_workspace(cx, move |workspace, window, cx| workspace.pane_left(&id, window, cx));
    }

    fn dump(&self, _: &App) -> PanelState {
        let mut state = PanelState::new(PANEL_NAME);
        state.info = PanelInfo::panel(json!({ "paneId": self.id.as_str() }));
        state
    }
}

impl Panel for PaneView {
    /// `None`: a tab shows the same icon-and-title element as a single pane's
    /// header, so tabs and titles read alike and carry the same id.
    fn tab_name(&self, _: &App) -> Option<SharedString> {
        None
    }

    /// The title element the skin wraps in the drag handle; its id lets a
    /// test take hold of exactly what a person would.
    fn title(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let title = kinds::title_of(self.route, self.app.read(cx).household());
        let context = kinds::context_of(self.route);
        h_flex()
            .id(self.title_element_id())
            .test_support()
            .min_w_0()
            .gap_2()
            .items_center()
            .child(Icon::new(kinds::icon_of(self.route)).small())
            .child(div().overflow_hidden().text_ellipsis().whitespace_nowrap().child(title))
            .when_some(context, |this, context| this.child(div().text_xs().text_color(muted).whitespace_nowrap().child(context)))
    }

    /// The pane's own commands, ahead of the skin's zoom and close entries.
    /// Each acts on *this* pane, whether or not it is the active one.
    fn dropdown_menu(&mut self, menu: PopupMenu, _: &mut Window, _: &mut Context<Self>) -> PopupMenu {
        let split_right = self.pane_command(|workspace, id, window, cx| workspace.split_pane_from_menu(id, Side::Right, window, cx));
        let split_below = self.pane_command(|workspace, id, window, cx| workspace.split_pane_from_menu(id, Side::Bottom, window, cx));
        let back = self.pane_command(|workspace, id, window, cx| workspace.back_pane_from_menu(id, window, cx));
        menu.item(PopupMenuItem::new("Split right").icon(gpui_kit::assets::IconName::SquareSplitHorizontal).on_click(split_right))
            .item(PopupMenuItem::new("Split below").icon(gpui_kit::assets::IconName::SquareSplitVertical).on_click(split_below))
            .when(!self.history.is_empty(), |menu| menu.item(PopupMenuItem::new("Back").icon(gpui_kit::assets::IconName::ArrowLeft).on_click(back)))
    }
}

impl PaneView {
    /// A menu handler that runs `f` on the workspace for this pane.
    fn pane_command(&self, f: impl Fn(&mut WorkspaceView, &PaneId, &mut Window, &mut Context<WorkspaceView>) + 'static) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        let workspace = self.workspace.clone();
        let id = self.id.clone();
        move |_, window, cx| {
            let _ = workspace.update(cx, |workspace, cx| f(workspace, &id, window, cx));
        }
    }
}

impl EventEmitter<PanelEvent> for PaneView {}

impl Focusable for PaneView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for PaneView {
    /// The pane's content: the screen inside its own scroll region, with a
    /// hairline in the focus-ring colour when this is the active pane. The
    /// dock skin draws the pane as a cached view, so this runs only when the
    /// pane (or the app it observes) was notified.
    ///
    /// Two scroll regions, one per axis, because the width the screen wraps
    /// its text at has to be definite: the body scrolls sideways as a plain
    /// block whose child is `max(the pane's width, MIN_CONTENT_WIDTH)` wide,
    /// and that child is the vertical scroll region the screen lives in. One
    /// two-axis scroll area would size its content to the widest unwrapped
    /// line instead. Each region is locked to its own axis, so a wheel
    /// scrolls the screen and a sideways swipe scrolls the body.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let route = self.route;
        let content = self.app.update(cx, |app, cx| app.render_route(route, cx));
        let border = if self.active { cx.theme().ring } else { transparent_black() };
        let id = self.id.clone();
        let workspace = self.workspace.clone();
        // The canvas fills the pane and does nothing but note where it was
        // laid out: an absolutely placed element, so it takes no space and
        // no input.
        let recorded = self.bounds.clone();
        let recorder_id = self.id.clone();
        let recorder = canvas(
            move |bounds, _, _| {
                recorded.borrow_mut().insert(recorder_id, bounds);
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        div()
            .id(self.element_id())
            .test_support()
            .relative()
            .size_full()
            .border_1()
            .border_color(border)
            .track_focus(&self.focus_handle)
            // A press anywhere in the pane makes it the pane commands act on.
            // Capture phase, so a control inside that stops propagation (a
            // button, a table row) still counts as working in this pane.
            .capture_any_mouse_down(move |_, window, cx| {
                let _ = workspace.update(cx, |workspace, cx| workspace.set_active_pane_from_pointer(&id, window, cx));
            })
            .child(
                div()
                    .id("pane-body")
                    .size_full()
                    .track_scroll(&self.horizontal_scroll)
                    .overflow_x_scroll()
                    .lock_scroll_axis()
                    .child(v_flex().id("pane-scroll").size_full().min_w(MIN_CONTENT_WIDTH).p_6().gap_6().child(content).overflow_y_scrollbar()),
            )
            .child(div().absolute().inset_0().child(Scrollbar::horizontal(&self.horizontal_scroll).viewport_from_layout()))
            .child(recorder)
    }
}
