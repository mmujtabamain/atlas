//! The ghost window: the chip that follows the pointer while a pane is
//! dragged, in a window of its own.
//!
//! gpui draws a drag's preview in every window, each at that window's own
//! last pointer position, and the workspace's windows can overlap; so the
//! chip is neither the engine's preview nor an element of any workspace
//! window. It is this: a transparent pop-up window the size of the display
//! the pointer is on — above every other window, never focused, never
//! moved (gpui cannot move a window, only resize it, which is why the
//! window covers the display instead of following the pointer) — that draws
//! the chip at the pointer's screen position and nothing else. It opens
//! with the drag, follows the pointer to another display by reopening
//! there, and closes with the drag. The platform delivers the drag's mouse
//! events to the window the press was in, so a window on top of everything
//! takes nothing from the drag.
//!
//! Without a compositor (a bare X server, as on a headless box) a
//! transparent window shows as a black rectangle; `ATLAS_DRAG_GHOST_WINDOW=0`
//! keeps the ghost off there, and the drag overlay draws the chip inside
//! the window that owns the drag instead.

use gpui_kit::*;

use super::view::WorkspaceView;

/// The root view of the ghost window: the chip, at the pointer.
pub struct GhostView {
    workspace: Entity<WorkspaceView>,
    /// The display's origin on screen: the window covers the display, so a
    /// screen point is this much into the window.
    origin: Point<Pixels>,
    _observe_workspace: Subscription,
}

impl GhostView {
    pub fn new(workspace: Entity<WorkspaceView>, origin: Point<Pixels>, cx: &mut Context<Self>) -> Self {
        // Every pointer move notifies the workspace; the chip follows.
        let _observe_workspace = cx.observe(&workspace, |_, _, cx| cx.notify());
        GhostView { workspace, origin, _observe_workspace }
    }
}

impl Render for GhostView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let origin = self.origin;
        let chip = self.workspace.update(cx, |workspace, cx| {
            let drag = workspace.drag_in_flight()?.clone();
            let at = drag.screen - origin;
            workspace.render_drag_chip(&drag, at, cx)
        });
        div().id("drag-ghost").size_full().children(chip)
    }
}

/// The window options of a ghost covering `display`.
pub fn window_options(display: &dyn PlatformDisplay) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(display.bounds())),
        titlebar: None,
        focus: false,
        show: true,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        display_id: Some(display.id()),
        window_background: WindowBackgroundAppearance::Transparent,
        ..Default::default()
    }
}
