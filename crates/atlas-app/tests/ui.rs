//! UI integration tests: the production `AtlasApp` view in a headless window,
//! driven through real pointer and keyboard events (gpui-kit `test-support`).
//!
//! Sidebar items are registered by gpui-kit as `<group>-0-<item>` inside the
//! `main-sidebar` scope: People is `0-0-1`, Settings is `4-0-1`.

use atlas_app::screens::Section;
use atlas_app::{AtlasApp, Launch};
use atlas_core::fixtures;
use atlas_core::ids::ObjectRef;
use atlas_core::Disclosure;
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, Entity, ScrollDelta, TestAppContext, point, px, size};

/// gpui-kit dialogs fade in over 250 ms of wall-clock time (not test time) and
/// do not take pointer input until the animation has finished, so a test must
/// let real time pass before clicking a dialog button.
fn let_dialog_settle() {
    std::thread::sleep(std::time::Duration::from_millis(400));
}

/// Scrolls the main column so content below the fold becomes visible. The
/// wheel event is dispatched at the anchor's centre, so the anchor must be an
/// element that is currently on screen (a filter control, a figure …).
fn scroll_down(window: &mut gpui_kit::Window, anchor: &'static str, cx: &mut gpui_kit::App) {
    window.scroll(anchor, ScrollDelta::Pixels(point(px(0.), px(-12000.))), cx);
    window.render_frame(cx);
}

fn open_app(cx: &mut TestAppContext, launch: Launch) -> (gpui_kit::WindowHandle<Root>, Entity<AtlasApp>) {
    cx.update(gpui_kit::init);
    let mut app_view = None;
    let handle = cx.open_window(size(px(1600.), px(1000.)), |window, cx| {
        let view = cx.new(|cx| AtlasApp::new(&launch, window, cx));
        app_view = Some(view.clone());
        Root::new(view, window, cx)
    });
    (handle, app_view.expect("view created"))
}

#[gpui_kit::test]
fn household_overview_opens_the_explain_sheet(cx: &mut TestAppContext) {
    let (handle, app) = open_app(cx, Launch::default());

    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-household").visible());
        assert!(window.find("figure-free-cash").visible());
        assert!(window.find("figure-conditional-cash").visible());
        assert!(window.try_find("explain-chain").is_none(), "no sheet before the click");

        window.click("why-free-cash", cx);
        let chain = window.find("explain-chain");
        assert!(chain.visible(), "the Why? button opens the chain sheet");
    })
    .unwrap();

    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.section(), Section::Household);
        let overview = app.overview().expect("overview computed");
        let free = overview.money.iter().find(|f| f.id.as_ref() == "free-cash").unwrap();
        assert_eq!(free.calc.money(), fixtures::pkr(2_650_000));
        assert_eq!(free.disclosure(), Disclosure::Full, "the owner sees everything");
        assert!(free.calc.node().verify_sums().is_empty());
    });
}

#[gpui_kit::test]
fn sidebar_navigation_switches_screens(cx: &mut TestAppContext) {
    let (handle, app) = open_app(cx, Launch::default());

    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.within("main-sidebar").click("0-0-1", cx);
        assert!(window.find("screen-people").visible(), "People placeholder is shown");
        assert!(window.try_find("screen-household").is_none());

        window.within("main-sidebar").click("4-0-1", cx);
        assert!(window.find("screen-settings").visible());
    })
    .unwrap();

    cx.update(|cx| assert_eq!(app.read(cx).section(), Section::Settings));
}

#[gpui_kit::test]
fn person_b_gets_aggregates_not_person_a_details(cx: &mut TestAppContext) {
    let launch = Launch { viewer: 'b', ..Launch::default() };
    let (handle, app) = open_app(cx, launch);

    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("figure-free-cash").visible());
        window.click("why-free-cash", cx);
        assert!(window.find("explain-chain").visible());
    })
    .unwrap();

    cx.update(|cx| {
        let app = app.read(cx);
        let household = app.household();
        assert_eq!(app.viewer().person, fixtures::ids::PERSON_B);
        assert_eq!(
            household.disclosure_for(app.viewer(), ObjectRef::Account(fixtures::ids::PERSON_A_CURRENT)),
            Disclosure::Aggregate,
            "§7.5: the private account contributes as an aggregate only"
        );
        let overview = app.overview().unwrap();
        let free = overview.money.iter().find(|f| f.id.as_ref() == "free-cash").unwrap();
        // Same authoritative number as for Person A (M55 invariant 4)…
        assert_eq!(free.calc.money(), fixtures::pkr(2_650_000));
        // …but the explanation is a projection.
        assert_eq!(free.disclosure(), Disclosure::Aggregate);
        let text = free.calc.node().render_chain();
        assert!(!text.contains("Person A current account"), "{text}");
        assert!(text.contains("Owner-authorized restricted contribution"), "{text}");
        assert!(free.calc.node().verify_sums().is_empty());
        // The private Leave Job assumption stays with Person A (§18.5).
        assert!(overview.assumptions.iter().all(|a| a.private_to.is_none()));
    });
}

