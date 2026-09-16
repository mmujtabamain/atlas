//! UI integration tests of the pane workspace: the production `Shell` with its
//! `WorkspaceView` in a headless window (gpui-kit `test-support`), driven
//! through the workspace's own commands, the launcher, and pointer events.
//!
//! The layout model is checked through its character grid
//! (`atlas_workspace::grid::render_numbered`: panes numbered in reading
//! order, one character per cell), the screen through the `screen-<slug>`
//! ids every screen root carries, and the panes through their `pane-<n>` ids.
//!
//! Drags are real pointer sequences on the pane titles (`pane-title-<n>`),
//! the drag handle of a single-pane stack, dropped on the engine's zones of
//! another pane — its centre for a tab, an edge for a split — or on the
//! workspace's own bands (`dock-band-<side>-<depth>`) for docking beside a
//! group, a run of siblings, or along the window.

mod common;

use atlas_app::nav::{Destination, Route};
use atlas_app::workspace::pane::MIN_CONTENT_WIDTH;
use atlas_app::workspace::view::{REFUSED_SPLIT_MESSAGE, REFUSED_SPLIT_TOAST_ID};
use atlas_app::workspace::{WorkspaceView, kinds};
use atlas_app::{AtlasApp, Launch};
use atlas_workspace::grid::render_numbered;
use atlas_workspace::resolver::Intent;
use atlas_workspace::{Axis, LayoutNode, PaneId, Side, SplitLimits};
use common::*;
use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, Bounds, Entity, InputEvent as _, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollDelta, SharedString, TestAppContext, Window, point, px};

/// The model's picture of the main window, `cols` × `rows` cells.
fn grid(cx: &mut TestAppContext, workspace: &Entity<WorkspaceView>, cols: usize, rows: usize) -> String {
    cx.update(|cx| {
        let layout = workspace.read(cx).layout();
        match layout.main_window().and_then(|window| window.root.as_ref()) {
            Some(root) => render_numbered(root, cols, rows),
            None => String::new(),
        }
    })
}

/// Runs `f` on the workspace with the window at hand, then lets the deferred
/// work (pane callbacks, the dock's layout events) settle and draws a frame.
fn drive<R>(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle, workspace: &Entity<WorkspaceView>, f: impl FnOnce(&mut WorkspaceView, &mut gpui_kit::Window, &mut gpui_kit::Context<WorkspaceView>) -> R) -> R {
    let result = cx.update_window(window, |_, window, cx| workspace.update(cx, |workspace, cx| f(workspace, window, cx))).unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| window.render_frame(cx)).unwrap();
    result
}

fn active(cx: &mut TestAppContext, workspace: &Entity<WorkspaceView>) -> PaneId {
    cx.update(|cx| workspace.read(cx).active_pane().expect("an active pane"))
}

fn pane_count(cx: &mut TestAppContext, workspace: &Entity<WorkspaceView>) -> usize {
    cx.update(|cx| workspace.read(cx).pane_count())
}

/// Today beside Accounts: the two-pane workspace most tests start from.
fn today_and_accounts(cx: &mut TestAppContext) -> (gpui_kit::AnyWindowHandle, Entity<AtlasApp>, Entity<WorkspaceView>) {
    let (handle, app, workspace) = open_workspace(cx, sample(Route::Today));
    let window: gpui_kit::AnyWindowHandle = handle.into();
    drive(cx, window, &workspace, |workspace, window, cx| workspace.split_active(Side::Right, Route::Accounts, window, cx).expect("split"));
    (window, app, workspace)
}

/// `1|2|3`: Today, Accounts and Rules in one row (Rules split off Accounts,
/// so the columns are 0.65, 0.2275 and 0.1225 of the width).
fn three_columns(cx: &mut TestAppContext) -> (gpui_kit::AnyWindowHandle, Entity<AtlasApp>, Entity<WorkspaceView>) {
    let (window, app, workspace) = today_and_accounts(cx);
    drive(cx, window, &workspace, |workspace, window, cx| workspace.split_active(Side::Right, Route::Rules, window, cx).expect("split"));
    assert_eq!(grid(cx, &workspace, 8, 1), "11111223");
    (window, app, workspace)
}

/// Lets deferred work and the dock's events settle, then draws a frame.
fn settle(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle) {
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| window.render_frame(cx)).unwrap();
}

/// The model as the JSON it is saved as — the byte-identical yardstick.
fn layout_json(cx: &mut TestAppContext, workspace: &Entity<WorkspaceView>) -> String {
    cx.update(|cx| serde_json::to_string(workspace.read(cx).layout()).expect("layout serializes"))
}

/// The history's undo labels, oldest first.
fn history_labels(cx: &mut TestAppContext, workspace: &Entity<WorkspaceView>) -> Vec<String> {
    cx.update(|cx| workspace.read(cx).history().labels().into_iter().map(str::to_owned).collect())
}

/// The main window's tree.
fn root(cx: &mut TestAppContext, workspace: &Entity<WorkspaceView>) -> LayoutNode {
    cx.update(|cx| workspace.read(cx).layout().main_window().and_then(|window| window.root.clone()).expect("a tree"))
}

fn pane_id(n: u64) -> SharedString {
    SharedString::from(format!("pane-{n}"))
}

fn title_id(n: u64) -> SharedString {
    SharedString::from(format!("pane-title-{n}"))
}

/// Where inside a pane a drop lands: the engine's centre zone (a tab of that
/// pane's stack) or one of its four edge zones (a split on that side).
#[derive(Clone, Copy, Debug)]
enum Zone {
    Centre,
    Left,
    Right,
    Top,
    Bottom,
}

/// A point well inside `zone` of `bounds` (the zones start 35 % in from each edge).
fn zone_point(bounds: Bounds<Pixels>, zone: Zone) -> Point<Pixels> {
    let centre = bounds.center();
    match zone {
        Zone::Centre => centre,
        Zone::Left => point(bounds.left() + bounds.size.width * 0.1, centre.y),
        Zone::Right => point(bounds.left() + bounds.size.width * 0.9, centre.y),
        Zone::Top => point(centre.x, bounds.top() + bounds.size.height * 0.1),
        Zone::Bottom => point(centre.x, bounds.top() + bounds.size.height * 0.9),
    }
}

/// Drags pane `from`'s title onto `zone` of pane `onto` — press, move in
/// steps, release — and lets the drop settle.
fn drag_pane(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle, from: u64, onto: u64, zone: Zone) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let handle = window.find(title_id(from)).bounds().center();
        let target = zone_point(window.find(pane_id(onto)).bounds(), zone);
        window.drag(handle, target, cx);
    })
    .unwrap();
    settle(cx, window);
}

fn press_at(window: &mut Window, position: Point<Pixels>, cx: &mut gpui_kit::App) {
    window.dispatch_event(MouseMoveEvent { position, pressed_button: None, modifiers: Default::default() }.to_platform_input(), cx);
    window.render_frame(cx);
    window.dispatch_event(MouseDownEvent { button: MouseButton::Left, position, modifiers: Default::default(), click_count: 1, first_mouse: false }.to_platform_input(), cx);
    window.render_frame(cx);
}

fn move_pressed_to(window: &mut Window, position: Point<Pixels>, cx: &mut gpui_kit::App) {
    window.dispatch_event(MouseMoveEvent { position, pressed_button: Some(MouseButton::Left), modifiers: Default::default() }.to_platform_input(), cx);
    window.render_frame(cx);
}

fn release_at(window: &mut Window, position: Point<Pixels>, cx: &mut gpui_kit::App) {
    window.dispatch_event(MouseUpEvent { button: MouseButton::Left, position, modifiers: Default::default(), click_count: 1 }.to_platform_input(), cx);
    window.render_frame(cx);
}

/// The weights of the main window's root split.
fn root_weights(cx: &mut TestAppContext, workspace: &Entity<WorkspaceView>) -> Vec<f64> {
    root(cx, workspace).weights().to_vec()
}

fn about(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() < 1e-6
}

#[gpui_kit::test]
fn the_sample_opens_with_one_pane_showing_today(cx: &mut TestAppContext) {
    let (handle, app, workspace) = open_workspace(cx, sample(Route::Today));
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-today").visible(), "the launch route shows in a pane");
        assert!(window.find("pane-1").visible(), "the pane carries its id");
        assert!(window.try_find("workspace-empty").is_none());
    })
    .unwrap();
    assert_eq!(pane_count(cx, &workspace), 1);
    assert_eq!(grid(cx, &workspace, 3, 1), "111");
    cx.update(|cx| {
        let workspace = workspace.read(cx);
        assert_eq!(workspace.active_route(cx), Some(Route::Today));
        assert_eq!(workspace.layout().scope.household_id.as_deref(), Some("sample"), "the layout is scoped to the household");
        assert_eq!(app.read(cx).route(), Route::Today);
    });
}

