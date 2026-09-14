//! UI integration tests: the production `AtlasApp` view in a headless window,
//! driven through real pointer and keyboard events (gpui-kit `test-support`).
//!
//! Sidebar items are registered by gpui-kit as `<group>-0-<item>` inside the
//! `main-sidebar` scope. The groups follow `Destination::GROUPS`: Today is
//! `0-0-0`, Accounts is `1-0-0`, Rules & taxes is `2-0-0`.

use atlas_app::launch::Start;
use atlas_app::nav::{Destination, Route};
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

/// Dismisses the result toasts and waits for them to leave. They sit in the
/// top-right corner — over the trailing commands of every workspace — and keep
/// their hitbox while they animate out.
fn dismiss_toasts(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.clear_notifications(cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
}

/// Scrolls the main column down by `pixels` from an on-screen anchor.
fn scroll_by(window: &mut gpui_kit::Window, anchor: &'static str, pixels: f32, cx: &mut gpui_kit::App) {
    window.scroll(anchor, ScrollDelta::Pixels(point(px(0.), px(-pixels))), cx);
    window.render_frame(cx);
}

/// Clicks `id`, scrolling from `anchor` first when it starts below the fold.
fn scroll_to_and_click(window: &mut gpui_kit::Window, anchor: &'static str, id: &'static str, cx: &mut gpui_kit::App) {
    if !window.try_find(id).is_some_and(|e| e.visible()) {
        scroll_by(window, anchor, 900., cx);
    }
    window.click(id, cx);
}

/// Asserts the element exists in this frame, whether or not it is scrolled
/// into view.
fn present(window: &gpui_kit::Window, id: &'static str) -> bool {
    window.try_find(id).is_some()
}

/// The sample household, a chosen viewer and a starting route: what almost
/// every test opens.
fn sample(route: Route) -> Launch {
    Launch { start: Start::Sample, viewer: Some('a'), route, ..Launch::default() }
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
    // Tall enough that a rebuilt workspace fits without scrolling; the real
    // window scrolls, and one test drives that deliberately.
    let handle = cx.open_window(size(px(1600.), px(2400.)), |window, cx| {
        let shell = cx.new(|cx| Shell::new(&launch, window, cx));
        shell_view = Some(shell.clone());
        Root::new(shell, window, cx)
    });
    (handle, shell_view.expect("view created"))
}

/// Navigates through the app's own API (the sidebar and tabs are covered by
/// their own tests) and draws the resulting frame.
fn go(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle, app: &Entity<AtlasApp>, route: Route) {
    cx.update(|cx| app.update(cx, |app, cx| app.navigate(route, cx)));
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| window.render_frame(cx)).unwrap();
}

// ----- Shell and first experience -------------------------------------------------

#[gpui_kit::test]
fn welcome_offers_the_three_ways_in(cx: &mut TestAppContext) {
    // With no household on the command line the app opens on Welcome, with no
    // sidebar and no sample data loaded behind it.
    let (handle, app) = open_app(cx, Launch::default());
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-welcome").visible());
        assert!(window.find("welcome-create").visible());
        assert!(window.find("welcome-open").visible());
        assert!(window.find("welcome-sample").visible());
        assert!(window.try_find("main-sidebar").is_none(), "no navigation until a household is open");
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(!app.is_opened());
        assert_eq!(app.route(), Route::Welcome);
        assert!(app.household().people.is_empty(), "the sample is not loaded behind Welcome");
    });
}

#[gpui_kit::test]
fn the_sample_asks_who_is_looking_before_showing_anything(cx: &mut TestAppContext) {
    let launch = Launch { start: Start::Sample, ..Launch::default() };
    let (handle, app) = open_app(cx, launch);
    cx.run_until_parked();
    let_dialog_settle();
    cx.update(|cx| assert!(app.read(cx).viewer_pending(), "content waits for the chooser"));
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-viewer-gate").visible());
        assert!(window.try_find("screen-today").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn sidebar_and_workspace_tabs_navigate(cx: &mut TestAppContext) {
    let (handle, app) = open_app(cx, sample(Route::Today));
    let window = handle.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-today").visible());
        // Accounts is the first item of the second group.
        window.within("main-sidebar").click("1-0-0", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-accounts").visible());
        // Its second tab is Earmarks.
        window.within("tabs-accounts").click(1usize, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-earmarks").visible(), "the workspace tab switched routes");
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.route(), Route::Earmarks);
        assert_eq!(app.destination(), Some(Destination::Accounts), "the sidebar stays on Accounts");
    });
}

#[gpui_kit::test]
fn status_bar_shows_the_file_state_and_frame_meter(cx: &mut TestAppContext) {
    let (handle, app) = open_app(cx, sample(Route::Today));
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("perf-counter").visible(), "the counter sits in the status bar");
        assert!(window.find("file-state").visible());
        window.render_frame(cx);
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.file_state_text(), "Not saved to a file");
        let meter = app.perf();
        assert!(meter.frames() >= 2, "two frames rendered: {}", meter.frames());
        let last = meter.last().expect("the first frame is complete once the second began");
        assert_eq!(last.section, "today");
        assert!(last.build > std::time::Duration::ZERO, "{last:?}");
        assert!(last.draw.is_some(), "the paint probe painted: {last:?}");
        // The counter is gpui's own reading (draw p50 / fps from its profiler
        // histograms), refreshed by the once-a-second summary.
        assert!(meter.status_text().starts_with("gpui: "), "{}", meter.status_text());
    });
}

