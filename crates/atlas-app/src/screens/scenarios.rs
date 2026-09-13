//! Decisions / Scenarios: the library of overlays with every change they
//! make, the compatibility of the chosen set, and the baseline-versus-set
//! comparison that accounts for the difference exactly.
//!
//! The library is a master–detail: the scenarios on the left with the tick
//! that puts one in the comparison, the inspected scenario's changes beside
//! them. A register whose detail sits *under* it pushes the changes below the
//! fold as soon as the household has a handful of scenarios, and reads as two
//! unrelated bands rather than as one thing and its contents.

use std::sync::Arc;

use atlas_core::forecast::Case;
use atlas_core::ids::{EntityRef, ObjectRef, ScenarioId};
use atlas_core::model::Household;
use atlas_core::{Calc, Disclosure, Money};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    alert::Alert,
    button::{Button, ButtonVariants as _, DropdownButton},
    chart::AreaChart,
    checkbox::Checkbox,
    description_list::{DescriptionItem, DescriptionList},
    h_flex,
    list::ListItem,
    menu::PopupMenuItem,
    tab::{Tab, TabBar},
    tag::Tag,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::common::{detail_header, workspace_header};
use crate::app::AtlasApp;
use crate::entry::Entry;
use crate::models::scenarios::{ComparisonPoint, ScenariosModel};
use crate::nav::{Destination, Route};
use crate::widgets::chart::{self, Legend, PathCommand};
use crate::widgets::copy::copy_button;
use crate::widgets::explain::{self, ExplainContent};
use crate::widgets::figure::Figure;
use crate::widgets::master::master_detail;
use crate::widgets::record::{self, Lane};
use crate::widgets::scope;
use crate::widgets::states::{action_bar, columns, count_line, empty_state, fact, hairline, info_card, note, section};

fn date(d: chrono::NaiveDate) -> String {
    d.format("%d %b %Y").to_string()
}

fn change_count(n: usize) -> String {
    format!("{n} change{}", if n == 1 { "" } else { "s" })
}

/// One overlay line: the change on its own full-width row, its kind and where
/// it came from on the line beneath.
///
/// The kind is a `Tag` and the change is a long sentence; the two must not
/// share a wrap row (`docs/perf.md` §3.3) — taffy re-measures the sentence for
/// the tag's row at every ancestor pass, which is what made this screen the
/// second most expensive in the app.
fn overlay_row(entry: &atlas_core::scenario::OverlayEntry, cx: &App) -> AnyElement {
    let theme = cx.theme();
    v_flex()
        .w_full()
        .gap_1()
        .py_2()
        .border_b_1()
        .border_color(theme.border)
        .child(div().w_full().text_sm().whitespace_normal().child(entry.text.clone()))
        .child(
            h_flex()
                .w_full()
                .gap_2()
                .items_center()
                .child(if entry.explicit { Tag::secondary().xsmall().outline().child(entry.kind) } else { Tag::secondary().xsmall().child(entry.kind) })
                .child(div().text_xs().text_color(theme.muted_foreground).child(if entry.explicit { "Explicit change" } else { "Tagged with this scenario" })),
        )
        .into_any_element()
}

/// The standing verdict on the ticked set. Compatible is a *fact* about the
/// selection and gets a bordered card beside the changes it is about; a
/// conflict is something wrong and gets an alert across the whole screen,
/// above the list, where it cannot be mistaken for a property of whichever
/// scenario happens to be inspected.
fn compatibility(model: &ScenariosModel, household: &Household, cx: &App) -> AnyElement {
    if model.incompatibilities.is_empty() {
        let (title, body) = match model.selection.len() {
            0 => ("Nothing selected for comparison".to_string(), "Tick a scenario to compare it with the baseline; tick two or more to combine them.".to_string()),
            1 => ("One scenario selected".to_string(), "Tick another to check compatibility and compose.".to_string()),
            n => (format!("{n} selected and compatible"), "No two changes touch the same series, company or tax rule.".to_string()),
        };
        info_card("scenario-compatibility", IconName::GitBranch, title, body, cx)
    } else {
        let pairs = model
            .incompatibilities
            .iter()
            .map(|p| {
                format!(
                    "{} × {}: {}",
                    household.scenario(p.first).map(|s| s.name.clone()).unwrap_or_default(),
                    household.scenario(p.second).map(|s| s.name.clone()).unwrap_or_default(),
                    p.reason
                )
            })
            .collect::<Vec<_>>()
            .join(" · ");
        div()
            .id("scenario-compatibility")
            .test_support()
            .w_full()
            .child(Alert::warning("scenario-conflicts", format!("{pairs}. A comparison can still run in the order selected; composing cannot.")).title("The selected scenarios contain conflicting changes"))
            .into_any_element()
    }
}

