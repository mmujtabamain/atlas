//! UI integration tests: the production `AtlasApp` view in a headless window,
//! driven through real pointer and keyboard events (gpui-kit `test-support`).
//!
//! Sidebar items are registered by gpui-kit as `<group>-0-<item>` inside the
//! `main-sidebar` scope: People is `0-0-1`, Settings is `4-0-1`.

use atlas_app::screens::Section;
use atlas_app::{AtlasApp, Launch, Shell};
use atlas_core::fixtures;
use atlas_core::ids::ObjectRef;
use atlas_core::Disclosure;
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, Entity, Modifiers, MouseMoveEvent, PlatformInput, ScrollDelta, ScrollWheelEvent, TestAppContext, TouchPhase, point, px, size};

/// gpui-kit dialogs fade in over 250 ms of wall-clock time (not test time) and
/// do not take pointer input until the animation has finished, so a test must
/// let real time pass before clicking a dialog button.
fn let_dialog_settle() {
    std::thread::sleep(std::time::Duration::from_millis(400));
}

/// Dismisses the "saved" toasts and waits for them to leave. They sit in the
/// top-right corner — over the "New …" buttons of every screen — and keep
/// their hitbox while they animate out, so a click there in the same update
/// lands on the toast instead of the button.
fn dismiss_toasts(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.clear_notifications(cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
}

/// Scrolls the main column so content below the fold becomes visible. The
/// wheel event is dispatched at the anchor's centre, so the anchor must be an
/// element that is currently on screen (a filter control, a figure …).
fn scroll_down(window: &mut gpui_kit::Window, anchor: &'static str, cx: &mut gpui_kit::App) {
    scroll_by(window, anchor, 12000., cx);
}

/// Scrolls the main column down by `pixels` from an on-screen anchor.
fn scroll_by(window: &mut gpui_kit::Window, anchor: &'static str, pixels: f32, cx: &mut gpui_kit::App) {
    window.scroll(anchor, ScrollDelta::Pixels(point(px(0.), px(-pixels))), cx);
    window.render_frame(cx);
}

fn open_app(cx: &mut TestAppContext, launch: Launch) -> (gpui_kit::WindowHandle<Root>, Entity<AtlasApp>) {
    let (handle, shell) = open_shell(cx, launch);
    let app = cx.update(|cx| shell.read(cx).app().clone());
    (handle, app)
}

/// Opens the window and returns the root view (the shell around the content).
fn open_shell(cx: &mut TestAppContext, launch: Launch) -> (gpui_kit::WindowHandle<Root>, Entity<Shell>) {
    cx.update(gpui_kit::init);
    let mut shell_view = None;
    let handle = cx.open_window(size(px(1600.), px(1000.)), |window, cx| {
        let shell = cx.new(|cx| Shell::new(&launch, window, cx));
        shell_view = Some(shell.clone());
        Root::new(shell, window, cx)
    });
    (handle, shell_view.expect("view created"))
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
        // §7.6 / V073: A's account is the only restricted term next to disclosed ones, so the
        // breakdown is suppressed rather than exposed as a difference; its balance never appears.
        assert!(text.contains("suppressed"), "{text}");
        assert!(!text.contains("1,500,000"), "{text}");
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
    let conditional_before = cx.update(|cx| app.read(cx).overview().unwrap().conditional.calc.money());

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
    // The occurrences are a virtualised grid whose rows follow the model: the
    // next frame syncs the new rows in, formatted once at compute time.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("timeline-occurrences-grid").visible());
        let app = app.read(cx);
        let grid = app.grids().timeline_occurrences.read(cx);
        let rows = grid.delegate().rows();
        assert_eq!(rows.len(), baseline_count + 1, "the grid shows every occurrence");
        let (_, dump) = grid.dump_range(0..rows.len(), cx);
        assert!(dump.iter().any(|row| row.iter().any(|cell| cell.contains("scenario “Buy car”"))), "the scenario row is in the grid: {dump:?}");
        let model = app.timeline().unwrap();
        assert!(std::sync::Arc::ptr_eq(rows, &model.rows), "the grid holds the model's rows, not a copy");
    })
    .unwrap();

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
        // Four rent payments in the window, each 10,000 higher.
        assert_eq!(overview.conditional.calc.money(), conditional_before - fixtures::pkr(4 * 10_000));
    });
}

