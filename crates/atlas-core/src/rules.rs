//! User-defined rules (§14, M54): deterministic, versioned, effective-dated
//! instructions with an inspectable evaluation.
//!
//! - A [`Rule`] has a scope (whose specificity breaks ties), a trigger, typed
//!   conditions, one action, a priority, an effective range, an enabled flag,
//!   optional scenario applicability and a version history (§14.1–§14.2).
//! - [`evaluate`] applies rules to the window's occurrences and records one
//!   [`RuleDecision`] per (occurrence, action kind) — every candidate, why it
//!   lost, and the winner — so conflicts are never silent (§14.7).
//! - Fee actions become forecast postings ([`fee_postings`]); funding
//!   preferences become an ordered [`funding_order`] (§14.5); bank selection
//!   answers [`account_for_expense`] (§14.6).
//! - [`simulate`] answers "if this rule had been active …" (§14.8).
//! - [`validate`] refuses inverted periods, bad units and classification
//!   cycles with explicit errors (V054).

use crate::forecast::{BoundaryForecast, Case, ForecastOptions, forecast};
use crate::ids::*;
use crate::liquidity::Boundary;
use crate::model::Household;
use crate::money::Money;
use crate::timeline::{Direction, Occurrence};
use crate::{EngineError, EngineResult};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// §14.2 — what a rule applies to. More specific scopes win ties (§14.7).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum RuleScope {
    Household,
    Category(String),
    Institution(String),
    Person(PersonId),
    Company(CompanyId),
    Account(AccountId),
    Scenario(ScenarioId),
}

impl RuleScope {
    /// Specificity rank used after priority (§14.7 "scope specificity").
    pub fn specificity(&self) -> u8 {
        match self {
            RuleScope::Household => 0,
            RuleScope::Category(_) => 1,
            RuleScope::Institution(_) => 2,
            RuleScope::Person(_) | RuleScope::Company(_) => 3,
            RuleScope::Account(_) => 4,
            RuleScope::Scenario(_) => 5,
        }
    }

    pub fn describe(&self, household: &Household) -> String {
        match self {
            RuleScope::Household => "whole household".into(),
            RuleScope::Category(c) => format!("category “{c}”"),
            RuleScope::Institution(i) => format!("institution {i}"),
            RuleScope::Person(id) => household.entity_name(EntityRef::Person(*id)),
            RuleScope::Company(id) => household.entity_name(EntityRef::Company(*id)),
            RuleScope::Account(id) => household.account(*id).map(|a| a.name.clone()).unwrap_or_else(|| id.to_string()),
            RuleScope::Scenario(id) => household.scenario(*id).map(|s| format!("scenario “{}”", s.name)).unwrap_or_else(|| id.to_string()),
        }
    }

    fn matches(&self, household: &Household, occurrence: &Occurrence, category: &str) -> bool {
        match self {
            RuleScope::Household => true,
            RuleScope::Category(c) => c.eq_ignore_ascii_case(category),
            RuleScope::Institution(i) => household.account(occurrence.account).is_some_and(|a| a.institution.eq_ignore_ascii_case(i)),
            RuleScope::Person(id) => occurrence.entity == EntityRef::Person(*id),
            RuleScope::Company(id) => occurrence.entity == EntityRef::Company(*id),
            RuleScope::Account(id) => occurrence.account == *id || occurrence.linked_account == Some(*id) || matches!(occurrence.direction, Direction::Transfer { to } if to == *id),
            RuleScope::Scenario(id) => occurrence.scenario == Some(*id),
        }
    }
}

/// §14.1 — what fires a rule.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Trigger {
    /// Any posting of a matching occurrence.
    AnyPosting,
    Expense,
    Income,
    Transfer,
    /// Evaluated by the funding search (§14.5), not by postings.
    Funding,
}

impl Trigger {
    pub fn label(self) -> &'static str {
        match self {
            Trigger::AnyPosting => "any posting",
            Trigger::Expense => "expenses",
            Trigger::Income => "incomes",
            Trigger::Transfer => "transfers",
            Trigger::Funding => "funding searches",
        }
    }

    fn matches(self, direction: Direction) -> bool {
        match (self, direction) {
            (Trigger::AnyPosting, _) => true,
            (Trigger::Expense, Direction::Expense) | (Trigger::Income, Direction::Income) | (Trigger::Transfer, Direction::Transfer { .. }) => true,
            _ => false,
        }
    }
}

