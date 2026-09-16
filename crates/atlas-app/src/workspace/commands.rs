//! The workspace's keyboard commands.
//!
//! Each command is a gpui action handled by [`WorkspaceView`](super::WorkspaceView)
//! on the workspace's root element, so it applies wherever the focus is inside
//! the workspace — in a pane, in a control of a screen, on a tab bar. The
//! bindings live under the `Workspace` key context: a window that shows no
//! workspace (Welcome, the viewer gate) never sees them, and on macOS the
//! more specific context is what lets `cmd-w` close a pane here while it
//! still closes the window everywhere else.
//!
//! | action | keys | what it does |
//! |---|---|---|
//! | [`SplitRight`] | `cmd-\` / `ctrl-\` | the active pane's screen again, in a new pane to its right |
//! | [`SplitBelow`] | `cmd-shift-\` / `ctrl-shift-\` | the same, below |
//! | [`ClosePane`] | `cmd-w` / `ctrl-w` | closes the active pane |
//! | [`FocusNextPane`] | `cmd-alt-right` / `ctrl-alt-right` | makes the next pane (reading order) the active one |
//! | [`Back`] | `cmd-[` / `alt-left` | the active pane shows the screen it showed before |
//!
//! The pane title bar's menu offers the same commands for *its* pane; it calls
//! the same `WorkspaceView` methods these handlers do.

use gpui_kit::{App, Global, KeyBinding};

gpui_kit::actions!(workspace, [SplitRight, SplitBelow, ClosePane, FocusNextPane, Back]);

/// The key context the bindings are scoped to; the workspace's root element
/// carries it.
pub const KEY_CONTEXT: &str = "Workspace";

/// Marks that the bindings were installed, so a second window does not add
/// them again.
struct KeysBound;

impl Global for KeysBound {}

/// Installs the workspace key bindings once per application.
pub fn bind_keys(cx: &mut App) {
    if cx.try_global::<KeysBound>().is_some() {
        return;
    }
    cx.set_global(KeysBound);
    let context = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("cmd-\\", SplitRight, context),
        KeyBinding::new("ctrl-\\", SplitRight, context),
        KeyBinding::new("cmd-shift-\\", SplitBelow, context),
        KeyBinding::new("ctrl-shift-\\", SplitBelow, context),
        KeyBinding::new("cmd-w", ClosePane, context),
        KeyBinding::new("ctrl-w", ClosePane, context),
        KeyBinding::new("cmd-alt-right", FocusNextPane, context),
        KeyBinding::new("ctrl-alt-right", FocusNextPane, context),
        KeyBinding::new("cmd-[", Back, context),
        KeyBinding::new("alt-left", Back, context),
    ]);
    log::info!("workspace: key bindings installed (split, close, next pane, back)");
}