#[gpui_kit::test]
fn projections_switch_case_and_overlay_scenario(cx: &mut TestAppContext) {
    use atlas_core::forecast::Case;
    let launch = Launch { section: Section::Projections, ..Launch::default() };
    let (handle, app) = open_app(cx, launch);
    let window = handle.into();

    let (expected_end, expected_lowest) = cx.update(|cx| {
        let model = app.read(cx).projection().unwrap();
        assert_eq!(model.forecast.case, Case::Expected);
        (model.forecast.end.money(), model.forecast.lowest.money())
    });

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-projections").visible());
        assert!(window.find("figure-household-proj-end").visible());
        assert!(window.try_find("projection-runway-summary").is_some(), "the runway panel exists below the fold");
        // Conservative is the first case tab.
        window.within("projection-cases").click(0usize, cx);
    })
    .unwrap();
    cx.update(|cx| {
        let model = app.read(cx).projection().unwrap();
        assert_eq!(model.forecast.case, Case::Conservative);
        assert!(model.forecast.end.money().minor() < expected_end.minor(), "low income / high expenses end lower");
        assert!(model.forecast.lowest.money().minor() <= expected_lowest.minor());
        assert_eq!(model.forecast.end.node().result_strength(), atlas_core::ResultStrength::ScenarioTested);
    });

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("projection-buy-car", cx);
    })
    .unwrap();
    cx.update(|cx| {
        let model = app.read(cx).projection().unwrap();
        assert_eq!(model.forecast.scenario, Some(fixtures::ids::BUY_CAR));
        // §11.3: the household aggregate survives the 2,500,000 down payment, but the
        // account it is paid from does not — the per-account path and the transfer
        // points make that visible.
        assert!(model.forecast.breach.first_breach.is_none(), "{}", model.forecast.breach.summary());
        let shared = model.forecast.accounts.iter().find(|a| a.account == fixtures::ids::SHARED_SAVINGS).unwrap();
        assert!(shared.negative_from.is_some(), "shared savings goes negative on the car date");
        assert!(model.forecast.transfer_points.iter().any(|t| t.account == fixtures::ids::SHARED_SAVINGS && t.coverable));
        assert!(model.forecast.record.input_hash != 0);
    });
}

#[gpui_kit::test]
fn assumptions_accept_and_derive_through_the_ui(cx: &mut TestAppContext) {
    use atlas_core::assumptions::Derivation;
    use atlas_core::ids::AssumptionId;
    use atlas_core::model::Freshness;
    let launch = Launch { section: Section::Assumptions, ..Launch::default() };
    let (handle, app) = open_app(cx, launch);
    let window = handle.into();

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-assumptions").visible());
        // Assumption #3 (rent) was accepted in May: stale, with an Accept action.
        window.click("accept-assumption-3", cx);
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        let rent = app.household().assumption(AssumptionId::new(3)).unwrap();
        assert_eq!(rent.accepted_on, Some(app.household().as_of));
        assert_eq!(rent.freshness(app.household().as_of), Freshness::Fresh);
    });

    // Switch the derivation formula to the mean and apply it to assumption #1.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("accept-assumption-3").is_none(), "a fresh assumption has no Accept action");
        scroll_by(window, "accept-assumption-5", 800., cx);
        window.within("derivation-formula").click(2usize, cx);
    })
    .unwrap();
    cx.update(|cx| {
        let model = app.read(cx).assumptions().unwrap();
        assert_eq!(model.derivation, Derivation::Mean { last_n: 6 });
        assert_eq!(model.derived.as_ref().unwrap().amount.expected(), fixtures::pkr(500_000));
        assert!(model.sensitivity.caveat.contains("do NOT guarantee"), "V043");
        assert!(model.statement.render().contains("Coverage:"), "V031");
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("apply-derivation", cx);
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        let salary = app.household().assumption(AssumptionId::new(1)).unwrap();
        assert_eq!(salary.accepted_on, None, "a re-derived assumption must be accepted again (§2.5)");
        assert!(salary.source.describe().contains("arithmetic mean"));
        assert_eq!(app.household().series_by_id(fixtures::ids::SALARY_A).unwrap().amount.expected(), fixtures::pkr(500_000));
    });
}

#[gpui_kit::test]
fn taxes_e05_and_user_rule_dialog(cx: &mut TestAppContext) {
    let launch = Launch { section: Section::Taxes, ..Launch::default() };
    let (handle, app) = open_app(cx, launch);
    let window = handle.into();

    cx.update(|cx| {
        let model = app.read(cx).taxes().unwrap();
        // E05 through the screen model: 22,000 vs 14,000 incremental.
        assert_eq!(model.e05_strategies[0].incremental.money(), fixtures::pkr(22_000));
        assert_eq!(model.e05_strategies[1].incremental.money(), fixtures::pkr(14_000));
        assert!(model.assessment.packs_used.iter().any(|p| p.contains("unverified")));
    });

    let packs_before = cx.update(|cx| app.read(cx).household().tax_packs.len());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-taxes").visible());
        window.click("new-tax-rule", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("save-tax-rule").visible());
        // Missing name and rate: rejected, dialog stays open.
        window.click("save-tax-rule", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "validation kept the dialog open");
        window.click("tax-rule-name", cx);
        window.input("Municipal levy", cx);
        window.click("tax-rule-rate", cx);
        window.input("2.5", cx);
    })
    .unwrap();
    // The effective-from date comes from a DatePicker; set it through the household's
    // own API path would bypass the form, so give the form a date via the picker state.
    cx.update(|cx| {
        app.update(cx, |app, cx| app.set_tax_form_effective_from(fixtures::as_of(), cx));
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("save-tax-rule", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx), "the rule was accepted");
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        let household = app.household();
        assert_eq!(household.tax_packs.len(), packs_before + 1, "a user pack was created");
        let pack = household.tax_packs.last().unwrap();
        assert!(!pack.verified);
        assert_eq!(pack.rules[0].name, "Municipal levy");
        // 2.5% flat on the "Card spending" category (first category alphabetically).
        assert!(matches!(pack.rules[0].kind, atlas_core::model::TaxKind::FlatRate { rate_basis_points: 250 }));
        let model = app.taxes().unwrap();
        assert!(model.assessment.events.iter().any(|e| e.rule_name == "Municipal levy"), "the new rule produces events");
    });
}