/// §14.1 — typed conditions (M54: typed variables and currency units).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Condition {
    AmountAbove(Money),
    AmountBelow(Money),
    CategoryIs(String),
    AccountIs(AccountId),
    /// The account's currency differs from the household base currency (§14.4).
    ForeignCurrency,
    OnOrAfter(NaiveDate),
    OnOrBefore(NaiveDate),
}

impl Condition {
    pub fn describe(&self, household: &Household) -> String {
        match self {
            Condition::AmountAbove(m) => format!("amount > {}", m.format()),
            Condition::AmountBelow(m) => format!("amount < {}", m.format()),
            Condition::CategoryIs(c) => format!("category = {c}"),
            Condition::AccountIs(id) => format!("account = {}", household.account(*id).map(|a| a.name.clone()).unwrap_or_else(|| id.to_string())),
            Condition::ForeignCurrency => "account currency ≠ base currency".into(),
            Condition::OnOrAfter(d) => format!("date ≥ {}", d.format("%d %b %Y")),
            Condition::OnOrBefore(d) => format!("date ≤ {}", d.format("%d %b %Y")),
        }
    }

    fn holds(&self, household: &Household, occurrence: &Occurrence, amount: Money, category: &str) -> bool {
        match self {
            Condition::AmountAbove(m) => amount.minor() > m.minor(),
            Condition::AmountBelow(m) => amount.minor() < m.minor(),
            Condition::CategoryIs(c) => c.eq_ignore_ascii_case(category),
            Condition::AccountIs(id) => occurrence.account == *id,
            Condition::ForeignCurrency => household.account(occurrence.account).is_some_and(|a| a.currency != household.base_currency),
            Condition::OnOrAfter(d) => occurrence.due >= *d,
            Condition::OnOrBefore(d) => occurrence.due <= *d,
        }
    }
}

/// §14.1 — what a rule does.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum RuleAction {
    /// Adds a fee posting on the occurrence's account (§14.4 "1.5% bank fee event").
    AddFee { basis_points: u32, fixed: Option<Money>, label: String },
    /// Deterministic classification: the occurrence counts under this category (§2.2).
    Classify { category: String },
    /// §14.5 funding preference: use this account (keeping `preserve`) in this order.
    PreferAccount { account: AccountId, preserve: Option<Money> },
    /// §14.5 "Do not use Company Alpha unless purchase date is after Dec 1".
    ForbidAccount { account: AccountId, unless_after: Option<NaiveDate> },
    /// §14.6 bank selection for ordinary expenses.
    BankSelection { prefer: AccountId, fallback: AccountId, when_below: Money },
}

impl RuleAction {
    /// Actions of the same kind compete for one decision (§14.7).
    pub fn kind(&self) -> &'static str {
        match self {
            RuleAction::AddFee { .. } => "fee",
            RuleAction::Classify { .. } => "classification",
            RuleAction::PreferAccount { .. } | RuleAction::ForbidAccount { .. } => "funding",
            RuleAction::BankSelection { .. } => "bank selection",
        }
    }

    pub fn describe(&self, household: &Household) -> String {
        let name = |id: AccountId| household.account(id).map(|a| a.name.clone()).unwrap_or_else(|| id.to_string());
        match self {
            RuleAction::AddFee { basis_points, fixed, label } => {
                let mut parts = Vec::new();
                if *basis_points > 0 {
                    parts.push(format!("{}.{:02}%", basis_points / 100, basis_points % 100));
                }
                if let Some(fixed) = fixed {
                    parts.push(fixed.format());
                }
                if parts.is_empty() {
                    format!("waive the fee ({label})")
                } else {
                    format!("add a {} fee event “{label}”", parts.join(" + "))
                }
            }
            RuleAction::Classify { category } => format!("classify as “{category}”"),
            RuleAction::PreferAccount { account, preserve } => match preserve {
                Some(floor) => format!("use {} while preserving {}", name(*account), floor.format()),
                None => format!("use {}", name(*account)),
            },
            RuleAction::ForbidAccount { account, unless_after } => match unless_after {
                Some(date) => format!("do not use {} unless the date is after {}", name(*account), date.format("%d %b %Y")),
                None => format!("never use {}", name(*account)),
            },
            RuleAction::BankSelection { prefer, fallback, when_below } => {
                format!("prefer {}; use {} only when {} would fall below {}", name(*prefer), name(*fallback), name(*prefer), when_below.format())
            }
        }
    }
}

