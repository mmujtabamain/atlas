//! The workspace's keyboard commands.
//!
//! Each command is a gpui action handled by [`WorkspaceView`] on a window's
//! root element, so it applies wherever the focus is inside the workspace —
//! in a pane, in a control of a screen, on a tab bar. The bindings live under
//! the `Workspace` key context: a window that shows no workspace (Welcome,
//! the viewer gate) never sees them, and on macOS the more specific context
//! is what lets `cmd-w` close a pane here while it still closes the window
//! everywhere else. A text field's own bindings (its `cmd-z`) come first
//! while it has the focus, as they should.
//!
//! | action | keys | what it does |
//! |---|---|---|
//! | [`SplitRight`] | `cmd-\` / `ctrl-\` | the active pane's screen again, in a new pane to its right |
//! | [`SplitBelow`] | `cmd-shift-\` / `ctrl-shift-\` | the same, below |
//! | [`DuplicatePane`] | `cmd-shift-d` / `ctrl-shift-d` | the active pane again, its state included, to its right |
//! | [`ClosePane`] | `cmd-w` / `ctrl-w` | closes the active pane |
//! | [`ReopenClosedPane`] | `cmd-shift-t` / `ctrl-shift-t` | puts the most recently closed pane back where it was |
//! | [`FocusLeft`] … [`FocusDown`] | `cmd-alt-←↑→↓` / `ctrl-alt-←↑→↓` | the pane in that direction becomes the active one |
//! | [`FocusNextPane`] / [`FocusPreviousPane`] | `cmd-alt-]` `[` / `ctrl-alt-]` `[` | the next or previous pane in reading order |
//! | [`MoveLeft`] … [`MoveDown`] | `cmd-alt-shift-←↑→↓` / `ctrl-alt-shift-←↑→↓` | moves the active pane one step that way |
//! | [`ZoomPane`] | `cmd-shift-enter` / `ctrl-shift-enter` | the active pane fills the window, or comes back |
//! | [`Back`] | `cmd-[` / `alt-left` | the active pane shows the screen it showed before |
//! | [`DetachPane`] | `cmd-shift-n` / `ctrl-shift-n` | moves the active pane into a floating window of its own |
//! | [`Undo`] / [`Redo`] | `cmd-z` / `ctrl-z`, `cmd-shift-z` / `ctrl-shift-z` / `ctrl-y` | one layout change back, or forward |
//!
//! The pane title bar's menu offers the same commands for *its* pane; it calls
//! the same `WorkspaceView` methods these handlers do. [`attach`] puts every
//! handler on a window's root, so the main window and a floating window
//! answer the same keys.
//!
//! [`WorkspaceView`]: super::WorkspaceView

use atlas_workspace::Side;
use atlas_workspace::focus::Direction;
use gpui_kit::{App, Entity, Global, InteractiveElement, KeyBinding};

use super::view::WorkspaceView;