#[gpui_kit::test]
fn real_data_new_household_entry_save_and_reopen(cx: &mut TestAppContext) {
    use atlas_app::launch::Start;
    let dir = std::env::temp_dir().join(format!("atlas-ui-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("ours.atlas.sqlite");

    let launch = Launch { start: Start::Empty, section: Section::People, as_of: Some(fixtures::as_of()), owner: "tester".into(), ..Launch::default() };
    let (handle, app) = open_app(cx, launch);
    let window = handle.into();

    // 1. A person.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-people").visible());
        window.click("new-person", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("entry-person-name", cx);
        window.input("Ada", cx);
        window.click("entry-save-person", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.household().people.len(), 1);
        assert_eq!(app.household().people[0].name, "Ada");
        assert_eq!(app.viewer().person, app.household().people[0].id, "the first person becomes the viewer");
        assert!(app.is_dirty());
    });

    // 2. An account with an opening balance. The "saved" toast from step 1 sits
    // in the top-right corner, over the "New account…" button, and keeps its
    // hitbox while it animates out — so dismiss it and let it leave first.
    dismiss_toasts(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("main-sidebar").click("0-0-3", cx);
        assert!(window.find("screen-accounts").visible());
        window.click("new-account", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("entry-account-name", cx);
        window.input("Joint current", cx);
        window.click("entry-account-balance", cx);
        window.input("12,500", cx);
        window.click("entry-save-account", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update(|cx| {
        let app = app.read(cx);
        let household = app.household();
        assert_eq!(household.accounts.len(), 1);
        assert_eq!(household.accounts[0].settled_balance, atlas_core::Money::from_major(12_500, atlas_core::Currency::USD));
        let account = household.accounts[0].id;
        assert_eq!(household.disclosure_for(app.viewer(), atlas_core::ids::ObjectRef::Account(account)), Disclosure::Full, "F162: the creator sees what they created");
        assert_eq!(app.overview().unwrap().money[0].calc.money(), atlas_core::Money::from_major(12_500, atlas_core::Currency::USD));
    });

    // 3. A monthly salary series.
    dismiss_toasts(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("main-sidebar").click("1-0-1", cx);
        assert!(window.find("screen-timeline").visible());
        window.click("new-series", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("entry-series-name", cx);
        window.input("Salary", cx);
        window.click("entry-series-amount", cx);
        window.input("4,000", cx);
        window.click("entry-save-series", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    let end_before_save = cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.household().series.len(), 1);
        assert_eq!(app.household().series[0].name, "Salary");
        let end = app.overview().unwrap().conditional.calc.money();
        assert!(end.minor() > atlas_core::Money::from_major(12_500, atlas_core::Currency::USD).minor(), "the expense-free salary raises the projection: {}", end.format());
        end
    });

    // 4. Save as… to a file.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.clear_notifications(cx);
        app.update(cx, |app, cx| app.open_save_as(window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    let path_text = path.display().to_string();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("save-as-path", cx);
        window.press("ctrl-a", cx);
        window.input(&path_text, cx);
        window.click("confirm-save-as", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(!app.is_dirty());
        assert_eq!(app.file_path(), Some(path.clone()));
    });
    assert!(path.exists(), "the SQLite file was written");

    // 5. Reopen from the file in a fresh app: same household, same figures.
    let launch = Launch { start: Start::File(path.clone()), owner: "tester".into(), take_over: true, ..Launch::default() };
    let (handle2, app2) = open_app(cx, launch);
    cx.update_window(handle2.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-household").visible());
    })
    .unwrap();
    cx.update(|cx| {
        let reopened = app2.read(cx);
        assert_eq!(reopened.household().people[0].name, "Ada");
        assert_eq!(reopened.household().accounts[0].name, "Joint current");
        assert_eq!(reopened.household().series[0].name, "Salary");
        assert_eq!(reopened.overview().unwrap().conditional.calc.money(), end_before_save);
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn rules_inspector_simulation_and_editor(cx: &mut TestAppContext) {
    let launch = Launch { section: Section::Rules, ..Launch::default() };
    let (handle, app) = open_app(cx, launch);
    let window = handle.into();

    // The fixture rules: two visible conflicts (the promo waiver beats the category fee
    // on scope specificity in Oct and Nov), seven fee postings.
    cx.update(|cx| {
        let model = app.read(cx).rules().unwrap();
        assert_eq!(model.rules.len(), 7);
        assert_eq!(model.conflicts.len(), 2);
        assert!(model.conflicts.iter().all(|d| d.resolution == "scope specificity"));
        assert_eq!(model.evaluation.fees.len(), 7);
        assert!(model.simulation.is_none());
        assert!(model.funding.is_empty(), "funding rules are scenario-scoped");
    });

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-rules").visible());
        assert!(window.find("rule-row-1").visible());
        // Simulate the transfer fee, then evaluate inside the scenario.
        window.click("rule-simulate-3", cx);
        window.click("rules-buy-car", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let model = app.read(cx).rules().unwrap();
        let sim = model.simulation.as_ref().expect("simulation ran");
        assert_eq!(sim.rule, fixtures::ids::RULE_TRANSFER_FEE);
        assert!(sim.end_delta.is_negative(), "the fee rule costs money");
        assert_eq!(sim.end_delta, sim.fee_delta);
        assert_eq!(model.funding.len(), 3, "the car-purchase funding order is visible inside the scenario");
        assert!(model.funding[0].forbidden);
    });

    // Disabling a rule records a version and removes its fees; the household is dirty.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("rule-toggle-1", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let app = app.read(cx);
        let rule = app.household().rule(fixtures::ids::RULE_FOREIGN_FEE).unwrap();
        assert!(!rule.enabled);
        assert_eq!(rule.version, 2);
        assert!(app.is_dirty());
        let model = app.rules().unwrap();
        assert_eq!(model.conflicts.len(), 0, "without the category fee there is nothing to conflict with");
        assert_eq!(model.evaluation.fees.len(), 5);
    });
    // The fee grid follows the model once a frame has synced it (7 → 5 rows).
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let grid = app.read(cx).grids().rule_fees.read(cx);
        assert_eq!(grid.delegate().rows().len(), 5, "the fee grid dropped the disabled rule's postings");
    })
    .unwrap();

    // The editor: a 2% fee on the "Living" category, validated by the engine.
    dismiss_toasts(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("new-rule", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("rule-save").visible());
        window.click("rule-save", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx), "a nameless rule is refused");
        window.click("rule-name", cx);
        window.input("Living levy", cx);
        window.click("rule-percent", cx);
        window.input("2", cx);
        window.click("rule-save", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx), "the rule was accepted");
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        let household = app.household();
        assert_eq!(household.rules.len(), 8);
        let rule = household.rules.last().unwrap();
        assert_eq!(rule.name, "Living levy");
        assert!(matches!(&rule.action, atlas_core::rules::RuleAction::AddFee { basis_points: 200, .. }));
        assert_eq!(rule.history.len(), 1);
        let model = app.rules().unwrap();
        assert!(model.evaluation.fees.iter().any(|f| f.rule == rule.id), "the new rule fires in the window");
        // The new fee lands on the Visa card (the first category is "Card spending"), which is
        // outside household cash; the record still names every rule that posted inside the boundary.
        let projection = app.projection().unwrap();
        assert!(projection.forecast.record.rules_applied.iter().any(|r| r.contains("Bank B transfer fee")), "the forecast record names the rules that posted");
        assert!(projection.forecast.end.node().render_chain().contains("Fees from user rules"), "the §2.1 chain carries the fee term");
    });
}