// ----- Today ------------------------------------------------------------------------

#[gpui_kit::test]
fn today_leads_with_free_cash_and_explains_it(cx: &mut TestAppContext) {
    let (handle, app) = open_app(cx, sample(Route::Today));
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-today").visible());
        assert!(window.find("figure-free-cash").visible());
        assert!(window.try_find("explain-chain").is_none(), "no sheet before the click");
        window.click("explain-free-cash", cx);
        assert!(window.find("explain-chain").visible(), "Explain… opens the calculation sheet");
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.route(), Route::Today);
        let overview = app.overview().expect("overview computed");
        let free = overview.money.iter().find(|f| f.id.as_ref() == "free-cash").unwrap();
        assert_eq!(free.calc.money(), fixtures::pkr(2_650_000));
        assert_eq!(free.disclosure(), Disclosure::Full, "the owner sees everything");
        assert!(free.calc.node().verify_sums().is_empty());
    });
}

#[gpui_kit::test]
fn person_b_gets_aggregates_not_person_a_details(cx: &mut TestAppContext) {
    let launch = Launch { start: Start::Sample, viewer: Some('b'), route: Route::Today, ..Launch::default() };
    let (handle, app) = open_app(cx, launch);
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-today").visible());
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        let overview = app.overview().unwrap();
        let free = overview.money.iter().find(|f| f.id.as_ref() == "free-cash").unwrap();
        assert_eq!(free.disclosure(), Disclosure::Aggregate, "Person A's private account is an aggregate term");
        assert!(free.calc.node().verify_sums().is_empty(), "the projected chain still adds up");
        let household = app.household();
        assert_eq!(household.disclosure_for(app.viewer(), ObjectRef::Account(fixtures::ids::PERSON_A_CURRENT)), Disclosure::Aggregate);
    });
}

// ----- Accounts ----------------------------------------------------------------------

#[gpui_kit::test]
fn accounts_register_selects_and_opens_an_account(cx: &mut TestAppContext) {
    let (handle, app) = open_app(cx, sample(Route::Accounts));
    let window = handle.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-accounts").visible());
        assert!(window.find("count-accounts").visible(), "the count line is computed from the data");
        let row: &'static str = &*Box::leak(format!("account-{}", fixtures::ids::PERSON_A_CURRENT.raw()).into_boxed_str());
        window.click(row, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("accounts-open", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-account").visible(), "the detail replaced the register");
    })
    .unwrap();
    cx.update(|cx| assert_eq!(app.read(cx).route(), Route::Account(fixtures::ids::PERSON_A_CURRENT)));
}

