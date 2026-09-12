//! Settings and diagnostics: household, session, file and launch facts; the
//! theme and the viewer are the only things changed here. Currency, name and
//! horizon are facts, not preferences.

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, Theme, ThemeMode,
    accordion::Accordion,
    button::{Button, ButtonVariants as _},
    description_list::{DescriptionItem, DescriptionList},
    h_flex,
    radio::RadioGroup,
    v_flex,
};
use gpui_kit::*;

use crate::alerting;
use crate::app::AtlasApp;
use crate::nav::Route;
use crate::widgets::states::{page_header, section};

pub fn render(app: &AtlasApp, cx: &mut Context<AtlasApp>) -> AnyElement {
    let theme = cx.theme();
    let is_dark = theme.is_dark();
    let household = app.household();
    let counts = app.disclosure_counts();
    let file_state = app.file_state_text();
    let path = app.file_path().map(|p| p.display().to_string());
    let lock = app.lock_text();
    let log_path = crate::logging::file_path_from_env().unwrap_or_else(|| std::path::PathBuf::from(crate::logging::DEFAULT_FILE)).display().to_string();
    let log_path_copy = log_path.clone();
    let path_copy = path.clone();
    v_flex()
        .id("screen-settings")
        .test_support()
        .w_full()
        .gap_8()
        .child(page_header("Settings", None, vec![], cx))
        .child(
            section("settings-household", "Household").child(
                DescriptionList::new()
                    .columns(2)
                    .child(DescriptionItem::new("Name").value(household.name.clone()))
                    .child(DescriptionItem::new("Currency").value(format!("{} — one currency per household; no conversion", household.base_currency.code())))
                    .child(DescriptionItem::new("Balances as of").value(household.as_of.format("%d %b %Y").to_string()))
                    .child(DescriptionItem::new("Forecast through").value(app.horizon().format("%d %b %Y").to_string()))
                    .child(DescriptionItem::new("Accounts").value(counts.accounts))
                    .child(DescriptionItem::new("Planned series").value(counts.series))
                    .child(DescriptionItem::new("Earmarks").value(counts.reservations))
                    .child(DescriptionItem::new("Policies").value(counts.policies)),
            ),
        )
        .child(
            section("settings-session", "This session")
                .child(
                    v_flex().gap_2().child(div().text_xs().text_color(theme.muted_foreground).child("Theme — this session only")).child(
                        RadioGroup::horizontal("settings-theme")
                            .children(["Light", "Dark"])
                            .selected_index(Some(if is_dark { 1 } else { 0 }))
                            .on_change(|index, window, cx| {
                                Theme::change(if *index == 1 { ThemeMode::Dark } else { ThemeMode::Light }, Some(window), cx);
                            }),
                    ),
                )
                .child(
                    DescriptionList::new()
                        .columns(2)
                        .child(DescriptionItem::new("Who is looking").value(app.viewer_display_name_with_role()))
                        .child(DescriptionItem::new("Rule tie-break").value(household.rule_tie_break.label().to_string())),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(Button::new("settings-change-viewer").small().outline().icon(IconName::Eye).label("Change viewer…").on_click(cx.listener(|this, _, window, cx| this.open_viewer_picker(window, cx))))
                        .child(Button::new("settings-tie-break").small().ghost().label("Change tie-break…").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Rules, cx)))),
                ),
        )
        .child(
            section("settings-files", "Files")
                .child(
                    DescriptionList::new()
                        .columns(1)
                        .child(DescriptionItem::new("File").value(path.clone().unwrap_or_else(|| "Not saved to a file yet".into())))
                        .child(DescriptionItem::new("State").value(file_state))
                        .child(DescriptionItem::new("Lock").value(lock))
                        .child(DescriptionItem::new("Default folder").value(atlas_store::default_folder().display().to_string()))
                        .child(DescriptionItem::new("Backups").value(format!("A copy of the previous file is kept in a `backups` folder next to it on every save; up to {} kept.", atlas_store::BACKUPS_KEPT))),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(Button::new("settings-save").small().outline().icon(IconName::Save).label(app.save_command_label()).on_click(cx.listener(|this, _, window, cx| this.save(window, cx))))
                        .child(Button::new("settings-save-as").small().outline().label("Save as…").on_click(cx.listener(|this, _, window, cx| this.open_save_as(window, cx))))
                        .child(Button::new("settings-copy-path").small().ghost().icon(IconName::Copy).label("Copy file path").on_click(move |_, window, cx| {
                            use gpui_kit::component::WindowExt as _;
                            match &path_copy {
                                Some(p) => {
                                    cx.write_to_clipboard(ClipboardItem::new_string(p.clone()));
                                    window.push_notification("File path copied.", cx);
                                }
                                None => window.push_notification("Not saved to a file yet.", cx),
                            }
                        })),
                ),
        )
        .child(
            section("settings-diagnostics", "Diagnostics")
                .child(
                    DescriptionList::new()
                        .columns(2)
                        .child(DescriptionItem::new("Version").value(format!("Atlas Financer {}", env!("CARGO_PKG_VERSION"))))
                        .child(DescriptionItem::new("Failures").value(if alerting::is_configured() { "Posted to the team chat".to_string() } else { "Written to logs only — set DEVBENCH_NOTIFY_URL and DEVBENCH_NOTIFY_TOKEN in the launch environment to post them".to_string() }))
                        .child(DescriptionItem::new("Log").value(log_path))
                        .child(DescriptionItem::new("Frame-time readout").value(if app.perf_overlay() { "Overlay on; gpui's reading is in the status bar".to_string() } else { "Overlay off; gpui's reading is in the status bar".to_string() })),
                )
                .child(
                    h_flex().gap_2().child(Button::new("settings-copy-log").small().ghost().icon(IconName::Copy).label("Copy log path").on_click(move |_, window, cx| {
                        use gpui_kit::component::WindowExt as _;
                        cx.write_to_clipboard(ClipboardItem::new_string(log_path_copy.clone()));
                        window.push_notification("Log path copied.", cx);
                    })),
                )
                .child(
                    Accordion::new("settings-launch").bordered(true).item(|item| {
                        item.title("Launch options").child(
                            DescriptionList::new()
                                .columns(1)
                                .child(DescriptionItem::new("--theme light|dark").value("Initial theme; the toggle above changes this session only."))
                                .child(DescriptionItem::new("--size WxH").value("Initial window size; default 1600×1000, centred."))
                                .child(DescriptionItem::new("--screen <area>").value(format!("Start surface; the original area names still work. Known: {}.", Route::slugs().join(", "))))
                                .child(DescriptionItem::new("--viewer a|b|<person id>").value("Who is looking, chosen before anything renders; otherwise the chooser opens."))
                                .child(DescriptionItem::new("--household FILE | --new | --sample").value("Open a file, start an empty household, or load the fictitious sample; with none, Welcome."))
                                .child(DescriptionItem::new("--as-of YYYY-MM-DD").value("Reconciliation date of a new household."))
                                .child(DescriptionItem::new("--owner NAME").value("The name written into the file lock; distinct from the viewer."))
                                .child(DescriptionItem::new("--take-over").value("Take over another owner's lock at launch; there is no in-app force unlock."))
                                .child(DescriptionItem::new("--perf-overlay").value("gpui's frame-time overlay (ATLAS_PERF_OVERLAY=1); the status bar keeps its reading either way.")),
                        )
                    }),
                ),
        )
        .child(h_flex().child(Button::new("settings-figure-meanings").small().outline().icon(IconName::BookOpen).label("Figure meanings…").on_click(cx.listener(|this, _, window, cx| this.open_figure_meanings(None, window, cx)))))
        .into_any_element()
}