#[gpui_kit::test]
fn accounts_master_detail_shows_the_selected_account(cx: &mut TestAppContext) {
    let launch = Launch { section: Section::Accounts, ..Launch::default() };
    let (handle, app) = open_app(cx, launch);

    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-accounts").visible());
        // The first visible account is selected by default.
        assert!(window.find("account-detail-1").visible());
        assert!(window.find("figure-account-1-free").visible());

        window.click("account-2", cx);
        assert!(window.find("account-detail-2").visible(), "clicking a master row selects it");
        assert!(window.try_find("account-detail-1").is_none());
        assert!(window.find("figure-account-2-free").visible());

        window.click("why-account-2-free", cx);
        assert!(window.find("explain-chain").visible());
    })
    .unwrap();

    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.selected_account(), Some(fixtures::ids::PERSON_A_CURRENT));
        let model = app.entities().unwrap().account(fixtures::ids::PERSON_A_CURRENT).unwrap();
        // 1,500,000 settled − 400,000 buffer (which already covers the 300,000 bank minimum).
        assert_eq!(model.free.calc.money(), fixtures::pkr(1_100_000));
        assert!(model.free.calc.node().render_chain().contains("(excluded) Bank minimum balance"));
    });
}

#[gpui_kit::test]
fn person_b_sees_only_the_planning_safe_company_output(cx: &mut TestAppContext) {
    let launch = Launch { section: Section::Companies, viewer: 'b', ..Launch::default() };
    let (handle, app) = open_app(cx, launch);

    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-companies").visible());
        assert!(window.find("company-detail-1").visible());
        // Only the ceiling figure is rendered for a summary viewer (§8.7).
        assert!(window.find("figure-company-1-ceiling").visible());
        assert!(window.try_find("figure-company-1-cash").is_none(), "business cash is not disclosed");
    })
    .unwrap();

    cx.update(|cx| {
        let app = app.read(cx);
        let models = app.entities().unwrap();
        let alpha = models.company(fixtures::ids::ALPHA).unwrap();
        assert_eq!(alpha.disclosure, Disclosure::Aggregate);
        // E07: 2,350,000 cash − 1,500,000 committed, and the chain hides the accounts.
        assert_eq!(alpha.ceiling.calc.money(), fixtures::pkr(850_000));
        let text = alpha.ceiling.calc.node().render_chain();
        assert!(!text.contains("Company Alpha operating"), "{text}");
        assert!(!text.contains("Committed payroll"), "{text}");
        // Person B's own accounts screen never lists Person A's private account (V062).
        assert!(models.account(fixtures::ids::PERSON_A_CURRENT).is_none());
        assert!(models.account(fixtures::ids::PERSON_A_VISA).is_some(), "shared-balance accounts are listed");
    });
}

#[gpui_kit::test]
fn liquidity_adds_a_reservation_through_the_dialog(cx: &mut TestAppContext) {
    let launch = Launch { section: Section::Liquidity, ..Launch::default() };
    let (handle, app) = open_app(cx, launch);
    let window = handle.into();

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-liquidity").visible());
        assert!(window.find("figure-household-figure-2").visible(), "free current cash figure");
        assert!(window.find("runway-summary").visible());
        scroll_down(window, "screen-liquidity", cx);
        window.click("new-reservation", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("save-reservation").visible(), "the dialog opened");
        // An empty form is rejected and the dialog stays open.
        window.click("save-reservation", cx);
    })
    .unwrap();
    cx.run_until_parked();

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("save-reservation").visible(), "validation kept the dialog open");
        window.click("reservation-name", cx);
        window.input("Car reserve", cx);
        window.click("reservation-amount", cx);
        window.input("250,000", cx);
        window.click("save-reservation", cx);
    })
    .unwrap();
    cx.run_until_parked();

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx), "the dialog closed after saving");
    })
    .unwrap();

    cx.update(|cx| {
        let app = app.read(cx);
        let household = app.household();
        assert_eq!(household.reservations.len(), 7);
        let added = household.reservations.last().unwrap();
        assert_eq!(added.name, "Car reserve");
        assert_eq!(added.amount, fixtures::pkr(250_000));
        assert_eq!(added.account, fixtures::ids::SHARED_SAVINGS, "the first editable account is preselected");
        // Ledger cash unchanged, free cash reduced (§17).
        let model = app.liquidity().unwrap();
        assert_eq!(model.figures[0].calc.money(), fixtures::pkr(4_400_000));
        assert_eq!(model.figures[2].calc.money(), fixtures::pkr(2_650_000 - 250_000));
    });
}