#[gpui_kit::test]
fn earmarks_add_and_release_show_the_computed_effect(cx: &mut TestAppContext) {
    let (handle, app) = open_app(cx, sample(Route::Earmarks));
    let window = handle.into();
    let before = cx.update(|cx| app.read(cx).household().reservations.len());

    // Add one through the dialog.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-earmarks").visible());
        window.click("new-reservation", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("reservation-name", cx);
        window.input("School trip", cx);
        window.click("reservation-amount", cx);
        window.input("50,000", cx);
        window.click("save-reservation", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.household().reservations.len(), before + 1);
        assert!(app.is_dirty());
    });

    // Pay and release an earmark: the dialog states the computed before and
    // after. This one covers the bank minimum, so free cash moves by less than
    // the payment — never the algebraic claim that it cannot move at all.
    dismiss_toasts(cx, window);
    let (settled_before, free_before) = cx.update(|cx| {
        let app = app.read(cx);
        let liquidity = atlas_core::liquidity::account_liquidity(app.household(), fixtures::ids::PERSON_A_CURRENT).unwrap();
        (liquidity.ledger_cash.money(), liquidity.free.money())
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let id: &'static str = &*Box::leak(format!("release-{}", fixtures::ids::A_LIQUIDITY_BUFFER.raw()).into_boxed_str());
        scroll_to_and_click(window, "earmarks-money", id, cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("confirm-release", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let app = app.read(cx);
        let liquidity = atlas_core::liquidity::account_liquidity(app.household(), fixtures::ids::PERSON_A_CURRENT).unwrap();
        let paid = settled_before.minor() - liquidity.ledger_cash.money().minor();
        assert!(paid > 0, "the payment left the account");
        let free_fall = free_before.minor() - liquidity.free.money().minor();
        assert!(free_fall < paid, "free cash falls by less than the payment: the money was already set aside ({free_fall} of {paid})");
        assert!(app.last_result().is_some(), "the result sentence went to the status lane");
    });
}

// ----- Activity ------------------------------------------------------------------------

#[gpui_kit::test]
fn upcoming_inspector_skips_one_occurrence(cx: &mut TestAppContext) {
    let (handle, app) = open_app(cx, sample(Route::Upcoming));
    let window = handle.into();
    // Select the first occurrence through the app (the grid's rows are
    // virtualised) and read its inspector.
    let key = cx.update(|cx| {
        let app = app.read(cx);
        let model = app.timeline().expect("timeline computed");
        let o = model.occurrences.first().expect("the sample has occurrences");
        (o.series, o.original_due)
    });
    cx.update(|cx| app.update(cx, |app, cx| app.select_occurrence(key.0, key.1, cx)));
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-upcoming").visible());
        assert!(present(window, "occurrence-inspector"), "the inspector opened for the selected row");
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    // The menu item routes through the app handle; drive the command directly
    // and check the engine recorded the exception.
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| app.confirm_skip_occurrence(key.0, key.1, window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("confirm-ok", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let app = app.read(cx);
        let series = app.household().series_by_id(key.0).unwrap();
        let exception = series.exception_on(key.1).expect("the exception is keyed on the original due date");
        assert!(matches!(exception.kind, atlas_core::timeline::ExceptionKind::Skip));
        assert!(app.last_result().is_some_and(|r| r.contains("Skipped")), "{:?}", app.last_result());
    });
}

#[gpui_kit::test]
fn actuals_inspector_matches_an_existing_transaction(cx: &mut TestAppContext) {
    let (handle, app) = open_app(cx, sample(Route::Actuals));
    let window = handle.into();
    let transaction = cx.update(|cx| {
        let app = app.read(cx);
        let model = app.timeline().unwrap();
        *model.actual_ids.first().expect("the sample records transactions")
    });
    let (links_before, unallocated) = cx.update(|cx| {
        let app = app.read(cx);
        let links = app.household().links.iter().filter(|l| l.transaction == transaction).count();
        let unallocated = atlas_app::occurrence_entry::actual_unallocated(app.household(), transaction).unwrap().1;
        (links, unallocated)
    });
    cx.update(|cx| app.update(cx, |app, cx| app.select_actual_row(0, cx)));
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-actuals").visible());
        assert!(window.find("actual-inspector").visible());
    })
    .unwrap();
    if !unallocated.is_positive() {
        // The first transaction is fully matched already: the command is
        // disabled, which is what the design asks for.
        return;
    }
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("actual-match", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("match-amount").visible(), "the match sheet opened");
        window.click("match-cancel", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.household().links.iter().filter(|l| l.transaction == transaction).count(), links_before, "cancelling records nothing");
    });
}

#[gpui_kit::test]
fn series_detail_reads_the_whole_schedule(cx: &mut TestAppContext) {
    let (handle, app) = open_app(cx, sample(Route::Series));
    let window = handle.into();
    let series = cx.update(|cx| app.read(cx).household().series[0].id);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-series").visible());
        assert!(window.find("count-series").visible());
    })
    .unwrap();
    go(cx, window, &app, Route::SeriesDetail(series));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-series-detail").visible());
        assert!(window.find("series-facts").visible(), "the lags and same-day order are readable");
        assert!(window.find("series-upcoming").visible());
    })
    .unwrap();
}

// ----- People & companies ---------------------------------------------------------------

#[gpui_kit::test]
fn person_detail_shows_holdings_and_the_horizon_aligned_tax(cx: &mut TestAppContext) {
    let (handle, app) = open_app(cx, sample(Route::People));
    let window = handle.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-people").visible());
    })
    .unwrap();
    go(cx, window, &app, Route::Person(fixtures::ids::PERSON_A));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-person").visible());
        assert!(present(window, "person-accounts"));
        assert!(present(window, "person-tax"));
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        let model = app.entities().expect("entity models computed");
        let person = model.person(fixtures::ids::PERSON_A).unwrap();
        let tax = person.tax.as_ref().expect("the assessment ran");
        assert_eq!(tax.through, app.horizon(), "the person's tax attribution follows the forecast horizon");
    });
}

#[gpui_kit::test]
fn person_b_sees_only_the_planning_safe_company_output(cx: &mut TestAppContext) {
    let launch = Launch { start: Start::Sample, viewer: Some('b'), route: Route::Company(fixtures::ids::ALPHA), ..Launch::default() };
    let (handle, app) = open_app(cx, launch);
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-company").visible());
        assert!(present(window, "company-summary"), "Person B gets the summary variant");
        assert!(window.try_find("company-employees").is_none(), "no payroll for a summary viewer");
        assert!(window.try_find("company-accounts").is_none());
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.household().disclosure_for(app.viewer(), ObjectRef::Company(fixtures::ids::ALPHA)), Disclosure::Aggregate, "Person B may know it exists and see a planning-safe ceiling only");
    });
}