gpui_kit::actions!(
    workspace,
    [
        SplitRight,
        SplitBelow,
        DuplicatePane,
        ClosePane,
        ReopenClosedPane,
        FocusNextPane,
        FocusPreviousPane,
        FocusLeft,
        FocusRight,
        FocusUp,
        FocusDown,
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        ZoomPane,
        Back,
        DetachPane,
        Undo,
        Redo
    ]
);

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
        KeyBinding::new("cmd-shift-d", DuplicatePane, context),
        KeyBinding::new("ctrl-shift-d", DuplicatePane, context),
        KeyBinding::new("cmd-w", ClosePane, context),
        KeyBinding::new("ctrl-w", ClosePane, context),
        KeyBinding::new("cmd-shift-t", ReopenClosedPane, context),
        KeyBinding::new("ctrl-shift-t", ReopenClosedPane, context),
        KeyBinding::new("cmd-alt-]", FocusNextPane, context),
        KeyBinding::new("ctrl-alt-]", FocusNextPane, context),
        KeyBinding::new("cmd-alt-[", FocusPreviousPane, context),
        KeyBinding::new("ctrl-alt-[", FocusPreviousPane, context),
        KeyBinding::new("cmd-alt-left", FocusLeft, context),
        KeyBinding::new("ctrl-alt-left", FocusLeft, context),
        KeyBinding::new("cmd-alt-right", FocusRight, context),
        KeyBinding::new("ctrl-alt-right", FocusRight, context),
        KeyBinding::new("cmd-alt-up", FocusUp, context),
        KeyBinding::new("ctrl-alt-up", FocusUp, context),
        KeyBinding::new("cmd-alt-down", FocusDown, context),
        KeyBinding::new("ctrl-alt-down", FocusDown, context),
        KeyBinding::new("cmd-alt-shift-left", MoveLeft, context),
        KeyBinding::new("ctrl-alt-shift-left", MoveLeft, context),
        KeyBinding::new("cmd-alt-shift-right", MoveRight, context),
        KeyBinding::new("ctrl-alt-shift-right", MoveRight, context),
        KeyBinding::new("cmd-alt-shift-up", MoveUp, context),
        KeyBinding::new("ctrl-alt-shift-up", MoveUp, context),
        KeyBinding::new("cmd-alt-shift-down", MoveDown, context),
        KeyBinding::new("ctrl-alt-shift-down", MoveDown, context),
        KeyBinding::new("cmd-shift-enter", ZoomPane, context),
        KeyBinding::new("ctrl-shift-enter", ZoomPane, context),
        KeyBinding::new("cmd-[", Back, context),
        KeyBinding::new("alt-left", Back, context),
        KeyBinding::new("cmd-shift-n", DetachPane, context),
        KeyBinding::new("ctrl-shift-n", DetachPane, context),
        KeyBinding::new("cmd-z", Undo, context),
        KeyBinding::new("ctrl-z", Undo, context),
        KeyBinding::new("cmd-shift-z", Redo, context),
        KeyBinding::new("ctrl-shift-z", Redo, context),
        KeyBinding::new("ctrl-y", Redo, context),
    ]);
    log::info!("workspace: key bindings installed (split, duplicate, close, reopen, focus, move, zoom, back, detach, undo, redo)");
}

/// Puts every workspace command's handler on `element`, a window's root.
pub fn attach<E: InteractiveElement>(element: E, workspace: Entity<WorkspaceView>) -> E {
    macro_rules! handle {
        ($element:expr, $action:ty, |$workspace:ident, $window:ident, $cx:ident| $body:expr) => {{
            let handle = workspace.clone();
            $element.on_action(move |_: &$action, $window, $cx| handle.update($cx, |$workspace, $cx| $body))
        }};
    }
    let element = handle!(element, SplitRight, |workspace, window, cx| workspace.command_split(Side::Right, window, cx));
    let element = handle!(element, SplitBelow, |workspace, window, cx| workspace.command_split(Side::Bottom, window, cx));
    let element = handle!(element, DuplicatePane, |workspace, window, cx| workspace.command_duplicate(window, cx));
    let element = handle!(element, ClosePane, |workspace, window, cx| workspace.command_close(window, cx));
    let element = handle!(element, ReopenClosedPane, |workspace, window, cx| workspace.command_reopen(window, cx));
    let element = handle!(element, FocusNextPane, |workspace, window, cx| workspace.command_focus_next(window, cx));
    let element = handle!(element, FocusPreviousPane, |workspace, window, cx| workspace.command_focus_previous(window, cx));
    let element = handle!(element, FocusLeft, |workspace, window, cx| workspace.command_focus_direction(Direction::Left, window, cx));
    let element = handle!(element, FocusRight, |workspace, window, cx| workspace.command_focus_direction(Direction::Right, window, cx));
    let element = handle!(element, FocusUp, |workspace, window, cx| workspace.command_focus_direction(Direction::Up, window, cx));
    let element = handle!(element, FocusDown, |workspace, window, cx| workspace.command_focus_direction(Direction::Down, window, cx));
    let element = handle!(element, MoveLeft, |workspace, window, cx| workspace.command_move_direction(Direction::Left, window, cx));
    let element = handle!(element, MoveRight, |workspace, window, cx| workspace.command_move_direction(Direction::Right, window, cx));
    let element = handle!(element, MoveUp, |workspace, window, cx| workspace.command_move_direction(Direction::Up, window, cx));
    let element = handle!(element, MoveDown, |workspace, window, cx| workspace.command_move_direction(Direction::Down, window, cx));
    let element = handle!(element, ZoomPane, |workspace, window, cx| workspace.command_zoom(window, cx));
    let element = handle!(element, Back, |workspace, window, cx| workspace.command_back(window, cx));
    let element = handle!(element, DetachPane, |workspace, window, cx| workspace.command_detach(window, cx));
    let element = handle!(element, Undo, |workspace, window, cx| {
        let _ = workspace.undo(window, cx);
    });
    handle!(element, Redo, |workspace, window, cx| {
        let _ = workspace.redo(window, cx);
    })
}