/// One entry of a rule's history (§14.1 "Version/history").
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RuleVersion {
    pub version: u32,
    pub changed_on: NaiveDate,
    pub summary: String,
}

/// §14.1 — a deterministic user rule.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Rule {
    pub id: RuleId,
    pub name: String,
    pub scope: RuleScope,
    pub trigger: Trigger,
    pub conditions: Vec<Condition>,
    pub action: RuleAction,
    /// Higher wins (§14.7 "explicit priority").
    pub priority: i32,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
    pub enabled: bool,
    /// Applies only inside this scenario when set (§14.1 "scenario applicability").
    pub scenario: Option<ScenarioId>,
    pub explanation: String,
    pub version: u32,
    pub history: Vec<RuleVersion>,
}

impl Rule {
    pub fn is_effective_on(&self, date: NaiveDate) -> bool {
        date >= self.effective_from && self.effective_to.is_none_or(|end| date <= end)
    }

    /// Records a change as a new version.
    pub fn bump(&mut self, on: NaiveDate, summary: impl Into<String>) {
        self.version += 1;
        self.history.push(RuleVersion { version: self.version, changed_on: on, summary: summary.into() });
    }
}

/// §14.7 — how ties after priority and specificity are broken.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
pub enum TieBreak {
    /// The rule created first wins.
    #[default]
    OldestRule,
    /// The most recently changed rule wins.
    NewestVersion,
}

impl TieBreak {
    pub const ALL: [TieBreak; 2] = [TieBreak::OldestRule, TieBreak::NewestVersion];

    pub fn slug(self) -> &'static str {
        match self {
            TieBreak::OldestRule => "oldest-rule",
            TieBreak::NewestVersion => "newest-version",
        }
    }

    pub fn from_slug(slug: &str) -> Option<TieBreak> {
        TieBreak::ALL.into_iter().find(|t| t.slug() == slug)
    }

    pub fn label(self) -> &'static str {
        match self {
            TieBreak::OldestRule => "oldest rule wins",
            TieBreak::NewestVersion => "most recently changed rule wins",
        }
    }
}

/// One candidate in a decision, with why it did or did not win.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Candidate {
    pub rule: RuleId,
    pub name: String,
    pub priority: i32,
    pub specificity: u8,
    pub outcome: String,
}

/// §14.7 — an inspectable conflict resolution for one occurrence and one action kind.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RuleDecision {
    pub occurrence_label: String,
    pub date: NaiveDate,
    pub account: AccountId,
    pub action_kind: &'static str,
    pub candidates: Vec<Candidate>,
    pub chosen: Option<RuleId>,
    pub resolution: String,
}

/// A fee posting produced by a rule.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FeePosting {
    pub date: NaiveDate,
    pub account: AccountId,
    /// Negative: cash leaves.
    pub amount: Money,
    pub label: String,
    pub rule: RuleId,
    pub base_series: SeriesId,
    pub intraday_order: u16,
}

/// Everything one evaluation pass produced.
#[derive(Clone, Debug, Default)]
pub struct RuleEvaluation {
    pub fees: Vec<FeePosting>,
    pub decisions: Vec<RuleDecision>,
    /// Occurrence label → category after classification rules.
    pub classifications: Vec<(String, NaiveDate, String, String)>,
}