// ----- Forecast ---------------------------------------------------------------------------

#[gpui_kit::test]
fn forecast_path_switches_case_and_opens_the_record(cx: &mut TestAppContext) {
    let (handle, app) = open_app(cx, sample(Route::ForecastPath));
    let window = handle.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-forecast").visible());
        assert!(present(window, "forecast-path"), "the cash path is on show");
        assert!(present(window, "forecast-breach-summary"));
    })
    .unwrap();
    let expected_end = cx.update(|cx| app.read(cx).projection().unwrap().forecast.end.money());
    cx.update(|cx| app.update(cx, |app, cx| app.select_projection_case(atlas_core::forecast::Case::Conservative, cx)));
    cx.run_until_parked();
    cx.update(|cx| {
        let app = app.read(cx);
        let conservative = app.projection().unwrap();
        assert_eq!(conservative.forecast.case, atlas_core::forecast::Case::Conservative);
        assert!(conservative.forecast.end.money().minor() <= expected_end.minor(), "the conservative path is not above the expected one");
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("forecast-record", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("copy-forecast-record").visible(), "the read-only record sheet opened");
        window.click("close-forecast-record", cx);
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn assumptions_accept_and_derive_through_the_ui(cx: &mut TestAppContext) {
    let (handle, app) = open_app(cx, sample(Route::Assumptions));
    let window = handle.into();
    let stale = cx.update(|cx| {
        let app = app.read(cx);
        let as_of = app.household().as_of;
        app.household().assumptions.iter().find(|a| a.freshness(as_of) == atlas_core::model::Freshness::Stale).map(|a| a.id).expect("the sample has a stale assumption")
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-assumptions").visible());
        let id: &'static str = &*Box::leak(format!("accept-assumption-{}", stale.raw()).into_boxed_str());
        window.click(id, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let app = app.read(cx);
        let assumption = app.household().assumptions.iter().find(|a| a.id == stale).unwrap();
        assert_eq!(assumption.accepted_on, Some(app.household().as_of), "accepting records the reconciliation date");
        assert_eq!(assumption.freshness(app.household().as_of), atlas_core::model::Freshness::Fresh);
    });

    // Derive from history: the tool computes from a fixed sample, and applying
    // it clears the target's acceptance.
    dismiss_toasts(cx, window);
    go(cx, window, &app, Route::Derive);
    let target = cx.update(|cx| {
        let app = app.read(cx);
        let model = app.assumptions().expect("assumptions model");
        let derived = model.derived.as_ref().expect("the sample has derivable history");
        assert!(derived.sample_size >= 2, "{derived:?}");
        app.household().assumptions.iter().find(|a| a.applies_to.contains(&model.derivation_series)).map(|a| a.id).expect("a target assumption exists")
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-derive").visible());
        assert!(present(window, "derive-history"), "the sample rows are listed");
        app.update(cx, |app, cx| app.apply_derivation(target, window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let app = app.read(cx);
        let assumption = app.household().assumptions.iter().find(|a| a.id == target).unwrap();
        assert!(matches!(assumption.source, atlas_core::model::AssumptionSource::DerivedFromHistory { .. }));
        assert_eq!(assumption.accepted_on, None, "applying a derivation clears acceptance");
    });
}

#[gpui_kit::test]
fn sensitivity_waits_for_an_explicit_run(cx: &mut TestAppContext) {
    let (handle, app) = open_app(cx, sample(Route::Sensitivity));
    let window = handle.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-sensitivity").visible());
        assert!(window.find("sensitivity-summary").visible());
        assert!(present(window, "sensitivity-breakpoints"), "the breakpoints and their joint caveat are here");
    })
    .unwrap();
    // A scope change marks the result out of date; the command becomes live.
    cx.update(|cx| app.update(cx, |app, cx| app.select_sensitivity_boundary(atlas_core::liquidity::Boundary::Account(fixtures::ids::PERSON_A_CURRENT), cx)));
    cx.run_until_parked();
    cx.update(|cx| assert!(app.read(cx).sensitivity_pending(), "the result on show is behind the chosen scope"));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("run-sensitivity", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| assert!(!app.read(cx).sensitivity_pending(), "running brings the result up to date"));
    cx.update(|cx| {
        let model = app.read(cx).assumptions().unwrap();
        assert_eq!(model.sensitivity.boundary, atlas_core::liquidity::Boundary::Account(fixtures::ids::PERSON_A_CURRENT));
    });
}

// ----- Decisions ---------------------------------------------------------------------------

#[gpui_kit::test]
fn purchase_builder_steps_to_a_result_and_saves_a_scenario(cx: &mut TestAppContext) {
    let (handle, app) = open_app(cx, sample(Route::Purchase));
    let window = handle.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-purchase").visible());
        assert!(present(window, "purchase-step-1"), "step 1 first");
        scroll_to_and_click(window, "purchase-stepper", "purchase-next", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(present(window, "purchase-sources"), "step 2 shows the funding editor");
        scroll_to_and_click(window, "purchase-stepper", "purchase-next", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(present(window, "purchase-step-3"));
        scroll_to_and_click(window, "purchase-stepper", "purchase-next", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(present(window, "purchase-step-4"));
        scroll_to_and_click(window, "purchase-stepper", "purchase-calculate", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-purchase-result").visible(), "Calculate opens the result");
        assert!(present(window, "decision-verdict"));
        assert!(present(window, "decision-recommendation"));
        assert!(present(window, "decision-affordability"), "Affordability is the first report tab");
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.route(), Route::PurchaseResult);
        let decision = app.decision().expect("evaluated");
        assert!(!decision.grid.is_empty(), "the combination grid was searched");
        assert!(!decision.metrics.is_empty());
    });

    // Funding, Combinations and Basis are the same result, read differently.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        scroll_by(window, "decision-verdict", 900., cx);
        window.within("decision-report-tabs").click(1usize, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(present(window, "decision-strategy-status"));
        window.within("decision-report-tabs").click(2usize, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(present(window, "decision-combinations"));
        window.within("decision-report-tabs").click(4usize, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(present(window, "decision-statement-claim"), "the basis states what the result establishes");
    })
    .unwrap();

    // Save as scenario once, then the command becomes Open saved scenario.
    let scenarios_before = cx.update(|cx| app.read(cx).household().scenarios.len());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        scroll_to_and_click(window, "decision-verdict", "decision-save-scenario", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.household().scenarios.len(), scenarios_before + 1, "the plan became a scenario");
        assert!(app.household().scenarios.last().unwrap().name.starts_with("Decision:"));
        assert!(app.is_dirty(), "it is stored when the file is saved");
    });
    dismiss_toasts(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(present(window, "decision-open-scenario"), "a second save is not offered");
    })
    .unwrap();
}

#[gpui_kit::test]
fn scenarios_compare_compose_and_stay_private(cx: &mut TestAppContext) {
    let (handle, app) = open_app(cx, sample(Route::Scenarios));
    let window = handle.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-scenarios").visible());
        assert!(present(window, "scenario-compatibility"));
        assert!(present(window, "scenario-detail"), "the first scenario is inspected");
    })
    .unwrap();
    // Comparing the first scenario with the baseline, and the attribution adds up.
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("scenario-compare", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-compare").visible());
        assert!(present(window, "comparison-figures"));
        assert!(present(window, "comparison-chart"));
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        let model = app.scenarios().expect("comparison computed");
        let comparison = model.comparison.as_ref().expect("a scenario is selected");
        assert!(comparison.attribution_verified, "the difference attribution sums to the end difference exactly");
    });

    // Person B does not see Person A's private scenario at all.
    let launch = Launch { start: Start::Sample, viewer: Some('b'), route: Route::Scenarios, ..Launch::default() };
    let (handle_b, app_b) = open_app(cx, launch);
    cx.update_window(handle_b.into(), |_, window, cx| window.render_frame(cx)).unwrap();
    cx.update(|cx| {
        let app = app_b.read(cx);
        let model = app.scenarios().unwrap();
        assert!(model.hidden_count > 0, "a private scenario is counted, not named");
        assert!(model.cards.iter().all(|c| !c.private || c.name.is_empty()), "no private scenario of someone else is listed");
    });
}