#[gpui_kit::test]
fn scenarios_compare_compose_and_stay_private(cx: &mut TestAppContext) {
    // Person B: the private "Leave job" scenario and its composition never appear (§18.5).
    let launch = Launch { section: Section::Scenarios, viewer: 'b', ..Launch::default() };
    let (handle, app) = open_app(cx, launch);
    cx.update(|cx| {
        let model = app.read(cx).scenarios().unwrap();
        assert_eq!(model.cards.len(), 1, "only “Buy car” is visible to Person B");
        assert_eq!(model.hidden_count, 2);
        assert_eq!(model.selection, vec![fixtures::ids::BUY_CAR]);
        let comparison = model.comparison.as_ref().expect("Buy car compares");
        assert!(comparison.attribution_verified);
        assert_eq!(comparison.end_delta, fixtures::pkr(-2_500_000));
    });
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-scenarios").visible());
        assert!(window.find("scenario-card-1").visible());
        assert!(window.try_find("scenario-card-2").is_none(), "no private card for B");
        assert!(window.try_find("scenario-card-3").is_none());
    })
    .unwrap();

    // Person A: select both base scenarios, see them compatible, compose, then add a change.
    let launch = Launch { section: Section::Scenarios, ..Launch::default() };
    let (handle, app) = open_app(cx, launch);
    let window = handle.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("scenario-card-2").visible());
        window.click("scenario-select-2", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let model = app.read(cx).scenarios().unwrap();
        assert_eq!(model.selection, vec![fixtures::ids::BUY_CAR, fixtures::ids::LEAVE_JOB]);
        assert!(model.incompatibilities.is_empty());
        let comparison = model.comparison.as_ref().unwrap();
        assert!(comparison.attribution_verified);
        assert_eq!(comparison.end_delta, fixtures::pkr(-3_430_000), "car −2,500,000, two salaries −1,000,000, withholding +70,000");
        assert!(comparison.attribution.iter().any(|a| a.kind == "taxes" && a.delta == fixtures::pkr(70_000)));
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("scenario-compose", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("compose-name", cx);
        window.input("Car after leaving", cx);
        window.click("compose-save", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let scenarios_before = 3;
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx), "composition accepted");
        window.clear_notifications(cx);
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        let household = app.household();
        assert_eq!(household.scenarios.len(), scenarios_before + 1);
        let composed = household.scenarios.last().unwrap();
        assert_eq!(composed.name, "Car after leaving");
        assert_eq!(composed.composed_of, vec![fixtures::ids::BUY_CAR, fixtures::ids::LEAVE_JOB]);
        assert!(composed.private_to.is_some(), "a composition with a private member is private");
        assert!(household.policy_for(ObjectRef::Scenario(composed.id)).is_some());
        assert!(app.is_dirty());
        let model = app.scenarios().unwrap();
        assert_eq!(model.selection, vec![composed.id]);
        assert_eq!(model.comparison.as_ref().unwrap().end_delta, fixtures::pkr(-3_430_000), "the composition equals the pair");
    });

    // Add an explicit change through the dialog: end the first baseline series after today.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("scenario-add-change", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("scenario-change-reason", cx);
        window.input("test change", cx);
        window.click("scenario-change-save", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx), "the change was accepted");
    })
    .unwrap();
    cx.update(|cx| {
        let household = app.read(cx).household();
        let target = household.scenarios.first().unwrap();
        assert_eq!(target.changes.len(), 1, "the first scenario in the picker received the change");
        assert!(matches!(&target.changes[0], atlas_core::scenario::ScenarioChange::EndSeries { reason, .. } if reason == "test change"));
    });
}

