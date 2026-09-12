//! Scenarios (§18, M8): overlays over the baseline, listed with every change
//! they make; composition with the compatibility check; baseline versus
//! scenario side by side with the §18.4 metrics; and the F139 difference
//! attribution, which sums to the end-of-window difference exactly.

use atlas_core::authz::Viewer;
use atlas_core::forecast::Case;
use atlas_core::ids::{ObjectRef, ScenarioId};
use atlas_core::liquidity::Boundary;
use atlas_core::model::Household;
use atlas_core::scenario::{AttributionLine, Incompatibility, OverlayEntry, ScenarioComparison, compare, project_attribution};
use atlas_core::{Disclosure, EngineResult};
use chrono::NaiveDate;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    alert::Alert,
    button::Button,
    chart::AreaChart,
    checkbox::Checkbox,
    group_box::GroupBox, h_flex,
    radio::RadioGroup,
    table::{Table, TableBody, TableCell, TableHead, TableHeader, TableRow},
    tag::Tag, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::AtlasApp;
use crate::widgets::master::page_header;
use crate::widgets::table::{money_cell, muted_cell, signed_money_cell};

/// One scenario as the viewer may see it (§18.5).
#[derive(Clone, Debug)]
pub struct ScenarioCard {
    pub id: ScenarioId,
    pub name: String,
    pub description: String,
    pub private: bool,
    pub selected: bool,
    pub overlay: Vec<OverlayEntry>,
}

#[derive(Clone, Debug)]
pub struct ComparisonPoint {
    pub label: SharedString,
    pub baseline: f64,
    pub scenario: f64,
}

#[derive(Clone, Debug)]
pub struct ScenariosModel {
    pub through: NaiveDate,
    pub case: Case,
    pub cards: Vec<ScenarioCard>,
    pub selection: Vec<ScenarioId>,
    pub hidden_count: usize,
    pub incompatibilities: Vec<Incompatibility>,
    pub comparison: Option<ScenarioComparison>,
    pub comparison_error: Option<String>,
    pub chart: Vec<ComparisonPoint>,
    /// The attribution as this viewer may see it (§7.6).
    pub attribution: Vec<AttributionLine>,
    pub suppression_note: Option<String>,
}

impl ScenariosModel {
    pub fn compute(household: &Household, viewer: Viewer, through: NaiveDate, case: Case, selection: &[ScenarioId]) -> EngineResult<Self> {
        // §18.5: a private scenario never appears in another person's list; a composition is
        // visible only when every member is.
        let visible = |id: ScenarioId| -> bool {
            let closure = household.scenario_closure(&[id]);
            closure.iter().all(|s| !matches!(household.disclosure_for(viewer, ObjectRef::Scenario(*s)), Disclosure::Hidden))
        };
        let cards: Vec<ScenarioCard> = household
            .scenarios
            .iter()
            .filter(|s| visible(s.id))
            .map(|s| ScenarioCard { id: s.id, name: s.name.clone(), description: s.description.clone(), private: s.private_to.is_some(), selected: selection.contains(&s.id), overlay: household.scenario_overlay(s.id) })
            .collect();
        let hidden_count = household.scenarios.len() - cards.len();
        let selection: Vec<ScenarioId> = selection.iter().copied().filter(|id| cards.iter().any(|c| c.id == *id)).collect();
        log::info!("scenarios: {} visible to {} ({hidden_count} hidden), comparing {selection:?} ({} case)", cards.len(), viewer.person, case.label());
        let incompatibilities = household.check_compatibility(&selection);
        let (comparison, comparison_error) = if selection.is_empty() {
            (None, None)
        } else {
            match compare(household, Boundary::Household, &selection, through, case) {
                Ok(c) => (Some(c), None),
                Err(err) => {
                    crate::alerting::report(crate::alerting::Level::Warning, format!("scenario comparison failed for {selection:?}: {err}"));
                    (None, Some(err.to_string()))
                }
            }
        };
        let (attribution, suppression_note) = match &comparison {
            Some(c) => project_attribution(household, viewer, &c.attribution, c.end_delta),
            None => (Vec::new(), None),
        };
        let per_major = 10f64.powi(household.base_currency.minor_digits() as i32);
        let chart = comparison
            .as_ref()
            .map(|c| c.merged_path.iter().map(|(date, base, over)| ComparisonPoint { label: SharedString::from(date.format("%d %b").to_string()), baseline: base.minor() as f64 / per_major, scenario: over.minor() as f64 / per_major }).collect())
            .unwrap_or_default();
        Ok(ScenariosModel { through, case, cards, selection, hidden_count, incompatibilities, comparison, comparison_error, chart, attribution, suppression_note })
    }
}