#[gpui_kit::test]
fn extraction_illustration_steps_to_its_comparison(cx: &mut TestAppContext) {
    let (handle, app) = open_app(cx, sample(Route::Extraction));
    let window = handle.into();
    for step in 0..3 {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("screen-extraction").visible());
            let id: &'static str = &*Box::leak(format!("extraction-step-{}", step + 1).into_boxed_str());
            assert!(window.find(id).visible(), "step {} is on show", step + 1);
            window.click("extraction-next", cx);
        })
        .unwrap();
        cx.run_until_parked();
    }
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("extraction-result").visible());
    })
    .unwrap();
    cx.update(|cx| {
        let model = app.read(cx).taxes().expect("tax model");
        assert_eq!(model.e05_strategies.len(), 2, "year 1 only and the even split");
        assert!(model.e05_strategies[1].total_tax.minor() <= model.e05_strategies[0].total_tax.minor(), "splitting is not worse on these brackets");
    });
}

// ----- Rules & taxes ---------------------------------------------------------------------------

#[gpui_kit::test]
fn rules_toggle_priority_and_simulate(cx: &mut TestAppContext) {
    let (handle, app) = open_app(cx, sample(Route::Rules));
    let window = handle.into();
    let rule = cx.update(|cx| app.read(cx).household().rules[0].id);
    let (enabled_before, priority_before, version_before) = cx.update(|cx| {
        let r = app.read(cx).household().rule(rule).unwrap();
        (r.enabled, r.priority, r.version)
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-rules").visible());
        let id: &'static str = &*Box::leak(format!("rule-enabled-{}", rule.raw()).into_boxed_str());
        scroll_to_and_click(window, "count-rules", id, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let r = app.read(cx).household().rule(rule).unwrap();
        assert_eq!(r.enabled, !enabled_before, "the switch flipped it");
        assert!(r.version > version_before, "and recorded a new version");
    });
    dismiss_toasts(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let id: &'static str = &*Box::leak(format!("rule-raise-{}", rule.raw()).into_boxed_str());
        scroll_to_and_click(window, "count-rules", id, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let r = app.read(cx).household().rule(rule).unwrap();
        assert_eq!(r.priority, priority_before + 1, "exactly plus one");
    });

    // The detail's simulation runs with and without the rule and applies nothing.
    go(cx, window, &app, Route::Rule(rule));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-rule").visible());
        window.within("rule-tabs").click(2usize, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(present(window, "rule-simulation"));
        scroll_to_and_click(window, "screen-rule", "run-rule-simulation", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let app = app.read(cx);
        let model = app.rules().expect("rules model");
        let simulation = model.simulation.as_ref().expect("the simulation ran");
        assert_eq!(simulation.rule, rule);
        assert_eq!(app.household().rule(rule).unwrap().enabled, !enabled_before, "simulating changed nothing");
    });
}

#[gpui_kit::test]
fn create_rule_flow_adds_an_enabled_first_version(cx: &mut TestAppContext) {
    let (handle, app) = open_app(cx, sample(Route::CreateRule));
    let window = handle.into();
    let before = cx.update(|cx| app.read(cx).household().rules.len());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-create-rule").visible());
        assert!(present(window, "rule-preview"), "the preview reads the rule in words");
        window.click("rule-name", cx);
        window.input("Weekend card fee", cx);
        scroll_to_and_click(window, "create-rule-stepper", "create-rule-next", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(present(window, "create-rule-trigger"));
        scroll_to_and_click(window, "create-rule-stepper", "add-condition", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        scroll_to_and_click(window, "create-rule-stepper", "condition-money-0", cx);
        window.input("1,000", cx);
        scroll_to_and_click(window, "create-rule-stepper", "create-rule-next", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(present(window, "create-rule-action"));
        scroll_to_and_click(window, "create-rule-stepper", "rule-percent", cx);
        window.input("2", cx);
        scroll_to_and_click(window, "create-rule-stepper", "create-rule-next", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(present(window, "create-rule-review"));
        scroll_to_and_click(window, "create-rule-stepper", "create-rule-commit", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.household().rules.len(), before + 1);
        let rule = app.household().rules.last().unwrap();
        assert_eq!(rule.name, "Weekend card fee");
        assert!(rule.enabled);
        assert_eq!(rule.version, 1);
        assert_eq!(rule.conditions.len(), 1, "the typed condition was kept");
        assert_eq!(app.route(), Route::Rule(rule.id), "the flow opens what it created");
    });
}

#[gpui_kit::test]
fn taxes_filter_events_and_packs_read_out_their_rules(cx: &mut TestAppContext) {
    let (handle, app) = open_app(cx, sample(Route::Taxes));
    let window = handle.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-taxes").visible());
        assert!(present(window, "tax-by-entity"));
        assert!(present(window, "tax-reserve"), "what falls due after the horizon is separate");
        assert!(present(window, "count-tax-events"));
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        let model = app.taxes().unwrap();
        assert!(!model.assessment.events.is_empty());
        assert!(model.assessment.events.iter().any(|e| e.cash_date > model.assessment.through), "the sample has an after-horizon event");
    });
    go(cx, window, &app, Route::TaxPacks);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-tax-packs").visible());
        assert!(present(window, "tax-packs"), "each pack reads out its rules");
    })
    .unwrap();
}