pub fn render_list(app: &AtlasApp, model: &ScenariosModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let compose_possible = model.selection.len() >= 2 && model.incompatibilities.is_empty();
    // `Compose…` leaves the title bar: it acts on the ticked set, so it
    // belongs under the ticks, not beside the commands of the whole screen.
    let header = workspace_header(
        Destination::Decisions,
        Route::Scenarios,
        vec![
            Button::new("new-scenario").small().outline().icon(IconName::Plus).label("Create scenario…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Scenario, window, cx))).into_any_element(),
            Button::new("scenario-compare").small().primary().icon(IconName::ChartLine).label("Compare selected").disabled(model.selection.is_empty()).tooltip(if model.selection.is_empty() { "Tick at least one scenario" } else { "Compare the ticked scenarios with the baseline" }).on_click(cx.listener(|this, _, _, cx| this.navigate(Route::ScenarioCompare, cx))).into_any_element(),
        ],
        cx,
    );
    let inspected = app.scenario_detail.filter(|id| model.cards.iter().any(|c| c.id == *id)).or_else(|| model.cards.first().map(|c| c.id));

    // The master list: the tick that includes a scenario in the comparison and
    // the row that inspects it are two different gestures on one row, so the
    // checkbox leads and the rest of the row selects.
    let rows: Vec<ListItem> = model
        .cards
        .iter()
        .map(|card| {
            let id = card.id;
            let meta = format!("{} · {}", change_count(card.overlay.len()), if card.private { "Private to its owner" } else { "Shared" });
            ListItem::new(ElementId::Name(format!("scenario-{}", id.raw()).into()))
                .selected(inspected == Some(id))
                .on_click(cx.listener(move |this, _, _, cx| this.inspect_scenario(id, cx)))
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap_3()
                        .py_1()
                        .child(
                            div().flex_shrink_0().child(
                                Checkbox::new(ElementId::Name(format!("scenario-select-{}", id.raw()).into()))
                                    .checked(card.selected)
                                    .on_change(cx.listener(move |this, checked, _, cx| this.select_scenario(id, *checked, cx))),
                            ),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(div().text_sm().overflow_hidden().text_ellipsis().child(card.name.clone()))
                                .child(div().text_xs().text_color(muted).overflow_hidden().text_ellipsis().child(meta)),
                        ),
                )
        })
        .collect();

    // Under the list, in the same panel: what is ticked, in the order it will
    // be compared, and the one command that acts on it.
    let selected_names: Vec<String> = model.selection.iter().filter_map(|id| model.cards.iter().find(|c| c.id == *id)).map(|c| c.name.clone()).collect();
    let selection_block = v_flex()
        .id("scenario-selection")
        .test_support()
        .w_full()
        .gap_2()
        .child(div().w_full().text_xs().text_color(muted).child("Selected for comparison"))
        .child(if selected_names.is_empty() {
            note("Nothing ticked yet.", cx).into_any_element()
        } else {
            v_flex().w_full().children(selected_names.iter().enumerate().map(|(i, name)| div().w_full().text_sm().overflow_hidden().text_ellipsis().child(format!("{}. {name}", i + 1)))).into_any_element()
        })
        .child(
            h_flex().w_full().child(
                Button::new("scenario-compose")
                    .small()
                    .outline()
                    .label("Compose…")
                    .disabled(!compose_possible)
                    .tooltip(if compose_possible { "Combine the ticked scenarios into a new one" } else { "Tick two or more compatible scenarios" })
                    .on_click(cx.listener(|this, _, window, cx| this.open_compose_scenarios(window, cx))),
            ),
        )
        .child(note("Tick two or more compatible scenarios to combine them.", cx));

    let master = v_flex()
        .w_full()
        .gap_4()
        .child(v_flex().id("scenarios-list").w_full().gap_0p5().children(rows))
        .child(hairline(cx))
        .child(selection_block);

    // One verdict element, placed by what it says: the alert above everything,
    // the card at the foot of the detail it sits beside.
    let incompatible = !model.incompatibilities.is_empty();
    let verdict = compatibility(model, household, cx);
    let mut verdict = Some(verdict);
    let banner = if incompatible { verdict.take() } else { None };

    let detail = inspected.and_then(|id| model.cards.iter().find(|c| c.id == id)).map(|card| {
        v_flex().w_full().gap_6().child(render_detail(app, card, household, cx)).children(verdict.take()).into_any_element()
    });

    v_flex()
        .id("screen-scenarios")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(count_line(model.cards.len(), model.cards.len() + model.hidden_count, "scenarios", cx))
        .children(banner)
        .child(if model.cards.is_empty() {
            empty_state("scenarios-empty", "No scenarios yet", "A scenario is a set of changes laid over the baseline. Create one, then add its changes.", Some(Button::new("scenarios-add-first").small().outline().icon(IconName::Plus).label("Create scenario…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Scenario, window, cx))).into_any_element()), cx)
        } else {
            master_detail("scenarios-master-detail", master, detail.unwrap_or_else(|| div().into_any_element()), cx).into_any_element()
        })
        // With no scenario there is no detail pane to carry the verdict, and
        // the id must not disappear with it.
        .children(verdict)
        .into_any_element()
}

fn render_detail(app: &AtlasApp, card: &crate::models::scenarios::ScenarioCard, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let id = card.id;
    let viewer = app.viewer();
    let owner = household.policy_for(ObjectRef::Scenario(id)).is_some_and(|p| p.full_access.contains(&viewer.person));
    let members: Vec<String> = household.scenario(id).map(|s| s.composed_of.iter().filter_map(|m| household.scenario(*m)).map(|m| m.name.clone()).collect()).unwrap_or_default();
    let theme = cx.theme();
    let muted = theme.muted_foreground;

    // `Add change…` and `Compare this scenario` act on the scenario; `View
    // policy` and `Change policy…` lead somewhere else, so they sit on the
    // leading side of the bar.
    let mut leading: Vec<AnyElement> = vec![
        Button::new("scenario-view-policy")
            .small()
            .ghost()
            .label("View policy")
            .on_click(cx.listener(move |this, _, _, cx| this.open_policy_for(ObjectRef::Scenario(id), cx)))
            .into_any_element(),
    ];
    if owner {
        leading.push(
            DropdownButton::new("scenario-detail-more")
                .small()
                .button(Button::new("scenario-detail-more-button").small().ghost().label("More"))
                .dropdown_menu(move |menu, _, _| menu.item(PopupMenuItem::new("Change policy…").on_click(move |_, window, cx| crate::app::with_app(cx, |app, cx| app.open_policy_editor_for(Some(ObjectRef::Scenario(id)), window, cx)))))
                .into_any_element(),
        );
    }
    let trailing: Vec<AnyElement> = vec![
        Button::new("scenario-compare-this").small().ghost().icon(IconName::ChartLine).label("Compare this scenario").on_click(cx.listener(move |this, _, _, cx| this.compare_only(id, cx))).into_any_element(),
        Button::new("scenario-add-change").small().outline().icon(IconName::Plus).label("Add change…").on_click(cx.listener(move |this, _, window, cx| this.open_scenario_change_for(id, window, cx))).into_any_element(),
    ];

    section("scenario-detail", card.name.clone())
        // The privacy state beside the name, where the mockup puts it: it
        // identifies the scenario rather than describing what it does.
        .badge(if card.private { "Private to its owner" } else { "Shared with the household" })
        .description(if card.description.is_empty() { "No description recorded.".to_string() } else { card.description.clone() })
        // A composition is its members first: it is visible only when they all are.
        .when(!members.is_empty(), |this| {
            this.child(
                v_flex()
                    .w_full()
                    .gap_0p5()
                    .child(div().w_full().text_xs().text_color(muted).child("Composed of"))
                    .children(members.iter().map(|m| div().w_full().text_sm().child(m.clone()))),
            )
        })
        .child(
            v_flex()
                .w_full()
                .gap_1()
                .child(
                    h_flex()
                        .w_full()
                        .justify_between()
                        .items_center()
                        .gap_4()
                        .child(div().text_xs().text_color(muted).child("Changes and tagged objects"))
                        .child(div().flex_shrink_0().text_xs().text_color(muted).child(change_count(card.overlay.len()))),
                )
                .child(hairline(cx))
                .children(card.overlay.iter().map(|entry| overlay_row(entry, cx)))
                .when(card.overlay.is_empty(), |this| this.child(div().w_full().py_2().child(note("Nothing overlaid yet: no explicit change, and no series, rule or assumption tagged with it.", cx)))),
        )
        .child(note(
            if card.private { "Only its owner sees it, in every screen. A change cannot be edited or removed once added; the baseline is never touched." } else { "Every person in the household sees it. A change cannot be edited or removed once added; the baseline is never touched." },
            cx,
        ))
        .child(action_bar("scenario-detail-actions", leading, trailing, cx))
        .into_any_element()
}

/// A figure of the comparison, with its `ⓘ` and its vocabulary terms.
fn comparison_figure(id: &'static str, label: &'static str, calc: &Calc<Money>, viewer_name: &str, cx: &App) -> AnyElement {
    let _ = cx;
    let content = Arc::new(ExplainContent::new(label, calc.money(), calc.shared_node(), viewer_name, Disclosure::Full));
    Figure::new(id, label, calc, content).into_any_element()
}

/// The end difference. The engine gives a subtraction of two figures, not a
/// chain of its own, so this is not a [`Figure`] and does not pretend to be
/// one: it says where it comes from instead of offering a calculation there
/// is no record of.
fn difference_cell(delta: Money, cx: &App) -> AnyElement {
    let theme = cx.theme();
    v_flex()
        .w_full()
        .gap_1()
        .child(div().w_full().text_xs().text_color(theme.muted_foreground).child("End difference"))
        .child(
            div()
                .id("comparison-end-delta")
                .test_support()
                .font_family(theme.mono_font_family.clone())
                .font_weight(FontWeight::SEMIBOLD)
                .text_xl()
                .when(delta.is_negative(), |d| d.text_color(theme.danger))
                .child(delta.format_signed()),
        )
        .child(div().w_full().text_xs().text_color(theme.muted_foreground).child("With the scenarios, less the baseline"))
        .into_any_element()
}

pub fn render_comparison(app: &AtlasApp, model: &ScenariosModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let names: Vec<String> = model.selection.iter().filter_map(|id| household.scenario(*id)).map(|s| s.name.clone()).collect();
    let header = detail_header(
        Destination::Decisions,
        Route::Scenarios,
        "Scenarios",
        "Comparison",
        Some(div().text_sm().text_color(cx.theme().muted_foreground).child(format!("Household · {} · through {}", if names.is_empty() { "no scenario selected".to_string() } else { names.join(" + ") }, date(model.through))).into_any_element()),
        vec![
            Button::new("comparison-choose").small().outline().label("Choose scenarios…").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Scenarios, cx))).into_any_element(),
            Button::new("comparison-compose").small().ghost().label("Compose…").disabled(model.selection.len() < 2 || !model.incompatibilities.is_empty()).on_click(cx.listener(|this, _, window, cx| this.open_compose_scenarios(window, cx))).into_any_element(),
        ],
        cx,
    );
    let Some(comparison) = &model.comparison else {
        return v_flex()
            .id("screen-compare")
            .test_support()
            .w_full()
            .gap_6()
            .child(header)
            .child(match &model.comparison_error {
                Some(err) => v_flex().gap_2().child(Alert::error("comparison-error", err.clone()).title("The comparison could not be computed")).child(h_flex().child(Button::new("comparison-retry").small().outline().label("Retry").on_click(cx.listener(|this, _, _, cx| this.invalidate_scenarios(cx))))).into_any_element(),
                None => empty_state("comparison-empty", "No scenario selected", "Tick at least one scenario to compare it with the baseline. A baseline-to-baseline difference of zero is not a scenario outcome.", Some(Button::new("comparison-choose-2").small().outline().label("Choose scenarios…").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Scenarios, cx))).into_any_element()), cx),
            })
            .into_any_element();
    };
    let tab = app.comparison_tab;
    let report: AnyElement = match tab {
        1 => render_difference(app, model, comparison, cx),
        2 => render_comparison_basis(app, model, household, cx),
        _ => render_metrics(comparison, cx),
    };
    let points = model.chart.clone();
    let lowest_index = points.iter().enumerate().min_by(|(_, a), (_, b)| a.scenario.total_cmp(&b.scenario)).map(|(i, _)| i);
    let commands = lowest_index.map(|i| PathCommand { id: "comparison-show-lowest", label: "Show lowest point", select: i }).into_iter().collect();
    let selected = app.comparison_path_state.read(cx).selected.and_then(|i| comparison.merged_path.get(i).map(|p| (i, p)));
    let readout = selected.map(|(i, (d, base, over))| {
        chart::readout(
            format!("Selected: {} · point {} of {}", date(*d), i + 1, comparison.merged_path.len()),
            vec![fact("Baseline", base.format(), cx).into_any_element(), fact("With the scenarios", over.format(), cx).into_any_element(), fact("Difference", (*over - *base).format_signed(), cx).into_any_element()],
            cx,
        )
    });
    let theme = cx.theme();
    let (c1, c2) = (theme.chart_1, theme.chart_2);
    let legend = vec![Legend { name: "Baseline".into(), color: c1 }, Legend { name: "With the scenarios".into(), color: c2 }];
    let tick_margin = (points.len() / 8).max(1);
    let chart_el = div()
        .h_64()
        .w_full()
        .child(
            AreaChart::new(points)
                .id("comparison-area-chart")
                .x(|p: &ComparisonPoint| p.label.clone())
                .y(|p: &ComparisonPoint| p.baseline)
                .stroke(c1)
                .fill(c1.opacity(0.0))
                .name("Baseline")
                .step_after()
                .y(|p: &ComparisonPoint| p.scenario)
                .stroke(c2)
                .fill(c2.opacity(0.08))
                .name("With the scenarios")
                .step_after()
                .tick_margin(tick_margin),
        )
        .into_any_element();
    let values_lanes: [(&str, Lane); 4] = [("Date", Lane::fixed(130.)), ("Baseline", Lane::money(170.)), ("With the scenarios", Lane::money(190.)), ("Difference", Lane::money(150.))];
    const MAX_VALUES: usize = 40;
    let state = app.comparison_path_state.clone();
    let value_rows: Vec<_> = comparison
        .merged_path
        .iter()
        .enumerate()
        .take(MAX_VALUES)
        .map(|(i, (d, base, over))| {
            let state = state.clone();
            record::row(
                SharedString::from(format!("comparison-value-{i}")),
                selected.is_some_and(|(s, _)| s == i),
                vec![
                    (values_lanes[0].1, record::text(date(*d))),
                    (values_lanes[1].1, record::money(*base, cx)),
                    (values_lanes[2].1, record::money(*over, cx)),
                    (values_lanes[3].1, record::muted((*over - *base).format_signed(), cx)),
                ],
                move |_, _, cx| {
                    state.update(cx, |s, cx| {
                        s.selected = Some(i);
                        cx.notify();
                    })
                },
            )
        })
        .collect();
    let values = v_flex()
        .w_full()
        .gap_1()
        .child(record::list("comparison-values", record::header(&values_lanes, cx), value_rows))
        .when(comparison.merged_path.len() > MAX_VALUES, |this| this.child(div().text_xs().text_color(theme.muted_foreground).child(format!("{} more points after these.", comparison.merged_path.len() - MAX_VALUES))))
        .into_any_element();
    // The sum check is a standing fact about the breakdown, so it is a card
    // and not a tag after a heading; a check that failed is an alert, because
    // then something is actually wrong.
    let verification: AnyElement = if comparison.attribution_verified {
        info_card("comparison-verification", IconName::ShieldCheck, "Sums exactly to the end difference", format!("Checked to the minor unit: {}", comparison.attribution_total.format_signed()), cx)
    } else {
        Alert::warning("attribution-discrepancy", format!("The attribution sums to {} but the end difference is {}. The comparison figures above stand; the breakdown does not.", comparison.attribution_total.format_signed(), comparison.end_delta.format_signed())).title("Difference check failed").into_any_element()
    };
    let viewer_name = household.entity_name(EntityRef::Person(app.viewer().person));

    v_flex()
        .id("screen-compare")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(scope::bar(
            vec![scope::select("Case", &app.comparison_case_choice, px(180.), cx).into_any_element(), scope::fixed("Through", date(model.through), cx).into_any_element()],
            Some("Both paths are computed in the same case over the same window.".into()),
            cx,
        ))
        .when(!model.incompatibilities.is_empty(), |this| {
            this.child(Alert::warning("comparison-incompatible", model.incompatibilities.iter().map(|p| format!("{} × {}: {}", household.scenario(p.first).map(|s| s.name.clone()).unwrap_or_default(), household.scenario(p.second).map(|s| s.name.clone()).unwrap_or_default(), p.reason)).collect::<Vec<_>>().join(" · ")).title("Incompatible selection — compared in the order selected"))
        })
        .child(
            section("comparison-figures", "Baseline against the selection")
                // Five figures as one even grid across the width: they are five
                // readings of one comparison, not one answer and four supports.
                .child(columns([
                    comparison_figure("comparison-baseline-end", "Baseline at the end", &comparison.baseline.end, &viewer_name, cx),
                    comparison_figure("comparison-overlaid-end", "With the scenarios", &comparison.overlaid.end, &viewer_name, cx),
                    difference_cell(comparison.end_delta, cx),
                    comparison_figure("comparison-baseline-lowest", "Lowest baseline", &comparison.baseline.lowest, &viewer_name, cx),
                    comparison_figure("comparison-overlaid-lowest", "Lowest with the scenarios", &comparison.overlaid.lowest, &viewer_name, cx),
                ]))
                .child(hairline(cx))
                // The one-line equation behind the overlaid end, not the whole
                // chain as an inline table: the terms of the terms are what
                // `Full calculation…` opens.
                .child(explain::render_equation(
                    "comparison-end-equation",
                    comparison.overlaid.end.node(),
                    Arc::new(ExplainContent::new("Cash at the end with the scenarios", comparison.overlaid.end.money(), comparison.overlaid.end.shared_node(), viewer_name.clone(), Disclosure::Full)),
                    cx,
                )),
        )
        .child(section("comparison-chart", "Cash paths").divider(true).child(chart::cash_path("comparison-path", &app.comparison_path_state, format!("Household · {} case · through {}", model.case.label(), date(model.through)), chart_el, values, legend, commands, readout, cx)))
        .child(
            section("comparison-report", "The comparison in detail")
                .divider(true)
                .child(verification)
                .child(
                    TabBar::new("comparison-tabs")
                        .selected_index(tab)
                        .on_click(cx.listener(|this, index: &usize, _, cx| {
                            this.comparison_tab = *index;
                            cx.notify();
                        }))
                        .children([Tab::new().label("Metrics"), Tab::new().label("Difference"), Tab::new().label("Basis")]),
                )
                .child(report),
        )
        .into_any_element()
}

