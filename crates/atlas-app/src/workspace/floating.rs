//! A floating window: another root for the same workspace.
//!
//! There is one workspace — one layout model, one set of pane views, one
//! history — and any number of windows showing parts of it. The main window
//! is the app's own ([`crate::shell::Shell`], with the title bar, the launcher
//! and the status bar); a floating window is this view: a slim title bar, its
//! own dock area over the model's tree for that window, and the same drag
//! bands, commands and dialog layers. There is no difference between a pane
//! here and one in the main window: they are the same entities, moved by the
//! same operations, and a pane can be dragged within either window, dragged
//! from one window into another (it lands as a tab of the pane it is dropped
//! on), moved between them from its menu, or dropped outside every window to
//! open a window of its own.
//!
//! A window holding a single pane has one bar, not two: the window's title
//! bar carries that pane's title (still the pane's drag handle), its close
//! button and its menu beside the gather button, and the pane draws no bar
//! of its own. With two panes or more the panes draw their bars as in the
//! main window and the title bar names the window.
//!
//! The window closes on its own when its last pane leaves (the model drops the
//! window and [`WorkspaceView`] closes it); closing it from its close button
//! moves its panes back to the main window instead of losing them.

use std::cell::Cell;
use std::rc::Rc;

use atlas_workspace::{PaneId, WindowId};
use gpui_kit::assets::IconName;
use gpui_kit::component::dock::{AnyDrag, DockArea, DragPanel, PanelId, PanelView as _};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Root, Sizable as _, TitleBar,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::commands;
use super::skin::TabGhost;
use super::view::WorkspaceView;

/// The root view of a floating window.
pub struct FloatingView {
    workspace: Entity<WorkspaceView>,
    id: WindowId,
    area: Entity<DockArea>,
    focus_handle: FocusHandle,
    /// Where the pane area was drawn, so the drag bands (in window
    /// coordinates) can be placed relative to it.
    root_bounds: Rc<Cell<Bounds<Pixels>>>,
    _observe_workspace: Subscription,
    _observe_activation: Subscription,
}

impl FloatingView {
    pub fn new(workspace: Entity<WorkspaceView>, id: WindowId, area: Entity<DockArea>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // The workspace notifies on every change that could matter here (a
        // drag in flight, a pane moved, the model rebuilt), so this view follows it.
        let _observe_workspace = cx.observe(&workspace, |_, _, cx| cx.notify());
        // The platform raises a window it activates; the workspace's
        // stacking order follows.
        let _observe_activation = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                let id = this.id.clone();
                this.workspace.update(cx, |workspace, cx| workspace.raise_window(&id, cx));
            }
        });
        FloatingView { workspace, id, area, focus_handle: cx.focus_handle(), root_bounds: Rc::new(Cell::new(Bounds::default())), _observe_workspace, _observe_activation }
    }

    /// The model's id of this window.
    pub fn window_id(&self) -> &WindowId {
        &self.id
    }

    /// The dock area of this window.
    pub fn area(&self) -> &Entity<DockArea> {
        &self.area
    }

    /// The title bar: the one pane's own title, close button and menu when
    /// the window holds a single pane, else the window's name; the gather
    /// button either way.
    fn render_title_bar(&self, single: Option<PaneId>, fullscreen: bool, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let workspace = self.workspace.clone();
        let id = self.id.clone();
        let heading: AnyElement = match single.and_then(|pane| self.workspace.read(cx).pane_view(&pane).map(|view| (pane, view))) {
            Some((pane, view)) => self.render_pane_heading(pane, view, window, cx),
            None => h_flex()
                .items_center()
                .gap_2()
                .child(Icon::new(IconName::Wallet).small())
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Atlas Financer"))
                .child(div().text_xs().text_color(muted).child("Floating window"))
                .into_any_element(),
        };
        TitleBar::new()
            .when_fullscreen(fullscreen)
            .child(heading)
            .child(
                h_flex().items_center().justify_end().px_2().child(
                    Button::new("floating-gather")
                        .small()
                        .ghost()
                        .compact()
                        .icon(IconName::PanelLeftClose)
                        .tooltip("Move the panes to the active pane in the main window")
                        .on_click(move |_, window, cx| workspace.update(cx, |workspace, cx| workspace.gather_window(&id, window, cx))),
                ),
            )
    }

    /// The one pane's heading in the title bar: its title as the drag
    /// handle (a press on it does not move the window), its close button
    /// and its menu.
    fn render_pane_heading(&self, pane: PaneId, view: Entity<super::pane::PaneView>, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let title = view.title(window, cx);
        let node = view.read(cx).group().and_then(|group| group.upgrade()).map(|group| group.read(cx).node());
        let panel_id = PanelId::from(view.entity_id());
        let workspace_for_close = self.workspace.clone();
        let pane_for_close = pane.clone();
        let menu_view = view.clone();
        let workspace_for_menu = self.workspace.clone();
        let pane_for_menu = pane.clone();
        h_flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap_1()
            .pr_2()
            .child(
                div()
                    .id("floating-pane-title")
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_sm()
                    .child(title)
                    .when_some(node, |this, node| {
                        this.on_drag(DragPanel::new(panel_id, node), move |drag, offset, _, cx| {
                            cx.stop_propagation();
                            drag.set_drag_offset(offset);
                            cx.new(|_| TabGhost)
                        })
                    })
                    // The bar moves the window on a press-and-drag; a press
                    // on the title is the pane's, not the window's.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
            )
            .child(Button::new("floating-pane-close").icon(IconName::X).xsmall().ghost().tab_stop(false).tooltip("Close pane").on_click(move |_, window, cx| {
                workspace_for_close.update(cx, |workspace, cx| {
                    let _ = workspace.close_pane(&pane_for_close, window, cx);
                });
            }))
            .child(Button::new("floating-pane-menu").icon(IconName::Ellipsis).xsmall().ghost().tab_stop(false).dropdown_menu(move |menu, window, cx| {
                let workspace = workspace_for_menu.clone();
                let pane = pane_for_menu.clone();
                menu_view.dropdown_menu(menu, window, cx).separator().item(gpui_kit::component::menu::PopupMenuItem::new("Close").on_click(move |_, window, cx| {
                    workspace.update(cx, |workspace, cx| {
                        let _ = workspace.close_pane(&pane, window, cx);
                    });
                }))
            }))
            .into_any_element()
    }
}