// ----- Sharing ------------------------------------------------------------------------------------

#[gpui_kit::test]
fn sharing_policies_grants_and_audit(cx: &mut TestAppContext) {
    let (handle, app) = open_app(cx, sample(Route::Policies));
    let window = handle.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-policies").visible());
        assert!(present(window, "policy-detail"), "the first policy is selected");
        assert!(present(window, "policy-sections"), "the nine aspects and the versions are here");
    })
    .unwrap();

    // A grant for one purpose, then its revocation, both audited.
    go(cx, window, &app, Route::Grants);
    let grants_before = cx.update(|cx| app.read(cx).household().grants.len());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-grants").visible());
        window.click("grant-access", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("grant-save", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let_dialog_settle();
    let granted = cx.update(|cx| {
        let app = app.read(cx);
        assert_eq!(app.household().grants.len(), grants_before + 1, "the grant was added");
        app.household().grants.last().unwrap().id
    });
    dismiss_toasts(cx, window);
    cx.update(|cx| app.update(cx, |app, cx| app.select_grant(granted, cx)));
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(present(window, "grant-detail"));
        app.update(cx, |app, cx| app.revoke_grant(granted, window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let app = app.read(cx);
        let grant = app.household().grants.iter().find(|g| g.id == granted).unwrap();
        assert_eq!(grant.revoked_on, Some(app.household().as_of), "revoking records a date and keeps the grant");
    });

    go(cx, window, &app, Route::Audit);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-audit").visible());
        assert!(present(window, "count-sharing-events"));
    })
    .unwrap();
    cx.update(|cx| {
        let app = app.read(cx);
        let model = app.privacy();
        assert!(model.audit.iter().any(|e| matches!(e.event.kind, atlas_core::authz::AuditKind::GrantAdded)));
        assert!(model.audit.iter().any(|e| matches!(e.event.kind, atlas_core::authz::AuditKind::GrantRevoked)));
    });
}