fn render_metrics(comparison: &atlas_core::scenario::ScenarioComparison, cx: &mut Context<AtlasApp>) -> AnyElement {
    let lanes_def: [(&str, Lane); 5] = [("Metric", Lane::fixed(240.)), ("Baseline", Lane::fixed(180.)), ("With the scenarios", Lane::fixed(180.)), ("Difference", Lane::money(150.)), ("Note", Lane::flex())];
    let rows: Vec<_> = comparison
        .metrics
        .iter()
        .enumerate()
        .map(|(i, row)| {
            record::row(
                SharedString::from(format!("comparison-metric-{i}")),
                false,
                vec![
                    (lanes_def[0].1, record::text(row.name.clone())),
                    (lanes_def[1].1, record::text(row.baseline.clone())),
                    (lanes_def[2].1, record::text(row.scenario.clone())),
                    (lanes_def[3].1, match row.delta {
                        Some(delta) => record::muted(delta.format_signed(), cx),
                        None => record::muted("—", cx),
                    }),
                    (lanes_def[4].1, record::muted(row.note.clone(), cx)),
                ],
                |_, _, _| {},
            )
        })
        .collect();
    v_flex().id("comparison-metrics").test_support().w_full().child(record::list("comparison-metrics-list", record::header(&lanes_def, cx), rows)).into_any_element()
}