pub fn render(model: &ScenariosModel, household: &Household, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    v_flex()
        .id("screen-scenarios")
        .test_support()
        .w_full()
        .gap_6()
        .child(page_header(
            "Scenarios",
            "A scenario is an overlay over the baseline, never a separate database (§18): the events, rules and assumptions tagged with it plus explicit changes. Tick scenarios to compare them with the baseline; tick two or more to compose.",
            cx,
        ))
        .child(
            Alert::info("scenario-privacy", "Private scenarios are listed only for their owner and never leak through comparisons, lists or derived differences (§18.5). Comparison figures are conditional projections of one named case — scenario-tested, not a probability (§10.6).")
                .title("Overlay, privacy, one case at a time"),
        )
        .child(render_list(model, household, cx))
        .child(render_comparison(model, household, cx))
}

fn render_list(model: &ScenariosModel, household: &Household, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let cards: Vec<AnyElement> = model.cards.iter().enumerate().map(|(index, card)| render_card(index, card, cx).into_any_element()).collect();
    let theme = cx.theme();
    let compose_possible = model.selection.len() >= 2 && model.incompatibilities.is_empty();
    GroupBox::new().id("scenario-list").title(format!("Scenarios visible to you ({}{})", model.cards.len(), if model.hidden_count > 0 { format!(", {} private to someone else", model.hidden_count) } else { String::new() })).child(
        v_flex()
            .gap_4()
            .child(
                h_flex()
                    .justify_between()
                    .items_start()
                    .gap_4()
                    .child(div().flex_1().min_w_0().text_xs().text_color(theme.muted_foreground).child(
                        "Every line of an overlay is either an object tagged with the scenario (events, funding rules, assumptions) or an explicit change (§18.2). Composition members are listed first.",
                    ))
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .gap_2()
                            .child(Button::new("new-scenario").small().outline().icon(IconName::Plus).label("New scenario…").on_click(cx.listener(|this, _, window, cx| this.open_entry(crate::entry::Entry::Scenario, window, cx))))
                            .child(Button::new("scenario-add-change").small().outline().icon(IconName::Pencil).label("Add change…").on_click(cx.listener(|this, _, window, cx| this.open_scenario_change(window, cx))))
                            .child(
                                Button::new("scenario-compose")
                                    .small()
                                    .outline()
                                    .disabled(!compose_possible)
                                    .tooltip(if compose_possible { "Compose the ticked scenarios into a new one" } else { "Tick two or more compatible scenarios" })
                                    .label("Compose…")
                                    .on_click(cx.listener(|this, _, window, cx| this.open_compose_scenarios(window, cx))),
                            ),
                    ),
            )
            .child(if cards.is_empty() {
                div().text_sm().text_color(theme.muted_foreground).child("No scenario yet. Create one, then add changes or tag series with it.").into_any_element()
            } else {
                v_flex().gap_2().children(cards).into_any_element()
            })
            .child(if model.incompatibilities.is_empty() {
                div().id("scenario-compatibility").test_support().text_xs().text_color(theme.muted_foreground).child(match model.selection.len() {
                    0 => "Nothing selected.".to_string(),
                    1 => "One scenario selected; tick another to check compatibility and compose.".to_string(),
                    n => format!("{n} scenarios selected and compatible: no two changes touch the same series, company or tax rule (§18.3)."),
                }).into_any_element()
            } else {
                v_flex()
                    .id("scenario-compatibility")
                    .test_support()
                    .gap_1()
                    .children(model.incompatibilities.iter().map(|p| {
                        h_flex().gap_2().items_center().child(Tag::danger().xsmall().outline().child("incompatible")).child(div().text_sm().child(format!(
                            "{} × {}: {}",
                            household.scenario(p.first).map(|s| s.name.clone()).unwrap_or_default(),
                            household.scenario(p.second).map(|s| s.name.clone()).unwrap_or_default(),
                            p.reason
                        )))
                    }))
                    .into_any_element()
            }),
    )
}

