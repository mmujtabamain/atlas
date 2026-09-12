//! Projections (§11, §2.1, §2.4, §10.6): the chronological forecast of a
//! boundary under a named case — path chart against the hard floor, the
//! §2.1 chain, lowest balance and breaches, per-account paths and transfer
//! points, the assumptions the path depends on, and the §11.4 record that
//! makes the run reproducible.

use atlas_core::authz::Viewer;
use atlas_core::forecast::{BoundaryForecast, Case, ForecastOptions, forecast};
use atlas_core::ids::{EntityRef, ObjectRef, ScenarioId};
use atlas_core::liquidity::Boundary;
use atlas_core::model::Household;
use atlas_core::{Disclosure, EngineResult};
use chrono::NaiveDate;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _,
    chart::AreaChart,
    checkbox::Checkbox,
    description_list::{DescriptionItem, DescriptionList},
    group_box::GroupBox, h_flex,
    tab::{Tab, TabBar},
    table::{Table, TableBody, TableCell, TableHead, TableHeader, TableRow},
    tag::Tag, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::AtlasApp;
use crate::widgets::explain;
use crate::widgets::figure::{ExplainedFigure, card};
use crate::widgets::labels;
use crate::widgets::master::page_header;
use crate::widgets::table::{money_cell, muted_cell};

/// One chart sample: the boundary balance after a posting instant.
#[derive(Clone, Debug)]
pub struct ChartPoint {
    pub label: SharedString,
    pub balance: f64,
    pub floor: f64,
}

#[derive(Clone, Debug)]
pub struct ProjectionModel {
    pub forecast: BoundaryForecast,
    pub boundaries: Vec<Boundary>,
    pub start: ExplainedFigure,
    pub end: ExplainedFigure,
    pub lowest: ExplainedFigure,
    pub injection: ExplainedFigure,
    pub chart: Vec<ChartPoint>,
}

impl ProjectionModel {
    pub fn compute(household: &Household, viewer: Viewer, boundary: Boundary, case: Case, scenario: Option<ScenarioId>, through: NaiveDate) -> EngineResult<Self> {
        log::info!("computing projection {:?} {:?} scenario {:?} through {}", boundary, case, scenario, through);
        let mut boundaries = vec![Boundary::Household];
        boundaries.extend(household.people.iter().map(|p| Boundary::Person(p.id)));
        boundaries.extend(
            household
                .companies
                .iter()
                .filter(|c| matches!(household.disclosure_for(viewer, ObjectRef::Company(c.id)), Disclosure::Full | Disclosure::SelectedFields))
                .map(|c| Boundary::Company(c.id)),
        );
        let boundary = if boundaries.contains(&boundary) { boundary } else { Boundary::Household };
        let result = forecast(household, boundary, ForecastOptions { through, scenario, case })?;
        let slug = boundary.slug();
        let per_major = household.base_currency.minor_per_major() as f64;
        let floor = result.floor.minor() as f64 / per_major;
        let chart = result
            .path
            .iter()
            .map(|p| ChartPoint { label: SharedString::from(p.date.format("%d %b").to_string()), balance: p.balance.minor() as f64 / per_major, floor })
            .collect();
        Ok(ProjectionModel {
            start: ExplainedFigure::new(format!("{slug}-proj-start"), "Reconciled starting cash", &result.start, household, viewer),
            end: ExplainedFigure::new(format!("{slug}-proj-end"), "Conditional projected cash at horizon", &result.end, household, viewer),
            lowest: ExplainedFigure::new(format!("{slug}-proj-lowest"), "Lowest projected cash", &result.lowest, household, viewer),
            injection: ExplainedFigure::new(format!("{slug}-proj-injection"), "Minimum immediate injection K*", &result.breach.minimum_injection, household, viewer),
            chart,
            boundaries,
            forecast: result,
        })
    }
}

