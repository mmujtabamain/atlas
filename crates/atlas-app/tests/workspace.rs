//! UI integration tests of the pane workspace: the production `Shell` with its
//! `WorkspaceView` in a headless window (gpui-kit `test-support`), driven
//! through the workspace's own commands, the sidebar, and pointer events.
//!
//! The layout model is checked through its character grid
//! (`atlas_workspace::grid::render_numbered`: panes numbered in reading
//! order, one character per cell), the screen through the `screen-<slug>`
//! ids every screen root carries, and the panes through their `pane-<n>` ids.
//!
//! Drags are real pointer sequences on the pane titles (`pane-title-<n>`),
//! the drag handle of a single-pane stack, dropped on the engine's zones of
//! another pane: its centre for a tab, an edge for a split.

mod common;

use atlas_app::nav::Route;
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
        assert_eq!(app.read(cx).route(), Route::Accounts, "the sidebar follows the active pane");
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
fn a_press_in_a_pane_makes_it_active_and_the_sidebar_navigates_that_pane(cx: &mut TestAppContext) {
    let (window, app, workspace) = today_and_accounts(cx);
    let order = cx.update(|cx| workspace.read(cx).panes_in_order());
    let today = order[0].clone();
    assert_ne!(active(cx, &workspace), today, "the new Accounts pane is active after the split");

    // Press in the padding of the Today pane, away from any control in it.
    cx.update_window(window, |_, window, cx| window.click_at("pane-1", point(px(8.), px(8.)), cx)).unwrap();
    cx.run_until_parked();
    assert_eq!(active(cx, &workspace), today, "the press made Today's pane the active one");
    cx.update(|cx| assert_eq!(app.read(cx).route(), Route::Today, "the chrome follows the active pane"));

    // Rules & taxes is the first item of the third sidebar group.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("main-sidebar").click("2-0-0", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-rules").visible(), "the active pane navigated");
        assert!(window.find("screen-accounts").visible(), "the other pane kept its screen");
        assert!(window.try_find("screen-today").is_none(), "Today left with the navigation");
    })
    .unwrap();
    cx.update(|cx| {
        let workspace = workspace.read(cx);
        assert_eq!(workspace.pane_route(&today, cx), Some(Route::Rules));
        assert_eq!(workspace.pane_route(&order[1], cx), Some(Route::Accounts));
        assert_eq!(app.read(cx).route(), Route::Rules);
    });
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
        assert!(window.within("main-sidebar").find("0-0-0").visible(), "the sidebar stays");
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
