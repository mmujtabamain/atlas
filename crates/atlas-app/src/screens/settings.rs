//! Settings and diagnostics: household, session, file and launch facts; the
//! theme and the viewer are the only things changed here. Currency, name and
//! horizon are facts, not preferences.
//!
//! The facts are a borderless key/value grid — a settings page is a list of
//! plain facts, and a border around every cell makes it read as a
//! spreadsheet. Only the sections are ruled off from each other, and each
//! section's own commands sit at the trailing edge of its heading rather
//! than in a row of buttons underneath it.

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, Theme, ThemeMode,
    accordion::Accordion,
    alert::Alert,
    button::{Button, ButtonVariants as _},
    h_flex,
    radio::RadioGroup,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::alerting;
use crate::app::AtlasApp;
use crate::nav::Route;
use crate::widgets::facts::facts;
use crate::widgets::states::{action_bar, columns, note, page_header, section};

/// The label lane of a full-width or half-width pair.
const LANE: Pixels = px(150.);
/// The label lane of the four-up counts row, whose labels are all short.
const LANE_TIGHT: Pixels = px(120.);
/// The label lane of the launch-options list, whose labels are whole flags.
const LANE_FLAG: Pixels = px(280.);

pub fn render(app: &AtlasApp, cx: &mut Context<AtlasApp>) -> AnyElement {
    let theme = cx.theme();
    let is_dark = theme.is_dark();
    let household = app.household();
    let counts = app.disclosure_counts();
    let file_state = app.file_state_text();
    let path = app.file_path().map(|p| p.display().to_string());
    let lock = app.lock_text();
    let held = app.lock_holder.clone();
    let log_path = crate::logging::file_path_from_env().unwrap_or_else(|| std::path::PathBuf::from(crate::logging::DEFAULT_FILE)).display().to_string();
    let log_path_copy = log_path.clone();
    let launch_open = app.settings_launch_open;
    let path_copy = path.clone();
    v_flex()
        .id("screen-settings")
        .test_support()
        .w_full()
        .gap_8()
        .child(page_header("Settings", None, vec![], cx))
        // A file someone else has open is the one thing on this screen that
        // can be wrong, so it is stated once at the top as well as in the
        // Files section, where it is only one fact among several.
        .when_some(held, |this, (owner, since)| {
            this.child(
                Alert::warning("settings-lock-notice", format!("{owner} opened it {since}. Save as… writes a copy at a different path; the original file is never written from this session."))
                    .title("Viewing only — someone else has this file open"),
            )
        })
        .child(
            section("settings-household", "Household")
                .child(facts().id("settings-household-facts").lane(LANE).columns(2).pair("Name", household.name.clone()).pair("Base currency", format!("{} — one currency per household; no conversion", household.base_currency.code())).pair("Balances as of", household.as_of.format("%d %b %Y").to_string()).pair("Forecast through", app.horizon().format("%d %b %Y").to_string()))
                .child(facts().id("settings-count-facts").lane(LANE_TIGHT).columns(4).pair("Accounts", counts.accounts).pair("Planned series", counts.series).pair("Earmarks", counts.reservations).pair("Policies", counts.policies)),
        )
        .child(
            section("settings-session", "This session")
                .divider(true)
                .action(Button::new("settings-change-viewer").small().outline().icon(IconName::Eye).label("Change viewer…").on_click(cx.listener(|this, _, window, cx| this.open_viewer_picker(window, cx))))
                .action(Button::new("settings-tie-break").small().ghost().label("Change tie-break…").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Rules, cx))))
                .child(columns(vec![
                    // The theme sits in the same lane as the facts beside it,
                    // so the radio group lines up with "Person A · Owner".
                    // The heading already says the change is session-only.
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap_4()
                        .child(div().w(LANE).flex_shrink_0().text_sm().text_color(theme.muted_foreground).child("Theme"))
                        .child(
                            RadioGroup::horizontal("settings-theme")
                                .children(["Light", "Dark"])
                                .selected_index(Some(if is_dark { 1 } else { 0 }))
                                .on_change(|index, window, cx| {
                                    Theme::change(if *index == 1 { ThemeMode::Dark } else { ThemeMode::Light }, Some(window), cx);
                                }),
                        )
                        .into_any_element(),
                    facts()
                        .lane(LANE)
                        .pair("Who is looking", app.viewer_display_name_with_role())
                        .pair("Rule tie-break", household.rule_tie_break.label().to_string())
                        .into_any_element(),
                ])),
        )
        .child(
            section("settings-files", "Files")
                .divider(true)
                .action(Button::new("settings-save").small().outline().icon(IconName::Save).label(app.save_command_label()).on_click(cx.listener(|this, _, window, cx| this.save(window, cx))))
                .action(Button::new("settings-save-as").small().outline().label("Save as…").on_click(cx.listener(|this, _, window, cx| this.open_save_as(window, cx))))
                .action(Button::new("settings-copy-path").small().ghost().icon(IconName::Copy).label("Copy file path").on_click(move |_, window, cx| {
                    use gpui_kit::component::WindowExt as _;
                    match &path_copy {
                        Some(p) => {
                            cx.write_to_clipboard(ClipboardItem::new_string(p.clone()));
                            window.push_notification("File path copied.", cx);
                        }
                        None => window.push_notification("Not saved to a file yet.", cx),
                    }
                }))
                // One column: every value here is a path or a state sentence
                // that carries one, and a path is the last thing to squeeze
                // into half a row.
                .child(facts().id("settings-file-facts").lane(LANE).pair("File", path.clone().unwrap_or_else(|| "Not saved to a file yet".into())).pair("State", file_state).pair("Lock", lock).pair("Default folder", atlas_store::default_folder().display().to_string()))
                .child(note(format!("A copy of the previous file is kept in a backups folder next to it on every save; up to {} kept.", atlas_store::BACKUPS_KEPT), cx)),
        )
        .child(
            section("settings-diagnostics", "Diagnostics")
                .divider(true)
                .action(Button::new("settings-copy-log").small().ghost().icon(IconName::Copy).label("Copy log path").on_click(move |_, window, cx| {
                    use gpui_kit::component::WindowExt as _;
                    cx.write_to_clipboard(ClipboardItem::new_string(log_path_copy.clone()));
                    window.push_notification("Log path copied.", cx);
                }))
                .child(facts().id("settings-version-facts").lane(LANE).columns(2).pair("Version", format!("Atlas Financer {}", env!("CARGO_PKG_VERSION"))).pair("Frame-time readout", if app.perf_overlay() { "Overlay on; gpui's reading is in the status bar" } else { "Overlay off; gpui's reading is in the status bar" }))
                .child(facts().id("settings-log-facts").lane(LANE).pair("Log", log_path).pair("Failures", if alerting::is_configured() { "Posted to the team chat".to_string() } else { "Written to logs only — set DEVBENCH_NOTIFY_URL and DEVBENCH_NOTIFY_TOKEN in the launch environment to post them".to_string() }))
                // The accordion is controlled: without `on_toggle_click` the
                // header is inert, because the next render puts every item's
                // open state back the way this screen declared it.
                .child(Accordion::new("settings-launch").bordered(true).on_toggle_click(cx.listener(|this, open: &[usize], _, cx| {
                    this.settings_launch_open = open.contains(&0);
                    cx.notify();
                })).item(move |item| {
                    item.title("Launch options").open(launch_open).child(facts().id("settings-launch-facts").lane(LANE_FLAG).pair("--theme light|dark", "Initial theme; the toggle above changes this session only.").pair("--size WxH", "Initial window size; default 1600×1000, centred.").pair("--screen <area>", format!("Start surface; the original area names still work. Known: {}.", Route::slugs().join(", "))).pair("--viewer a|b|<person id>", "Who is looking, chosen before anything renders; otherwise the chooser opens.").pair("--household FILE | --new | --sample", "Open a file, start an empty household, or load the fictitious sample; with none, Welcome.").pair("--as-of YYYY-MM-DD", "Reconciliation date of a new household.").pair("--owner NAME", "The name written into the file lock; distinct from the viewer.").pair("--take-over", "Take over another owner's lock at launch; there is no in-app force unlock.").pair("--perf-overlay", "gpui's frame-time overlay (ATLAS_PERF_OVERLAY=1); the status bar keeps its reading either way."))
                })),
        )
        .child(action_bar(
            "settings-reference",
            vec![
                Button::new("settings-figure-meanings")
                    .small()
                    .outline()
                    .icon(IconName::BookOpen)
                    .label("Figure meanings…")
                    .on_click(cx.listener(|this, _, window, cx| this.open_figure_meanings(None, window, cx)))
                    .into_any_element(),
            ],
            vec![],
            cx,
        ))
        .into_any_element()
}