/// Applies every enabled, effective, scope-matching rule to the window's live
/// occurrences with deterministic conflict resolution (§14.7).
pub fn evaluate(household: &Household, through: NaiveDate, scenario: Option<ScenarioId>, case: Case, tie_break: TieBreak) -> EngineResult<RuleEvaluation> {
    let mut evaluation = RuleEvaluation::default();
    let occurrences: Vec<Occurrence> = household.expand_all(household.as_of, through, scenario).into_iter().filter(|o| o.is_live()).collect();
    for occurrence in &occurrences {
        let Some(series) = household.series_by_id(occurrence.series) else { continue };
        let amount = case.amount(&occurrence.amount, series.direction).checked_sub(occurrence.fulfilled)?.clamped_at_zero();
        if amount.is_zero() {
            continue;
        }
        let mut category = series.category.clone();
        // Classification rules first: they can change the category later rules see.
        for kind in ["classification", "fee"] {
            let mut applicable: Vec<&Rule> = household
                .rules
                .iter()
                .filter(|r| r.enabled && r.action.kind() == kind && r.trigger != Trigger::Funding)
                .filter(|r| r.is_effective_on(occurrence.due))
                .filter(|r| r.scenario.is_none() || r.scenario == scenario)
                .filter(|r| r.trigger.matches(series.direction))
                .filter(|r| r.scope.matches(household, occurrence, &category))
                .filter(|r| r.conditions.iter().all(|c| c.holds(household, occurrence, amount, &category)))
                .collect();
            if applicable.is_empty() {
                continue;
            }
            applicable.sort_by(|a, b| {
                b.priority
                    .cmp(&a.priority)
                    .then(b.scope.specificity().cmp(&a.scope.specificity()))
                    .then_with(|| match tie_break {
                        TieBreak::OldestRule => a.id.cmp(&b.id),
                        TieBreak::NewestVersion => b.history.last().map(|h| h.changed_on).cmp(&a.history.last().map(|h| h.changed_on)),
                    })
            });
            let winner = applicable[0];
            let mut candidates = Vec::new();
            for (index, rule) in applicable.iter().enumerate() {
                let outcome = if index == 0 {
                    "chosen".to_string()
                } else if rule.priority < winner.priority {
                    format!("lower priority ({} < {})", rule.priority, winner.priority)
                } else if rule.scope.specificity() < winner.scope.specificity() {
                    format!("less specific scope ({} < {})", rule.scope.specificity(), winner.scope.specificity())
                } else {
                    format!("tie-break: {}", tie_break.label())
                };
                candidates.push(Candidate { rule: rule.id, name: rule.name.clone(), priority: rule.priority, specificity: rule.scope.specificity(), outcome });
            }
            let resolution = if applicable.len() == 1 {
                "only applicable rule".to_string()
            } else if applicable[1].priority < winner.priority {
                "explicit priority".to_string()
            } else if applicable[1].scope.specificity() < winner.scope.specificity() {
                "scope specificity".to_string()
            } else {
                format!("tie-break policy: {}", tie_break.label())
            };
            evaluation.decisions.push(RuleDecision {
                occurrence_label: occurrence.label.clone(),
                date: occurrence.due,
                account: occurrence.account,
                action_kind: kind,
                candidates,
                chosen: Some(winner.id),
                resolution,
            });
            match &winner.action {
                RuleAction::Classify { category: new_category } => {
                    evaluation.classifications.push((occurrence.label.clone(), occurrence.due, category.clone(), new_category.clone()));
                    category = new_category.clone();
                }
                RuleAction::AddFee { basis_points, fixed, label } => {
                    let mut fee = amount.share_basis_points(*basis_points);
                    if let Some(fixed) = fixed {
                        fee = fee.checked_add(*fixed)?;
                    }
                    if fee.is_positive() {
                        evaluation.fees.push(FeePosting {
                            date: occurrence.due,
                            account: occurrence.account,
                            amount: fee.negated(),
                            label: format!("Fee: {label} ({})", winner.name),
                            rule: winner.id,
                            base_series: occurrence.series,
                            intraday_order: occurrence.intraday_order + 1,
                        });
                    }
                }
                _ => {}
            }
        }
    }
    Ok(evaluation)
}

/// Fee postings for the forecast (each fee enters cash once, M01).
pub fn fee_postings(household: &Household, through: NaiveDate, scenario: Option<ScenarioId>, case: Case) -> EngineResult<Vec<FeePosting>> {
    Ok(evaluate(household, through, scenario, case, household_tie_break(household))?.fees)
}