#[gpui_kit::test]
fn paying_and_releasing_an_earmark_keeps_free_cash(cx: &mut TestAppContext) {
    let launch = Launch { section: Section::Liquidity, ..Launch::default() };
    let (handle, app) = open_app(cx, launch);
    let window = handle.into();

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        scroll_down(window, "screen-liquidity", cx);
        window.click("release-2", cx); // the 300,000 tax reserve
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("confirm-release").visible());
        window.click("confirm-release", cx);
    })
    .unwrap();
    cx.run_until_parked();

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx));
        scroll_down(window, "screen-liquidity", cx);
        assert!(window.try_find("release-2").is_none(), "a released earmark has no action");
    })
    .unwrap();

    cx.update(|cx| {
        let app = app.read(cx);
        let household = app.household();
        assert!(household.reservation(fixtures::ids::TAX_RESERVE).unwrap().released_on.is_some());
        assert_eq!(household.account(fixtures::ids::SHARED_SAVINGS).unwrap().settled_balance, fixtures::pkr(1_700_000));
        // E01 through the UI: 1,700,000 − (800,000 + 250,000) = 650,000, unchanged.
        let shared = atlas_core::liquidity::account_liquidity(household, fixtures::ids::SHARED_SAVINGS).unwrap();
        assert_eq!(shared.free.money(), fixtures::pkr(650_000));
        let model = app.liquidity().unwrap();
        assert_eq!(model.figures[0].calc.money(), fixtures::pkr(4_100_000));
        assert_eq!(model.figures[2].calc.money(), fixtures::pkr(2_650_000));
    });
}

#[test]
fn timeline_filters_narrow_the_occurrences() {
    use atlas_app::screens::timeline::{TimelineFilter, TimelineModel};
    use atlas_core::authz::Viewer;
    use atlas_core::ids::EntityRef;
    use atlas_core::timeline::OccurrenceStatus;
    let household = fixtures::plan_household();
    let viewer = Viewer::person(fixtures::ids::PERSON_A);
    let all = TimelineFilter { entity: None, account: None, certainty: None, status: None, scenario: None, through: fixtures::default_horizon() };
    let baseline = TimelineModel::compute(&household, viewer, all.clone()).unwrap();
    assert!(baseline.occurrences.windows(2).all(|w| w[0].sort_key() <= w[1].sort_key()));

    let only_b = TimelineModel::compute(&household, viewer, TimelineFilter { entity: Some(EntityRef::Person(fixtures::ids::PERSON_B)), ..all.clone() }).unwrap();
    assert!(!only_b.occurrences.is_empty());
    assert!(only_b.occurrences.iter().all(|o| o.entity == EntityRef::Person(fixtures::ids::PERSON_B)));

    let partial = TimelineModel::compute(&household, viewer, TimelineFilter { status: Some(OccurrenceStatus::PartiallyFulfilled), ..all.clone() }).unwrap();
    assert_eq!(partial.occurrences.len(), 1);
    assert_eq!(partial.occurrences[0].remaining_expected(), fixtures::pkr(200_000));

    let with_car = TimelineModel::compute(&household, viewer, TimelineFilter { scenario: Some(fixtures::ids::BUY_CAR), ..all.clone() }).unwrap();
    assert_eq!(with_car.occurrences.len(), baseline.occurrences.len() + 1);

    // Person B never sees Person A's private account series (V062).
    let b_view = TimelineModel::compute(&household, Viewer::person(fixtures::ids::PERSON_B), all).unwrap();
    assert!(b_view.occurrences.iter().all(|o| o.account != fixtures::ids::PERSON_A_CURRENT));
    assert!(b_view.hidden_series > 0);
}

#[gpui_kit::test]
fn timeline_scenario_toggle_and_series_editor(cx: &mut TestAppContext) {
    let launch = Launch { section: Section::Timeline, ..Launch::default() };
    let (handle, app) = open_app(cx, launch);
    let window = handle.into();

    let baseline_count = cx.update(|cx| app.read(cx).timeline().unwrap().occurrences.len());

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-timeline").visible());
        window.click("timeline-buy-car", cx);
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.timeline_filter().scenario, Some(fixtures::ids::BUY_CAR));
        assert_eq!(app.timeline().unwrap().occurrences.len(), baseline_count + 1, "the car down payment joins the timeline");
    });

    // Edit the rent series: raise the expected amount for the whole series.
    cx.update_window(window, |_, window, cx| {
        scroll_down(window, "timeline-buy-car", cx);
        window.click("edit-series-5", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("save-series").visible());
        window.click("series-amount", cx);
        window.press("ctrl-a", cx);
        window.input("190,000", cx);
        assert_eq!(window.find("series-amount").value(), Some("190,000"));
        window.click("save-series", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx));
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        let rent = app.household().series_by_id(fixtures::ids::RENT).unwrap();
        assert_eq!(rent.amount.expected(), fixtures::pkr(190_000));
        // Ranges widen to include the new expected value; the household chain follows.
        assert_eq!(rent.amount.low(), fixtures::pkr(180_000));
        assert_eq!(rent.amount.high(), fixtures::pkr(200_000));
        let overview = app.overview().unwrap();
        assert_eq!(overview.conditional.calc.money(), fixtures::pkr(6_195_000 - 4 * 10_000));
    });
}