pub fn render(model: &ProjectionModel, household: &Household, viewer: Viewer, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    let viewer_name = household.entity_name(EntityRef::Person(viewer.person));
    let f = &model.forecast;
    let boundary_index = model.boundaries.iter().position(|b| *b == f.boundary).unwrap_or(0);
    let boundaries = model.boundaries.clone();
    let case_index = Case::ALL.iter().position(|c| *c == f.case).unwrap_or(1);
    let scenario_name = f.scenario.and_then(|id| household.scenario(id)).map(|s| s.name.clone());

    v_flex()
        .id("screen-projections")
        .test_support()
        .w_full()
        .gap_6()
        .child(page_header(
            "Projections",
            format!(
                "Chronological forecast from {} through {} (§11.1): reconciled cash plus every signed posting in date and intraday order. Future money is conditional, never available (§2.4).",
                f.as_of.format("%d %b %Y"),
                f.through.format("%d %b %Y")
            ),
            cx,
        ))
        .child(
            h_flex()
                .flex_wrap()
                .gap_6()
                .items_end()
                .child(
                    v_flex().gap_1().child(div().text_xs().text_color(theme.muted_foreground).child("Boundary (§11.2)")).child(
                        TabBar::new("projection-boundaries")
                            .selected_index(boundary_index)
                            .on_click(cx.listener(move |this, index: &usize, _, cx| {
                                if let Some(boundary) = boundaries.get(*index) {
                                    this.select_projection_boundary(*boundary, cx);
                                }
                            }))
                            .children(model.boundaries.iter().map(|b| Tab::new().label(b.label(household)))),
                    ),
                )
                .child(
                    v_flex().gap_1().child(div().text_xs().text_color(theme.muted_foreground).child("Named case (§10.6)")).child(
                        TabBar::new("projection-cases")
                            .selected_index(case_index)
                            .on_click(cx.listener(|this, index: &usize, _, cx| {
                                if let Some(case) = Case::ALL.get(*index) {
                                    this.select_projection_case(*case, cx);
                                }
                            }))
                            .children(Case::ALL.iter().map(|c| Tab::new().label(c.label()))),
                    ),
                )
                .child(
                    Checkbox::new("projection-buy-car")
                        .label("Overlay scenario “Buy car” (§18)")
                        .checked(f.scenario.is_some())
                        .on_change(cx.listener(|this, checked, _, cx| this.set_projection_scenario(*checked, cx))),
                ),
        )
        .child(
            GroupBox::new()
                .id("projection-figures")
                .title(match &scenario_name {
                    Some(name) => format!("{} — {} case, with scenario “{name}”", f.boundary.label(household), f.case.label()),
                    None => format!("{} — {} case ({})", f.boundary.label(household), f.case.label(), f.case.description()),
                })
                .child(
                    v_flex()
                        .gap_4()
                        .child(
                            h_flex()
                                .flex_wrap()
                                .gap_8()
                                .child(card(model.start.figure(&viewer_name, false)))
                                .child(card(model.end.figure(&viewer_name, true)))
                                .child(
                                    card(
                                        v_flex().gap_1().child(model.lowest.figure(&viewer_name, false)).child(
                                            div().text_xs().text_color(theme.muted_foreground).child(match f.lowest_date {
                                                Some(date) => format!("on {}", date.format("%d %b %Y")),
                                                None => "never below the start".to_string(),
                                            }),
                                        ),
                                    ),
                                )
                                .child(card(model.injection.figure(&viewer_name, false))),
                        )
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .flex_wrap()
                                .child(labels::strength_tag(atlas_core::ResultStrength::ScenarioTested))
                                .child(div().text_xs().text_color(theme.muted_foreground).child(
                                    "Three named cases are three explicit paths, not an uncertainty envelope and not a confidence level (§10.6, V032). Robust and probabilistic labels require their own methods (M08–M12).",
                                )),
                        ),
                ),
        )
        .child(render_chart(model, household, cx))
        .child(
            GroupBox::new().id("projection-runway").title(format!("Runway against the {} hard floor (M13)", f.floor.format())).child(
                v_flex()
                    .gap_2()
                    .child(div().id("projection-runway-summary").test_support().text_sm().child(f.breach.summary()))
                    .child(div().text_xs().text_color(theme.muted_foreground).child(format!(
                        "Days below the floor {} · integrated shortfall {} currency-days (severity over time, not cash — E08).",
                        f.breach.days_below, f.breach.integrated_shortfall_currency_days
                    ))),
            ),
        )
        .child(render_accounts(model, household, cx))
        .child(render_transfer_points(model, household, cx))
        .child(
            GroupBox::new().id("projection-chain").title("Chain as the plan lays it out (§2.1)").child(explain::render_top_block(model.end.calc.node(), cx)),
        )
        .child(render_assumptions(model, cx))
        .child(render_record(model, household, cx))
}

