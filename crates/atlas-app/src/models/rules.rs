//! Rules: the user's deterministic rules, the conflict-resolution inspector
//! (every decision with its losing candidates and why), the fee events the
//! rules add to the window, the funding order, bank selection and a
//! with-vs-without simulation.

use atlas_core::authz::Viewer;
use atlas_core::forecast::{Case, ForecastOptions};
use atlas_core::ids::{AccountId, RuleId, ScenarioId};
use atlas_core::liquidity::Boundary;
use atlas_core::model::Household;
use atlas_core::rules::{FeePosting, FundingStep, Rule, RuleAction, RuleDecision, RuleEvaluation, RuleSimulation, TieBreak, Trigger, account_for_expense, evaluate, funding_order, simulate};
use atlas_core::{EngineResult, Money};
use chrono::NaiveDate;
use std::sync::Arc;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _,
    alert::Alert,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    group_box::GroupBox, h_flex,
    radio::RadioGroup,
    table::{Table, TableBody, TableCell, TableHead, TableHeader, TableRow},
    tag::Tag, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::{AtlasApp, Grids};
use crate::widgets::grid::{self, Cell, GridColumn, Row};
use crate::widgets::master::page_header;
use crate::widgets::table::{money_cell, muted_cell};

/// Everything the Rules screen shows, derived once per state change.
#[derive(Clone, Debug)]
pub struct RulesModel {
    pub through: NaiveDate,
    pub scenario_on: bool,
    pub tie_break: TieBreak,
    pub rules: Vec<Rule>,
    pub evaluation: RuleEvaluation,
    /// Decisions with more than one candidate: the conflicts the inspector explains.
    pub conflicts: Vec<RuleDecision>,
    pub fee_total: Money,
    pub funding: Vec<FundingStep>,
    pub funding_date: NaiveDate,
    /// One worked bank-selection answer per bank-selection rule, on today's settled balances.
    pub bank_selection: Vec<(String, AccountId, String)>,
    pub simulated: Option<RuleId>,
    pub simulation: Option<RuleSimulation>,
    /// The fee postings as grid rows, formatted once (see `widgets::grid`).
    pub fee_rows: grid::Rows,
    /// The scenario the toggle applies (see [`super::overlay_scenario`]).
    pub overlay_scenario: Option<ScenarioId>,
}

/// Columns of the fee-postings grid, in display order.
pub const FEE_COLUMNS: [GridColumn; 5] = [
    GridColumn::new("date", "Date", 104.),
    GridColumn::new("account", "Account", 192.),
    GridColumn::new("event", "Fee event", 420.),
    GridColumn::new("amount", "Amount", 128.).right(),
    GridColumn::new("rule", "Rule", 96.),
];

/// One fee posting as a grid row.
fn fee_row(fee: &FeePosting, household: &Household) -> Row {
    Row::new(vec![
        Cell::text(fee.date.format("%d %b %y").to_string()),
        Cell::muted(household.account(fee.account).map(|a| a.name.clone()).unwrap_or_default()),
        Cell::text(fee.label.clone()),
        Cell::money(fee.amount),
        Cell::muted(fee.rule.to_string()),
    ])
}