#[gpui_kit::test]
fn split_right_opens_a_second_pane_beside_the_first(cx: &mut TestAppContext) {
    let (window, app, workspace) = today_and_accounts(cx);
    cx.update_window(window, |_, window, _| {
        assert!(window.find("screen-today").visible(), "the first pane still shows Today");
        assert!(window.find("screen-accounts").visible(), "the new pane shows Accounts");
        assert!(window.find("pane-1").visible() && window.find("pane-2").visible());
        let today = window.find("pane-1").bounds();
        let accounts = window.find("pane-2").bounds();
        assert!(accounts.left() >= today.right(), "Accounts sits to the right: {today:?} {accounts:?}");
    })
    .unwrap();
    assert_eq!(pane_count(cx, &workspace), 2);
    assert_eq!(grid(cx, &workspace, 2, 1), "12");
    cx.update(|cx| {
        assert_eq!(workspace.read(cx).active_route(cx), Some(Route::Accounts), "the new pane is the active one");
        assert_eq!(app.read(cx).route(), Route::Accounts, "the launcher follows the active pane");
    });

    // A state change in the household (here: the earmarks boundary) is not a
    // new household: the layout stays as it is.
    cx.update(|cx| app.update(cx, |app, cx| app.select_boundary(atlas_core::liquidity::Boundary::Person(atlas_core::fixtures::ids::PERSON_A), cx)));
    cx.run_until_parked();
    assert_eq!(pane_count(cx, &workspace), 2, "an edit keeps the panes");
    assert_eq!(grid(cx, &workspace, 2, 1), "12");
}

#[gpui_kit::test]
fn split_below_stacks_a_pane_under_the_active_one(cx: &mut TestAppContext) {
    let (window, _app, workspace) = today_and_accounts(cx);
    drive(cx, window, &workspace, |workspace, window, cx| workspace.split_active(Side::Bottom, Route::Rules, window, cx).expect("split"));
    assert_eq!(grid(cx, &workspace, 2, 2), "12\n13");
    cx.update_window(window, |_, window, _| {
        assert!(window.find("screen-today").visible());
        assert!(window.find("screen-accounts").visible());
        assert!(window.find("screen-rules").visible());
        let accounts = window.find("pane-2").bounds();
        let rules = window.find("pane-3").bounds();
        assert!(rules.top() >= accounts.bottom(), "Rules sits below Accounts: {accounts:?} {rules:?}");
    })
    .unwrap();
}

#[gpui_kit::test]
fn closing_the_middle_pane_collapses_its_split(cx: &mut TestAppContext) {
    let (window, _app, workspace) = today_and_accounts(cx);
    drive(cx, window, &workspace, |workspace, window, cx| workspace.split_active(Side::Bottom, Route::Rules, window, cx).expect("split"));
    let order = cx.update(|cx| workspace.read(cx).panes_in_order());
    assert_eq!(order.len(), 3);
    // Reading order is Today, Accounts, Rules; Accounts is the middle one.
    let middle = order[1].clone();
    drive(cx, window, &workspace, |workspace, window, cx| workspace.close_pane(&middle, window, cx).expect("close"));
    assert_eq!(pane_count(cx, &workspace), 2);
    assert_eq!(grid(cx, &workspace, 2, 2), "12\n12", "the vertical split collapsed into the column");
    cx.update_window(window, |_, window, _| {
        assert!(window.find("screen-today").visible());
        assert!(window.find("screen-rules").visible());
        assert!(window.try_find("screen-accounts").is_none(), "the closed pane's screen is gone");
    })
    .unwrap();
    cx.update(|cx| {
        let workspace = workspace.read(cx);
        assert!(workspace.layout().validate().is_empty());
        assert_eq!(workspace.closed().len(), 1, "the closed pane is remembered");
    });
}

#[gpui_kit::test]
fn open_focuses_an_existing_pane_and_new_instance_adds_a_tab(cx: &mut TestAppContext) {
    let (window, _app, workspace) = today_and_accounts(cx);
    let order = cx.update(|cx| workspace.read(cx).panes_in_order());
    let (today, accounts) = (order[0].clone(), order[1].clone());
    drive(cx, window, &workspace, |workspace, window, cx| workspace.set_active_pane(&today, window, cx).expect("activate"));
    assert_eq!(active(cx, &workspace), today);

    let opened = drive(cx, window, &workspace, |workspace, window, cx| workspace.open(Route::Accounts, Intent::Open, window, cx).expect("open"));
    assert_eq!(opened, accounts, "Open finds the pane that already shows Accounts");
    assert_eq!(pane_count(cx, &workspace), 2, "no pane was added");
    assert_eq!(active(cx, &workspace), accounts, "and makes it the active one");

    let added = drive(cx, window, &workspace, |workspace, window, cx| workspace.open(Route::Accounts, Intent::NewInstance, window, cx).expect("open"));
    assert_ne!(added, accounts);
    assert_eq!(pane_count(cx, &workspace), 3, "a new instance is a new pane");
    cx.update(|cx| {
        let layout = workspace.read(cx).layout();
        assert_eq!(layout.stack_of(&added), layout.stack_of(&accounts), "as a tab of the active stack");
        assert_eq!(layout.active_pane(), Some(added.clone()));
    });
    assert_eq!(grid(cx, &workspace, 2, 1), "13", "the new tab is the one on show in its stack");
}

#[gpui_kit::test]
fn a_press_in_a_pane_makes_it_active_and_the_launcher_navigates_that_pane(cx: &mut TestAppContext) {
    let (window, app, workspace) = today_and_accounts(cx);
    let order = cx.update(|cx| workspace.read(cx).panes_in_order());
    let today = order[0].clone();
    assert_ne!(active(cx, &workspace), today, "the new Accounts pane is active after the split");

    // Press in the padding of the Today pane, away from any control in it.
    cx.update_window(window, |_, window, cx| window.click_at("pane-1", point(px(8.), px(8.)), cx)).unwrap();
    cx.run_until_parked();
    assert_eq!(active(cx, &workspace), today, "the press made Today's pane the active one");
    cx.update(|cx| assert_eq!(app.read(cx).route(), Route::Today, "the chrome follows the active pane"));

    // Rules & taxes, from the launcher: the launcher is not navigation. It
    // opens Rules as a new pane in the *active* stack — Today's — and Today
    // keeps its screen behind the new tab.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("launcher-rules", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-rules").visible(), "the new pane is displayed in the active stack");
        assert!(window.find("screen-accounts").visible(), "the other pane kept its screen");
        assert!(window.try_find("screen-today").is_none(), "Today is behind the Rules tab");
    })
    .unwrap();
    assert_eq!(pane_count(cx, &workspace), 3);
    cx.update(|cx| {
        let workspace = workspace.read(cx);
        assert_eq!(workspace.pane_route(&today, cx), Some(Route::Today), "Today's pane still shows Today");
        assert_eq!(workspace.pane_route(&order[1], cx), Some(Route::Accounts));
        let rules = workspace.active_pane().expect("the new pane is active");
        assert_eq!(workspace.pane_route(&rules, cx), Some(Route::Rules));
        assert_eq!(workspace.layout().stack_of(&rules), workspace.layout().stack_of(&today), "as a tab of Today's stack");
        assert_eq!(app.read(cx).route(), Route::Rules);
    });
    // Clicking the same launcher item again focuses that pane instead of
    // opening another; Shift-click opens another instance.
    cx.update_window(window, |_, window, cx| window.click("launcher-rules", cx)).unwrap();
    cx.run_until_parked();
    assert_eq!(pane_count(cx, &workspace), 3, "a second click focuses the existing Rules pane");
}

#[gpui_kit::test]
fn back_returns_the_pane_to_its_previous_route(cx: &mut TestAppContext) {
    let (handle, app, workspace) = open_workspace(cx, sample(Route::Today));
    let window: gpui_kit::AnyWindowHandle = handle.into();
    go(cx, window, &app, Route::Accounts);
    go(cx, window, &app, Route::Earmarks);
    cx.update_window(window, |_, window, _| assert!(window.find("screen-earmarks").visible())).unwrap();
    let pane = active(cx, &workspace);
    cx.update(|cx| assert_eq!(workspace.read(cx).pane(&pane).unwrap().read(cx).history(), [Route::Today, Route::Accounts]));

    cx.update(|cx| app.update(cx, |app, cx| app.go_back(cx)));
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-accounts").visible(), "Back shows the previous route");
    })
    .unwrap();
    cx.update(|cx| {
        assert_eq!(app.read(cx).route(), Route::Accounts);
        assert_eq!(workspace.read(cx).pane_route(&pane, cx), Some(Route::Accounts));
    });
    assert_eq!(pane_count(cx, &workspace), 1, "Back stays in the pane");

    // Back with nothing behind it goes to the route's parent.
    cx.update(|cx| app.update(cx, |app, cx| app.go_back(cx)));
    cx.update(|cx| app.update(cx, |app, cx| app.go_back(cx)));
    cx.run_until_parked();
    cx.update(|cx| assert_eq!(app.read(cx).route(), Route::Today));
}

#[gpui_kit::test]
fn the_open_flag_lays_panes_out_left_to_right_and_stacks_with_a_plus(cx: &mut TestAppContext) {
    let launch = Launch { route: Route::Today, extra: vec![Route::Accounts, Route::Rules], ..sample(Route::Today) };
    let (handle, _app, workspace) = open_workspace(cx, launch);
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-today").visible());
        assert!(window.find("screen-accounts").visible());
        assert!(window.find("screen-rules").visible());
    })
    .unwrap();
    assert_eq!(grid(cx, &workspace, 3, 1), "123");
    assert_eq!(pane_count(cx, &workspace), 3);

    // `--open accounts --open +series`: Series is a tab on Accounts.
    let launch = Launch { route: Route::Today, extra: vec![Route::Accounts], stacked: vec![(1, Route::Series)], ..sample(Route::Today) };
    let (handle, _app, workspace) = open_workspace(cx, launch);
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-series").visible(), "the tab added last is the one on show");
        assert!(window.try_find("screen-accounts").is_none(), "Accounts is behind it");
    })
    .unwrap();
    assert_eq!(pane_count(cx, &workspace), 3);
    cx.update(|cx| {
        let layout = workspace.read(cx).layout();
        let panes = layout.main_window().unwrap().panes();
        assert_eq!(layout.stack_of(&panes[1]), layout.stack_of(&panes[2]), "Series shares Accounts' stack");
    });
    assert_eq!(grid(cx, &workspace, 2, 1), "13");
}