#[gpui_kit::test]
fn decision_builder_steps_to_a_result_and_saves_a_scenario(cx: &mut TestAppContext) {
    let launch = Launch { section: Section::Decisions, ..Launch::default() };
    let (handle, app) = open_app(cx, launch);
    let window = handle.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-decisions").visible());
        assert!(window.find("decision-name").visible(), "step 1 shows the purchase form");
        window.click("decision-next", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| assert_eq!(app.read(cx).decision_step(), 1));
    cx.update(|cx| {
        let result = app.update(cx, |app, cx| app.apply_decision_step(1, cx));
        assert!(result.is_ok(), "step 2 with defaults is valid: {result:?}");
    });
    for expected_step in [2, 3, 4] {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            // Long steps push "Next" below the fold; scroll from an on-screen anchor first.
            scroll_down(window, "screen-decisions", cx);
            window.click("decision-next", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update(|cx| assert_eq!(app.read(cx).decision_step(), expected_step));
    }
    cx.update(|cx| {
        let app = app.read(cx);
        let plan = app.decision_plan();
        assert_eq!(plan.name, "Car");
        assert_eq!(plan.down_payment, fixtures::pkr(2_500_000));
        assert!(plan.financing.is_some());
        let decision = app.decision().expect("the result step evaluated the plan");
        assert!(!decision.strategies.strategies.is_empty());
        assert!(decision.strategies.status.contains("not a global optimum"));
        assert_eq!(decision.grid.len(), 42, "6 months × 7 down payments");
        assert!(decision.grid.iter().any(|c| c.best));
        assert!(decision.grid.iter().filter(|c| c.reserve_ok).count() >= 1);
        assert!(decision.recommendation.render().contains("not an AI recommendation"));
        assert!(decision.immediate_cash.node().verify_sums().is_empty());
        assert_eq!(decision.goals.len(), 2);
    });
    let scenarios_before = cx.update(|cx| app.read(cx).household().scenarios.len());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("decision-grid-status").is_some(), "the grid is on the result page");
        assert!(window.find("decision-recommendation").visible());
        window.click("decision-save-scenario-top", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let app = app.read(cx);
        let household = app.household();
        assert_eq!(household.scenarios.len(), scenarios_before + 1);
        let saved = household.scenarios.last().unwrap();
        assert!(saved.name.starts_with("Decision: Car"));
        let tagged = household.series.iter().filter(|s| s.scenario == Some(saved.id)).count();
        assert!(tagged >= 4, "down payment steps, instalments, other and running costs: {tagged}");
        assert!(app.is_dirty());
        let model = app.scenarios().unwrap();
        assert_eq!(model.selection, vec![saved.id]);
        assert!(model.comparison.as_ref().unwrap().attribution_verified);
    });

    // Validation keeps the builder on the step: a down payment above the price is refused.
    let launch = Launch { section: Section::Decisions, ..Launch::default() };
    let (handle, app) = open_app(cx, launch);
    let window = handle.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("decision-next", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("decision-down-payment", cx);
        window.input("0", cx);
        window.click("decision-next", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.decision_step(), 1, "25,000,000 exceeds the 8,000,000 price");
        assert_eq!(app.decision_plan().down_payment, fixtures::pkr(2_500_000), "the plan keeps the last valid value");
    });
}

