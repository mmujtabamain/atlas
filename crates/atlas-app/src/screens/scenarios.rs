//! Decisions / Scenarios: the library of overlays with every change they
//! make, the compatibility of the chosen set, and the baseline-versus-set
//! comparison that accounts for the difference exactly.

use atlas_core::forecast::Case;
use atlas_core::ids::{ObjectRef, ScenarioId};
use atlas_core::model::Household;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    alert::Alert,
    button::{Button, ButtonVariants as _, DropdownButton},
    chart::AreaChart,
    checkbox::Checkbox,
    description_list::{DescriptionItem, DescriptionList},
    h_flex,
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
use crate::widgets::explain;
use crate::widgets::record::{self, Lane};
use crate::widgets::scope;
use crate::widgets::states::{count_line, empty_state, fact, lanes, note, section};

fn date(d: chrono::NaiveDate) -> String {
    d.format("%d %b %Y").to_string()
}

const LANES: [(&str, Lane); 4] = [("", Lane::fixed(40.)), ("Scenario", Lane::flex()), ("Privacy", Lane::fixed(150.)), ("Changes", Lane::fixed(110.))];

pub fn render_list(app: &AtlasApp, model: &ScenariosModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let compose_possible = model.selection.len() >= 2 && model.incompatibilities.is_empty();
    let header = workspace_header(
        Destination::Decisions,
        Route::Scenarios,
        vec![
            Button::new("new-scenario").small().outline().icon(IconName::Plus).label("Create scenario…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Scenario, window, cx))).into_any_element(),
            Button::new("scenario-compare").small().primary().icon(IconName::ChartLine).label("Compare selected").disabled(model.selection.is_empty()).tooltip(if model.selection.is_empty() { "Tick at least one scenario" } else { "Compare the ticked scenarios with the baseline" }).on_click(cx.listener(|this, _, _, cx| this.navigate(Route::ScenarioCompare, cx))).into_any_element(),
            Button::new("scenario-compose").small().outline().label("Compose…").disabled(!compose_possible).tooltip(if compose_possible { "Combine the ticked scenarios into a new one" } else { "Tick two or more compatible scenarios" }).on_click(cx.listener(|this, _, window, cx| this.open_compose_scenarios(window, cx))).into_any_element(),
        ],
        cx,
    );
    let inspected = app.scenario_detail.filter(|id| model.cards.iter().any(|c| c.id == *id)).or_else(|| model.cards.first().map(|c| c.id));
    let rows: Vec<_> = model
        .cards
        .iter()
        .map(|card| {
            let id = card.id;
            record::row(
                SharedString::from(format!("scenario-{}", id.raw())),
                inspected == Some(id),
                vec![
                    (LANES[0].1, h_flex().child(Checkbox::new(ElementId::Name(format!("scenario-select-{}", id.raw()).into())).checked(card.selected).on_change(cx.listener(move |this, checked, _, cx| this.select_scenario(id, *checked, cx)))).into_any_element()),
                    (LANES[1].1, record::stack(card.name.clone(), if card.description.is_empty() { "No description".to_string() } else { card.description.clone() }, cx)),
                    (LANES[2].1, h_flex().child(if card.private { Tag::warning().xsmall().outline().child("Private to its owner") } else { Tag::secondary().xsmall().outline().child("Shared") }).into_any_element()),
                    (LANES[3].1, record::muted(format!("{} change{}", card.overlay.len(), if card.overlay.len() == 1 { "" } else { "s" }), cx)),
                ],
                move |_, _, cx| crate::app::with_app(cx, |app, cx| app.inspect_scenario(id, cx)),
            )
        })
        .collect();
    let detail = inspected.and_then(|id| model.cards.iter().find(|c| c.id == id)).map(|card| render_detail(app, card, household, cx));
    let theme = cx.theme();
    let compatibility: AnyElement = if model.incompatibilities.is_empty() {
        div()
            .id("scenario-compatibility")
            .test_support()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(match model.selection.len() {
                0 => "Nothing selected for comparison.".to_string(),
                1 => "One scenario selected; tick another to check compatibility and compose.".to_string(),
                n => format!("{n} selected and compatible: no two changes touch the same series, company or tax rule."),
            })
            .into_any_element()
    } else {
        v_flex()
            .id("scenario-compatibility")
            .test_support()
            .gap_1()
            .children(model.incompatibilities.iter().map(|p| {
                h_flex().gap_2().items_center().child(Tag::danger().xsmall().outline().child("Incompatible")).child(div().text_sm().child(format!(
                    "{} × {}: {}",
                    household.scenario(p.first).map(|s| s.name.clone()).unwrap_or_default(),
                    household.scenario(p.second).map(|s| s.name.clone()).unwrap_or_default(),
                    p.reason
                )))
            }))
            .child(div().text_xs().text_color(theme.muted_foreground).child("A comparison can still run in the order selected; composing cannot."))
            .into_any_element()
    };
    let selected_names: Vec<String> = model.selection.iter().filter_map(|id| household.scenario(*id)).map(|s| s.name.clone()).collect();

    v_flex()
        .id("screen-scenarios")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(count_line(model.cards.len(), model.cards.len() + model.hidden_count, "scenarios", cx))
        .child(if model.cards.is_empty() {
            empty_state("scenarios-empty", "No scenarios yet", "A scenario is a set of changes laid over the baseline. Create one, then add its changes.", Some(Button::new("scenarios-add-first").small().outline().icon(IconName::Plus).label("Create scenario…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Scenario, window, cx))).into_any_element()), cx)
        } else {
            record::list("scenarios-list", record::header(&LANES, cx), rows).into_any_element()
        })
        .child(v_flex().gap_1().child(div().text_xs().text_color(theme.muted_foreground).child(format!("Selected for comparison: {}", if selected_names.is_empty() { "none".to_string() } else { selected_names.join(" + ") }))).child(compatibility))
        .children(detail)
        .into_any_element()
}

fn render_detail(app: &AtlasApp, card: &crate::models::scenarios::ScenarioCard, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let id = card.id;
    let viewer = app.viewer();
    let owner = household.policy_for(ObjectRef::Scenario(id)).is_some_and(|p| p.full_access.contains(&viewer.person));
    let members: Vec<String> = household.scenario(id).map(|s| s.composed_of.iter().filter_map(|m| household.scenario(*m)).map(|m| m.name.clone()).collect()).unwrap_or_default();
    let theme = cx.theme();
    section("scenario-detail", card.name.clone())
        .description(if card.description.is_empty() { "No description recorded.".to_string() } else { card.description.clone() })
        .action(
            h_flex()
                .gap_2()
                .child(Button::new("scenario-add-change").small().outline().icon(IconName::Plus).label("Add change…").on_click(cx.listener(move |this, _, window, cx| this.open_scenario_change_for(id, window, cx))))
                .child(Button::new("scenario-compare-this").small().ghost().icon(IconName::ChartLine).label("Compare this scenario").on_click(cx.listener(move |this, _, _, cx| this.compare_only(id, cx))))
                .child(
                    DropdownButton::new("scenario-detail-more")
                        .small()
                        .button(Button::new("scenario-detail-more-button").small().ghost().label("More"))
                        .dropdown_menu(move |menu, _, _| {
                            menu.item(PopupMenuItem::new("View policy").on_click(move |_, _, cx| crate::app::with_app(cx, |app, cx| app.open_policy_for(ObjectRef::Scenario(id), cx))))
                                .when(owner, |menu| menu.item(PopupMenuItem::new("Change policy…").on_click(move |_, window, cx| crate::app::with_app(cx, |app, cx| app.open_policy_editor_for(Some(ObjectRef::Scenario(id)), window, cx)))))
                        }),
                ),
        )
        .child(h_flex().gap_2().items_center().child(if card.private { Tag::warning().xsmall().outline().child("Private to its owner") } else { Tag::secondary().xsmall().outline().child("Shared with the household") }).child(div().text_xs().text_color(theme.muted_foreground).child(if card.private { "Only its owner sees it, in every screen." } else { "Every person in the household sees it." })))
        .when(!members.is_empty(), |this| this.child(v_flex().gap_0p5().child(div().text_xs().text_color(theme.muted_foreground).child("Composed of")).children(members.iter().map(|m| div().text_sm().child(m.clone())))))
        .child(
            v_flex()
                .w_full()
                .gap_1()
                .child(div().text_xs().text_color(theme.muted_foreground).child("Changes and tagged objects"))
                .children(card.overlay.iter().map(|entry| {
                    h_flex()
                        .w_full()
                        .gap_2()
                        .items_start()
                        .child(h_flex().w_40().flex_shrink_0().child(if entry.explicit { Tag::secondary().xsmall().outline().child(entry.kind) } else { Tag::secondary().xsmall().child(entry.kind) }))
                        .child(div().flex_1().min_w_0().text_sm().whitespace_normal().child(entry.text.clone()))
                }))
                .when(card.overlay.is_empty(), |this| this.child(note("Nothing overlaid yet: no explicit change, and no series, rule or assumption tagged with it.", cx))),
        )
        .child(note("A change cannot be edited or removed once added; the baseline is never touched.", cx))
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
    let verification: AnyElement = if comparison.attribution_verified {
        h_flex().gap_2().items_center().child(Tag::secondary().xsmall().outline().child("Sums exactly to the end difference")).child(div().text_xs().text_color(theme.muted_foreground).child(format!("Checked to the minor unit: {}", comparison.attribution_total.format_signed()))).into_any_element()
    } else {
        Alert::warning("attribution-discrepancy", format!("The attribution sums to {} but the end difference is {}. The comparison figures above stand; the breakdown does not.", comparison.attribution_total.format_signed(), comparison.end_delta.format_signed())).title("Difference check failed").into_any_element()
    };

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
                .child(lanes([
                    fact("Baseline at the end", comparison.baseline.end.money().format(), cx).into_any_element(),
                    fact("With the scenarios", comparison.overlaid.end.money().format(), cx).into_any_element(),
                    fact("Difference", comparison.end_delta.format_signed(), cx).into_any_element(),
                    fact("Lowest baseline", comparison.baseline.lowest.money().format(), cx).into_any_element(),
                    fact("Lowest with the scenarios", comparison.overlaid.lowest.money().format(), cx).into_any_element(),
                ]))
                .child(explain::render_preview(comparison.overlaid.end.node(), std::sync::Arc::new(explain::ExplainContent::new("Cash at the end with the scenarios", comparison.overlaid.end.money(), comparison.overlaid.end.shared_node(), household.entity_name(atlas_core::ids::EntityRef::Person(app.viewer().person)), atlas_core::Disclosure::Full)), cx)),
        )
        .child(section("comparison-chart", "Cash paths").child(chart::cash_path("comparison-path", &app.comparison_path_state, format!("Household · {} case · through {}", model.case.label(), date(model.through)), chart_el, values, legend, commands, readout, cx)))
        .child(v_flex().w_full().gap_2().child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Difference check")).child(verification))
        .child(
            v_flex()
                .w_full()
                .gap_4()
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
    section("comparison-metrics", "Metrics").child(record::list("comparison-metrics-list", record::header(&lanes_def, cx), rows)).into_any_element()
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
    section("comparison-difference", "Where the difference comes from")
        .description("Every posting of the window belongs to exactly one bucket — a series, the tax postings, the fee events or the starting cash — so nothing is counted twice and nothing hides.")
        .child(record::list("attribution-list", record::header(&lanes_def, cx), rows))
        .when_some(model.suppression_note.clone(), |this, note_text| this.child(h_flex().gap_2().items_center().child(Tag::warning().xsmall().outline().child("Breakdown suppressed")).child(div().id("scenario-suppression-note").test_support().text_xs().text_color(cx.theme().muted_foreground).child(note_text))))
        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(format!("End difference {} · attribution total {}", comparison.end_delta.format_signed(), comparison.attribution_total.format_signed())))
        .into_any_element()
}

fn render_comparison_basis(app: &AtlasApp, model: &ScenariosModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let overlays: Vec<AnyElement> = model
        .selection
        .iter()
        .filter_map(|id| model.cards.iter().find(|c| c.id == *id))
        .map(|card| {
            v_flex()
                .w_full()
                .gap_1()
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(card.name.clone()))
                .children(card.overlay.iter().map(|e| h_flex().gap_2().items_start().child(h_flex().w_40().flex_shrink_0().child(Tag::secondary().xsmall().outline().child(e.kind))).child(div().flex_1().min_w_0().text_sm().whitespace_normal().child(e.text.clone()))))
                .into_any_element()
        })
        .collect();
    let viewer = app.viewer();
    let assumptions: Vec<AnyElement> = household
        .assumptions
        .iter()
        .filter(|a| a.private_to.is_none_or(|p| p == viewer.person))
        .map(|a| h_flex().gap_2().items_center().child(div().text_sm().child(a.text.clone())).child(crate::widgets::labels::certainty_tag(a.certainty)).child(crate::widgets::labels::freshness_tag(a.freshness(household.as_of))).into_any_element())
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
        .child(v_flex().w_full().gap_3().children(overlays))
        .child(v_flex().w_full().gap_1().child(div().text_xs().text_color(cx.theme().muted_foreground).child("Assumptions behind both paths")).children(assumptions))
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