impl RulesModel {
    pub fn compute(household: &Household, viewer: Viewer, through: NaiveDate, scenario_on: bool, simulated: Option<RuleId>) -> EngineResult<Self> {
        let overlay_scenario = super::overlay_scenario(household, viewer);
        let scenario = if scenario_on { overlay_scenario } else { None };
        log::info!("evaluating {} rules through {through} scenario={scenario_on} tie_break={}", household.rules.len(), household.rule_tie_break.slug());
        let evaluation = evaluate(household, through, scenario, Case::Expected, household.rule_tie_break)?;
        let conflicts: Vec<RuleDecision> = evaluation.decisions.iter().filter(|d| d.candidates.len() > 1).cloned().collect();
        let mut fee_total = Money::zero(household.base_currency);
        for fee in &evaluation.fees {
            fee_total = fee_total.checked_add(fee.amount)?;
        }
        let funding_date = household.as_of;
        let funding = funding_order(household, funding_date, scenario);
        let bank_selection = household
            .rules
            .iter()
            .filter(|r| r.enabled && matches!(r.action, RuleAction::BankSelection { .. }))
            .filter_map(|r| {
                let category = match &r.scope {
                    atlas_core::rules::RuleScope::Category(c) => c.clone(),
                    _ => household.categories().first().cloned().unwrap_or_default(),
                };
                let balance_of = |id: AccountId| household.account(id).map(|a| a.settled_balance).unwrap_or(Money::zero(household.base_currency));
                account_for_expense(household, &category, household.as_of, &balance_of).map(|(account, why)| (category, account, why))
            })
            .collect();
        let simulation = match simulated.filter(|id| household.rule(*id).is_some()) {
            Some(id) => Some(simulate(household, id, Boundary::Household, ForecastOptions { through, scenario, case: Case::Expected })?),
            None => None,
        };
        if !conflicts.is_empty() {
            log::info!("{} rule decisions, {} with competing candidates", evaluation.decisions.len(), conflicts.len());
        }
        let fee_rows = Arc::new(evaluation.fees.iter().map(|fee| fee_row(fee, household)).collect());
        Ok(RulesModel {
            fee_rows,
            overlay_scenario,
            through,
            scenario_on,
            tie_break: household.rule_tie_break,
            rules: household.rules.clone(),
            evaluation,
            conflicts,
            fee_total,
            funding,
            funding_date,
            bank_selection,
            simulated,
            simulation,
        })
    }
}

pub fn render(model: &RulesModel, grids: &Grids, household: &Household, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    v_flex()
        .id("screen-rules")
        .test_support()
        .w_full()
        .gap_6()
        .child(page_header(
            "Rules",
            "Your rules are deterministic: a scope, a trigger, conditions, one action, a priority and an effective range. Every decision they take can be inspected — the winner, every loser and the reason.",
            cx,
        ))
        .child(
            Alert::info("rules-caveat", "Rules never learn or guess. A rule that is not in force on a date does nothing, and an unknown situation is left alone. When two rules compete, priority wins, then the more specific scope, then the tie-break chosen below.")
                .title("Deterministic, not heuristic"),
        )
        .child(render_controls(model, household, cx))
        .child(render_register(model, household, cx))
        .child(render_conflicts(model, household, cx))
        .child(render_fees(model, grids, cx))
        .child(render_funding(model, household, cx))
        .child(render_simulation(model, household, cx))
}

fn render_controls(model: &RulesModel, household: &Household, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    h_flex()
        .flex_wrap()
        .gap_6()
        .items_end()
        .when_some(model.overlay_scenario.and_then(|id| household.scenario(id)).map(|s| s.name.clone()), |this, name| {
            this.child(
                Checkbox::new("rules-buy-car")
                    .label(format!("Evaluate inside scenario “{name}” (its rules apply too)"))
                    .checked(model.scenario_on)
                    .on_change(cx.listener(|this, checked, _, cx| this.set_rules_scenario(*checked, cx))),
            )
        })
        .child(
            v_flex().gap_1().child(div().text_xs().text_color(theme.muted_foreground).child("Tie-break when priority and scope are equal")).child(
                RadioGroup::horizontal("rules-tie-break")
                    .children(TieBreak::ALL.iter().map(|t| t.label()))
                    .selected_index(Some(TieBreak::ALL.iter().position(|t| *t == model.tie_break).unwrap_or(0)))
                    .on_change(cx.listener(|this, index: &usize, _, cx| this.set_rules_tie_break(TieBreak::ALL[(*index).min(1)], cx))),
            ),
        )
        .child(div().id("rules-summary").test_support().text_xs().text_color(theme.muted_foreground).child(format!(
            "{} rules ({} enabled) · {} decisions through {} · {} with competing candidates · fees added {}",
            model.rules.len(),
            model.rules.iter().filter(|r| r.enabled).count(),
            model.evaluation.decisions.len(),
            model.through.format("%d %b %Y"),
            model.conflicts.len(),
            model.fee_total.format()
        )))
}