#[gpui_kit::test]
fn closing_every_pane_shows_the_empty_workspace_that_opens_one_again(cx: &mut TestAppContext) {
    let (handle, app, workspace) = open_workspace(cx, sample(Route::Today));
    let window: gpui_kit::AnyWindowHandle = handle.into();
    drive(cx, window, &workspace, |workspace, window, cx| workspace.close_active(window, cx).expect("close"));
    assert_eq!(pane_count(cx, &workspace), 0);
    cx.update_window(window, |_, window, cx| {
        assert!(window.find("workspace-empty").visible(), "the empty state takes the column");
        assert!(window.find("workspace-add-pane").visible());
        assert!(window.try_find("screen-today").is_none());
        assert!(window.find("launcher-today").visible(), "the launcher stays");
        window.click("workspace-add-pane", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("popup-menu").visible(), "Add pane opens the list of destinations");
        // Today, Decisions, Forecast, Accounts, …: Accounts is the fourth entry.
        window.within("popup-menu").click(3usize, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-accounts").visible(), "the chosen destination opened in a pane");
        assert!(window.try_find("workspace-empty").is_none());
    })
    .unwrap();
    assert_eq!(pane_count(cx, &workspace), 1);
    cx.update(|cx| assert_eq!(app.read(cx).route(), Route::Accounts));
}

#[gpui_kit::test]
fn the_mirror_reads_measured_weights_back_and_undo_restores_the_layout(cx: &mut TestAppContext) {
    let (window, _app, workspace) = today_and_accounts(cx);
    let weights = cx.update(|cx| workspace.read(cx).layout().main_window().unwrap().root.as_ref().unwrap().weights().to_vec());
    assert!((weights[0] - 0.65).abs() < 1e-9 && (weights[1] - 0.35).abs() < 1e-9, "the model gave the new pane its default share: {weights:?}");

    // A frame measured the split; the mirror reads the measurement back.
    drive(cx, window, &workspace, |workspace, window, cx| workspace.mirror_from_area(window, cx));
    let mirrored = cx.update(|cx| workspace.read(cx).layout().main_window().unwrap().root.as_ref().unwrap().weights().to_vec());
    assert!((mirrored[0] - 0.65).abs() < 0.02 && (mirrored[1] - 0.35).abs() < 0.02, "measured sizes reproduce the weights: {mirrored:?}");
    assert_eq!(grid(cx, &workspace, 2, 1), "12", "an echo of the workspace's own edit changes nothing");
    cx.update(|cx| assert!(workspace.read(cx).layout().validate().is_empty()));

    let undone = drive(cx, window, &workspace, |workspace, window, cx| workspace.undo(window, cx));
    assert!(undone, "the split is one undo step");
    assert_eq!(grid(cx, &workspace, 2, 1), "11");
    assert_eq!(pane_count(cx, &workspace), 1);
    cx.update_window(window, |_, window, _| {
        assert!(window.find("screen-today").visible());
        assert!(window.try_find("screen-accounts").is_none(), "the undone pane left the screen");
    })
    .unwrap();
    let redone = drive(cx, window, &workspace, |workspace, window, cx| workspace.redo(window, cx));
    assert!(redone);
    assert_eq!(grid(cx, &workspace, 2, 1), "12");
    cx.update_window(window, |_, window, _| assert!(window.find("screen-accounts").visible(), "redo brings the pane back")).unwrap();
}

#[gpui_kit::test]
fn a_pane_removed_by_the_engine_leaves_the_model_too(cx: &mut TestAppContext) {
    let (window, app, workspace) = today_and_accounts(cx);
    let accounts = active(cx, &workspace);
    let view = cx.update(|cx| workspace.read(cx).pane(&accounts).cloned().expect("the pane's view"));
    // What the tab's close button does: the engine drops the panel and tells it.
    cx.update_window(window, |_, window, cx| {
        let area = workspace.read(cx).area().clone();
        area.update(cx, |area, cx| area.remove_panel(view, window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| window.render_frame(cx)).unwrap();
    assert_eq!(pane_count(cx, &workspace), 1, "the model followed the engine");
    assert_eq!(grid(cx, &workspace, 2, 1), "11");
    cx.update(|cx| {
        let workspace = workspace.read(cx);
        assert!(workspace.layout().validate().is_empty());
        assert!(workspace.layout().pane(&accounts).is_none());
        assert_eq!(workspace.active_route(cx), Some(Route::Today), "the remaining pane is active");
        assert_eq!(app.read(cx).route(), Route::Today, "and the chrome follows");
    });
}

// ----- panes never squash their screen ---------------------------------------------------

#[gpui_kit::test]
fn a_narrow_pane_gives_its_screen_the_minimum_width_and_scrolls_sideways(cx: &mut TestAppContext) {
    let (handle, _app, workspace) = open_workspace(cx, sample(Route::Today));
    let window: gpui_kit::AnyWindowHandle = handle.into();
    // The pane's padding and hairline border, on both sides.
    let inset = px(50.);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let pane = window.find("pane-1").bounds();
        let screen = window.find("screen-today").bounds();
        assert!(pane.size.width > MIN_CONTENT_WIDTH, "the whole column is wider than the minimum: {pane:?}");
        assert!((screen.size.width - (pane.size.width - inset)).abs() < px(4.), "a pane wider than the minimum gives its screen the whole width: {pane:?} {screen:?}");
    })
    .unwrap();

    drive(cx, window, &workspace, |workspace, window, cx| workspace.split_active(Side::Right, Route::Accounts, window, cx).expect("split"));
    cx.update_window(window, |_, window, cx| {
        let pane = window.find("pane-2").bounds();
        let screen = window.find("screen-accounts").bounds();
        assert!(pane.size.width < MIN_CONTENT_WIDTH, "the Accounts pane is narrower than the minimum: {pane:?}");
        assert!((screen.size.width - (MIN_CONTENT_WIDTH - inset)).abs() < px(4.), "the screen keeps the minimum width instead of shrinking with the pane: {screen:?}");
        assert!(screen.size.width > pane.size.width, "so it is wider than the pane and clipped at its edge");
        assert!(screen.left() >= pane.left(), "and starts at the pane's left edge before any scrolling: {pane:?} {screen:?}");
        // A sideways wheel over the pane reveals what is clipped on the right.
        window.scroll("pane-2", ScrollDelta::Pixels(point(px(-200.), px(0.))), cx);
        let scrolled = window.find("screen-accounts").bounds();
        assert!((screen.left() - scrolled.left() - px(200.)).abs() < px(1.), "the screen moved left by the wheel's delta: {screen:?} -> {scrolled:?}");
        assert!((scrolled.size.width - screen.size.width).abs() < px(1.), "without changing width");
    })
    .unwrap();
}

// ----- drag and drop -----------------------------------------------------------------------

#[gpui_kit::test]
fn dropping_a_pane_on_the_centre_of_another_stacks_them(cx: &mut TestAppContext) {
    let (window, _app, workspace) = today_and_accounts(cx);
    let order = cx.update(|cx| workspace.read(cx).panes_in_order());
    let (today, accounts) = (order[0].clone(), order[1].clone());
    drag_pane(cx, window, 2, 1, Zone::Centre);

    let tree = root(cx, &workspace);
    assert!(tree.is_stack(), "one stack holds both panes: {tree:?}");
    assert_eq!(tree.stack_panes(), [today.clone(), accounts.clone()]);
    assert_eq!(active(cx, &workspace), accounts, "the dragged pane is the active one");
    assert_eq!(grid(cx, &workspace, 2, 1), "22", "and the one on show");
    assert_eq!(pane_count(cx, &workspace), 2);
    cx.update(|cx| {
        let workspace = workspace.read(cx);
        assert_eq!(workspace.layout().stack_of(&today), workspace.layout().stack_of(&accounts));
        assert_eq!(workspace.pane_route(&today, cx), Some(Route::Today), "the hidden tab's pane keeps its screen");
        assert_eq!(workspace.pane_route(&accounts, cx), Some(Route::Accounts));
        assert!(workspace.layout().validate().is_empty());
    });
    cx.update_window(window, |_, window, _| {
        assert!(window.find("screen-accounts").visible(), "the dragged pane's screen is on show");
        assert!(window.try_find("screen-today").is_none(), "Today is behind its tab");
    })
    .unwrap();
    assert_eq!(history_labels(cx, &workspace), ["Open Today", "Open Accounts", "Move pane"]);
}

#[gpui_kit::test]
fn dropping_a_pane_on_an_edge_splits_there_and_undo_restores_the_layout_exactly(cx: &mut TestAppContext) {
    let (window, _app, workspace) = three_columns(cx);
    let before = layout_json(cx, &workspace);
    let weights_before = root_weights(cx, &workspace);
    let order = cx.update(|cx| workspace.read(cx).panes_in_order());

    // Rules (3) onto the left edge of Today (1): 3|1|2.
    drag_pane(cx, window, 3, 1, Zone::Left);
    assert_eq!(grid(cx, &workspace, 3, 1), "123", "numbered in reading order: Rules is now first");
    let panes = cx.update(|cx| workspace.read(cx).panes_in_order());
    assert_eq!(panes, [order[2].clone(), order[0].clone(), order[1].clone()]);
    assert_eq!(active(cx, &workspace), order[2], "the moved pane is the active one");
    let weights = root_weights(cx, &workspace);
    assert!(about(weights[2], weights_before[1]), "Accounts, which the drop did not touch, keeps its width: {weights:?} vs {weights_before:?}");
    assert!(about(weights[0], weights[1]), "Rules and Today share the slot Today had plus the one Rules left: {weights:?}");
    cx.update_window(window, |_, window, _| {
        let rules = window.find("pane-3").bounds();
        let today = window.find("pane-1").bounds();
        assert!(rules.right() <= today.left(), "Rules is drawn left of Today: {rules:?} {today:?}");
        assert!(window.find("screen-rules").visible() && window.find("screen-today").visible() && window.find("screen-accounts").visible());
    })
    .unwrap();
    assert_eq!(history_labels(cx, &workspace), ["Open Today", "Open Accounts", "Open Rules", "Move pane"]);

    let undone = drive(cx, window, &workspace, |workspace, window, cx| workspace.undo(window, cx));
    assert!(undone);
    assert_eq!(layout_json(cx, &workspace), before, "undo restores the layout byte for byte");
    assert_eq!(grid(cx, &workspace, 8, 1), "11111223");
    cx.update_window(window, |_, window, _| {
        let today = window.find("pane-1").bounds();
        let rules = window.find("pane-3").bounds();
        assert!(today.right() <= rules.left(), "and the screen follows: {today:?} {rules:?}");
    })
    .unwrap();
}

#[gpui_kit::test]
fn dropping_a_pane_below_another_wraps_that_pane_in_a_column_and_leaves_the_rest_alone(cx: &mut TestAppContext) {
    let (window, _app, workspace) = three_columns(cx);
    let weights_before = root_weights(cx, &workspace);
    let order = cx.update(|cx| workspace.read(cx).panes_in_order());
    let (today, accounts, rules) = (order[0].clone(), order[1].clone(), order[2].clone());

    // Rules (3) onto the bottom edge of Today (1): horizontal[vertical[1, 3], 2].
    drag_pane(cx, window, 3, 1, Zone::Bottom);
    let tree = root(cx, &workspace);
    assert_eq!(tree.axis(), Some(Axis::Horizontal), "{tree:?}");
    assert_eq!(tree.children().len(), 2);
    let column = &tree.children()[0];
    assert_eq!(column.axis(), Some(Axis::Vertical));
    assert_eq!(column.panes(), [today.clone(), rules.clone()]);
    assert!(about(column.weights()[0], 0.5) && about(column.weights()[1], 0.5), "the dropped pane takes half, as the indicator showed: {:?}", column.weights());
    assert_eq!(tree.children()[1].stack_panes(), [accounts.clone()]);
    assert!(about(tree.weights()[1], weights_before[1]), "Accounts keeps its weight: {:?} vs {weights_before:?}", tree.weights());
    assert!(about(tree.weights()[0], weights_before[0] + weights_before[2]), "the column has Today's slot plus the one Rules left");
    cx.update_window(window, |_, window, _| {
        let today = window.find("pane-1").bounds();
        let rules = window.find("pane-3").bounds();
        let accounts = window.find("pane-2").bounds();
        assert!(rules.top() >= today.bottom(), "Rules is drawn under Today: {today:?} {rules:?}");
        assert!(accounts.left() >= today.right() && accounts.left() >= rules.right(), "Accounts stays to the right of both");
    })
    .unwrap();
    cx.update(|cx| assert!(workspace.read(cx).layout().validate().is_empty()));
}

#[gpui_kit::test]
fn escape_cancels_a_drag_with_nothing_changed(cx: &mut TestAppContext) {
    let (window, _app, workspace) = today_and_accounts(cx);
    let before = layout_json(cx, &workspace);
    let labels = history_labels(cx, &workspace);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let handle = window.find(title_id(2)).bounds().center();
        let target = zone_point(window.find(pane_id(1)).bounds(), Zone::Right);
        press_at(window, handle, cx);
        move_pressed_to(window, handle + point(px(12.), px(4.)), cx);
        move_pressed_to(window, target, cx);
        assert!(cx.has_active_drag(), "the title started a drag");
        window.press("escape", cx);
        assert!(!cx.has_active_drag(), "Escape ended it");
        release_at(window, target, cx);
    })
    .unwrap();
    settle(cx, window);
    assert_eq!(layout_json(cx, &workspace), before, "nothing about the layout changed");
    assert_eq!(history_labels(cx, &workspace), labels, "and nothing was recorded");
    assert_eq!(grid(cx, &workspace, 2, 1), "12");
    cx.update_window(window, |_, window, cx| {
        assert!(window.notifications(cx).is_empty(), "no toast either");
        assert!(window.find("screen-today").visible() && window.find("screen-accounts").visible());
        // The pane body is not a drag handle: pressing and moving inside a
        // screen selects and scrolls, it never picks the pane up. (In the
        // active pane, so the press changes nothing else either.)
        let inside = window.find(pane_id(2)).bounds().origin + point(px(8.), px(8.));
        press_at(window, inside, cx);
        move_pressed_to(window, inside + point(px(60.), px(40.)), cx);
        assert!(!cx.has_active_drag(), "a press in the body does not start a drag");
        release_at(window, inside + point(px(60.), px(40.)), cx);
    })
    .unwrap();
    settle(cx, window);
    assert_eq!(layout_json(cx, &workspace), before);
}