fn render_chart(model: &ProjectionModel, household: &Household, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let balance_color = theme.chart_1;
    let floor_color = theme.danger;
    let background = theme.background;
    let points = model.chart.clone();
    let tick_margin = (points.len() / 8).max(1);
    GroupBox::new().id("projection-chart").title("Path: boundary balance after every posting, against the hard floor").child(
        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .gap_4()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(h_flex().gap_1().items_center().child(div().size_2().rounded_full().bg(balance_color)).child(format!("{} cash", model.forecast.boundary.label(household))))
                    .child(h_flex().gap_1().items_center().child(div().size_2().rounded_full().bg(floor_color)).child("Hard floor")),
            )
            .child(
                div().h_64().w_full().child(
                    AreaChart::new(points)
                        .x(|p: &ChartPoint| p.label.clone())
                        .y(|p: &ChartPoint| p.balance)
                        .stroke(balance_color)
                        .fill(linear_gradient(0., linear_color_stop(balance_color.opacity(0.3), 1.), linear_color_stop(background.opacity(0.1), 0.)))
                        .name("Cash")
                        .y(|p: &ChartPoint| p.floor)
                        .stroke(floor_color)
                        .fill(linear_gradient(0., linear_color_stop(floor_color.opacity(0.05), 1.), linear_color_stop(background.opacity(0.0), 0.)))
                        .name("Hard floor")
                        .tick_margin(tick_margin)
                        .id("projection-area-chart"),
                ),
            )
            .child(div().text_xs().text_color(theme.muted_foreground).child(
                "Chart values are the exact minor-unit balances shown in major units; the chain and figures remain exact.",
            )),
    )
}

fn render_accounts(model: &ProjectionModel, household: &Household, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    GroupBox::new().id("projection-accounts").title("Accounts of the boundary (§11.3: an aggregate can be fine while one account fails)").child(
        Table::new()
            .child(
                TableHeader::new().child(
                    TableRow::new()
                        .child(TableHead::new().min_w_0().child("Account"))
                        .child(TableHead::new().w_40().flex_shrink_0().text_right().child("Start"))
                        .child(TableHead::new().w_40().flex_shrink_0().text_right().child("End"))
                        .child(TableHead::new().w_48().flex_shrink_0().text_right().child("Lowest"))
                        .child(TableHead::new().w_32().flex_shrink_0().text_right().child("Hard floor"))
                        .child(TableHead::new().w_20().flex_shrink_0().text_right().child("Postings"))
                        .child(TableHead::new().w_72().flex_shrink_0().child("Status")),
                ),
            )
            .child(TableBody::new().children(model.forecast.accounts.iter().enumerate().map(|(index, a)| {
                let name = household.account(a.account).map(|acc| acc.name.clone()).unwrap_or_default();
                let lowest = format!("{}{}", a.lowest.format(), a.lowest_date.map(|d| format!(" ({})", d.format("%d %b"))).unwrap_or_default());
                let status: AnyElement = if let Some(date) = a.negative_from {
                    Tag::danger().xsmall().outline().child(format!("negative from {}", date.format("%d %b %Y"))).into_any_element()
                } else if let Some(date) = a.breach.first_breach {
                    Tag::warning().xsmall().outline().child(format!("below floor from {}", date.format("%d %b %Y"))).into_any_element()
                } else {
                    Tag::secondary().xsmall().outline().child("floor held").into_any_element()
                };
                TableRow::new()
                    .when(index % 2 == 1, |row| row.bg(theme.table_even))
                    .child(TableCell::new().min_w_0().overflow_hidden().text_ellipsis().child(name))
                    .child(money_cell(a.start, cx).w_40().flex_shrink_0())
                    .child(money_cell(a.end, cx).w_40().flex_shrink_0())
                    .child(TableCell::new().w_48().flex_shrink_0().text_right().font_family(theme.mono_font_family.clone()).child(lowest))
                    .child(money_cell(a.floor, cx).w_32().flex_shrink_0())
                    .child(muted_cell(a.postings.len().to_string(), cx).w_20().flex_shrink_0().text_right())
                    .child(TableCell::new().w_72().flex_shrink_0().child(status))
            }))),
    )
}