// ----- Every screen, both viewers -------------------------------------------------------------------

#[gpui_kit::test]
fn every_screen_renders_for_both_viewers(cx: &mut TestAppContext) {
    // Every route's root renders for the owner and for Person B, whatever it
    // may or may not disclose.
    for viewer in ['a', 'b'] {
        let launch = Launch { start: Start::Sample, viewer: Some(viewer), route: Route::Today, ..Launch::default() };
        let (handle, app) = open_app(cx, launch);
        let window: gpui_kit::AnyWindowHandle = handle.into();
        for slug in Route::slugs() {
            let Some(route) = Route::from_slug(slug) else { continue };
            if route == Route::Welcome {
                continue;
            }
            go(cx, window, &app, route);
            let root: &'static str = &*Box::leak(format!("screen-{}", route.slug()).into_boxed_str());
            cx.update_window(window, |_, window, cx| {
                window.render_frame(cx);
                assert!(window.try_find(root).is_some(), "{root} renders for viewer {viewer}");
            })
            .unwrap();
            cx.update(|cx| assert_eq!(app.read(cx).route(), route));
        }
    }
}

// ----- Lifecycle -------------------------------------------------------------------------------------

#[gpui_kit::test]
fn real_data_new_household_entry_save_and_reopen(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("atlas-ui-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("ours.atlas.sqlite");

    let launch = Launch { start: Start::Empty, route: Route::People, as_of: Some(fixtures::as_of()), owner: "tester".into(), ..Launch::default() };
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

    // 2. An account with an opening balance.
    dismiss_toasts(cx, window);
    go(cx, window, &app, Route::Accounts);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-accounts").visible());
        window.click("accounts-add-first", cx);
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
        assert_eq!(household.disclosure_for(app.viewer(), ObjectRef::Account(account)), Disclosure::Full, "the creator sees what they created");
        assert_eq!(app.overview().unwrap().money[0].calc.money(), atlas_core::Money::from_major(12_500, atlas_core::Currency::USD));
    });

    // 3. A monthly salary series.
    dismiss_toasts(cx, window);
    go(cx, window, &app, Route::Upcoming);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-upcoming").visible());
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
        assert_eq!(app.household().series[0].certainty, atlas_core::vocab::Certainty::Expected, "a new plan defaults to Expected");
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
        #[cfg(target_os = "macos")]
        window.press("cmd-a", cx);
        #[cfg(not(target_os = "macos"))]
        window.press("ctrl-a", cx);
        window.input(&path_text, cx);
        window.click("confirm-save-as", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(!app.is_saving(), "the background save has finished");
        assert!(!app.is_dirty());
        assert_eq!(app.file_path(), Some(path.clone()));
        assert!(app.file_state_text().starts_with("Saved"), "{}", app.file_state_text());
    });
    assert!(path.exists(), "the SQLite file was written");

    // 4b. The save runs off the UI thread: an edit made while the file is
    // being written is not lost — the household stays unsaved.
    let written_at = std::fs::metadata(&path).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    cx.update_window(window, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.save(window, cx);
            assert!(app.is_saving(), "the save is in flight");
            app.mark_dirty();
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let app = app.read(cx);
        assert!(!app.is_saving());
        assert!(app.is_dirty(), "the edit that landed during the save keeps the household unsaved");
    });
    assert!(std::fs::metadata(&path).unwrap().modified().unwrap() > written_at, "the second save wrote the file");

    // 5. Reopen from the file in a fresh app: same household, same figures.
    let launch = Launch { start: Start::File(path.clone()), owner: "tester".into(), take_over: true, viewer: Some('a'), ..Launch::default() };
    let (handle2, app2) = open_app(cx, launch);
    cx.update_window(handle2.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-today").visible());
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
fn a_real_household_purchase_starts_empty(cx: &mut TestAppContext) {
    // The sample may prefill its Car example; a real household must not open
    // with a large purchase in the wrong currency.
    let launch = Launch { start: Start::Empty, route: Route::Purchase, as_of: Some(fixtures::as_of()), owner: "tester".into(), ..Launch::default() };
    let (handle, app) = open_app(cx, launch);
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("screen-purchase").visible());
    })
    .unwrap();
    cx.update(|cx| {
        let plan = app.read(cx).decision_plan();
        assert!(plan.name.is_empty(), "no name is prefilled");
        assert!(plan.price.is_zero(), "no price is prefilled: {}", plan.price.format());
        assert!(plan.reserve.is_zero(), "the reserve is entered, never inferred");
        assert!(plan.financing.is_none());
    });
}