fn render_card(index: usize, card: &ScenarioCard, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    let id = card.id;
    v_flex()
        .id(ElementId::Name(format!("scenario-card-{}", id.raw()).into()))
        .test_support()
        .gap_2()
        .px_3()
        .py_2()
        .rounded(theme.radius)
        .when(index % 2 == 1, |c| c.bg(theme.table_even))
        .when(card.selected, |c| c.border_1().border_color(theme.primary))
        .child(
            h_flex()
                .gap_3()
                .items_center()
                .flex_wrap()
                .child(
                    Checkbox::new(ElementId::Name(format!("scenario-select-{}", id.raw()).into()))
                        .label(card.name.clone())
                        .checked(card.selected)
                        .on_change(cx.listener(move |this, checked, _, cx| this.select_scenario(id, *checked, cx))),
                )
                .when(card.private, |row| row.child(Tag::warning().xsmall().outline().child("private to its owner (§18.5)")))
                .child(Tag::secondary().xsmall().outline().child(format!("{} change{}", card.overlay.len(), if card.overlay.len() == 1 { "" } else { "s" })))
                .child(div().text_xs().text_color(theme.muted_foreground).child(card.id.to_string())),
        )
        .child(div().text_xs().text_color(theme.muted_foreground).child(card.description.clone()))
        .children(card.overlay.iter().map(|entry| {
            h_flex()
                .gap_2()
                .items_start()
                .pl_6()
                .child(h_flex().w_32().flex_shrink_0().child(if entry.explicit { Tag::secondary().xsmall().outline().child(entry.kind) } else { Tag::secondary().xsmall().child(entry.kind) }))
                .child(div().flex_1().min_w_0().text_sm().child(entry.text.clone()))
        }))
}

fn render_comparison(model: &ScenariosModel, household: &Household, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    let names = model.selection.iter().filter_map(|id| household.scenario(*id)).map(|s| format!("“{}”", s.name)).collect::<Vec<_>>().join(" + ");
    GroupBox::new().id("scenario-comparison").title(format!("Baseline versus {} — household cash through {} (§18.4)", if names.is_empty() { "scenario".to_string() } else { names }, model.through.format("%d %b %Y"))).child(
        v_flex()
            .gap_4()
            .child(
                h_flex().flex_wrap().gap_6().items_end().child(
                    v_flex().gap_1().child(div().text_xs().text_color(theme.muted_foreground).child("Named case (§10.6)")).child(
                        RadioGroup::horizontal("scenario-case")
                            .children(Case::ALL.iter().map(|c| c.label()))
                            .selected_index(Some(Case::ALL.iter().position(|c| *c == model.case).unwrap_or(1)))
                            .on_change(cx.listener(|this, index: &usize, _, cx| this.set_scenario_case(Case::ALL[(*index).min(2)], cx))),
                    ),
                ),
            )
            .child(match (&model.comparison, &model.comparison_error) {
                (Some(comparison), _) => render_comparison_body(comparison, model, household, cx).into_any_element(),
                (None, Some(err)) => Alert::error("scenario-comparison-error", err.clone()).title("The comparison could not be computed").into_any_element(),
                (None, None) => div().text_sm().text_color(theme.muted_foreground).child("Tick a scenario to compare it with the baseline.").into_any_element(),
            }),
    )
}

