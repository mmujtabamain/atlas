//! Harness helpers shared by the UI integration tests (`ui.rs`,
//! `workspace.rs`): opening the production shell in a headless window, the
//! sample household with a chosen viewer, navigation through the app's own
//! API, and the timing quirks of gpui-kit's overlays.
#![allow(dead_code)]

use atlas_app::launch::Start;
use atlas_app::nav::Route;
use atlas_app::{AtlasApp, Launch, Shell, WorkspaceView};
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, Entity, TestAppContext, px, size};

/// gpui-kit dialogs fade in over 250 ms of wall-clock time (not test time) and
/// do not take pointer input until the animation has finished, so a test must
/// let real time pass before clicking a dialog button.
pub fn let_dialog_settle() {
    std::thread::sleep(std::time::Duration::from_millis(400));
}

/// Dismisses the result toasts and waits for them to leave. They sit in the
/// top-right corner — over the trailing commands of every workspace — and keep
/// their hitbox while they animate out.
pub fn dismiss_toasts(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.clear_notifications(cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
}

/// Asserts the element exists in this frame, whether or not it is scrolled
/// into view.
pub fn present(window: &gpui_kit::Window, id: &'static str) -> bool {
    window.try_find(id).is_some()
}

/// The sample household, a chosen viewer and a starting route: what almost
/// every test opens.
pub fn sample(route: Route) -> Launch {
    Launch { start: Start::Sample, viewer: Some('a'), route, ..Launch::default() }
}

pub fn open_app(cx: &mut TestAppContext, launch: Launch) -> (gpui_kit::WindowHandle<Root>, Entity<AtlasApp>) {
    let (handle, shell) = open_shell(cx, launch);
    let app = cx.update(|cx| shell.read(cx).app().clone());
    (handle, app)
}

/// Opens the window and returns the root view (the shell around the content).
pub fn open_shell(cx: &mut TestAppContext, launch: Launch) -> (gpui_kit::WindowHandle<Root>, Entity<Shell>) {
    cx.update(gpui_kit::init);
    let mut shell_view = None;
    // Tall enough that a rebuilt workspace fits without scrolling; the real
    // window scrolls, and one test drives that deliberately.
    let handle = cx.open_window(size(px(1600.), px(2400.)), |window, cx| {
        let shell = cx.new(|cx| Shell::new(&launch, window, cx));
        shell_view = Some(shell.clone());
        Root::new(shell, window, cx)
    });
    (handle, shell_view.expect("view created"))
}

/// Opens the window and returns the content view and the pane workspace.
pub fn open_workspace(cx: &mut TestAppContext, launch: Launch) -> (gpui_kit::WindowHandle<Root>, Entity<AtlasApp>, Entity<WorkspaceView>) {
    let (handle, shell) = open_shell(cx, launch);
    let (app, workspace) = cx.update(|cx| {
        let shell = shell.read(cx);
        (shell.app().clone(), shell.workspace().clone())
    });
    (handle, app, workspace)
}

/// Navigates through the app's own API (the sidebar and tabs are covered by
/// their own tests) and draws the resulting frame.
pub fn go(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle, app: &Entity<AtlasApp>, route: Route) {
    cx.update(|cx| app.update(cx, |app, cx| app.navigate(route, cx)));
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| window.render_frame(cx)).unwrap();
}