#[gpui_kit::test]
fn dropping_a_pane_back_where_it_was_records_nothing(cx: &mut TestAppContext) {
    let (window, _app, workspace) = today_and_accounts(cx);
    let before = layout_json(cx, &workspace);
    let labels = history_labels(cx, &workspace);
    // Accounts already sits on Today's right; dropping it there is the layout
    // it came from. The engine halves Today's slot while working that out and
    // the model puts it back, so the weights are the old ones too.
    drag_pane(cx, window, 2, 1, Zone::Right);
    assert_eq!(layout_json(cx, &workspace), before, "the layout is byte for byte what it was");
    assert_eq!(history_labels(cx, &workspace), labels, "and no step was recorded");
    cx.update_window(window, |_, window, cx| {
        assert!(window.notifications(cx).is_empty());
        let today = window.find("pane-1").bounds();
        let accounts = window.find("pane-2").bounds();
        assert!(accounts.left() >= today.right());
        assert!((today.size.width / (today.size.width + accounts.size.width) - 0.65).abs() < 0.02, "the screen shows the old widths: {today:?} {accounts:?}");
    })
    .unwrap();
}

#[gpui_kit::test]
fn dragging_a_divider_resizes_the_split_as_one_undo_step_per_run(cx: &mut TestAppContext) {
    let (window, _app, workspace) = today_and_accounts(cx);
    let before = root_weights(cx, &workspace);
    let labels_before = history_labels(cx, &workspace);
    let divider_drag = |cx: &mut TestAppContext, distance: f32| {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let accounts = window.find("pane-2").bounds();
            // The divider's grab area straddles the slot edge, 4 px to its left.
            let from = point(accounts.left() - px(2.), accounts.center().y);
            window.drag(from, from + point(px(distance), px(0.)), cx);
        })
        .unwrap();
        settle(cx, window);
    };

    divider_drag(cx, 150.);
    let after_first = root_weights(cx, &workspace);
    assert!(after_first[0] > before[0] + 0.05, "Today grew by the drag: {before:?} -> {after_first:?}");
    assert!(about(after_first[0] + after_first[1], 1.0));
    let mut expected = labels_before.clone();
    expected.push("Resize split".to_owned());
    assert_eq!(history_labels(cx, &workspace), expected, "one entry for the drag");

    divider_drag(cx, 100.);
    let after_second = root_weights(cx, &workspace);
    assert!(after_second[0] > after_first[0] + 0.03, "the second drag moved it further: {after_first:?} -> {after_second:?}");
    assert_eq!(history_labels(cx, &workspace), expected, "a second drag of the same divider joins the entry");

    let undone = drive(cx, window, &workspace, |workspace, window, cx| workspace.undo(window, cx));
    assert!(undone);
    let restored = root_weights(cx, &workspace);
    assert!(about(restored[0], before[0]) && about(restored[1], before[1]), "undo goes back to before the first drag: {restored:?} vs {before:?}");
    cx.update(|cx| assert!(workspace.read(cx).layout().validate().is_empty()));
}

