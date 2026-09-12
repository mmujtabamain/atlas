//! Taxes: rule packs with their versions and effective dates, the tax events
//! of the window with their cash dates and entity attribution, the tax
//! reserve for what is payable later, and the with-vs-without extraction
//! comparison.

use atlas_core::authz::Viewer;
use atlas_core::forecast::Case;
use atlas_core::ids::{EntityRef, ObjectRef, ScenarioId};
use atlas_core::model::{Bracket, Household, TaxRulePack};
use atlas_core::tax::{TaxAssessment, TaxEvent, YearStrategy, assess, multi_year_comparison};
use atlas_core::{Disclosure, EngineResult, Money};
use chrono::NaiveDate;
use std::sync::Arc;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _,
    alert::Alert,
    button::Button,
    checkbox::Checkbox,
    group_box::GroupBox, h_flex,
    input::Input,
    radio::RadioGroup,
    table::{Table, TableBody, TableCell, TableHead, TableHeader, TableRow},
    tag::Tag, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::{AtlasApp, Grids, TaxControls};
use crate::widgets::figure::{ExplainedFigure, card};
use crate::widgets::grid::{self, Cell, GridColumn, Row};
use crate::widgets::labels;
use crate::widgets::master::page_header;
use crate::widgets::table::money_cell;

/// Which bracket schedule the extraction comparison uses.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum E05Schedule {
    /// A fictitious example: 10% on the first 100,000, 30% above.
    PlanExample,
    /// The household's effective annual brackets for the current year.
    HouseholdPack,
}

#[derive(Clone, Debug)]
pub struct TaxModel {
    pub assessment: TaxAssessment,
    pub scenario_on: bool,
    pub by_entity: Vec<(EntityRef, ExplainedFigure)>,
    pub reserve: ExplainedFigure,
    pub e05_amount: Money,
    pub e05_split: bool,
    pub e05_schedule: E05Schedule,
    pub e05_baseline_total: Money,
    pub e05_strategies: Vec<YearStrategy>,
    pub e05_incrementals: Vec<ExplainedFigure>,
    pub e05_brackets: Vec<Bracket>,
    /// Packs the viewer may see (company-only data is not a pack concern; all packs are household objects).
    pub packs: Vec<TaxRulePack>,
    /// The tax events as grid rows, formatted once (see `widgets::grid`).
    pub event_rows: grid::Rows,
    /// The scenario the toggle applies (see [`super::overlay_scenario`]).
    pub overlay_scenario: Option<ScenarioId>,
}

/// Columns of the tax-events grid, in display order.
pub const EVENT_COLUMNS: [GridColumn; 9] = [
    GridColumn::new("cash", "Cash date", 104.),
    GridColumn::new("accrued", "Accrued", 104.),
    GridColumn::new("entity", "Entity", 128.),
    GridColumn::new("rule", "Rule", 224.),
    GridColumn::new("base", "Base", 300.),
    GridColumn::new("base-amount", "Base amount", 128.).right(),
    GridColumn::new("tax", "Tax", 128.).right(),
    GridColumn::new("kind", "Kind", 256.),
    GridColumn::new("account", "Account", 160.),
];

/// One tax event as a grid row.
fn event_row(e: &TaxEvent, through: NaiveDate, household: &Household) -> Row {
    let after = e.cash_date > through;
    Row::new(vec![
        Cell::text(e.cash_date.format("%d %b %y").to_string()),
        Cell::muted(e.accrual_date.format("%d %b %y").to_string()),
        Cell::muted(household.entity_name(e.entity)),
        Cell::text(e.rule_name.clone()),
        Cell::muted(e.base_label.clone()),
        Cell::money(e.base_amount),
        Cell::money(e.amount),
        Cell::Chip(if after { format!("{} · payable after horizon", e.kind.label()).into() } else { e.kind.label().into() }),
        Cell::muted(household.account(e.account).map(|a| a.name.clone()).unwrap_or_default()),
    ])
    .muted(after)
}