fn render_comparison_body(comparison: &ScenarioComparison, model: &ScenariosModel, household: &Household, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    let baseline_color = theme.chart_1;
    let scenario_color = theme.chart_2;
    let background = theme.background;
    let points = model.chart.clone();
    let tick_margin = (points.len() / 8).max(1);
    let _ = household;
    v_flex()
        .gap_4()
        .child(
            h_flex()
                .gap_4()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(h_flex().gap_1().items_center().child(div().size_2().rounded_full().bg(baseline_color)).child("Baseline"))
                .child(h_flex().gap_1().items_center().child(div().size_2().rounded_full().bg(scenario_color)).child("With the scenario")),
        )
        .child(
            div().h_64().w_full().child(
                AreaChart::new(points)
                    .x(|p: &ComparisonPoint| p.label.clone())
                    .y(|p: &ComparisonPoint| p.baseline)
                    .stroke(baseline_color)
                    .fill(linear_gradient(0., linear_color_stop(baseline_color.opacity(0.25), 1.), linear_color_stop(background.opacity(0.05), 0.)))
                    .name("Baseline")
                    .y(|p: &ComparisonPoint| p.scenario)
                    .stroke(scenario_color)
                    .fill(linear_gradient(0., linear_color_stop(scenario_color.opacity(0.15), 1.), linear_color_stop(background.opacity(0.0), 0.)))
                    .name("Scenario")
                    .tick_margin(tick_margin)
                    .id("scenario-area-chart"),
            ),
        )
        .child(div().id("scenario-summary").test_support().text_sm().child(format!(
            "End of window {} with the scenario vs {} baseline → {}. Lowest point {} vs {}.",
            comparison.overlaid.end.money().format(),
            comparison.baseline.end.money().format(),
            comparison.end_delta.format_signed(),
            comparison.overlaid.lowest.money().format(),
            comparison.baseline.lowest.money().format()
        )))
        .child(
            Table::new()
                .child(
                    TableHeader::new().child(
                        TableRow::new()
                            .child(TableHead::new().w_64().flex_shrink_0().child("Metric (§18.4)"))
                            .child(TableHead::new().flex_1().min_w_0().text_right().child("Baseline"))
                            .child(TableHead::new().flex_1().min_w_0().text_right().child("Scenario"))
                            .child(TableHead::new().w_40().flex_shrink_0().text_right().child("Difference"))
                            .child(TableHead::new().w_80().flex_shrink_0().child("Note")),
                    ),
                )
                .child(TableBody::new().children(comparison.metrics.iter().enumerate().map(|(index, row)| {
                    let money = row.delta.is_some();
                    let value_cell = |text: String| {
                        let cell = TableCell::new().flex_1().min_w_0().text_right();
                        if money { cell.font_family(theme.mono_font_family.clone()).child(text) } else { cell.text_xs().child(text) }
                    };
                    TableRow::new()
                        .when(index % 2 == 1, |r| r.bg(theme.table_even))
                        .child(TableCell::new().w_64().flex_shrink_0().child(row.name.clone()))
                        .child(value_cell(row.baseline.clone()))
                        .child(value_cell(row.scenario.clone()))
                        .child(match row.delta {
                            Some(delta) => signed_money_cell(delta, cx).w_40().flex_shrink_0(),
                            None => muted_cell("–", cx).w_40().flex_shrink_0().text_right(),
                        })
                        .child(muted_cell(row.note.clone(), cx).w_80().flex_shrink_0().text_xs())
                }))),
        )
        .child(
            v_flex()
                .gap_2()
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Where the difference comes from (F139)"))
                        .child(if comparison.attribution_verified {
                            Tag::secondary().xsmall().outline().child(format!("sums to the end difference exactly: {}", comparison.attribution_total.format_signed()))
                        } else {
                            Tag::danger().xsmall().outline().child(format!("does not sum: {} vs {}", comparison.attribution_total.format_signed(), comparison.end_delta.format_signed()))
                        })
                        .when_some(model.suppression_note.clone(), |row, note| row.child(Tag::warning().xsmall().outline().child("breakdown suppressed (§7.6)")).child(div().id("scenario-suppression-note").test_support().text_xs().text_color(theme.muted_foreground).child(note))),
                )
                .child(div().text_xs().text_color(theme.muted_foreground).child(
                    "Every posting of the window belongs to exactly one bucket (a series, tax postings, fee events, or the starting cash), so nothing is counted twice and nothing hides. Values are the household's share of each account.",
                ))
                .child(
                    Table::new()
                        .child(
                            TableHeader::new().child(
                                TableRow::new()
                                    .child(TableHead::new().w_32().flex_shrink_0().child("Kind"))
                                    .child(TableHead::new().min_w_0().child("Bucket"))
                                    .child(TableHead::new().w_40().flex_shrink_0().text_right().child("Baseline"))
                                    .child(TableHead::new().w_40().flex_shrink_0().text_right().child("Scenario"))
                                    .child(TableHead::new().w_40().flex_shrink_0().text_right().child("Difference")),
                            ),
                        )
                        .child(TableBody::new().children(model.attribution.iter().enumerate().map(|(index, line)| {
                            TableRow::new()
                                .when(index % 2 == 1, |r| r.bg(theme.table_even))
                                .child(TableCell::new().w_32().flex_shrink_0().child(h_flex().child(Tag::secondary().xsmall().outline().child(line.kind))))
                                .child(TableCell::new().min_w_0().overflow_hidden().text_ellipsis().child(line.label.clone()))
                                .child(money_cell(line.baseline, cx).w_40().flex_shrink_0())
                                .child(money_cell(line.scenario, cx).w_40().flex_shrink_0())
                                .child(signed_money_cell(line.delta, cx).w_40().flex_shrink_0())
                        }))),
                ),
        )
}