#[gpui_kit::test]
fn a_drop_that_would_squeeze_a_pane_below_the_minimum_is_refused_with_a_toast(cx: &mut TestAppContext) {
    let (window, _app, workspace) = today_and_accounts(cx);
    cx.update(|cx| workspace.update(cx, |workspace, _| workspace.set_split_limits(SplitLimits { min_share: 0.6, max_depth: 12 })));
    let before = layout_json(cx, &workspace);
    let labels = history_labels(cx, &workspace);

    // Accounts under Today would give Today half the height: below 0.6.
    drag_pane(cx, window, 2, 1, Zone::Bottom);
    assert_eq!(layout_json(cx, &workspace), before, "the layout is exactly what it was");
    assert_eq!(history_labels(cx, &workspace), labels, "nothing was recorded");
    assert_eq!(grid(cx, &workspace, 2, 1), "12");
    cx.update_window(window, |_, window, cx| {
        let toast = window.find(REFUSED_SPLIT_TOAST_ID);
        assert_eq!(toast.label(), Some(REFUSED_SPLIT_MESSAGE), "the toast says why and what to do instead");
        assert_eq!(window.notifications(cx).len(), 1);
        let today = window.find("pane-1").bounds();
        let accounts = window.find("pane-2").bounds();
        assert!(accounts.left() >= today.right(), "the screen shows the layout from before the drop: {today:?} {accounts:?}");
        assert!(window.find("screen-today").visible() && window.find("screen-accounts").visible());
    })
    .unwrap();

    // With room for it, the same kind of drop goes through: Accounts above
    // Today, and no new toast. (The dismissed one may still be on its way out.)
    dismiss_toasts(cx, window);
    let toasts_after_dismissal = cx.update_window(window, |_, window, cx| window.notifications(cx).len()).unwrap();
    cx.update(|cx| workspace.update(cx, |workspace, _| workspace.set_split_limits(SplitLimits::default())));
    drag_pane(cx, window, 2, 1, Zone::Top);
    let tree = root(cx, &workspace);
    assert_eq!(tree.axis(), Some(Axis::Vertical), "{tree:?}");
    let order = cx.update(|cx| workspace.read(cx).panes_in_order());
    assert_eq!(cx.update(|cx| workspace.read(cx).pane_route(&order[0], cx)), Some(Route::Accounts), "Accounts is now the top pane");
    assert_eq!(grid(cx, &workspace, 1, 2), "1\n2");
    cx.update_window(window, |_, window, cx| {
        assert!(window.notifications(cx).len() <= toasts_after_dismissal, "no toast for a drop that was allowed");
        let accounts = window.find("pane-2").bounds();
        let today = window.find("pane-1").bounds();
        assert!(today.top() >= accounts.bottom(), "Today is drawn under Accounts: {accounts:?} {today:?}");
    })
    .unwrap();
}

// ----- docking beside a group, a run of siblings, or the window ------------------------

/// Today, Accounts and Rules as three equal columns, with a fourth pane
/// (Forecast) tabbed behind Rules: the layout the cross-column cases start
/// from. The picture is `123/123/123` with Forecast (4) displayed on the right.
fn three_equal_columns_and_a_tab(cx: &mut TestAppContext) -> (gpui_kit::AnyWindowHandle, Entity<WorkspaceView>) {
    let mut launch = sample(Route::Today);
    launch.extra = vec![Route::Accounts, Route::Rules];
    launch.stacked = vec![(2, Route::ForecastPath)];
    let (handle, _app, workspace) = open_workspace(cx, launch);
    let window: gpui_kit::AnyWindowHandle = handle.into();
    settle(cx, window);
    assert_eq!(grid(cx, &workspace, 3, 3), "124\n124\n124", "Forecast is the displayed tab of the third column");
    assert_eq!(pane_count(cx, &workspace), 4);
    (window, workspace)
}

/// Starts dragging pane `from` by its title and moves the pointer to the
/// centre of pane `over`, so the overlay lays its bands over that pane.
/// Returns nothing; the drag is left in flight for the caller.
fn start_drag_over(window: &mut Window, from: u64, over: u64, cx: &mut gpui_kit::App) {
    window.render_frame(cx);
    let handle = window.find(title_id(from)).bounds().center();
    let over = window.find(pane_id(over)).bounds().center();
    press_at(window, handle, cx);
    move_pressed_to(window, handle + point(px(12.), px(4.)), cx);
    move_pressed_to(window, over, cx);
    // The overlay appears on the frame after the first move over the pane.
    window.render_frame(cx);
    assert!(cx.has_active_drag(), "the title started a drag");
}

/// Moves the drag in flight onto the band `dock-band-<side>-<depth>` and
/// returns the band's centre.
fn hover_band(window: &mut Window, side: &str, depth: usize, cx: &mut gpui_kit::App) -> Point<Pixels> {
    let id = SharedString::from(format!("dock-band-{side}-{depth}"));
    let centre = window.find(id).bounds().center();
    move_pressed_to(window, centre, cx);
    window.render_frame(cx);
    centre
}

/// Drags pane `from` onto the band `dock-band-<side>-<depth>` of the pane
/// `over` and drops it there.
fn drag_to_band(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle, from: u64, over: u64, side: &str, depth: usize) {
    cx.update_window(window, |_, window, cx| {
        start_drag_over(window, from, over, cx);
        let centre = hover_band(window, side, depth, cx);
        release_at(window, centre, cx);
    })
    .unwrap();
    settle(cx, window);
}

#[gpui_kit::test]
fn dropping_on_the_band_of_two_columns_spans_exactly_those_columns(cx: &mut TestAppContext) {
    let (window, workspace) = three_equal_columns_and_a_tab(cx);
    let before = root_weights(cx, &workspace);
    // The band just inside the window band, on the bottom of the third
    // column, is the run "Accounts and Rules": Forecast goes under those two.
    drag_to_band(cx, window, 4, 4, "bottom", 1);
    assert_eq!(grid(cx, &workspace, 3, 3), "123\n123\n144", "the cross-column layout, without touching Today");
    let tree = root(cx, &workspace);
    assert!(about(tree.weights()[0], before[0]), "Today keeps its width: {:?} vs {before:?}", tree.weights());
    assert_eq!(pane_count(cx, &workspace), 4, "no pane was lost or duplicated by the drop");
    assert_eq!(history_labels(cx, &workspace).last().map(String::as_str), Some("Move pane"));
    cx.update_window(window, |_, window, _| {
        assert!(window.try_find("dock-targets").is_none(), "the overlay is gone after the drop");
        let today = window.find(pane_id(1)).bounds();
        let forecast = window.find(pane_id(4)).bounds();
        let accounts = window.find(pane_id(2)).bounds();
        assert!(forecast.left() >= today.right() - px(2.), "Forecast starts where Today ends: {today:?} {forecast:?}");
        assert!(forecast.top() >= accounts.bottom() - px(2.), "and sits under Accounts: {accounts:?} {forecast:?}");
        assert!(window.find("screen-forecast").visible() && window.find("screen-today").visible());
    })
    .unwrap();
    // Undo puts the tab back.
    drive(cx, window, &workspace, |workspace, window, cx| assert!(workspace.undo(window, cx)));
    assert_eq!(grid(cx, &workspace, 3, 3), "124\n124\n124");
}

#[gpui_kit::test]
fn dropping_on_the_window_band_spans_the_whole_window(cx: &mut TestAppContext) {
    let (window, workspace) = three_equal_columns_and_a_tab(cx);
    drag_to_band(cx, window, 4, 4, "bottom", 0);
    assert_eq!(grid(cx, &workspace, 3, 3), "123\n123\n444");
    let tree = root(cx, &workspace);
    assert_eq!(tree.axis(), Some(Axis::Vertical));
    let row = &tree.children()[0];
    assert!(row.weights().iter().all(|weight| about(*weight, 1.0 / 3.0)), "the three columns keep their ratio: {:?}", row.weights());
}

#[gpui_kit::test]
fn dropping_on_a_groups_band_divides_only_that_groups_slot(cx: &mut TestAppContext) {
    // `1 | [2 / 3]`, then a fourth pane tabbed behind 3.
    let (window, _app, workspace) = today_and_accounts(cx);
    drive(cx, window, &workspace, |workspace, window, cx| workspace.split_active(Side::Bottom, Route::Rules, window, cx).expect("split below"));
    assert_eq!(grid(cx, &workspace, 4, 2), "1112\n1113");
    let rules = active(cx, &workspace);
    drive(cx, window, &workspace, |workspace, window, cx| workspace.stack_onto(&rules, Route::ForecastPath, window, cx).expect("tab"));
    let before = root_weights(cx, &workspace);
    // Right of the column: bands are the column (inner) and the window (outer).
    drag_to_band(cx, window, 4, 4, "right", 1);
    let tree = root(cx, &workspace);
    assert_eq!(tree.axis(), Some(Axis::Horizontal));
    assert_eq!(tree.children().len(), 3, "Today, the column, Forecast: {tree:?}");
    assert!(about(tree.weights()[0], before[0]), "Today's slot is untouched: {:?} vs {before:?}", tree.weights());
    assert!(about(tree.weights()[1] + tree.weights()[2], before[1]), "the column's slot was divided between the column and Forecast");
    assert_eq!(tree.children()[1].panes().len(), 2, "Accounts and Rules still share the column");
    assert_eq!(tree.children()[2].panes().len(), 1);
}

#[gpui_kit::test]
fn the_analysis_shape_is_reachable_by_a_drag_and_a_resize(cx: &mut TestAppContext) {
    let (window, workspace) = three_equal_columns_and_a_tab(cx);
    drag_to_band(cx, window, 4, 4, "bottom", 1);
    assert_eq!(grid(cx, &workspace, 3, 3), "123\n123\n144");
    // The new column is a vertical split of the 2–3 row over Forecast; a
    // third for the row and two thirds for Forecast is `123/144/144`.
    let column = cx.update(|cx| {
        let tree = workspace.read(cx).layout().main_window().and_then(|window| window.root.clone()).expect("a tree");
        tree.children()[1].id().clone()
    });
    drive(cx, window, &workspace, |workspace, window, cx| workspace.resize_split(&column, &[1.0 / 3.0, 2.0 / 3.0], window, cx).expect("resize"));
    assert_eq!(grid(cx, &workspace, 3, 3), "123\n144\n144");
}