#[gpui_kit::test]
fn privacy_screen_policy_editor_grants_and_fail_closed_view(cx: &mut TestAppContext) {
    // Person A: the register, a new policy version through the editor, a purpose grant.
    let launch = Launch { section: Section::Privacy, ..Launch::default() };
    let (handle, app) = open_app(cx, launch);
    let window = handle.into();
    cx.update(|cx| {
        let model = app.read(cx).privacy();
        assert_eq!(model.policies.len(), 16, "Person A may see every object");
        assert_eq!(model.hidden_policies, 0);
        assert!(model.problems.is_empty());
        assert_eq!(model.grants.len(), 0);
        assert!(!model.audit.is_empty(), "the fixture's own policy setup is on the log");
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-privacy").visible());
        assert!(window.find("policy-row-1").visible());
        window.click("privacy-edit-policy", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("policy-save").visible());
        window.click("policy-note", cx);
        window.input("shared summary from now on", cx);
        window.click("policy-save", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx), "the policy was set");
        window.clear_notifications(cx);
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        let household = app.household();
        // The first owned object in the picker is the shared savings account: now v2, "Shared summary".
        let policy = household.policy_for(ObjectRef::Account(fixtures::ids::SHARED_SAVINGS)).unwrap();
        assert_eq!(policy.version, 2);
        assert_eq!(policy.preset_label(), "Shared summary");
        assert_eq!(policy.previous_versions.len(), 1);
        assert!(household.audit.iter().any(|e| matches!(e.kind, atlas_core::authz::AuditKind::PolicyChanged { from_version: 1, to_version: 2 })));
        assert!(household.audit.iter().any(|e| e.summary.contains("shared summary from now on")));
        assert!(app.is_dirty());
        // Person B (co-owner) still sees it in full; a third person would not.
        assert_eq!(household.disclosure_for(atlas_core::authz::Viewer::person(fixtures::ids::PERSON_B), ObjectRef::Account(fixtures::ids::SHARED_SAVINGS)), Disclosure::Full);
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("privacy-add-grant", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("grant-save").visible());
        window.click("grant-save", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx), "the grant was added");
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        let household = app.household();
        assert_eq!(household.grants.len(), 1);
        let grant = &household.grants[0];
        assert_eq!(grant.grantee, atlas_core::authz::Grantee::Person(fixtures::ids::PERSON_B));
        assert_eq!(grant.purpose, atlas_core::authz::Purpose::HouseholdForecast);
        assert!(household.audit.iter().any(|e| matches!(e.kind, atlas_core::authz::AuditKind::GrantAdded)));
        assert_eq!(app.privacy().grants.len(), 1);
    });

    // Person B: private objects are absent from the register; the denial text names no object.
    let launch = Launch { section: Section::Privacy, viewer: 'b', ..Launch::default() };
    let (handle, app) = open_app(cx, launch);
    cx.update(|cx| {
        let model = app.read(cx).privacy();
        assert!(model.hidden_policies >= 3, "A's private account, payroll account and private scenarios");
        assert!(model.policies.iter().all(|p| !p.object_name.contains("Person A current account")));
        let example = model.denial_example.as_ref().expect("B is denied something");
        assert!(example.contains("not authorized") || example.contains("fails closed"));
        assert!(!example.contains("Person A current") && !example.contains("Leave job"));
        assert!(model.problems.is_empty(), "problems are for owners; B owns no problematic object");
    });
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("privacy-denial-example").is_some(), "the denial example is on the page (below the fold)");
        assert!(window.try_find("policy-row-2").is_none(), "A's private account policy is not rendered for B");
        assert!(window.try_find("policy-row-12").is_none(), "nor the private scenario's");
    })
    .unwrap();
}