trait TitleBarFullscreen {
    fn when_fullscreen(self, fullscreen: bool) -> Self;
}

impl TitleBarFullscreen for TitleBar {
    fn when_fullscreen(self, fullscreen: bool) -> Self {
        if fullscreen { self.pl_0() } else { self }
    }
}

impl Render for FloatingView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let id = self.id.clone();
        // One pane: the title bar carries its title and the pane draws no bar.
        let single = self.workspace.read(cx).single_pane_of_window(&id);
        if let Some(skin) = self.workspace.read(cx).skin_of(&id).cloned() {
            skin.set_merged_title(single.is_some(), cx);
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
        let origin = self.root_bounds.get().origin;
        let root_size = self.root_bounds.get().size;
        let overlay = self.workspace.update(cx, |workspace, cx| workspace.render_drag_overlay_for(&id, origin, root_size, cx));
        let workspace = self.workspace.clone();
        let body = div()
            .id("floating-workspace")
            .key_context(commands::KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .relative()
            .size_full()
            .on_drag_move({
                let workspace = workspace.clone();
                move |event: &DragMoveEvent<DragPanel>, window, cx| {
                    let dragged = event.drag(cx);
                    let (panel, grab) = (dragged.panel(), dragged.drag_offset());
                    let position = event.event.position;
                    workspace.update(cx, |workspace, cx| workspace.follow_drag(panel, position, grab, window, cx));
                }
            })
            .on_drag_move({
                let workspace = workspace.clone();
                move |event: &DragMoveEvent<AnyDrag>, window, cx| {
                    let item = event.drag(cx).clone();
                    let position = event.event.position;
                    workspace.update(cx, |workspace, cx| workspace.follow_launch_drag(&item, position, window, cx));
                }
            })
            // See the main window's root: a release inside the window is this
            // workspace's drop when another window lies on top of the pointer.
            .capture_any_mouse_up({
                let workspace = workspace.clone();
                move |event: &MouseUpEvent, window, cx| {
                    let position = event.position;
                    if workspace.update(cx, |workspace, cx| workspace.drag_released(position, window, cx)) {
                        cx.stop_propagation();
                    }
                }
            })
            .on_mouse_up_out(MouseButton::Left, {
                let workspace = workspace.clone();
                move |event: &MouseUpEvent, window, cx| {
                    let position = event.position;
                    workspace.update(cx, |workspace, cx| {
                        workspace.drag_released(position, window, cx);
                    });
                }
            })
            .child(recorder)
            .child(self.area.clone())
            .children(overlay);
        let body = commands::attach(body, workspace);
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_title_bar(single, window.is_fullscreen(), window, cx))
            .child(h_flex().items_stretch().flex_1().min_h_0().child(body))
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}