impl TaxModel {
    pub fn compute(household: &Household, viewer: Viewer, through: NaiveDate, scenario_on: bool, e05_amount: Money, e05_split: bool, e05_schedule: E05Schedule) -> EngineResult<Self> {
        log::info!("computing tax model through {through} scenario={scenario_on} e05 amount={} split={e05_split}", e05_amount.format());
        let overlay_scenario = super::overlay_scenario(household, viewer);
        let scenario = if scenario_on { overlay_scenario } else { None };
        let assessment = assess(household, through, scenario, Case::Expected)?;
        let by_entity = assessment
            .by_entity
            .iter()
            .filter(|(entity, _)| match entity {
                EntityRef::Company(id) => matches!(household.disclosure_for(viewer, ObjectRef::Company(*id)), Disclosure::Full | Disclosure::SelectedFields),
                _ => true,
            })
            .map(|(entity, calc)| (*entity, ExplainedFigure::new(format!("tax-{}", entity.to_string().replace(' ', "-")), format!("{} — tax cash in the window", household.entity_name(*entity)), calc, household, viewer)))
            .collect();
        let reserve = ExplainedFigure::new("tax-reserve", "Tax incurred, payable after the horizon (reserve)", &assessment.payable_after_horizon, household, viewer);

        let currency = household.base_currency;
        let e05_brackets = match e05_schedule {
            E05Schedule::PlanExample => vec![
                Bracket { lower: Money::zero(currency), upper: Some(Money::from_major(100_000, currency)), rate_basis_points: 1_000 },
                Bracket { lower: Money::from_major(100_000, currency), upper: None, rate_basis_points: 3_000 },
            ],
            E05Schedule::HouseholdPack => atlas_core::tax::effective_rules(household, household.as_of)
                .into_iter()
                .find_map(|(_, rule)| match &rule.kind {
                    atlas_core::model::TaxKind::AnnualBrackets { brackets } => Some(brackets.clone()),
                    _ => None,
                })
                .unwrap_or_default(),
        };
        let baseline = vec![Money::from_major(60_000, currency), Money::from_major(60_000, currency)];
        let half = Money::new(e05_amount.minor() / 2, currency);
        let other_half = e05_amount.checked_sub(half)?;
        let strategies = vec![
            (format!("Extract {} in year 1", e05_amount.format()), vec![e05_amount, Money::zero(currency)]),
            (format!("Extract {} in each year", half.format()), vec![half, other_half]),
        ];
        let (e05_baseline_total, e05_strategies) = if e05_brackets.is_empty() {
            (Money::zero(currency), Vec::new())
        } else {
            multi_year_comparison(&e05_brackets, &baseline, if e05_split { &strategies } else { &strategies[..1] })?
        };
        let e05_incrementals = e05_strategies
            .iter()
            .enumerate()
            .map(|(index, s)| ExplainedFigure::new(format!("e05-incremental-{index}"), format!("{} — incremental tax", s.name), &s.incremental, household, viewer))
            .collect();
        let event_rows = Arc::new(assessment.events.iter().map(|e| event_row(e, assessment.through, household)).collect());
        Ok(TaxModel {
            event_rows,
            assessment,
            scenario_on,
            by_entity,
            reserve,
            e05_amount,
            e05_split,
            e05_schedule,
            e05_baseline_total,
            e05_strategies,
            e05_incrementals,
            e05_brackets,
            packs: household.tax_packs.clone(),
            overlay_scenario,
        })
    }
}

pub fn render(model: &TaxModel, controls: &TaxControls, grids: &Grids, household: &Household, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    v_flex()
        .id("screen-taxes")
        .test_support()
        .w_full()
        .gap_6()
        .child(page_header(
            "Taxes",
            "Taxes are dated events with cash dates, not a percentage at the end. Every figure is an estimate under the configured rule packs — not a filing and not advice.",
            cx,
        ))
        .child(
            Alert::info("tax-caveat", "The DEMO packs are fictitious examples, not any country's law. Rules you add stay marked unverified until an official source is attached. No jurisdiction is ever inferred from the currency or the language.")
                .title("Planning estimates only"),
        )
        .child(render_packs(model, household, cx))
        .child(render_events(model, grids, household, cx))
        .child(render_e05(model, controls, cx))
}