// ----- Performance ---------------------------------------------------------------------------------------

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
    // The sidebar and the content are separate cached views. A frame
    // re-renders only the views that were notified; the rest reuse their
    // previous layout and paint. `render_frame` refreshes the window (which
    // bypasses every cache), so the frames under test are drawn directly, and
    // a frame's figures are read once the next frame has closed it.
    let (handle, shell) = open_shell(cx, sample(Route::Today));
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
        move_pointer_to(window, None, "explain-free-cash", cx);
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

    // A state change the sidebar does not show (the earmarks boundary) leaves
    // it cached; one it does show (the destination) re-renders it.
    let renders = cx.update(|cx| {
        let renders = sidebar.read(cx).renders();
        app.update(cx, |app, cx| app.select_boundary(atlas_core::liquidity::Boundary::Person(fixtures::ids::PERSON_A), cx));
        renders
    });
    cx.update_window(window, |_, window, cx| {
        window.draw(cx).clear(cx);
        assert_eq!(sidebar.read(cx).renders(), renders, "a boundary change does not touch the sidebar");
        assert_eq!(sidebar.read(cx).snapshot().destination, Some(Destination::Today));
    })
    .unwrap();
    cx.update(|cx| app.update(cx, |app, cx| app.navigate(Route::People, cx)));
    cx.update_window(window, |_, window, cx| {
        window.draw(cx).clear(cx);
        assert_eq!(sidebar.read(cx).renders(), renders + 1, "navigation re-renders the sidebar");
        assert_eq!(sidebar.read(cx).snapshot().destination, Some(Destination::Household));
        assert!(window.try_find("screen-people").is_some(), "and the content shows the new screen");
    })
    .unwrap();
}

#[gpui_kit::test]
fn derived_models_are_computed_only_for_the_screen_in_use(cx: &mut TestAppContext) {
    // An edit drops every derived model; only the visible screen's is
    // computed again (on its next frame), the others when their screen opens.
    let (handle, app) = open_app(cx, sample(Route::Rules));
    let window: gpui_kit::AnyWindowHandle = handle.into();

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let app = app.read(cx);
        assert!(app.is_model_computed(Route::Rules), "the screen on show has its model");
        for route in [Route::Today, Route::Upcoming, Route::Taxes, Route::ForecastPath, Route::Scenarios] {
            assert!(!app.is_model_computed(route), "{} is not computed until its screen is opened", route.slug());
        }
    })
    .unwrap();

    // An edit on the Rules screen: the rules model comes back on the next
    // frame; the timeline stays uncomputed.
    let rule = cx.update(|cx| app.read(cx).household().rules[0].id);
    cx.update_window(window, |_, window, cx| {
        let id: &'static str = &*Box::leak(format!("rule-enabled-{}", rule.raw()).into_boxed_str());
        window.click(id, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let app = app.read(cx);
        assert!(app.is_model_computed(Route::Rules), "the frame computed the rules model again");
        assert!(!app.is_model_computed(Route::Upcoming), "the timeline was not recomputed for a rules edit");
        assert!(!app.is_model_computed(Route::Today));
    })
    .unwrap();

    // Opening Upcoming computes it then; asking for a model directly (as
    // forms and tests do) computes it too.
    cx.update(|cx| app.update(cx, |app, cx| app.navigate(Route::Upcoming, cx)));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(app.read(cx).is_model_computed(Route::Upcoming));
        assert!(!app.read(cx).is_model_computed(Route::Today));
        assert!(app.read(cx).overview().is_some(), "an accessor computes on demand");
        assert!(app.read(cx).is_model_computed(Route::Today));
    })
    .unwrap();
}

#[gpui_kit::test]
fn long_registers_scroll_without_losing_the_header(cx: &mut TestAppContext) {
    // The content column owns the scroll; a workspace header stays reachable
    // and the register below the fold renders.
    let (handle, _app) = open_app(cx, sample(Route::Upcoming));
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(present(window, "upcoming-list"));
        scroll_by(window, "upcoming-totals", 600., cx);
        assert!(window.find("upcoming-grid").visible(), "the virtualised grid is still painted after a scroll");
    })
    .unwrap();
}