fn render_difference(app: &AtlasApp, model: &ScenariosModel, comparison: &atlas_core::scenario::ScenarioComparison, cx: &mut Context<AtlasApp>) -> AnyElement {
    let lanes_def: [(&str, Lane); 5] = [("Kind", Lane::fixed(140.)), ("Bucket", Lane::flex()), ("Baseline", Lane::money(160.)), ("With the scenarios", Lane::money(180.)), ("Difference", Lane::money(150.))];
    let rows: Vec<_> = model
        .attribution
        .iter()
        .enumerate()
        .map(|(i, line)| {
            record::row(
                SharedString::from(format!("attribution-{i}")),
                false,
                vec![
                    (lanes_def[0].1, h_flex().child(Tag::secondary().xsmall().outline().child(line.kind)).into_any_element()),
                    (lanes_def[1].1, record::text(line.label.clone())),
                    (lanes_def[2].1, record::money(line.baseline, cx)),
                    (lanes_def[3].1, record::money(line.scenario, cx)),
                    (lanes_def[4].1, record::muted(line.delta.format_signed(), cx)),
                ],
                |_, _, _| {},
            )
        })
        .collect();
    let _ = app;
    let theme = cx.theme();
    v_flex()
        .id("comparison-difference")
        .test_support()
        .w_full()
        .gap_2()
        .child(div().w_full().text_xs().text_color(theme.muted_foreground).child("Every posting of the window belongs to exactly one bucket — a series, the tax postings, the fee events or the starting cash — so nothing is counted twice and nothing hides."))
        .child(record::list("attribution-list", record::header(&lanes_def, cx), rows))
        .when_some(model.suppression_note.clone(), |this, note_text| this.child(h_flex().gap_2().items_center().child(Tag::warning().xsmall().outline().child("Breakdown suppressed")).child(div().id("scenario-suppression-note").test_support().text_xs().text_color(theme.muted_foreground).child(note_text))))
        .child(div().w_full().text_xs().text_color(theme.muted_foreground).child(format!("End difference {} · attribution total {}", comparison.end_delta.format_signed(), comparison.attribution_total.format_signed())))
        .into_any_element()
}

