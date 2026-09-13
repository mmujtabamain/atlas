//! Native macOS keyboard conventions for the application and its windows.

use gpui_kit::*;

use crate::actions::with_app;
use crate::nav::Route;

actions!(
    atlas_macos,
    [
        ToggleFullscreen,
        MinimizeWindow,
        CloseWindow,
        CloseAllWindows,
        QuitApplication,
        HideApplication,
        HideOtherApplications,
        NextWindow,
        PreviousWindow,
        OpenSettings,
    ]
);

/// Installs the standard macOS application and window shortcuts.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("fn-f", ToggleFullscreen, None),
        KeyBinding::new("ctrl-cmd-f", ToggleFullscreen, None),
        KeyBinding::new("cmd-m", MinimizeWindow, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("alt-cmd-w", CloseAllWindows, None),
        KeyBinding::new("cmd-q", QuitApplication, None),
        KeyBinding::new("cmd-h", HideApplication, None),
        KeyBinding::new("alt-cmd-h", HideOtherApplications, None),
        KeyBinding::new("cmd-`", NextWindow, None),
        KeyBinding::new("shift-cmd-`", PreviousWindow, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
    ]);

    cx.on_action(|_: &ToggleFullscreen, cx| {
        update_active_window(cx, |window| window.toggle_fullscreen());
    });
    cx.on_action(|_: &MinimizeWindow, cx| {
        update_active_window(cx, |window| window.minimize_window());
    });
    cx.on_action(|_: &CloseWindow, cx| {
        update_active_window(cx, Window::remove_window);
    });
    cx.on_action(|_: &CloseAllWindows, cx| {
        for handle in cx.windows() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
    });
    cx.on_action(|_: &QuitApplication, cx| cx.quit());
    cx.on_action(|_: &HideApplication, cx| cx.hide());
    cx.on_action(|_: &HideOtherApplications, cx| cx.hide_other_apps());
    cx.on_action(|_: &NextWindow, cx| activate_window_at(cx, 1));
    cx.on_action(|_: &PreviousWindow, cx| activate_last_window(cx));
    cx.on_action(|_: &OpenSettings, cx| {
        with_app(cx, |app, cx| app.navigate(Route::Settings, cx));
    });

    cx.set_menus([
        Menu::new("Atlas Financer").items([
            MenuItem::action("Settings…", OpenSettings),
            MenuItem::separator(),
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action("Hide Atlas Financer", HideApplication),
            MenuItem::action("Hide Others", HideOtherApplications),
            MenuItem::separator(),
            MenuItem::action("Quit Atlas Financer", QuitApplication),
        ]),
        Menu::new("Window").items([
            MenuItem::action("Minimize", MinimizeWindow),
            MenuItem::action("Enter Full Screen", ToggleFullscreen),
            MenuItem::separator(),
            MenuItem::action("Close Window", CloseWindow),
            MenuItem::action("Close All", CloseAllWindows),
            MenuItem::separator(),
            MenuItem::action("Next Window", NextWindow),
            MenuItem::action("Previous Window", PreviousWindow),
        ]),
    ]);
}

fn update_active_window(cx: &mut App, update: impl FnOnce(&mut Window)) {
    if let Some(handle) = cx.active_window() {
        let _ = handle.update(cx, |_, window, _| update(window));
    }
}

fn activate_window_at(cx: &mut App, index: usize) {
    let Some(handle) = cx
        .window_stack()
        .and_then(|windows| windows.get(index).copied())
    else {
        return;
    };
    let _ = handle.update(cx, |_, window, _| window.activate_window());
}

fn activate_last_window(cx: &mut App) {
    let Some(handle) = cx
        .window_stack()
        .and_then(|windows| windows.last().copied())
    else {
        return;
    };
    let _ = handle.update(cx, |_, window, _| window.activate_window());
}