#[gpui_kit::test]
fn every_screen_renders_for_both_viewers(cx: &mut TestAppContext) {
    // M11.2: each section's root renders for the owner and for Person B, with the
    // sidebar item registered as "<group>-0-<item>" inside the main sidebar.
    for viewer in ['a', 'b'] {
        let launch = Launch { viewer, ..Launch::default() };
        let (handle, app) = open_app(cx, launch);
        let window = handle.into();
        for (group_index, (_, sections)) in Section::GROUPS.iter().enumerate() {
            for (item_index, section) in sections.iter().enumerate() {
                let item: &'static str = Box::leak(format!("{group_index}-0-{item_index}").into_boxed_str());
                let root: &'static str = Box::leak(format!("screen-{}", section.slug()).into_boxed_str());
                cx.update_window(window, |_, window, cx| {
                    window.render_frame(cx);
                    window.within("main-sidebar").click(item, cx);
                })
                .unwrap();
                cx.run_until_parked();
                cx.update_window(window, |_, window, cx| {
                    window.render_frame(cx);
                    assert!(window.try_find(root).is_some(), "{root} renders for viewer {viewer}");
                })
                .unwrap();
                cx.update(|cx| assert_eq!(app.read(cx).section(), *section));
            }
        }
    }
}

#[gpui_kit::test]
fn status_bar_shows_the_frame_meter(cx: &mut TestAppContext) {
    // Perf diagnostics: the status bar carries the previous frame's fps /
    // build / draw readout, and every rendered frame is measured.
    let (handle, app) = open_app(cx, Launch::default());
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("perf-counter").visible(), "the counter sits in the status bar");
        window.render_frame(cx);
    })
    .unwrap();
    cx.update(|cx| {
        let meter = app.read(cx).perf();
        assert!(meter.frames() >= 2, "two frames rendered: {}", meter.frames());
        let last = meter.last().expect("the first frame is complete once the second began");
        assert_eq!(last.section, "household");
        assert!(last.build > std::time::Duration::ZERO, "{last:?}");
        assert!(last.draw.is_some(), "the paint probe painted: {last:?}");
        assert!(last.draw.unwrap() >= last.build, "draw covers the build: {last:?}");
        // The counter is gpui's own reading (draw p50 / fps from its
        // profiler histograms), refreshed by the once-a-second summary.
        let status = meter.status_text();
        assert!(status.starts_with("gpui: "), "{status}");
    });
}

/// Moves the pointer to the centre of an element without rendering a frame
/// first (unlike `hover`, which refreshes the window and so bypasses every
/// view cache). The harness draws the frame the move causes before returning.
fn move_pointer_to(window: &mut gpui_kit::Window, scope: Option<&'static str>, id: &'static str, cx: &mut gpui_kit::App) {
    let position = match scope {
        Some(scope) => window.within(scope).find(id).bounds().center(),
        None => window.find(id).bounds().center(),
    };
    window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position, pressed_button: None, modifiers: Modifiers::default() }), cx);
}