fn render_register(model: &RulesModel, household: &Household, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    GroupBox::new().id("rules-register").title("Rules — each with its version history").child(
        v_flex()
            .gap_4()
            .child(
                h_flex()
                    .justify_between()
                    .items_start()
                    .gap_4()
                    .child(div().flex_1().min_w_0().text_xs().text_color(theme.muted_foreground).child(
                        "Disabling or re-prioritising a rule records a new version; the history stays with the rule so an old forecast can be explained. Scenario-scoped rules apply only inside their scenario.",
                    ))
                    .child(
                        Button::new("new-rule")
                            .flex_shrink_0()
                            .small()
                            .outline()
                            .icon(IconName::Plus)
                            .label("New rule…")
                            .on_click(cx.listener(|this, _, window, cx| this.open_rule_editor(window, cx))),
                    ),
            )
            .child(if model.rules.is_empty() {
                div().text_sm().text_color(theme.muted_foreground).child("No rules yet. Add one to see fee events, funding orders and bank selection explained here.").into_any_element()
            } else {
                let rows: Vec<AnyElement> = model.rules.iter().enumerate().map(|(index, rule)| render_rule_row(index, rule, model, household, cx).into_any_element()).collect();
                v_flex().gap_1().children(rows).into_any_element()
            }),
    )
}

fn render_rule_row(index: usize, rule: &Rule, model: &RulesModel, household: &Household, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    let id = rule.id;
    let enabled = rule.enabled;
    let simulated = model.simulated == Some(id);
    let scenario_name = rule.scenario.and_then(|s| household.scenario(s)).map(|s| s.name.clone());
    let conditions = if rule.conditions.is_empty() { "no conditions".to_string() } else { rule.conditions.iter().map(|c| c.describe(household)).collect::<Vec<_>>().join(" and ") };
    h_flex()
        .id(ElementId::Name(format!("rule-row-{}", id.raw()).into()))
        .test_support()
        .gap_4()
        .items_start()
        .px_2()
        .py_2()
        .rounded(theme.radius)
        .when(index % 2 == 1, |row| row.bg(theme.table_even))
        .when(!enabled, |row| row.text_color(theme.muted_foreground))
        .child(
            v_flex()
                .w_80()
                .flex_shrink_0()
                .gap_1()
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(rule.name.clone()))
                .child(
                    h_flex()
                        .gap_1()
                        .flex_wrap()
                        .child(if enabled { Tag::secondary().xsmall().outline().child("enabled") } else { Tag::warning().xsmall().outline().child("disabled") })
                        .child(Tag::secondary().xsmall().outline().child(format!("priority {}", rule.priority)))
                        .child(Tag::secondary().xsmall().outline().child(format!("v{}", rule.version)))
                        .when_some(scenario_name, |row, name| row.child(Tag::warning().xsmall().outline().child(format!("only in “{name}”")))),
                )
                .child(div().text_xs().text_color(theme.muted_foreground).child(format!("{} · {}", rule.id, rule.explanation))),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_1()
                .child(div().text_sm().child(format!("When {} in {}: {}", rule.trigger.label(), rule.scope.describe(household), rule.action.describe(household))))
                .child(div().text_xs().text_color(theme.muted_foreground).child(format!("conditions: {conditions} · scope specificity {}", rule.scope.specificity())))
                .child(div().text_xs().text_color(theme.muted_foreground).child(format!(
                    "effective {} – {}",
                    rule.effective_from.format("%d %b %Y"),
                    rule.effective_to.map(|d| d.format("%d %b %Y").to_string()).unwrap_or_else(|| "open".into())
                )))
                .child(div().text_xs().text_color(theme.muted_foreground).child(format!(
                    "history: {}",
                    rule.history.iter().map(|h| format!("v{} {} — {}", h.version, h.changed_on.format("%d %b %Y"), h.summary)).collect::<Vec<_>>().join("; ")
                ))),
        )
        .child(
            v_flex()
                .flex_shrink_0()
                .gap_1()
                .items_end()
                .child(
                    h_flex()
                        .gap_1()
                        .child(
                            Button::new(ElementId::Name(format!("rule-priority-up-{}", id.raw()).into()))
                                .xsmall()
                                .ghost()
                                .icon(IconName::ChevronUp)
                                .tooltip("Raise priority by 1 (new version)")
                                .on_click(cx.listener(move |this, _, window, cx| this.bump_rule_priority(id, 1, window, cx))),
                        )
                        .child(
                            Button::new(ElementId::Name(format!("rule-priority-down-{}", id.raw()).into()))
                                .xsmall()
                                .ghost()
                                .icon(IconName::ChevronDown)
                                .tooltip("Lower priority by 1 (new version)")
                                .on_click(cx.listener(move |this, _, window, cx| this.bump_rule_priority(id, -1, window, cx))),
                        )
                        .child(
                            Button::new(ElementId::Name(format!("rule-toggle-{}", id.raw()).into()))
                                .xsmall()
                                .outline()
                                .label(if enabled { "Disable" } else { "Enable" })
                                .on_click(cx.listener(move |this, _, window, cx| this.toggle_rule(id, window, cx))),
                        )
                        .child(
                            Button::new(ElementId::Name(format!("rule-simulate-{}", id.raw()).into()))
                                .xsmall()
                                .map(|b| if simulated { b.primary() } else { b.outline() })
                                .label(if simulated { "Simulating" } else { "Simulate" })
                                .on_click(cx.listener(move |this, _, _, cx| this.simulate_rule(id, cx))),
                        )
                        .child(
                            Button::new(ElementId::Name(format!("rule-delete-{}", id.raw()).into()))
                                .xsmall()
                                .ghost()
                                .icon(IconName::Trash)
                                .tooltip("Delete this rule")
                                .on_click(cx.listener(move |this, _, window, cx| this.delete_rule(id, window, cx))),
                        ),
                ),
        )
}

