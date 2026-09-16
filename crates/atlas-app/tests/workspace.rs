//! UI integration tests of the pane workspace: the production `Shell` with its
//! `WorkspaceView` in a headless window (gpui-kit `test-support`), driven
//! through the workspace's own commands, the sidebar, and pointer events.
//!
//! The layout model is checked through its character grid
//! (`atlas_workspace::grid::render_numbered`: panes numbered in reading
//! order, one character per cell), the screen through the `screen-<slug>`
//! ids every screen root carries, and the panes through their `pane-<n>` ids.

mod common;

use atlas_app::nav::Route;
use atlas_app::workspace::{WorkspaceView, kinds};
use atlas_app::{AtlasApp, Launch};
use atlas_workspace::grid::render_numbered;
use atlas_workspace::resolver::Intent;
use atlas_workspace::{PaneId, Side};
use common::*;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, Entity, TestAppContext, point, px};

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