fn household_tie_break(household: &Household) -> TieBreak {
    household.rule_tie_break
}

/// One step of a funding order (§14.5).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FundingStep {
    pub account: AccountId,
    pub preserve: Option<Money>,
    pub forbidden: bool,
    pub reason: String,
    pub rule: RuleId,
}

/// §14.5 — the ordered accounts a funding search may use on `date` (higher
/// priority first), with floors and prohibitions from the funding rules.
pub fn funding_order(household: &Household, date: NaiveDate, scenario: Option<ScenarioId>) -> Vec<FundingStep> {
    let mut rules: Vec<&Rule> = household
        .rules
        .iter()
        .filter(|r| r.enabled && r.trigger == Trigger::Funding && r.is_effective_on(date))
        .filter(|r| r.scenario.is_none() || r.scenario == scenario)
        .collect();
    rules.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.id.cmp(&b.id)));
    let mut steps = Vec::new();
    for rule in rules {
        match &rule.action {
            RuleAction::PreferAccount { account, preserve } => steps.push(FundingStep {
                account: *account,
                preserve: *preserve,
                forbidden: false,
                reason: format!("{} (priority {})", rule.name, rule.priority),
                rule: rule.id,
            }),
            RuleAction::ForbidAccount { account, unless_after } => {
                let forbidden = unless_after.is_none_or(|d| date <= d);
                steps.push(FundingStep {
                    account: *account,
                    preserve: None,
                    forbidden,
                    reason: match unless_after {
                        Some(d) if forbidden => format!("{}: not before {}", rule.name, d.succ_opt().unwrap_or(*d).format("%d %b %Y")),
                        Some(d) => format!("{}: allowed after {}", rule.name, d.format("%d %b %Y")),
                        None => format!("{}: never", rule.name),
                    },
                    rule: rule.id,
                })
            }
            _ => {}
        }
    }
    steps
}

/// §14.6 — which account an ordinary expense of `category` should use on
/// `date`, given projected balances, with the decision spelled out.
pub fn account_for_expense(household: &Household, category: &str, date: NaiveDate, balance_of: &dyn Fn(AccountId) -> Money) -> Option<(AccountId, String)> {
    let mut rules: Vec<&Rule> = household
        .rules
        .iter()
        .filter(|r| r.enabled && r.is_effective_on(date) && matches!(r.action, RuleAction::BankSelection { .. }))
        .filter(|r| match &r.scope {
            RuleScope::Category(c) => c.eq_ignore_ascii_case(category),
            RuleScope::Household => true,
            _ => false,
        })
        .collect();
    rules.sort_by(|a, b| b.priority.cmp(&a.priority).then(b.scope.specificity().cmp(&a.scope.specificity())).then(a.id.cmp(&b.id)));
    let rule = rules.first()?;
    let RuleAction::BankSelection { prefer, fallback, when_below } = &rule.action else { return None };
    let balance = balance_of(*prefer);
    let name = |id: AccountId| household.account(id).map(|a| a.name.clone()).unwrap_or_else(|| id.to_string());
    if balance.minor() < when_below.minor() {
        Some((*fallback, format!("{}: {} would fall below {} ({}), so {} is used", rule.name, name(*prefer), when_below.format(), balance.format(), name(*fallback))))
    } else {
        Some((*prefer, format!("{}: {} stays above {} ({})", rule.name, name(*prefer), when_below.format(), balance.format())))
    }
}

/// §14.8 — the effect of one rule: the forecast with the rule enabled versus disabled.
#[derive(Clone, Debug)]
pub struct RuleSimulation {
    pub rule: RuleId,
    pub currently_enabled: bool,
    pub with_rule: BoundaryForecast,
    pub without_rule: BoundaryForecast,
    pub end_delta: Money,
    pub lowest_delta: Money,
    pub fee_delta: Money,
}

