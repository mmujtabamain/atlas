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
        let free = overview.money.iter().find(|f| f.id == "free-cash").unwrap();
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
        let free = overview.money.iter().find(|f| f.id == "free-cash").unwrap();
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