fn render_comparison_basis(app: &AtlasApp, model: &ScenariosModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let theme = cx.theme();
    let overlays: Vec<AnyElement> = model
        .selection
        .iter()
        .filter_map(|id| model.cards.iter().find(|c| c.id == *id))
        .map(|card| {
            v_flex()
                .w_full()
                .gap_1()
                .child(div().w_full().text_sm().font_weight(FontWeight::MEDIUM).child(card.name.clone()))
                .children(card.overlay.iter().map(|e| overlay_row(e, cx)))
                .into_any_element()
        })
        .collect();
    let viewer = app.viewer();
    // The sentence gets its own full-width row and the tags the line beneath:
    // a sentence beside a tag in a wrap row is re-measured at every sizing
    // pass (`docs/perf.md` §3.3).
    let assumptions: Vec<AnyElement> = household
        .assumptions
        .iter()
        .filter(|a| a.private_to.is_none_or(|p| p == viewer.person))
        .map(|a| {
            v_flex()
                .w_full()
                .gap_1()
                .py_2()
                .border_b_1()
                .border_color(theme.border)
                .child(div().w_full().text_sm().whitespace_normal().child(a.text.clone()))
                .child(h_flex().w_full().gap_2().items_center().child(crate::widgets::labels::certainty_tag(a.certainty)).child(crate::widgets::labels::freshness_tag(a.freshness(household.as_of))))
                .into_any_element()
        })
        .collect();
    let packs: Vec<String> = household.tax_packs.iter().map(|p| format!("{} ({})", p.name, if p.verified { "verified" } else { "unverified" })).collect();
    let text = format!(
        "Scenario comparison\nHousehold · {} case · through {}\nSelection: {}\nEnd difference {}\nAttribution {}\n",
        model.case.label(),
        date(model.through),
        model.selection.iter().filter_map(|id| household.scenario(*id)).map(|s| s.name.clone()).collect::<Vec<_>>().join(" + "),
        model.comparison.as_ref().map(|c| c.end_delta.format_signed()).unwrap_or_default(),
        model.comparison.as_ref().map(|c| if c.attribution_verified { "sums exactly".to_string() } else { "does not sum".to_string() }).unwrap_or_default()
    );
    section("comparison-basis", "Basis")
        .description("What both paths used, for this viewer.")
        .action(copy_button("copy-comparison", "Copy comparison", text))
        // The overlays and what both paths assumed side by side: two lists of
        // short lines read as one band across the width instead of one column
        // that leaves the right of the window empty.
        .child(columns([
            v_flex().w_full().gap_3().child(div().w_full().text_xs().text_color(theme.muted_foreground).child("Changes these scenarios make")).children(overlays).into_any_element(),
            v_flex().w_full().gap_1().child(div().w_full().text_xs().text_color(theme.muted_foreground).child("Assumptions behind both paths")).children(assumptions).into_any_element(),
        ]))
        .child(DescriptionList::new().columns(1).child(DescriptionItem::new("Tax packs").value(packs.join(" · "))).child(DescriptionItem::new("Case").value(format!("{} — {}", model.case.label(), model.case.description()))).child(DescriptionItem::new("Window").value(format!("{} through {}", date(household.as_of), date(model.through)))))
        .into_any_element()
}

impl AtlasApp {
    pub fn inspect_scenario(&mut self, id: ScenarioId, cx: &mut Context<Self>) {
        self.scenario_detail = Some(id);
        cx.notify();
    }

    /// `Compare this scenario`: the selection becomes just this one.
    pub fn compare_only(&mut self, id: ScenarioId, cx: &mut Context<Self>) {
        self.scenario_selection = vec![id];
        self.scenario_detail = Some(id);
        self.invalidate_scenarios(cx);
        self.navigate(Route::ScenarioCompare, cx);
    }

    /// The change sheet with `scenario` preselected.
    pub fn open_scenario_change_for(&mut self, scenario: ScenarioId, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(row) = self.scenario_forms.scenario_row(scenario) {
            Self::set_choice(&self.scenario_forms.scenario, row, window, cx);
        }
        self.open_scenario_change(window, cx);
    }

    /// The comparison case, from the retained choice.
    pub fn set_comparison_case(&mut self, case: Case, cx: &mut Context<Self>) {
        self.set_scenario_case(case, cx);
    }
}
