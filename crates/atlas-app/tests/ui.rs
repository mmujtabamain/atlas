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
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, Entity, TestAppContext, px, size};

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