#[gpui_kit::test]
fn shell_reuses_cached_views_between_frames(cx: &mut TestAppContext) {
    // Perf step 3: the sidebar and the content are separate cached views. A
    // frame re-renders only the views that were notified; the rest reuse their
    // previous layout and paint. `render_frame` refreshes the window (which
    // bypasses every cache), so the frames under test are drawn directly, and
    // a frame's figures are read once the next frame has closed it.
    let (handle, shell) = open_shell(cx, Launch::default());
    let (app, sidebar) = cx.update(|cx| {
        let shell = shell.read(cx);
        (shell.app().clone(), shell.sidebar().clone())
    });
    let window: gpui_kit::AnyWindowHandle = handle.into();

    // Nothing changed between two frames: both cached views are reused.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let renders = sidebar.read(cx).renders();
        assert!(renders >= 1, "the first frame rendered the sidebar");
        window.draw(cx).clear(cx);
        assert_eq!(sidebar.read(cx).renders(), renders, "an unchanged frame reuses the sidebar");
        window.draw(cx).clear(cx);
        let last = app.read(cx).perf().last().expect("the previous frame is closed");
        assert_eq!(last.content_render, None, "an unchanged frame reuses the content: {last:?}");
        // The first pointer event switches gpui's input modality, which
        // refreshes the window once; park the pointer on the status bar so
        // that is out of the way.
        move_pointer_to(window, None, "perf-counter", cx);
        window.draw(cx).clear(cx);
    })
    .unwrap();

    // Hovering a sidebar item re-renders the sidebar, not the screen.
    cx.update_window(window, |_, window, cx| {
        let renders = sidebar.read(cx).renders();
        move_pointer_to(window, Some("main-sidebar"), "0-0-1", cx);
        window.draw(cx).clear(cx);
        assert_eq!(sidebar.read(cx).renders(), renders + 1, "the hover re-renders the sidebar");
        window.draw(cx).clear(cx);
        let last = app.read(cx).perf().last().unwrap();
        assert_eq!(last.content_render, None, "a sidebar hover does not rebuild the screen: {last:?}");
    })
    .unwrap();

    // Hovering a button on the screen re-renders the content, not the sidebar
    // (the pointer leaves the sidebar first, which un-hovers its item).
    cx.update_window(window, |_, window, cx| {
        move_pointer_to(window, None, "perf-counter", cx);
        window.draw(cx).clear(cx);
        let renders = sidebar.read(cx).renders();
        move_pointer_to(window, None, "why-free-cash", cx);
        window.draw(cx).clear(cx);
        assert_eq!(sidebar.read(cx).renders(), renders, "a content hover leaves the sidebar cached");
        window.draw(cx).clear(cx);
        let last = app.read(cx).perf().last().unwrap();
        assert!(last.content_render.is_some(), "the hover re-rendered the content: {last:?}");
    })
    .unwrap();

    // Scrolling the screen re-renders the content, not the sidebar.
    cx.update_window(window, |_, window, cx| {
        move_pointer_to(window, None, "perf-counter", cx);
        window.draw(cx).clear(cx);
        let renders = sidebar.read(cx).renders();
        let position = window.find("figure-free-cash").bounds().center();
        window.dispatch_event(
            PlatformInput::ScrollWheel(ScrollWheelEvent { position, delta: ScrollDelta::Pixels(point(px(0.), px(-300.))), modifiers: Modifiers::default(), touch_phase: TouchPhase::Moved }),
            cx,
        );
        window.draw(cx).clear(cx);
        assert_eq!(sidebar.read(cx).renders(), renders, "a scroll tick leaves the sidebar cached");
        window.draw(cx).clear(cx);
        let last = app.read(cx).perf().last().unwrap();
        assert!(last.content_render.is_some(), "the scroll re-rendered the content: {last:?}");
        assert_eq!(last.wheel_events, 1, "{last:?}");
    })
    .unwrap();

    // A state change the sidebar does not show (the liquidity boundary) leaves
    // it cached; one it does show (the section) re-renders it. The sidebar
    // learns of the change through an observer, which runs when the update
    // that notified the app has flushed — hence one `update` per change.
    let renders = cx.update(|cx| {
        let renders = sidebar.read(cx).renders();
        app.update(cx, |app, cx| app.select_boundary(atlas_core::liquidity::Boundary::Person(fixtures::ids::PERSON_A), cx));
        renders
    });
    cx.update_window(window, |_, window, cx| {
        window.draw(cx).clear(cx);
        assert_eq!(sidebar.read(cx).renders(), renders, "a boundary change does not touch the sidebar");
        assert_eq!(sidebar.read(cx).snapshot().section, Section::Household);
    })
    .unwrap();
    cx.update(|cx| app.update(cx, |app, cx| app.navigate(Section::People, cx)));
    cx.update_window(window, |_, window, cx| {
        window.draw(cx).clear(cx);
        assert_eq!(sidebar.read(cx).renders(), renders + 1, "navigation re-renders the sidebar");
        assert_eq!(sidebar.read(cx).snapshot().section, Section::People);
        assert!(window.try_find("screen-people").is_some(), "and the content shows the new section");
    })
    .unwrap();
}

#[gpui_kit::test]
fn derived_models_are_computed_only_for_the_screen_in_use(cx: &mut TestAppContext) {
    // Perf step 4: an edit drops every derived model; only the visible
    // screen's is computed again (on its next frame), the others when their
    // screen is opened. Before, every edit ran all eleven engine models.
    let launch = Launch { section: Section::Rules, ..Launch::default() };
    let (handle, app) = open_app(cx, launch);
    let window: gpui_kit::AnyWindowHandle = handle.into();

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let app = app.read(cx);
        assert!(app.is_model_computed(Section::Rules), "the screen on show has its model");
        for section in [Section::Household, Section::Timeline, Section::Taxes, Section::Projections, Section::Scenarios, Section::Privacy] {
            assert!(!app.is_model_computed(section), "{} is not computed until its screen is opened", section.slug());
        }
    })
    .unwrap();

    // An edit on the Rules screen: the rules model comes back on the next
    // frame; the timeline stays uncomputed.
    cx.update_window(window, |_, window, cx| {
        window.click("rule-toggle-1", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        // (The harness draws a frame as soon as the edit's notify lands, so the
        // rules model is already back by now — as it would be on screen.)
        window.render_frame(cx);
        let app = app.read(cx);
        assert!(app.is_model_computed(Section::Rules), "the frame computed the rules model again");
        assert!(!app.is_model_computed(Section::Timeline), "the timeline was not recomputed for a rules edit");
        assert!(!app.is_model_computed(Section::Household));
    })
    .unwrap();

    // Opening the Timeline computes it then; asking for a model directly
    // (as forms and tests do) computes it too.
    cx.update(|cx| app.update(cx, |app, cx| app.navigate(Section::Timeline, cx)));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(app.read(cx).is_model_computed(Section::Timeline));
        assert!(!app.read(cx).is_model_computed(Section::Household));
        assert!(app.read(cx).overview().is_some(), "an accessor computes on demand");
        assert!(app.read(cx).is_model_computed(Section::Household));
    })
    .unwrap();
}

