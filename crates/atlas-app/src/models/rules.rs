//! Rules: the user's deterministic rules, the conflict-resolution inspector
//! (every decision with its losing candidates and why), the fee events the
//! rules add to the window, the funding order, bank selection and a
//! with-vs-without simulation.

use atlas_core::authz::Viewer;
use atlas_core::forecast::{Case, ForecastOptions};
use atlas_core::ids::{AccountId, RuleId, ScenarioId};
use atlas_core::liquidity::Boundary;
use atlas_core::model::Household;
use atlas_core::rules::{FeePosting, FundingStep, Rule, RuleAction, RuleDecision, RuleEvaluation, RuleSimulation, TieBreak, account_for_expense, evaluate, funding_order, simulate};
use atlas_core::{EngineResult, Money};
use chrono::NaiveDate;
use std::sync::Arc;

use crate::widgets::grid::{self, Cell, GridColumn, Row};

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