/// Runs the boundary forecast twice, with the rule enabled and disabled, and
/// reports the differences.
pub fn simulate(household: &Household, rule: RuleId, boundary: Boundary, options: ForecastOptions) -> EngineResult<RuleSimulation> {
    let currently_enabled = household.rules.iter().find(|r| r.id == rule).ok_or(EngineError::UnknownRule(rule))?.enabled;
    let mut on = household.clone();
    let mut off = household.clone();
    if let Some(r) = on.rules.iter_mut().find(|r| r.id == rule) {
        r.enabled = true;
    }
    if let Some(r) = off.rules.iter_mut().find(|r| r.id == rule) {
        r.enabled = false;
    }
    let with_rule = forecast(&on, boundary, options)?;
    let without_rule = forecast(&off, boundary, options)?;
    let fees_on = fee_postings(&on, options.through, options.scenario, options.case)?.iter().filter(|f| f.rule == rule).map(|f| f.amount.minor()).sum::<i64>();
    Ok(RuleSimulation {
        rule,
        currently_enabled,
        end_delta: with_rule.end.money().checked_sub(without_rule.end.money())?,
        lowest_delta: with_rule.lowest.money().checked_sub(without_rule.lowest.money())?,
        fee_delta: Money::new(fees_on, household.base_currency),
        with_rule,
        without_rule,
    })
}

/// V054 — refuses inverted effective periods, invalid units, unknown targets
/// and classification cycles, naming the problem.
pub fn validate(household: &Household, rule: &Rule) -> EngineResult<()> {
    if let Some(end) = rule.effective_to
        && end < rule.effective_from
    {
        return Err(EngineError::Insufficient(format!("rule “{}”: effective period ends ({end}) before it starts ({})", rule.name, rule.effective_from)));
    }
    match &rule.action {
        RuleAction::AddFee { basis_points, fixed, .. } => {
            if *basis_points > 10_000 {
                return Err(EngineError::Insufficient(format!("rule “{}”: a fee of {} basis points exceeds 100%", rule.name, basis_points)));
            }
            if let Some(fixed) = fixed
                && fixed.currency() != household.base_currency
            {
                return Err(crate::MoneyError::CurrencyMismatch { left: household.base_currency, right: fixed.currency() }.into());
            }
        }
        RuleAction::Classify { category } => {
            // A → B while another enabled rule maps B → A on the same trigger is a cycle.
            if let RuleScope::Category(from) = &rule.scope
                && household.rules.iter().any(|other| {
                    other.id != rule.id
                        && other.enabled
                        && matches!(&other.action, RuleAction::Classify { category: back } if back.eq_ignore_ascii_case(from))
                        && matches!(&other.scope, RuleScope::Category(c) if c.eq_ignore_ascii_case(category))
                })
            {
                return Err(EngineError::Insufficient(format!("rule “{}”: classification cycle {from} → {category} → {from}", rule.name)));
            }
        }
        RuleAction::PreferAccount { account, .. } | RuleAction::ForbidAccount { account, .. } => {
            household.account(*account).ok_or(EngineError::UnknownAccount(*account))?;
        }
        RuleAction::BankSelection { prefer, fallback, when_below } => {
            household.account(*prefer).ok_or(EngineError::UnknownAccount(*prefer))?;
            household.account(*fallback).ok_or(EngineError::UnknownAccount(*fallback))?;
            if when_below.currency() != household.base_currency {
                return Err(crate::MoneyError::CurrencyMismatch { left: household.base_currency, right: when_below.currency() }.into());
            }
        }
    }
    for condition in &rule.conditions {
        if let Condition::AmountAbove(m) | Condition::AmountBelow(m) = condition
            && m.currency() != household.base_currency
        {
            return Err(crate::MoneyError::CurrencyMismatch { left: household.base_currency, right: m.currency() }.into());
        }
    }
    Ok(())
}

impl Household {
    pub fn rule(&self, id: RuleId) -> Option<&Rule> {
        self.rules.iter().find(|r| r.id == id)
    }

    pub fn next_rule_id(&self) -> RuleId {
        RuleId::new(self.rules.iter().map(|r| r.id.raw()).max().unwrap_or(0) + 1)
    }

    /// Adds a validated rule with its first version entry.
    pub fn add_rule(&mut self, mut rule: Rule) -> EngineResult<RuleId> {
        validate(self, &rule)?;
        if rule.history.is_empty() {
            rule.history.push(RuleVersion { version: rule.version.max(1), changed_on: self.as_of, summary: "created".into() });
            rule.version = rule.version.max(1);
        }
        let id = rule.id;
        self.rules.push(rule);
        Ok(id)
    }