fn render_transfer_points(model: &ProjectionModel, household: &Household, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let points = &model.forecast.transfer_points;
    GroupBox::new().id("projection-transfers").title("Required transfer points (§11.3)").child(if points.is_empty() {
        div().text_sm().text_color(theme.muted_foreground).child("No account falls below its hard floor on this path; no internal transfer is required.").into_any_element()
    } else {
        v_flex()
            .gap_1()
            .text_sm()
            .children(points.iter().map(|t| {
                let name = household.account(t.account).map(|a| a.name.clone()).unwrap_or_default();
                h_flex().gap_2().child("•").child(format!(
                    "{}: {} needs {} to stay at its floor{}",
                    t.date.format("%d %b %Y"),
                    name,
                    t.shortfall.format(),
                    if t.coverable { " — other accounts of the boundary have the headroom" } else { " — not coverable from the boundary's other accounts" }
                ))
            }))
            .into_any_element()
    })
}

fn render_assumptions(model: &ProjectionModel, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    GroupBox::new().id("projection-assumptions").title("This path depends on (§10.2)").child(
        // Full-width rows, not wrap rows with a flex_1 sentence (see the
        // household screen): the difference is ~1,500 taffy measure callbacks
        // per assumption per frame.
        v_flex().w_full().gap_2().children(model.forecast.assumptions.iter().enumerate().map(|(index, a)| {
            v_flex()
                .w_full()
                .gap_1()
                .child(div().w_full().child(format!("{}. {}", index + 1, a.text)))
                .child(h_flex().gap_2().items_center().child(labels::certainty_tag(a.certainty)).child(div().text_xs().text_color(theme.muted_foreground).child(a.source.describe())))
        })),
    )
}

fn render_record(model: &ProjectionModel, household: &Household, _cx: &App) -> impl IntoElement {
    let r = &model.forecast.record;
    let snapshot: Vec<String> = r
        .starting_snapshot
        .iter()
        .map(|(id, money)| format!("{} {}", household.account(*id).map(|a| a.name.clone()).unwrap_or_else(|| id.to_string()), money.format()))
        .collect();
    let excluded: Vec<String> = r
        .excluded_accounts
        .iter()
        .map(|(id, reason)| format!("{} — {reason}", household.account(*id).map(|a| a.name.clone()).unwrap_or_else(|| id.to_string())))
        .collect();
    let series: Vec<String> = r.included_series.iter().filter_map(|id| household.series_by_id(*id)).map(|s| s.name.clone()).collect();
    let policies = format!("{} policies at versions {}", r.policy_versions.len(), r.policy_versions.iter().map(|(_, v)| format!("v{v}")).collect::<Vec<_>>().join(", "));
    GroupBox::new().id("projection-record").title("Forecast record — enough to reproduce this run (§11.4, §24)").child(
        DescriptionList::new()
            .columns(1)
            .child(DescriptionItem::new("Algorithm").value(r.algorithm.to_string()))
            .child(DescriptionItem::new("Starting balance snapshot").value(snapshot.join(" · ")))
            .child(DescriptionItem::new("Included event series").value(series.join(" · ")))
            .child(DescriptionItem::new("Excluded accounts").value(if excluded.is_empty() { "none".to_string() } else { excluded.join(" · ") }))
            .child(DescriptionItem::new("Rules applied").value(r.rules_applied.join(" · ")))
            .child(DescriptionItem::new("Tax rule packs").value(if r.tax_rule_packs.is_empty() { "none".to_string() } else { r.tax_rule_packs.join(" · ") }))
            .child(DescriptionItem::new("Assumptions").value(format!("{} listed above", r.assumptions.len())))
            .child(DescriptionItem::new("Scenario overrides").value(r.scenario.and_then(|id| household.scenario(id)).map(|s| s.name.clone()).unwrap_or_else(|| "none (baseline)".into())))
            .child(DescriptionItem::new("Authorization policy versions").value(policies))
            .child(DescriptionItem::new("Case").value(format!("{} — {}", r.case.label(), r.case.description())))
            .child(DescriptionItem::new("Input hash").value(format!("{:016x}", r.input_hash))),
    )
}