fn render_packs(model: &TaxModel, household: &Household, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    let _ = household;
    GroupBox::new().id("tax-packs").title("Rule packs").child(
        v_flex()
            .gap_4()
            .child(
                h_flex()
                    .justify_between()
                    .items_start()
                    .gap_4()
                    .child(div().flex_1().min_w_0().text_xs().text_color(theme.muted_foreground).child(
                        "Rules carry effective dates, so a 2027 forecast never silently applies a 2026 rule. Your own rules go into a separate, unverified pack.",
                    ))
                    .child(
                        Button::new("new-tax-rule")
                            .flex_shrink_0()
                            .small()
                            .outline()
                            .icon(IconName::Plus)
                            .label("New rule…")
                            .on_click(cx.listener(|this, _, window, cx| this.open_new_tax_rule(window, cx))),
                    ),
            )
            .children(model.packs.iter().map(|pack| {
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .flex_wrap()
                            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(format!("{} · {}", pack.name, pack.version)))
                            .child(if pack.verified {
                                Tag::secondary().xsmall().outline().child("verified")
                            } else {
                                Tag::warning().xsmall().outline().child("unverified — not any country's law")
                            })
                            .child(div().text_xs().text_color(theme.muted_foreground).child(pack.jurisdiction.clone())),
                    )
                    .children(pack.rules.iter().enumerate().map(|(index, rule)| {
                        h_flex()
                            .gap_4()
                            .items_start()
                            .px_2()
                            .py_2()
                            .rounded(theme.radius)
                            .when(index % 2 == 1, |row| row.bg(theme.table_even))
                            .child(
                                v_flex()
                                    .w_72()
                                    .flex_shrink_0()
                                    .child(div().text_sm().child(rule.name.clone()))
                                    .child(div().text_xs().text_color(theme.muted_foreground).child(rule.tax_type.clone()))
                                    .child(div().text_xs().text_color(theme.muted_foreground).child(format!("applies to: {}", rule.categories.join(", ")))),
                            )
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .child(div().text_sm().child(rule.describe_kind()))
                                    .child(div().text_xs().text_color(theme.muted_foreground).child(rule.timing.label()))
                                    .child(div().text_xs().text_color(theme.muted_foreground).child(rule.explanation.clone())),
                            )
                            .child(
                                v_flex()
                                    .w_64()
                                    .flex_shrink_0()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(format!("effective {} – {}", rule.effective_from.format("%d %b %Y"), rule.effective_to.map(|d| d.format("%d %b %Y").to_string()).unwrap_or_else(|| "open".into())))
                                    .child(format!("scope: {}", rule.scope))
                                    .child(format!("source: {}", rule.source)),
                            )
                    }))
            })),
    )
}

fn render_events(model: &TaxModel, grids: &Grids, household: &Household, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    grid::sync(&grids.tax_events, &model.event_rows, cx);
    let theme = cx.theme();
    let events = &model.assessment.events;
    GroupBox::new()
        .id("tax-events")
        .title(format!("Tax events through {}", model.assessment.through.format("%d %b %Y")))
        .child(
            v_flex()
                .gap_4()
                .child(
                    h_flex()
                        .flex_wrap()
                        .gap_6()
                        .items_end()
                        .when_some(model.overlay_scenario.and_then(|id| household.scenario(id)).map(|s| s.name.clone()), |this, name| {
                            this.child(
                                Checkbox::new("tax-buy-car")
                                    .label(format!("With scenario “{name}”"))
                                    .checked(model.scenario_on)
                                    .on_change(cx.listener(|this, checked, _, cx| this.set_tax_scenario(*checked, cx))),
                            )
                        })
                        .child(div().text_xs().text_color(theme.muted_foreground).child(format!(
                            "{} events · creditable withholding {} · packs used: {}",
                            events.len(),
                            model.assessment.creditable_withholding.format(),
                            if model.assessment.packs_used.is_empty() { "none".to_string() } else { model.assessment.packs_used.join("; ") }
                        ))),
                )
                .child(
                    h_flex()
                        .flex_wrap()
                        .gap_8()
                        .children(model.by_entity.iter().map(|(_, f)| card(f.figure(false))))
                        .child(card(model.reserve.figure(true))),
                )
                .child(div().text_xs().text_color(theme.muted_foreground).child(
                    "Each tax stays with the person or company that owes it. What falls due after the horizon is a reserve, not a posting in this window; a negative balance is a refund to come, not cash.",
                ))
                .child(if events.is_empty() {
                    div().text_sm().text_color(theme.muted_foreground).child("No rule applies to any occurrence in the window.").into_any_element()
                } else {
                    grid::render("tax-events-grid", &grids.tax_events, cx).into_any_element()
                }),
        )
}