#[gpui_kit::test]
fn hovering_a_band_shows_the_preview_and_escape_leaves_everything_as_it_was(cx: &mut TestAppContext) {
    let (window, workspace) = three_equal_columns_and_a_tab(cx);
    let before = layout_json(cx, &workspace);
    let labels = history_labels(cx, &workspace);
    cx.update_window(window, |_, window, cx| {
        start_drag_over(window, 4, 4, cx);
        assert!(window.try_find("dock-targets").is_some(), "the overlay is up while a pane is dragged");
        assert!(window.try_find("dock-preview").is_none(), "no preview until a band is hovered");
        hover_band(window, "bottom", 1, cx);
        let preview = window.find("dock-preview").bounds();
        let today = window.find(pane_id(1)).bounds();
        let accounts = window.find(pane_id(2)).bounds();
        assert!(preview.left() >= today.right() - px(2.) && preview.top() >= accounts.center().y, "the preview is the bottom of the 2–3 columns: {preview:?}");
        assert_eq!(window.find("dock-band-label").label(), Some("Dock below these 2 panes"));
        assert_eq!(window.find("dock-band-bottom-0").label(), Some("Dock along the bottom of the window"));
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(!cx.has_active_drag());
        assert!(window.try_find("dock-targets").is_none(), "Escape takes the overlay down");
        assert!(window.try_find("dock-preview").is_none());
        release_at(window, window.find(pane_id(4)).bounds().center(), cx);
    })
    .unwrap();
    settle(cx, window);
    assert_eq!(layout_json(cx, &workspace), before, "nothing changed");
    assert_eq!(history_labels(cx, &workspace), labels);
    assert_eq!(grid(cx, &workspace, 3, 3), "124\n124\n124");
}

#[gpui_kit::test]
fn space_cycles_the_inner_levels_while_the_window_band_stays(cx: &mut TestAppContext) {
    // Five columns: the middle one has four runs below it, more than fit.
    let mut launch = sample(Route::Today);
    launch.extra = vec![Route::Accounts, Route::Rules, Route::ForecastPath, Route::People];
    let (handle, _app, workspace) = open_workspace(cx, launch);
    let window: gpui_kit::AnyWindowHandle = handle.into();
    settle(cx, window);
    assert_eq!(grid(cx, &workspace, 5, 1), "12345");
    cx.update_window(window, |_, window, cx| {
        start_drag_over(window, 5, 3, cx);
        hover_band(window, "bottom", 2, cx);
        assert_eq!(window.find("dock-band-label").label(), Some("Dock below these 2 panes"), "the innermost band: Rules with Accounts");
        assert_eq!(window.find("dock-band-bottom-0").label(), Some("Dock along the bottom of the window"));
        window.press("space", cx);
        window.render_frame(cx);
        assert!(cx.has_active_drag(), "Space does not end the drag");
        assert_eq!(window.find("dock-band-label").label(), Some("Dock below these 3 panes"), "after Space the innermost band is the next run");
        assert_eq!(window.find("dock-band-bottom-0").label(), Some("Dock along the bottom of the window"), "the window band stays outermost");
        window.press("escape", cx);
        release_at(window, window.find(pane_id(3)).bounds().center(), cx);
    })
    .unwrap();
    settle(cx, window);
    assert_eq!(grid(cx, &workspace, 5, 1), "12345");
}

#[gpui_kit::test]
fn a_band_the_minimum_size_rule_refuses_takes_no_drop(cx: &mut TestAppContext) {
    let (window, workspace) = three_equal_columns_and_a_tab(cx);
    // Nothing under 40 % of the window: a third row for Forecast is too small.
    cx.update(|cx| workspace.update(cx, |workspace, _| workspace.set_split_limits(SplitLimits { min_share: 0.4, max_depth: 12 })));
    let before = layout_json(cx, &workspace);
    let labels = history_labels(cx, &workspace);
    cx.update_window(window, |_, window, cx| {
        start_drag_over(window, 4, 4, cx);
        hover_band(window, "bottom", 0, cx);
        assert_eq!(window.find("dock-band-bottom-0").label(), Some("Dock along the bottom of the window (not enough room)"));
        assert_eq!(window.find("dock-band-label").label(), Some("Not enough room here"));
        assert!(window.try_find("dock-preview").is_none(), "a refused band previews nothing");
        let centre = window.find("dock-band-bottom-0").bounds().center();
        release_at(window, centre, cx);
    })
    .unwrap();
    settle(cx, window);
    assert_eq!(layout_json(cx, &workspace), before, "the drop changed nothing");
    assert_eq!(history_labels(cx, &workspace), labels);
    assert_eq!(grid(cx, &workspace, 3, 3), "124\n124\n124");
}

// ----- pane definitions follow the pane; placeholders -------------------------------------

/// The route the app's navigation opens in the active pane, driven the way a
/// breadcrumb or a row click does it.
fn navigate(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle, app: &Entity<AtlasApp>, route: Route) {
    cx.update(|cx| app.update(cx, |app, cx| app.navigate(route, cx)));
    settle(cx, window);
}

#[gpui_kit::test]
fn a_pane_that_drills_into_a_record_is_found_by_the_resolver(cx: &mut TestAppContext) {
    let (window, app, workspace) = today_and_accounts(cx);
    let accounts_pane = active(cx, &workspace);
    let account = cx.update(|cx| app.read(cx).household().accounts[0].id);
    navigate(cx, window, &app, Route::Account(account));
    cx.update(|cx| {
        let layout = workspace.read(cx).layout();
        let definition = layout.pane(&accounts_pane).expect("the pane is still in the model");
        assert_eq!(definition.kind, "account", "the definition follows the pane into the record");
        assert_eq!(definition.resource, Some(serde_json::json!({ "accountId": account.raw() })));
        assert_eq!(definition.view_state["history"][0]["kind"], "accounts", "and remembers where it came from");
        assert_eq!(workspace.read(cx).pane_route(&accounts_pane, cx), Some(Route::Account(account)));
    });
    // "Open this account" now finds the pane that shows it…
    let found = drive(cx, window, &workspace, |workspace, window, cx| workspace.open(Route::Account(account), Intent::Open, window, cx).expect("open"));
    assert_eq!(found, accounts_pane);
    assert_eq!(pane_count(cx, &workspace), 2);
    // …while "open Accounts" no longer does, since no pane shows the register.
    let opened = drive(cx, window, &workspace, |workspace, window, cx| workspace.open(Route::Accounts, Intent::Open, window, cx).expect("open"));
    assert_ne!(opened, accounts_pane);
    assert_eq!(pane_count(cx, &workspace), 3);
    cx.update(|cx| {
        let layout = workspace.read(cx).layout();
        assert_eq!(layout.stack_of(&opened), layout.stack_of(&accounts_pane), "a plain open lands as a tab of the active stack");
    });
    cx.update_window(window, |_, window, _| {
        assert!(window.find("screen-accounts").visible(), "the new tab is displayed");
        assert!(window.try_find("screen-account").is_none(), "the record's pane is behind it");
    })
    .unwrap();
}

#[gpui_kit::test]
fn back_history_survives_a_closed_pane_being_restored(cx: &mut TestAppContext) {
    let (window, app, workspace) = today_and_accounts(cx);
    let accounts_pane = active(cx, &workspace);
    let account = cx.update(|cx| app.read(cx).household().accounts[0].id);
    navigate(cx, window, &app, Route::Account(account));
    drive(cx, window, &workspace, |workspace, window, cx| workspace.close_pane(&accounts_pane, window, cx).expect("close"));
    assert_eq!(pane_count(cx, &workspace), 1);
    // Undo rebuilds the pane from its definition: the record, and the history.
    drive(cx, window, &workspace, |workspace, window, cx| assert!(workspace.undo(window, cx)));
    assert_eq!(pane_count(cx, &workspace), 2);
    cx.update(|cx| {
        let workspace = workspace.read(cx);
        assert_eq!(workspace.pane_route(&accounts_pane, cx), Some(Route::Account(account)));
        let view = workspace.pane(&accounts_pane).expect("a view").read(cx);
        assert_eq!(view.history(), &[Route::Accounts], "Back still leads to the register");
        assert_eq!(workspace.active_pane(), Some(accounts_pane.clone()));
    });
    let back = drive(cx, window, &workspace, |workspace, _, cx| workspace.back_active(cx));
    assert_eq!(back, Some(Route::Accounts));
    cx.update(|cx| assert_eq!(workspace.read(cx).layout().pane(&accounts_pane).map(|definition| definition.kind.clone()), Some("accounts".to_string())));
}