    /// Enables or disables a rule as a new version (§14.1 history).
    pub fn set_rule_enabled(&mut self, id: RuleId, enabled: bool) -> EngineResult<()> {
        let as_of = self.as_of;
        let rule = self.rules.iter_mut().find(|r| r.id == id).ok_or(EngineError::UnknownRule(id))?;
        if rule.enabled != enabled {
            rule.enabled = enabled;
            rule.bump(as_of, if enabled { "enabled" } else { "disabled" });
        }
        Ok(())
    }

    /// Removes a rule; the evaluation simply no longer sees it.
    pub fn remove_rule(&mut self, id: RuleId) -> EngineResult<()> {
        let before = self.rules.len();
        self.rules.retain(|r| r.id != id);
        if self.rules.len() == before { Err(EngineError::UnknownRule(id)) } else { Ok(()) }
    }

    /// Changes a rule's priority as a new version.
    pub fn set_rule_priority(&mut self, id: RuleId, priority: i32) -> EngineResult<()> {
        let as_of = self.as_of;
        let rule = self.rules.iter_mut().find(|r| r.id == id).ok_or(EngineError::UnknownRule(id))?;
        if rule.priority != priority {
            let old = rule.priority;
            rule.priority = priority;
            rule.bump(as_of, format!("priority {old} → {priority}"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{self, ids, pkr};
    use chrono::Datelike;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn conflicts_resolve_by_priority_then_specificity_and_are_inspectable() {
        let household = fixtures::plan_household();
        let evaluation = evaluate(&household, fixtures::default_horizon(), None, Case::Expected, TieBreak::OldestRule).unwrap();
        // The foreign subscription: the category fee (1.5%) competes with the account-scoped
        // promo waiver (0%) for Oct–Nov; specificity picks the waiver, then the fee returns.
        let subscription: Vec<&RuleDecision> = evaluation.decisions.iter().filter(|dcn| dcn.occurrence_label.starts_with("Foreign software") && dcn.action_kind == "fee").collect();
        assert!(subscription.len() >= 4);
        let october = subscription.iter().find(|dcn| dcn.date == d(2026, 10, 3)).unwrap();
        assert_eq!(october.candidates.len(), 2);
        assert_eq!(october.resolution, "scope specificity");
        assert_eq!(october.chosen, Some(ids::RULE_FOREIGN_FEE_WAIVER));
        assert!(october.candidates.iter().any(|c| c.outcome.starts_with("less specific scope")));
        let december = subscription.iter().find(|dcn| dcn.date == d(2026, 12, 3)).unwrap();
        assert_eq!(december.candidates.len(), 1);
        assert_eq!(december.chosen, Some(ids::RULE_FOREIGN_FEE));
        // Fees: none in Oct/Nov (waiver), 1.5% of 12,000 = 180 from December.
        assert!(!evaluation.fees.iter().any(|f| f.base_series == ids::FOREIGN_SUBSCRIPTION && f.date.month() == 10));
        let december_fee = evaluation.fees.iter().find(|f| f.base_series == ids::FOREIGN_SUBSCRIPTION && f.date == d(2026, 12, 3)).unwrap();
        assert_eq!(december_fee.amount, pkr(-180));
        // The Bank B transfer fee: 250 fixed on the 80,000 ATM withdrawal (> 50,000).
        let atm = evaluation.fees.iter().find(|f| f.base_series == ids::ATM_WITHDRAWAL).unwrap();
        assert_eq!(atm.amount, pkr(-250));
        // Deterministic: the same evaluation twice.
        let again = evaluate(&household, fixtures::default_horizon(), None, Case::Expected, TieBreak::OldestRule).unwrap();
        assert_eq!(again.fees, evaluation.fees);
        assert_eq!(again.decisions, evaluation.decisions);
    }

    #[test]
    fn funding_order_and_bank_selection_follow_the_rules() {
        let household = fixtures::plan_household();
        let before = funding_order(&household, d(2026, 11, 15), Some(ids::BUY_CAR));
        assert_eq!(before[0].account, ids::ALPHA_OPERATING);
        assert!(before[0].forbidden, "Company Alpha is off limits before Dec 1 (§14.5)");
        assert_eq!(before[1].account, ids::SHARED_SAVINGS);
        assert_eq!(before[1].preserve, Some(pkr(1_000_000)));
        assert_eq!(before[2].account, ids::PERSON_A_CURRENT);
        let after = funding_order(&household, d(2026, 12, 15), Some(ids::BUY_CAR));
        assert!(!after[0].forbidden);
        // §14.6: prefer B checking; fall back to shared savings when it would drop below 100,000.
        let (account, why) = account_for_expense(&household, "Living", d(2026, 10, 15), &|_| pkr(500_000)).unwrap();
        assert_eq!(account, ids::PERSON_B_CHECKING);
        assert!(why.contains("stays above"));
        let (account, why) = account_for_expense(&household, "Living", d(2026, 10, 15), &|_| pkr(60_000)).unwrap();
        assert_eq!(account, ids::SHARED_SAVINGS);
        assert!(why.contains("would fall below"));
    }

    #[test]
    fn simulation_shows_what_a_rule_changes() {
        let household = fixtures::plan_household();
        let options = ForecastOptions { through: fixtures::default_horizon(), scenario: None, case: Case::Expected };
        let sim = simulate(&household, ids::RULE_TRANSFER_FEE, Boundary::Household, options).unwrap();
        assert!(sim.currently_enabled);
        // One 250 fee per transfer above 50,000 touching the account; the ATM withdrawals alone
        // give four, and the delta equals exactly the rule's own fee postings.
        let evaluation = evaluate(&household, fixtures::default_horizon(), None, Case::Expected, TieBreak::OldestRule).unwrap();
        let own_fees: Vec<&FeePosting> = evaluation.fees.iter().filter(|f| f.rule == ids::RULE_TRANSFER_FEE).collect();
        assert!(own_fees.iter().filter(|f| f.base_series == ids::ATM_WITHDRAWAL).count() >= 4);
        assert!(own_fees.iter().all(|f| f.amount == pkr(-250)));
        assert_eq!(sim.fee_delta, pkr(-250 * own_fees.len() as i64));
        assert_eq!(sim.end_delta, sim.fee_delta);
        assert!(sim.lowest_delta.minor() <= 0);
    }

    #[test]
    fn v054_validation_names_the_problem() {
        let mut household = fixtures::plan_household();
        let mut bad = household.rule(ids::RULE_FOREIGN_FEE).unwrap().clone();
        bad.id = household.next_rule_id();
        bad.effective_to = Some(d(2025, 1, 1));
        assert!(household.add_rule(bad.clone()).unwrap_err().to_string().contains("ends"));
        bad.effective_to = None;
        bad.action = RuleAction::AddFee { basis_points: 20_000, fixed: None, label: "x".into() };
        assert!(household.add_rule(bad.clone()).unwrap_err().to_string().contains("exceeds 100%"));
        // Classification cycle: Living → Housing while Housing → Living exists.
        let a = Rule {
            id: household.next_rule_id(),
            name: "A".into(),
            scope: RuleScope::Category("Living".into()),
            trigger: Trigger::Expense,
            conditions: Vec::new(),
            action: RuleAction::Classify { category: "Housing".into() },
            priority: 1,
            effective_from: d(2026, 1, 1),
            effective_to: None,
            enabled: true,
            scenario: None,
            explanation: String::new(),
            version: 1,
            history: Vec::new(),
        };
        household.add_rule(a.clone()).unwrap();
        let mut b = a.clone();
        b.id = household.next_rule_id();
        b.scope = RuleScope::Category("Housing".into());
        b.action = RuleAction::Classify { category: "Living".into() };
        assert!(household.add_rule(b).unwrap_err().to_string().contains("cycle"));
        // Version history grows with changes.
        household.set_rule_enabled(ids::RULE_FOREIGN_FEE, false).unwrap();
        let rule = household.rule(ids::RULE_FOREIGN_FEE).unwrap();
        assert_eq!(rule.version, 2);
        assert_eq!(rule.history.last().unwrap().summary, "disabled");
    }
}