fn render_conflicts(model: &RulesModel, household: &Household, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    let decisions = &model.evaluation.decisions;
    GroupBox::new().id("rules-conflicts").title("Decisions the rules took — who won, who lost, and why").child(
        v_flex()
            .gap_4()
            .child(div().text_xs().text_color(theme.muted_foreground).child(format!(
                "Order of resolution: explicit priority → scope specificity → tie-break ({}). Decisions with a single applicable rule are listed for completeness; the ones with competing candidates are the conflicts.",
                model.tie_break.label()
            )))
            .child(if decisions.is_empty() {
                div().text_sm().text_color(theme.muted_foreground).child("No rule applied to any occurrence in the window.").into_any_element()
            } else {
                Table::new()
                    .child(
                        TableHeader::new().child(
                            TableRow::new()
                                .child(TableHead::new().w_24().flex_shrink_0().child("Date"))
                                .child(TableHead::new().w_56().flex_shrink_0().child("Occurrence"))
                                .child(TableHead::new().w_32().flex_shrink_0().child("Decision"))
                                .child(TableHead::new().min_w_0().child("Candidates (priority · specificity → outcome)"))
                                .child(TableHead::new().w_48().flex_shrink_0().child("Resolved by")),
                        ),
                    )
                    .child(TableBody::new().children(decisions.iter().enumerate().map(|(index, d)| {
                        let conflict = d.candidates.len() > 1;
                        let candidates = d.candidates.iter().map(|c| format!("{} ({} · {}) → {}", c.name, c.priority, c.specificity, c.outcome)).collect::<Vec<_>>().join(" | ");
                        TableRow::new()
                            .when(index % 2 == 1, |row| row.bg(theme.table_even))
                            .child(TableCell::new().w_24().flex_shrink_0().child(d.date.format("%d %b %y").to_string()))
                            .child(muted_cell(format!("{} · {}", d.occurrence_label, household.account(d.account).map(|a| a.name.clone()).unwrap_or_default()), cx).w_56().flex_shrink_0().overflow_hidden().text_ellipsis())
                            .child(TableCell::new().w_32().flex_shrink_0().child(h_flex().child(if conflict { Tag::warning().xsmall().outline().child(format!("{} · conflict", d.action_kind)) } else { Tag::secondary().xsmall().outline().child(d.action_kind) })))
                            .child(TableCell::new().min_w_0().overflow_hidden().text_ellipsis().child(candidates))
                            .child(muted_cell(d.resolution.clone(), cx).w_48().flex_shrink_0().overflow_hidden().text_ellipsis())
                    })))
                    .into_any_element()
            }),
    )
}