#[gpui_kit::test]
fn an_unknown_pane_kind_shows_a_placeholder_that_can_be_replaced_or_closed(cx: &mut TestAppContext) {
    let (window, _app, workspace) = today_and_accounts(cx);
    let accounts_pane = active(cx, &workspace);
    // A layout from a newer version: one pane of a kind this build has never heard of.
    let mut layout = cx.update(|cx| workspace.read(cx).layout().clone());
    layout.replace_pane(&accounts_pane, atlas_workspace::PaneDefinition::new("tax-review")).expect("replace");
    drive(cx, window, &workspace, |workspace, window, cx| workspace.load_layout(layout, "Load a newer layout", window, cx));
    assert_eq!(pane_count(cx, &workspace), 2, "the pane keeps its place");
    cx.update(|cx| {
        let layout = workspace.read(cx).layout();
        assert_eq!(layout.pane(&accounts_pane).map(|definition| definition.kind.as_str()), Some("tax-review"), "the definition is kept, not dropped");
        assert!(workspace.read(cx).pane(&accounts_pane).unwrap().read(cx).placeholder().is_some());
    });
    cx.update_window(window, |_, window, cx| {
        assert!(window.find("pane-unsupported").visible(), "the placeholder stands in for the screen");
        assert!(window.find("screen-today").visible(), "the rest of the layout loaded as saved");
        assert!(window.try_find("screen-accounts").is_none());
        window.click("pane-replace", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("popup-menu").visible(), "Replace pane lists the screens");
        // Today, Decisions, Forecast, Accounts, …: Accounts is the fourth entry.
        window.within("popup-menu").click(3usize, cx);
    })
    .unwrap();
    settle(cx, window);
    cx.update(|cx| {
        let layout = workspace.read(cx).layout();
        assert_eq!(layout.pane(&accounts_pane).map(|definition| definition.kind.as_str()), Some("accounts"), "the definition is the replacement's");
        assert!(workspace.read(cx).pane(&accounts_pane).unwrap().read(cx).placeholder().is_none());
    });
    cx.update_window(window, |_, window, _| {
        assert!(window.find("screen-accounts").visible(), "the replacement screen shows in the same pane");
        assert!(window.try_find("pane-unsupported").is_none());
    })
    .unwrap();
    assert_eq!(history_labels(cx, &workspace).last().map(String::as_str), Some("Replace pane with Accounts"));

    // The other way out: the placeholder's Close button.
    let mut layout = cx.update(|cx| workspace.read(cx).layout().clone());
    layout.replace_pane(&accounts_pane, atlas_workspace::PaneDefinition::new("tax-review")).expect("replace");
    drive(cx, window, &workspace, |workspace, window, cx| workspace.load_layout(layout, "Load a newer layout", window, cx));
    cx.update_window(window, |_, window, cx| {
        assert!(window.find("pane-unsupported").visible());
        window.click("pane-close", cx);
    })
    .unwrap();
    settle(cx, window);
    assert_eq!(pane_count(cx, &workspace), 1);
    assert_eq!(grid(cx, &workspace, 1, 1), "1");
}

#[gpui_kit::test]
fn a_missing_record_shows_the_unavailable_placeholder(cx: &mut TestAppContext) {
    let (window, app, workspace) = today_and_accounts(cx);
    let accounts_pane = active(cx, &workspace);
    // An account that was deleted since the layout was saved.
    navigate(cx, window, &app, Route::Account(atlas_core::ids::AccountId::new(9_999)));
    cx.update_window(window, |_, window, _| {
        let placeholder = window.find("pane-unavailable");
        assert!(placeholder.visible(), "the pane says the account is unavailable instead of drawing the screen");
        assert!(window.find("pane-replace").visible() && window.find("pane-close").visible());
        assert!(window.find("screen-today").visible(), "the other pane is unaffected");
    })
    .unwrap();
    cx.update(|cx| {
        assert!(workspace.read(cx).pane(&accounts_pane).unwrap().read(cx).placeholder().is_none(), "an unavailable record is not an unknown kind: the pane still knows its route");
        assert_eq!(workspace.read(cx).layout().pane(&accounts_pane).map(|definition| definition.kind.as_str()), Some("account"));
    });
    // Back leads out of it like any other route.
    let back = drive(cx, window, &workspace, |workspace, _, cx| workspace.back_active(cx));
    assert_eq!(back, Some(Route::Accounts));
    cx.update_window(window, |_, window, _| {
        assert!(window.try_find("pane-unavailable").is_none());
        assert!(window.find("screen-accounts").visible());
    })
    .unwrap();
}

// ----- the launcher strip and the title bar --------------------------------------------

/// Clicks `id` with Shift held (the test helpers' `click` has no modifiers).
fn shift_click(window: &mut Window, id: &str, cx: &mut gpui_kit::App) {
    let position = window.find(SharedString::from(id.to_string())).bounds().center();
    let modifiers = gpui_kit::Modifiers { shift: true, ..Default::default() };
    window.dispatch_event(MouseMoveEvent { position, pressed_button: None, modifiers }.to_platform_input(), cx);
    window.render_frame(cx);
    window.dispatch_event(MouseDownEvent { button: MouseButton::Left, position, modifiers, click_count: 1, first_mouse: false }.to_platform_input(), cx);
    window.dispatch_event(MouseUpEvent { button: MouseButton::Left, position, modifiers, click_count: 1 }.to_platform_input(), cx);
    window.render_frame(cx);
}

#[gpui_kit::test]
fn shift_click_on_the_launcher_opens_another_instance(cx: &mut TestAppContext) {
    let (handle, _app, workspace) = open_workspace(cx, sample(Route::Today));
    let window: gpui_kit::AnyWindowHandle = handle.into();
    settle(cx, window);
    cx.update_window(window, |_, window, cx| window.click("launcher-today", cx)).unwrap();
    settle(cx, window);
    assert_eq!(pane_count(cx, &workspace), 1, "a click on the screen already shown focuses it");
    cx.update_window(window, |_, window, cx| shift_click(window, "launcher-today", cx)).unwrap();
    settle(cx, window);
    assert_eq!(pane_count(cx, &workspace), 2, "Shift-click opens a second Today pane");
    let order = cx.update(|cx| workspace.read(cx).panes_in_order());
    cx.update(|cx| {
        let workspace = workspace.read(cx);
        assert_eq!(workspace.pane_route(&order[0], cx), Some(Route::Today));
        assert_eq!(workspace.pane_route(&order[1], cx), Some(Route::Today));
        assert_eq!(workspace.layout().stack_of(&order[0]), workspace.layout().stack_of(&order[1]), "in the active stack, as tabs");
    });
}

#[gpui_kit::test]
fn the_launcher_menu_opens_a_screen_to_the_right(cx: &mut TestAppContext) {
    let (handle, _app, workspace) = open_workspace(cx, sample(Route::Today));
    let window: gpui_kit::AnyWindowHandle = handle.into();
    settle(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.right_click("launcher-item-forecast", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("popup-menu").visible(), "right-click opens the item's menu");
        // Open, Open new instance, Open right, …
        window.within("popup-menu").click(2usize, cx);
    })
    .unwrap();
    settle(cx, window);
    assert_eq!(pane_count(cx, &workspace), 2);
    assert_eq!(grid(cx, &workspace, 3, 1), "112", "Forecast opened beside Today, taking the default share");
    let forecast = active(cx, &workspace);
    assert_eq!(cx.update(|cx| workspace.read(cx).pane_route(&forecast, cx)), Some(Route::ForecastPath));
}

#[gpui_kit::test]
fn a_screen_dragged_from_the_launcher_becomes_a_pane_where_it_is_dropped(cx: &mut TestAppContext) {
    let (window, _app, workspace) = today_and_accounts(cx);
    // Onto the bottom edge zone of the Today pane: the engine reports the
    // drop and a new Forecast pane opens under Today.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let handle = window.find("launcher-item-forecast").bounds().center();
        let target = zone_point(window.find(pane_id(1)).bounds(), Zone::Bottom);
        window.drag(handle, target, cx);
    })
    .unwrap();
    settle(cx, window);
    assert_eq!(pane_count(cx, &workspace), 3);
    let tree = root(cx, &workspace);
    assert_eq!(tree.axis(), Some(Axis::Horizontal));
    let column = &tree.children()[0];
    assert_eq!(column.axis(), Some(Axis::Vertical), "Today's slot became a column: {tree:?}");
    let order = cx.update(|cx| workspace.read(cx).panes_in_order());
    assert_eq!(cx.update(|cx| workspace.read(cx).pane_route(&order[1], cx)), Some(Route::ForecastPath));
    assert_eq!(history_labels(cx, &workspace).last().map(String::as_str), Some("Open Forecast"));

    // Onto the window's bottom band: a Rules pane along the whole window.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let handle = window.find("launcher-item-rules").bounds().center();
        let over = window.find(pane_id(2)).bounds().center();
        press_at(window, handle, cx);
        move_pressed_to(window, handle + point(px(12.), px(4.)), cx);
        move_pressed_to(window, over, cx);
        window.render_frame(cx);
        assert!(cx.has_active_drag());
        assert!(window.try_find("dock-targets").is_some(), "the bands show for a launcher drag too");
        let band = window.find("dock-band-bottom-0").bounds().center();
        move_pressed_to(window, band, cx);
        window.render_frame(cx);
        assert_eq!(window.find("dock-band-label").label(), Some("Dock along the bottom of the window"));
        release_at(window, band, cx);
    })
    .unwrap();
    settle(cx, window);
    assert_eq!(pane_count(cx, &workspace), 4);
    let tree = root(cx, &workspace);
    assert_eq!(tree.axis(), Some(Axis::Vertical), "the new pane spans the window's bottom: {tree:?}");
    let order = cx.update(|cx| workspace.read(cx).panes_in_order());
    assert_eq!(cx.update(|cx| workspace.read(cx).pane_route(order.last().unwrap(), cx)), Some(Route::Rules));
}

