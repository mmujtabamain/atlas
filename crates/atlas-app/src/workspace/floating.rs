//! A floating window: another root for the same workspace.
//!
//! There is one workspace — one layout model, one set of pane views, one
//! history — and any number of windows showing parts of it. The main window
//! is the app's own ([`crate::shell::Shell`], with the title bar, the launcher
//! and the status bar); a floating window is this view: a slim title bar, its
//! own dock area over the model's tree for that window, and the same drag
//! bands, commands and dialog layers. There is no difference between a pane
//! here and one in the main window: they are the same entities, moved by the
//! same operations, and a pane can be dragged within either window, moved
//! between them from its menu, or dropped outside the window it is in to
//! open a window of its own.
//!
//! The window closes on its own when its last pane leaves (the model drops the
//! window and [`WorkspaceView`] closes it); closing it from its close button
//! moves its panes back to the main window instead of losing them.

use std::cell::Cell;
use std::rc::Rc;

use atlas_workspace::WindowId;
use gpui_kit::assets::IconName;
use gpui_kit::component::dock::{AnyDrag, DockArea, DragPanel};
use gpui_kit::component::{
    ActiveTheme as _, Icon, Root, Sizable as _, TitleBar,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};
use gpui_kit::*;

use super::commands;
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
}

impl FloatingView {
    pub fn new(workspace: Entity<WorkspaceView>, id: WindowId, area: Entity<DockArea>, cx: &mut Context<Self>) -> Self {
        // The workspace notifies on every change that could matter here (a
        // drag in flight, a pane moved, the model rebuilt), so this view follows it.
        let _observe_workspace = cx.observe(&workspace, |_, _, cx| cx.notify());
        FloatingView { workspace, id, area, focus_handle: cx.focus_handle(), root_bounds: Rc::new(Cell::new(Bounds::default())), _observe_workspace }
    }

    /// The model's id of this window.
    pub fn window_id(&self) -> &WindowId {
        &self.id
    }

    /// The dock area of this window.
    pub fn area(&self) -> &Entity<DockArea> {
        &self.area
    }

    fn render_title_bar(&self, fullscreen: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let workspace = self.workspace.clone();
        let id = self.id.clone();
        TitleBar::new()
            .when_fullscreen(fullscreen)
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(Icon::new(IconName::Wallet).small())
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Atlas Financer"))
                    .child(div().text_xs().text_color(muted).child("Floating window")),
            )
            .child(
                h_flex().items_center().justify_end().px_2().child(
                    Button::new("floating-gather")
                        .small()
                        .ghost()
                        .compact()
                        .icon(IconName::PanelLeftClose)
                        .label("Move panes to the main window")
                        .tooltip("Every pane here goes back to the main window and this window closes")
                        .on_click(move |_, window, cx| workspace.update(cx, |workspace, cx| workspace.gather_window(&id, window, cx))),
                ),
            )
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
                move |event: &DragMoveEvent<DragPanel>, _, cx| {
                    let panel = event.drag(cx).panel();
                    let position = event.event.position;
                    workspace.update(cx, |workspace, cx| workspace.follow_drag(panel, position, cx));
                }
            })
            .on_drag_move({
                let workspace = workspace.clone();
                move |event: &DragMoveEvent<AnyDrag>, _, cx| {
                    let item = event.drag(cx).clone();
                    let position = event.event.position;
                    workspace.update(cx, |workspace, cx| workspace.follow_launch_drag(&item, position, cx));
                }
            })
            .capture_any_mouse_up({
                let workspace = workspace.clone();
                move |_, _, cx| workspace.update(cx, |workspace, cx| workspace.end_drag_overlay(cx))
            })
            .on_mouse_up_out(MouseButton::Left, {
                let workspace = workspace.clone();
                move |event: &MouseUpEvent, window, cx| {
                    let position = event.position;
                    workspace.update(cx, |workspace, cx| workspace.drag_released_outside(position, window, cx));
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
            .child(self.render_title_bar(window.is_fullscreen(), cx))
            .child(h_flex().items_stretch().flex_1().min_h_0().child(body))
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}