fn render_e05(model: &TaxModel, controls: &TaxControls, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    let years = model.e05_strategies.first().map(|s| s.taxable_by_year.len()).unwrap_or(2);
    GroupBox::new().id("tax-e05").title("Extra tax from taking money out: all at once versus split over two years").child(
        v_flex()
            .gap_4()
            .child(div().text_xs().text_color(theme.muted_foreground).child(
                "Starts from a taxable income of 60,000 in each of two years. Enter an amount you could take out and compare taking it all in year 1 with splitting it evenly — same brackets each year, nothing else changing. A split only works when the money is not needed in year 1.",
            ))
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_6()
                    .items_end()
                    .child(v_flex().gap_1().child(div().text_xs().text_color(theme.muted_foreground).child("Extraction amount")).child(Input::new(&controls.e05_amount).id("e05-amount").small().w_48()))
                    .child(
                        v_flex().gap_1().child(div().text_xs().text_color(theme.muted_foreground).child("Strategies")).child(
                            RadioGroup::horizontal("e05-split")
                                .children(["Year 1 only", "Year 1 only vs. split evenly"])
                                .selected_index(Some(if model.e05_split { 1 } else { 0 }))
                                .on_change(cx.listener(|this, index: &usize, _, cx| this.set_e05_split(*index == 1, cx))),
                        ),
                    )
                    .child(
                        v_flex().gap_1().child(div().text_xs().text_color(theme.muted_foreground).child("Bracket schedule")).child(
                            RadioGroup::horizontal("e05-schedule")
                                .children(["Example: 10% up to 100,000, 30% above", "This household's pack (current year)"])
                                .selected_index(Some(match model.e05_schedule { E05Schedule::PlanExample => 0, E05Schedule::HouseholdPack => 1 }))
                                .on_change(cx.listener(|this, index: &usize, _, cx| this.set_e05_schedule(if *index == 1 { E05Schedule::HouseholdPack } else { E05Schedule::PlanExample }, cx))),
                        ),
                    ),
            )
            .child(div().id("e05-summary").test_support().text_sm().child(format!(
                "Baseline tax over {years} years: {} · brackets: {}",
                model.e05_baseline_total.format(),
                model.e05_brackets.iter().map(|b| match b.upper {
                    Some(u) => format!("{}% {}–{}", b.rate_basis_points / 100, b.lower.format(), u.format()),
                    None => format!("{}% above {}", b.rate_basis_points / 100, b.lower.format()),
                }).collect::<Vec<_>>().join(", ")
            )))
            .child(if model.e05_strategies.is_empty() {
                div().text_sm().text_color(theme.muted_foreground).child("No annual bracket rule is effective; add one to compare strategies.").into_any_element()
            } else {
                v_flex()
                    .gap_4()
                    .child(
                        Table::new()
                            .child(
                                TableHeader::new().child(
                                    TableRow::new()
                                        .child(TableHead::new().min_w_0().child("Strategy"))
                                        .children((0..years).map(|y| TableHead::new().w_48().flex_shrink_0().text_right().child(format!("Taxable · tax, year {}", y + 1))))
                                        .child(TableHead::new().w_32().flex_shrink_0().text_right().child("Total tax"))
                                        .child(TableHead::new().w_40().flex_shrink_0().text_right().child("Incremental vs baseline")),
                                ),
                            )
                            .child(TableBody::new().children(model.e05_strategies.iter().enumerate().map(|(index, s)| {
                                TableRow::new()
                                    .when(index % 2 == 1, |row| row.bg(theme.table_even))
                                    .child(TableCell::new().min_w_0().child(s.name.clone()))
                                    .children((0..years).map(|y| {
                                        TableCell::new().w_48().flex_shrink_0().text_right().font_family(theme.mono_font_family.clone()).child(format!(
                                            "{} · {}",
                                            s.taxable_by_year.get(y).map(|m| m.format()).unwrap_or_default(),
                                            s.tax_by_year.get(y).map(|c| c.money().format()).unwrap_or_default()
                                        ))
                                    }))
                                    .child(money_cell(s.total_tax, cx).w_32().flex_shrink_0())
                                    .child(money_cell(s.incremental.money(), cx).w_40().flex_shrink_0())
                            }))),
                    )
                    .child(h_flex().flex_wrap().gap_8().children(model.e05_incrementals.iter().map(|f| card(f.figure(false)))))
                    .child(div().text_xs().text_color(theme.muted_foreground).child(
                        "The cheaper split is only available when next year's rules and the household's cash allow it — timing and eligibility matter as much as the rate.",
                    ))
                    .into_any_element()
            }),
    )
}

/// Tag for a pack's verification status, shared with other screens.
pub fn verification_tag(verified: bool) -> Tag {
    if verified { labels::strength_tag(atlas_core::ResultStrength::ExactAccounting) } else { Tag::warning().xsmall().outline().child("unverified") }
}