#[gpui_kit::test]
fn the_launcher_can_be_rearranged_and_the_arrangement_is_kept(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("atlas-launcher-ui-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut launch = sample(Route::Today);
    launch.data_dir = Some(dir.clone());
    let (handle, shell) = open_shell(cx, launch);
    let launcher = cx.update(|cx| shell.read(cx).launcher().clone());
    let window: gpui_kit::AnyWindowHandle = handle.into();
    settle(cx, window);

    // Drag Sharing onto Today: Sharing moves to the front.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let from = window.find("launcher-item-policies").bounds().center();
        let to = window.find("launcher-item-today").bounds().center();
        window.drag(from, to, cx);
    })
    .unwrap();
    settle(cx, window);
    cx.update(|cx| {
        let config = launcher.read(cx).config().clone();
        assert_eq!(config.pinned_destinations()[0], Destination::Sharing, "{:?}", config.pinned);
        assert_eq!(config.pinned_destinations()[1], Destination::Today);
    });

    // Unpin Forecast from its menu: it leaves the strip for the … menu.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.right_click("launcher-item-forecast", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        // Open, Open new instance, Open right, Open below, Open in new window, ─, Unpin.
        window.within("popup-menu").click(6usize, cx);
    })
    .unwrap();
    settle(cx, window);
    cx.update_window(window, |_, window, _| assert!(window.try_find("launcher-forecast").is_none(), "Forecast is off the strip")).unwrap();
    cx.update(|cx| assert_eq!(launcher.read(cx).config().hidden_destinations(), vec![Destination::Forecast]));

    // The arrangement was written, and reads back the same.
    let saved = atlas_app::workspace::launcher::LauncherConfig::load(Some(&dir));
    cx.update(|cx| assert_eq!(&saved, launcher.read(cx).config()));

    // The … menu offers Forecast back; pin it.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("launcher-more", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        // Forecast, Pin Forecast to the launcher, ─, Restore the default launcher.
        window.within("popup-menu").click(1usize, cx);
    })
    .unwrap();
    settle(cx, window);
    cx.update_window(window, |_, window, _| assert!(window.find("launcher-forecast").visible(), "Forecast is back on the strip")).unwrap();
    cx.update(|cx| assert!(launcher.read(cx).config().hidden.is_empty()));

    // Restore the default: Today first again.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("launcher-more", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("popup-menu").click(0usize, cx);
    })
    .unwrap();
    settle(cx, window);
    cx.update(|cx| assert_eq!(launcher.read(cx).config(), &atlas_app::workspace::launcher::LauncherConfig::default()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn the_title_bar_opens_settings_and_resets_the_layout(cx: &mut TestAppContext) {
    let (window, _app, workspace) = today_and_accounts(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("main-sidebar").is_none(), "the sidebar is gone");
        assert!(window.find("launcher").visible());
        window.click("title-settings", cx);
    })
    .unwrap();
    settle(cx, window);
    assert_eq!(pane_count(cx, &workspace), 3);
    cx.update_window(window, |_, window, _| assert!(window.find("screen-settings").visible(), "Settings opened as a pane")).unwrap();
    // Layout ▾ → Reset layout: one pane, showing the active screen.
    cx.update_window(window, |_, window, cx| window.click("layout-menu", cx)).unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        // Undo, Redo, ─, Reset layout.
        window.within("popup-menu").click(3usize, cx);
    })
    .unwrap();
    settle(cx, window);
    assert_eq!(pane_count(cx, &workspace), 1);
    assert_eq!(grid(cx, &workspace, 1, 1), "1");
    let only = active(cx, &workspace);
    assert_eq!(cx.update(|cx| workspace.read(cx).pane_route(&only, cx)), Some(Route::Settings));
    assert_eq!(history_labels(cx, &workspace).last().map(String::as_str), Some("Reset layout"));
    // And undo brings the three panes back.
    drive(cx, window, &workspace, |workspace, window, cx| assert!(workspace.undo(window, cx)));
    assert_eq!(pane_count(cx, &workspace), 3);
}

// ----- background jobs -----------------------------------------------------------------------

#[gpui_kit::test]
fn a_sensitivity_run_outlives_the_pane_that_started_it(cx: &mut TestAppContext) {
    let (handle, app, workspace) = open_workspace(cx, sample(Route::Sensitivity));
    let window: gpui_kit::AnyWindowHandle = handle.into();
    settle(cx, window);
    let jobs = cx.update(|cx| app.read(cx).jobs().clone());
    // A new scope makes the result on show out of date; Run recomputes it.
    cx.update(|cx| app.update(cx, |app, cx| app.select_sensitivity_boundary(atlas_core::liquidity::Boundary::Account(atlas_core::fixtures::ids::PERSON_A_CURRENT), cx)));
    settle(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("jobs").is_none(), "no indicator while there are no jobs");
        window.click("run-sensitivity", cx);
        // The job is under way; the background executor has not run it yet.
        assert_eq!(jobs.read(cx).running_count(), 1, "the run is a job");
        assert!(app.read(cx).sensitivity_running(cx));
        window.render_frame(cx);
        assert!(window.find("jobs").visible(), "the title bar shows the running job");
    })
    .unwrap();
    // Close the pane that started it: the job keeps going.
    let pane = active(cx, &workspace);
    cx.update_window(window, |_, window, cx| workspace.update(cx, |workspace, cx| workspace.close_pane(&pane, window, cx).expect("close"))).unwrap();
    cx.update(|cx| {
        assert_eq!(workspace.read(cx).pane_count(), 0);
        assert_eq!(jobs.read(cx).running_count(), 1, "closing the pane did not stop the job");
    });
    // Let it finish: the result is installed, the person is told.
    settle(cx, window);
    cx.update(|cx| {
        assert_eq!(jobs.read(cx).running_count(), 0);
        let job = jobs.read(cx).jobs().last().cloned().expect("the job is listed");
        assert!(matches!(job.state, atlas_workspace::JobState::Completed { .. }), "{job:?}");
        assert_eq!(job.source, "sensitivity");
        assert!(!app.read(cx).sensitivity_pending(), "the result on show is up to date");
        let model = app.read(cx).assumptions().expect("the model the job computed");
        assert_eq!(model.sensitivity.boundary, atlas_core::liquidity::Boundary::Account(atlas_core::fixtures::ids::PERSON_A_CURRENT));
    });
    cx.update_window(window, |_, window, cx| {
        assert!(!window.notifications(cx).is_empty(), "a toast says the job finished although its pane is gone");
        assert!(window.find("jobs").visible(), "the indicator stays, showing the outcome");
    })
    .unwrap();
    // The jobs list leads back to the job's screen: reopened, showing the new result.
    cx.update_window(window, |_, window, cx| window.click("jobs", cx)).unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("popup-menu").visible(), "the indicator lists the jobs");
        window.within("popup-menu").click(0usize, cx);
    })
    .unwrap();
    settle(cx, window);
    assert_eq!(pane_count(cx, &workspace), 1);
    cx.update_window(window, |_, window, _| {
        assert!(window.find("screen-sensitivity").visible(), "the job's screen opened again");
        assert!(window.find("sensitivity-summary").visible());
    })
    .unwrap();
    cx.update(|cx| assert_eq!(app.read(cx).route(), Route::Sensitivity));
}

#[gpui_kit::test]
fn a_second_run_while_one_is_running_is_ignored_and_an_edit_supersedes_the_result(cx: &mut TestAppContext) {
    let (handle, app, _workspace) = open_workspace(cx, sample(Route::Sensitivity));
    let window: gpui_kit::AnyWindowHandle = handle.into();
    settle(cx, window);
    let jobs = cx.update(|cx| app.read(cx).jobs().clone());
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.select_sensitivity_boundary(atlas_core::liquidity::Boundary::Account(atlas_core::fixtures::ids::PERSON_A_CURRENT), cx);
            app.run_sensitivity(cx);
            app.run_sensitivity(cx);
        });
        assert_eq!(jobs.read(cx).running_count(), 1, "one job, not two");
        // An edit while the job runs: its result is out of date when it lands.
        app.update(cx, |app, _| app.mark_dirty());
    });
    settle(cx, window);
    cx.update(|cx| {
        assert_eq!(jobs.read(cx).running_count(), 0);
        let job = jobs.read(cx).jobs().last().cloned().unwrap();
        assert!(matches!(&job.state, atlas_workspace::JobState::Completed { summary } if summary.contains("superseded")), "{job:?}");
        // The screen computes afresh on its next look, with the chosen scope.
        let model = app.read(cx).assumptions().expect("recomputed on demand");
        assert_eq!(model.sensitivity.boundary, atlas_core::liquidity::Boundary::Account(atlas_core::fixtures::ids::PERSON_A_CURRENT));
    });
}

#[test]
fn every_route_round_trips_through_the_pane_registry() {
    for slug in Route::slugs() {
        let route = Route::from_slug(slug).unwrap();
        let definition = kinds::definition_of(route);
        let expected = (route != Route::Welcome).then_some(route);
        assert_eq!(kinds::route_of(&definition), expected, "{slug}");
    }
    let account = Route::Account(atlas_core::ids::AccountId::new(3));
    assert_eq!(kinds::route_of(&kinds::definition_of(account)), Some(account));
}