fn render_fees(model: &RulesModel, grids: &Grids, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    grid::sync(&grids.rule_fees, &model.fee_rows, cx);
    let theme = cx.theme();
    let fees: &[FeePosting] = &model.evaluation.fees;
    GroupBox::new().id("rules-fees").title(format!("Fees the rules add to the window — {} postings, {}", fees.len(), model.fee_total.format())).child(
        v_flex()
            .gap_4()
            .child(div().text_xs().text_color(theme.muted_foreground).child(
                "A fee is its own event on the same date, right after the payment it belongs to. Fees enter the projection once and appear in its calculation as “Fees from user rules”.",
            ))
            .child(if fees.is_empty() {
                div().text_sm().text_color(theme.muted_foreground).child("No fee rule fired in the window.").into_any_element()
            } else {
                grid::render("rule-fees-grid", &grids.rule_fees, cx).into_any_element()
            }),
    )
}

fn render_funding(model: &RulesModel, household: &Household, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    let name = |id: AccountId| household.account(id).map(|a| a.name.clone()).unwrap_or_else(|| id.to_string());
    GroupBox::new().id("rules-funding").title(format!("Funding order and bank selection on {}", model.funding_date.format("%d %b %Y"))).child(
        v_flex()
            .gap_4()
            .child(div().text_xs().text_color(theme.muted_foreground).child(
                "Funding rules are read by the funding search in priority order: preferred accounts with their floors first, prohibitions as hard exclusions. The steps below are what the search would be allowed to use today; a prohibition with a date lifts itself once that date passes.",
            ))
            .child(if model.funding.is_empty() {
                div().text_sm().text_color(theme.muted_foreground).child(if model.scenario_on || model.overlay_scenario.is_none() { "No funding rule is in force today.".to_string() } else { format!("No household-wide funding rule is in force today; rules scoped to scenario “{}” apply only when the scenario is ticked above.", model.overlay_scenario.and_then(|id| household.scenario(id)).map(|s| s.name.clone()).unwrap_or_default()) }).into_any_element()
            } else {
                v_flex()
                    .gap_1()
                    .children(model.funding.iter().enumerate().map(|(index, step)| {
                        h_flex()
                            .gap_3()
                            .items_center()
                            .child(div().w_6().flex_shrink_0().text_xs().text_color(theme.muted_foreground).child(format!("{}.", index + 1)))
                            .child(div().w_64().flex_shrink_0().text_sm().child(name(step.account)))
                            .child(h_flex().w_40().flex_shrink_0().child(if step.forbidden {
                                Tag::danger().xsmall().outline().child("not allowed")
                            } else if let Some(floor) = step.preserve {
                                Tag::secondary().xsmall().outline().child(format!("keep ≥ {}", floor.format()))
                            } else {
                                Tag::secondary().xsmall().outline().child("allowed")
                            }))
                            .child(div().flex_1().min_w_0().text_xs().text_color(theme.muted_foreground).child(step.reason.clone()))
                    }))
                    .into_any_element()
            })
            .child(div().text_xs().text_color(theme.muted_foreground).child("Bank selection for ordinary expenses, evaluated on today's settled balances:"))
            .child(if model.bank_selection.is_empty() {
                div().text_sm().text_color(theme.muted_foreground).child("No bank-selection rule.").into_any_element()
            } else {
                v_flex()
                    .gap_1()
                    .children(model.bank_selection.iter().map(|(category, account, why)| {
                        h_flex()
                            .gap_3()
                            .items_center()
                            .child(div().w_40().flex_shrink_0().text_sm().child(format!("“{category}” expenses")))
                            .child(div().w_64().flex_shrink_0().text_sm().font_weight(FontWeight::MEDIUM).child(name(*account)))
                            .child(div().flex_1().min_w_0().text_xs().text_color(theme.muted_foreground).child(why.clone()))
                    }))
                    .into_any_element()
            }),
    )
}

fn render_simulation(model: &RulesModel, household: &Household, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    GroupBox::new().id("rules-simulation").title("Simulation: the household forecast with the rule versus without it").child(
        v_flex()
            .gap_4()
            .child(div().text_xs().text_color(theme.muted_foreground).child(
                "Pick “Simulate” on a rule. The engine runs the same expected-case forecast twice — rule enabled, rule disabled — and reports the difference. Nothing is applied; the register is unchanged.",
            ))
            .child(match &model.simulation {
                None => div().text_sm().text_color(theme.muted_foreground).child("No rule selected for simulation.").into_any_element(),
                Some(sim) => {
                    let rule_name = household.rule(sim.rule).map(|r| r.name.clone()).unwrap_or_default();
                    let trigger = household.rule(sim.rule).map(|r| r.trigger).unwrap_or(Trigger::AnyPosting);
                    v_flex()
                        .gap_3()
                        .child(div().id("rules-simulation-summary").test_support().text_sm().child(format!(
                            "“{rule_name}” ({}, currently {}): end of window {} with vs {} without → {}; lowest point {} vs {} → {}; fee postings from this rule {}.",
                            trigger.label(),
                            if sim.currently_enabled { "enabled" } else { "disabled" },
                            sim.with_rule.end.money().format(),
                            sim.without_rule.end.money().format(),
                            sim.end_delta.format_signed(),
                            sim.with_rule.lowest.money().format(),
                            sim.without_rule.lowest.money().format(),
                            sim.lowest_delta.format_signed(),
                            sim.fee_delta.format()
                        )))
                        .child(
                            Table::new()
                                .child(
                                    TableHeader::new().child(
                                        TableRow::new()
                                            .child(TableHead::new().w_48().flex_shrink_0().child("Figure"))
                                            .child(TableHead::new().w_64().flex_shrink_0().text_right().child("With the rule"))
                                            .child(TableHead::new().w_64().flex_shrink_0().text_right().child("Without the rule"))
                                            .child(TableHead::new().min_w_0().child("Difference / note")),
                                    ),
                                )
                                .child(
                                    TableBody::new()
                                        .child(
                                            TableRow::new()
                                                .child(TableCell::new().w_48().flex_shrink_0().child("End of window"))
                                                .child(money_cell(sim.with_rule.end.money(), cx).w_64().flex_shrink_0())
                                                .child(money_cell(sim.without_rule.end.money(), cx).w_64().flex_shrink_0())
                                                .child(muted_cell(sim.end_delta.format_signed(), cx).min_w_0()),
                                        )
                                        .child(
                                            TableRow::new()
                                                .bg(theme.table_even)
                                                .child(TableCell::new().w_48().flex_shrink_0().child("Lowest point"))
                                                .child(money_cell(sim.with_rule.lowest.money(), cx).w_64().flex_shrink_0())
                                                .child(money_cell(sim.without_rule.lowest.money(), cx).w_64().flex_shrink_0())
                                                .child(muted_cell(
                                                    format!(
                                                        "{} · with: {} · without: {}",
                                                        sim.lowest_delta.format_signed(),
                                                        sim.with_rule.lowest_date.map(|d| d.format("%d %b %Y").to_string()).unwrap_or_else(|| "–".into()),
                                                        sim.without_rule.lowest_date.map(|d| d.format("%d %b %Y").to_string()).unwrap_or_else(|| "–".into())
                                                    ),
                                                    cx,
                                                )
                                                .min_w_0()),
                                        )
                                        .child(
                                            TableRow::new()
                                                .child(TableCell::new().w_48().flex_shrink_0().child("Floor breach"))
                                                .child(muted_cell(sim.with_rule.breach.summary(), cx).w_64().flex_shrink_0().text_right().overflow_hidden().text_ellipsis())
                                                .child(muted_cell(sim.without_rule.breach.summary(), cx).w_64().flex_shrink_0().text_right().overflow_hidden().text_ellipsis())
                                                .child(muted_cell("Whether the household floor is crossed in the window.", cx).min_w_0()),
                                        )
                                        .child(
                                            TableRow::new()
                                                .bg(theme.table_even)
                                                .child(TableCell::new().w_48().flex_shrink_0().child("Rules applied (record)"))
                                                .child(muted_cell(sim.with_rule.record.rules_applied.len().to_string(), cx).w_64().flex_shrink_0().text_right())
                                                .child(muted_cell(sim.without_rule.record.rules_applied.len().to_string(), cx).w_64().flex_shrink_0().text_right())
                                                .child(muted_cell(sim.with_rule.record.rules_applied.join(" · "), cx).min_w_0().overflow_hidden().text_ellipsis()),
                                        ),
                                ),
                        )
                        .into_any_element()
                }
            }),
    )
}
