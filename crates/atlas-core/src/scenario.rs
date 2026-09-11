//! Scenario engine (§18, M8): a scenario is an overlay over the baseline —
//! a list of explicit, typed changes plus the events, rules and assumptions
//! tagged with it — never a separate database.
//!
//! - [`ScenarioChange`] is the explicit part of an overlay (§18.2).
//! - [`Household::apply_scenarios`] builds the overlaid household the rest of
//!   the engine runs on unchanged; composition (§18.3) is the same function
//!   over several scenarios after [`Household::check_compatibility`].
//! - [`compare`] produces the §18.4 comparison metrics and the F139 difference
//!   attribution: every posting of the window belongs to exactly one bucket,
//!   so the buckets sum to the end-of-window difference to the minor unit.

use crate::breach::PathPoint;
use crate::forecast::{BoundaryForecast, Case, ForecastOptions, forecast};
use crate::ids::*;
use crate::liquidity::Boundary;
use crate::model::{Company, Employee, Household, Scenario, TaxRule};
use crate::money::Money;
use crate::timeline::{AmountSpec, DateSpec, Recurrence};
use crate::{EngineError, EngineResult};
use chrono::{Days, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// §18.2 — one explicit change of an overlay. Events, funding rules and
/// assumptions *added* by a scenario are the objects tagged with it.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum ScenarioChange {
    RemoveSeries { series: SeriesId, reason: String },
    /// Ends a stream: the last occurrence is on or before `last_on` (§9.4).
    EndSeries { series: SeriesId, last_on: NaiveDate, reason: String },
    ChangeAmount { series: SeriesId, from: NaiveDate, amount: AmountSpec, reason: String },
    /// Moves every date of a series by `days` (negative = earlier).
    MoveDates { series: SeriesId, days: i64, reason: String },
    AddEmployment { company: CompanyId, employee: Employee },
    AddCompany { company: Company },
    RemoveCompany { company: CompanyId, reason: String },
    AddTaxRule { rule: TaxRule },
    DisableTaxRule { rule: TaxRuleId, reason: String },
}

impl ScenarioChange {
    /// The series the change touches, for compatibility checks.
    pub fn series(&self) -> Option<SeriesId> {
        match self {
            ScenarioChange::RemoveSeries { series, .. }
            | ScenarioChange::EndSeries { series, .. }
            | ScenarioChange::ChangeAmount { series, .. }
            | ScenarioChange::MoveDates { series, .. } => Some(*series),
            _ => None,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            ScenarioChange::RemoveSeries { .. } => "remove events",
            ScenarioChange::EndSeries { .. } => "end a stream",
            ScenarioChange::ChangeAmount { .. } => "change amount",
            ScenarioChange::MoveDates { .. } => "change dates",
            ScenarioChange::AddEmployment { .. } => "add employment",
            ScenarioChange::AddCompany { .. } => "add company",
            ScenarioChange::RemoveCompany { .. } => "remove company",
            ScenarioChange::AddTaxRule { .. } => "change tax rules",
            ScenarioChange::DisableTaxRule { .. } => "change tax rules",
        }
    }

    pub fn describe(&self, household: &Household) -> String {
        let series_name = |id: SeriesId| household.series_by_id(id).map(|s| s.name.clone()).unwrap_or_else(|| id.to_string());
        let company_name = |id: CompanyId| household.company(id).map(|c| c.name.clone()).unwrap_or_else(|| id.to_string());
        match self {
            ScenarioChange::RemoveSeries { series, reason } => format!("remove “{}” — {reason}", series_name(*series)),
            ScenarioChange::EndSeries { series, last_on, reason } => format!("end “{}” after {} — {reason}", series_name(*series), last_on.format("%d %b %Y")),
            ScenarioChange::ChangeAmount { series, from, amount, reason } => format!("“{}” becomes {} from {} — {reason}", series_name(*series), amount.describe(), from.format("%d %b %Y")),
            ScenarioChange::MoveDates { series, days, reason } => format!("move “{}” by {days} day{} — {reason}", series_name(*series), if days.abs() == 1 { "" } else { "s" }),
            ScenarioChange::AddEmployment { company, employee } => format!("{} employs {} at {} per month from {}", company_name(*company), employee.name, employee.monthly_gross.format(), employee.start.format("%d %b %Y")),
            ScenarioChange::AddCompany { company } => format!("add company “{}”", company.name),
            ScenarioChange::RemoveCompany { company, reason } => format!("remove company “{}” — {reason}", company_name(*company)),
            ScenarioChange::AddTaxRule { rule } => format!("tax rule “{}” ({}) applies inside the scenario", rule.name, rule.describe_kind()),
            ScenarioChange::DisableTaxRule { rule, reason } => format!("tax rule {rule} does not apply — {reason}"),
        }
    }
}

/// One line of an overlay listing: explicit changes and tagged objects alike.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct OverlayEntry {
    pub scenario: ScenarioId,
    pub kind: &'static str,
    pub text: String,
    pub explicit: bool,
}

/// §18.3 — why two scenarios cannot be combined.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Incompatibility {
    pub first: ScenarioId,
    pub second: ScenarioId,
    pub reason: String,
}

impl Household {
    /// Every scenario the set expands to (composition members, recursively).
    pub fn scenario_closure(&self, ids: &[ScenarioId]) -> Vec<ScenarioId> {
        let mut seen: Vec<ScenarioId> = Vec::new();
        let mut stack: Vec<ScenarioId> = ids.to_vec();
        while let Some(id) = stack.pop() {
            if seen.contains(&id) {
                continue;
            }
            seen.push(id);
            if let Some(scenario) = self.scenario(id) {
                stack.extend(scenario.composed_of.iter().copied());
            }
        }
        seen
    }

    /// §18.3 — pairs of scenarios in the set whose explicit changes collide.
    pub fn check_compatibility(&self, ids: &[ScenarioId]) -> Vec<Incompatibility> {
        let closure = self.scenario_closure(ids);
        let mut problems = Vec::new();
        for (i, first) in closure.iter().enumerate() {
            for second in closure.iter().skip(i + 1) {
                let (Some(a), Some(b)) = (self.scenario(*first), self.scenario(*second)) else { continue };
                for change_a in &a.changes {
                    for change_b in &b.changes {
                        if change_a == change_b {
                            continue;
                        }
                        let reason = match (change_a, change_b) {
                            (x, y) if x.series().is_some() && x.series() == y.series() => {
                                Some(format!("both change “{}” ({} vs {})", self.series_by_id(x.series().unwrap()).map(|s| s.name.clone()).unwrap_or_default(), x.kind(), y.kind()))
                            }
                            (ScenarioChange::AddCompany { company: x }, ScenarioChange::AddCompany { company: y }) if x.name.eq_ignore_ascii_case(&y.name) => Some(format!("both add a company named “{}”", x.name)),
                            (ScenarioChange::RemoveCompany { company: x, .. }, ScenarioChange::AddEmployment { company: y, .. }) | (ScenarioChange::AddEmployment { company: y, .. }, ScenarioChange::RemoveCompany { company: x, .. }) if x == y => {
                                Some(format!("one removes {} while the other adds employment there", self.company(*x).map(|c| c.name.clone()).unwrap_or_default()))
                            }
                            (ScenarioChange::AddTaxRule { rule: x }, ScenarioChange::DisableTaxRule { rule: y, .. }) | (ScenarioChange::DisableTaxRule { rule: y, .. }, ScenarioChange::AddTaxRule { rule: x }) if x.id == *y => Some(format!("one adds tax rule {} while the other disables it", x.id)),
                            _ => None,
                        };
                        if let Some(reason) = reason {
                            problems.push(Incompatibility { first: *first, second: *second, reason });
                        }
                    }
                }
            }
        }
        problems
    }

    /// The overlay of one scenario: explicit changes plus tagged events,
    /// funding rules and assumptions (§18.2), in a fixed order.
    pub fn scenario_overlay(&self, id: ScenarioId) -> Vec<OverlayEntry> {
        let mut entries = Vec::new();
        let Some(scenario) = self.scenario(id) else { return entries };
        for member in &scenario.composed_of {
            entries.push(OverlayEntry { scenario: id, kind: "composed of", text: self.scenario(*member).map(|s| format!("scenario “{}”", s.name)).unwrap_or_else(|| member.to_string()), explicit: true });
        }
        for series in self.series.iter().filter(|s| s.scenario == Some(id)) {
            entries.push(OverlayEntry { scenario: id, kind: "add events", text: format!("“{}”: {} · {}", series.name, series.amount.describe(), series.recurrence.describe()), explicit: false });
        }
        for change in &scenario.changes {
            entries.push(OverlayEntry { scenario: id, kind: change.kind(), text: change.describe(self), explicit: true });
        }
        for rule in self.rules.iter().filter(|r| r.scenario == Some(id)) {
            entries.push(OverlayEntry { scenario: id, kind: "funding rule", text: format!("{}: {}", rule.name, rule.action.describe(self)), explicit: false });
        }
        for assumption in self.assumptions.iter().filter(|a| matches!(&a.source, crate::model::AssumptionSource::Scenario(s) if *s == id)) {
            entries.push(OverlayEntry { scenario: id, kind: "assumption", text: assumption.text.clone(), explicit: false });
        }
        entries
    }

    /// §18.1–§18.3 — the baseline with the scenarios applied. Objects tagged
    /// with any scenario of the set are re-tagged to `ids[0]` (so the rest of
    /// the engine treats them as that scenario's), other scenarios' objects
    /// are dropped, explicit changes are applied, and the applied scenarios
    /// carry no changes any more (the overlay is not applied twice).
    pub fn apply_scenarios(&self, ids: &[ScenarioId]) -> EngineResult<Household> {
        let Some(primary) = ids.first().copied() else { return Ok(self.clone()) };
        let closure = self.scenario_closure(ids);
        for id in &closure {
            self.scenario(*id).ok_or(EngineError::UnknownScenario(*id))?;
        }
        let problems = self.check_compatibility(&closure);
        if let Some(problem) = problems.first() {
            return Err(EngineError::Insufficient(format!(
                "scenarios “{}” and “{}” are incompatible: {}",
                self.scenario(problem.first).map(|s| s.name.clone()).unwrap_or_default(),
                self.scenario(problem.second).map(|s| s.name.clone()).unwrap_or_default(),
                problem.reason
            )));
        }
        let mut out = self.clone();
        let in_set = |s: Option<ScenarioId>| s.is_some_and(|s| closure.contains(&s));
        out.series.retain(|s| s.scenario.is_none() || in_set(s.scenario));
        for series in out.series.iter_mut().filter(|s| in_set(s.scenario)) {
            series.scenario = Some(primary);
        }
        out.rules.retain(|r| r.scenario.is_none() || in_set(r.scenario));
        for rule in out.rules.iter_mut() {
            if in_set(rule.scenario) {
                rule.scenario = Some(primary);
            }
            if let crate::rules::RuleScope::Scenario(s) = rule.scope
                && closure.contains(&s)
            {
                rule.scope = crate::rules::RuleScope::Scenario(primary);
            }
        }
        for assumption in out.assumptions.iter_mut() {
            if let crate::model::AssumptionSource::Scenario(s) = &assumption.source
                && closure.contains(s)
            {
                assumption.source = crate::model::AssumptionSource::Scenario(primary);
            }
        }
        // Explicit changes, in closure order (composition members first, then the scenario itself).
        let mut ordered: Vec<ScenarioId> = closure.clone();
        ordered.reverse();
        let mut pack_name: Option<String> = None;
        for id in &ordered {
            let changes = self.scenario(*id).map(|s| s.changes.clone()).unwrap_or_default();
            for change in changes {
                match change {
                    ScenarioChange::RemoveSeries { series, .. } => {
                        out.series.retain(|s| s.id != series);
                    }
                    ScenarioChange::EndSeries { series, last_on, .. } => {
                        if let Some(s) = out.series.iter_mut().find(|s| s.id == series) {
                            s.end_after(last_on);
                        }
                    }
                    ScenarioChange::ChangeAmount { series, from, amount, .. } => {
                        if let Some(s) = out.series.iter_mut().find(|s| s.id == series) {
                            s.change_amount_from(from, amount);
                        }
                    }
                    ScenarioChange::MoveDates { series, days, .. } => {
                        if let Some(s) = out.series.iter_mut().find(|s| s.id == series) {
                            shift_recurrence(&mut s.recurrence, days);
                        }
                    }
                    ScenarioChange::AddEmployment { company, employee } => {
                        if let Some(c) = out.companies.iter_mut().find(|c| c.id == company) {
                            c.employees.push(employee);
                        }
                    }
                    ScenarioChange::AddCompany { company } => {
                        if !out.companies.iter().any(|c| c.id == company.id) {
                            out.companies.push(company);
                        }
                    }
                    ScenarioChange::RemoveCompany { company, .. } => {
                        out.companies.retain(|c| c.id != company);
                    }
                    ScenarioChange::AddTaxRule { rule } => {
                        let name = pack_name.get_or_insert_with(|| format!("SCENARIO-{}-v1", self.scenario(primary).map(|s| s.name.to_uppercase().replace(' ', "-")).unwrap_or_default())).clone();
                        match out.tax_packs.iter_mut().find(|p| p.name == name) {
                            Some(pack) => pack.rules.push(rule),
                            None => out.tax_packs.push(crate::model::TaxRulePack {
                                name,
                                version: "v1".into(),
                                jurisdiction: "scenario-only rules (fictitious; not any country's law)".into(),
                                verified: false,
                                rules: vec![rule],
                            }),
                        }
                    }
                    ScenarioChange::DisableTaxRule { rule, .. } => {
                        for pack in out.tax_packs.iter_mut() {
                            pack.rules.retain(|r| r.id != rule);
                        }
                    }
                }
            }
        }
        for scenario in out.scenarios.iter_mut().filter(|s| closure.contains(&s.id)) {
            scenario.changes.clear();
            scenario.composed_of.clear();
        }
        Ok(out)
    }
}

/// Moves every date of a recurrence by `days`.
fn shift_recurrence(recurrence: &mut Recurrence, days: i64) {
    let shift = |date: NaiveDate| -> NaiveDate {
        if days >= 0 { date.checked_add_days(Days::new(days as u64)).unwrap_or(date) } else { date.checked_sub_days(Days::new(days.unsigned_abs())).unwrap_or(date) }
    };
    match recurrence {
        Recurrence::OneTime { on } => {
            *on = match on {
                DateSpec::Exact(d) => DateSpec::Exact(shift(*d)),
                DateSpec::Range { earliest, expected, latest } => DateSpec::Range { earliest: shift(*earliest), expected: shift(*expected), latest: shift(*latest) },
            }
        }
        Recurrence::Daily { from, .. } | Recurrence::Weekly { from, .. } | Recurrence::Monthly { from, .. } | Recurrence::DaysOfMonth { from, .. } | Recurrence::LastDayOfMonth { from, .. } | Recurrence::Yearly { from, .. } => {
            *from = shift(*from);
        }
    }
}

/// §18.4 — one comparison row.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MetricRow {
    pub name: String,
    pub baseline: String,
    pub scenario: String,
    /// Scenario minus baseline when the metric is money.
    pub delta: Option<Money>,
    pub note: String,
}

/// F139 — one bucket of the end-of-window difference.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AttributionLine {
    pub kind: &'static str,
    pub label: String,
    pub baseline: Money,
    pub scenario: Money,
    pub delta: Money,
}

/// The side-by-side result of [`compare`].
#[derive(Clone, Debug)]
pub struct ScenarioComparison {
    pub scenarios: Vec<ScenarioId>,
    pub boundary: Boundary,
    pub baseline: BoundaryForecast,
    pub overlaid: BoundaryForecast,
    pub metrics: Vec<MetricRow>,
    pub attribution: Vec<AttributionLine>,
    pub attribution_total: Money,
    pub end_delta: Money,
    /// The buckets sum to the end difference exactly (no hidden double counting).
    pub attribution_verified: bool,
    /// Balances of both paths on the union of their dates, for a chart.
    pub merged_path: Vec<(NaiveDate, Money, Money)>,
}

fn balance_on(path: &[PathPoint], start: Money, date: NaiveDate) -> Money {
    path.iter().filter(|p| p.date <= date).last().map(|p| p.balance).unwrap_or(start)
}

fn runway_days(forecast: &BoundaryForecast) -> Option<i64> {
    forecast.breach.first_breach.map(|d| (d - forecast.as_of).num_days())
}

/// §18.4 / F139 — baseline versus the composed scenarios on one boundary.
pub fn compare(household: &Household, boundary: Boundary, scenarios: &[ScenarioId], through: NaiveDate, case: Case) -> EngineResult<ScenarioComparison> {
    let primary = scenarios.first().copied().ok_or(EngineError::Insufficient("pick at least one scenario to compare".into()))?;
    let overlaid_household = household.apply_scenarios(scenarios)?;
    let baseline = forecast(household, boundary, ForecastOptions { through, scenario: None, case })?;
    let overlaid = forecast(&overlaid_household, boundary, ForecastOptions { through, scenario: Some(primary), case })?;
    let currency = household.base_currency;

    // §18.4 metrics.
    let mut metrics = Vec::new();
    let money_row = |name: &str, a: Money, b: Money, note: &str| -> EngineResult<MetricRow> {
        Ok(MetricRow { name: name.into(), baseline: a.format(), scenario: b.format(), delta: Some(b.checked_sub(a)?), note: note.into() })
    };
    let one_month = household.as_of.checked_add_months(chrono::Months::new(1)).unwrap_or(through).min(through);
    metrics.push(money_row(&format!("Balance on {}", one_month.format("%d %b %Y")), balance_on(&baseline.path, baseline.start.money(), one_month), balance_on(&overlaid.path, overlaid.start.money(), one_month), "balance after that day's postings")?);
    metrics.push(money_row(&format!("Balance on {} (end of window)", through.format("%d %b %Y")), baseline.end.money(), overlaid.end.money(), "conditional projected cash, this case")?);
    metrics.push(money_row("Lowest balance", baseline.lowest.money(), overlaid.lowest.money(), "after intraday ordering (V010)")?);
    metrics.push(MetricRow {
        name: "Date of lowest balance".into(),
        baseline: baseline.lowest_date.map(|d| d.format("%d %b %Y").to_string()).unwrap_or_else(|| "–".into()),
        scenario: overlaid.lowest_date.map(|d| d.format("%d %b %Y").to_string()).unwrap_or_else(|| "–".into()),
        delta: None,
        note: String::new(),
    });
    let breaches = |f: &BoundaryForecast| match f.breach.first_breach {
        None => format!("none through {}", f.through.format("%d %b %Y")),
        Some(first) => format!("first {}, worst {} below", first.format("%d %b %Y"), f.breach.worst_deficit.format()),
    };
    metrics.push(MetricRow {
        name: "Reserve breaches (hard floor)".into(),
        baseline: breaches(&baseline),
        scenario: breaches(&overlaid),
        delta: None,
        note: format!("floor {} — hard earmarks and uncovered bank minimums", baseline.floor.format()),
    });
    let taxes = |f: &BoundaryForecast| -> Money {
        let mut total = Money::zero(currency);
        for account in &f.accounts {
            for posting in account.postings.iter().filter(|p| p.tax_rule.is_some()) {
                total = total.checked_add(posting.amount.share_basis_points(account.share_basis_points)).unwrap_or(total);
            }
        }
        total.negated()
    };
    let fees = |f: &BoundaryForecast| -> Money {
        let mut total = Money::zero(currency);
        for account in &f.accounts {
            for posting in account.postings.iter().filter(|p| p.fee_rule.is_some()) {
                total = total.checked_add(posting.amount.share_basis_points(account.share_basis_points)).unwrap_or(total);
            }
        }
        total.negated()
    };
    let base_tax = taxes(&baseline);
    let over_tax = taxes(&overlaid);
    metrics.push(money_row("Total taxes paid in the window", base_tax, over_tax, "tax postings on the boundary's accounts (§12.3)")?);
    metrics.push(MetricRow { name: "Incremental taxes".into(), baseline: "–".into(), scenario: over_tax.checked_sub(base_tax)?.format_signed(), delta: Some(over_tax.checked_sub(base_tax)?), note: "scenario minus baseline (E05 logic)".into() });
    metrics.push(money_row("Fees from rules", fees(&baseline), fees(&overlaid), "fee events added by rules (§14.4)")?);
    let debt = |f: &BoundaryForecast| -> Money {
        let mut total = Money::zero(currency);
        for account in &f.accounts {
            if account.end.is_negative() {
                total = total.checked_add(account.end.abs()).unwrap_or(total);
            }
        }
        total
    };
    metrics.push(money_row("Debt (accounts below zero at the end)", debt(&baseline), debt(&overlaid), "overdrafts and card balances inside the boundary")?);
    let runway = |f: &BoundaryForecast| match runway_days(f) {
        Some(days) => format!("{days} days, to {}", f.breach.first_breach.map(|d| d.format("%d %b %Y").to_string()).unwrap_or_default()),
        None => format!("beyond {}", f.through.format("%d %b %Y")),
    };
    metrics.push(MetricRow { name: "Cash runway (first passage below the floor, M13)".into(), baseline: runway(&baseline), scenario: runway(&overlaid), delta: None, note: "days until the floor is first crossed; never “infinite” (E08)".into() });
    for company in &household.companies {
        let base = forecast(household, Boundary::Company(company.id), ForecastOptions { through, scenario: None, case });
        let over = forecast(&overlaid_household, Boundary::Company(company.id), ForecastOptions { through, scenario: Some(primary), case });
        if let (Ok(base), Ok(over)) = (base, over) {
            metrics.push(money_row(&format!("{} working capital at the end", company.name), base.end.money(), over.end.money(), "company cash stays company cash (§8.5)")?);
            let payroll: Money = company.employees.iter().filter(|e| e.end.is_none_or(|end| end >= through)).fold(Money::zero(currency), |acc, e| acc.checked_add(e.monthly_gross).unwrap_or(acc));
            let coverage = |cash: Money| if payroll.is_zero() { "no payroll".to_string() } else { format!("{:.1} months of payroll ({}/month)", cash.minor() as f64 / payroll.minor() as f64, payroll.format()) };
            metrics.push(MetricRow { name: format!("{} payroll coverage", company.name), baseline: coverage(base.lowest.money()), scenario: coverage(over.lowest.money()), delta: None, note: "lowest company cash divided by monthly gross payroll".into() });
        }
    }
    metrics.push(MetricRow { name: "Goal delays".into(), baseline: "no goals defined".into(), scenario: "no goals defined".into(), delta: None, note: "goals arrive with the decision builder (M9)".into() });

    // F139 attribution: every posting in exactly one bucket.
    #[derive(Default)]
    struct Bucket {
        kind: &'static str,
        label: String,
        baseline: i64,
        scenario: i64,
    }
    let mut buckets: Vec<Bucket> = Vec::new();
    let mut add = |kind: &'static str, label: String, side: bool, minor: i64| {
        let bucket = match buckets.iter_mut().find(|b| b.kind == kind && b.label == label) {
            Some(b) => b,
            None => {
                buckets.push(Bucket { kind, label, ..Default::default() });
                buckets.last_mut().expect("just pushed")
            }
        };
        if side { bucket.scenario += minor } else { bucket.baseline += minor }
    };
    for (side, f) in [(false, &baseline), (true, &overlaid)] {
        for account in &f.accounts {
            for posting in &account.postings {
                let minor = posting.amount.share_basis_points(account.share_basis_points).minor();
                if posting.tax_rule.is_some() {
                    add("taxes", "Tax postings".into(), side, minor);
                } else if posting.fee_rule.is_some() {
                    add("fees", "Fee events from rules".into(), side, minor);
                } else {
                    let name = household.series_by_id(posting.series).or_else(|| overlaid_household.series_by_id(posting.series)).map(|s| s.name.clone()).unwrap_or_else(|| posting.series.to_string());
                    add("events", name, side, minor);
                }
            }
        }
    }
    let start_delta = overlaid.start.money().checked_sub(baseline.start.money())?;
    let mut attribution: Vec<AttributionLine> = buckets
        .into_iter()
        .filter(|b| b.baseline != b.scenario)
        .map(|b| {
            let kind = match (b.kind, b.baseline, b.scenario) {
                ("events", 0, _) => "events added",
                ("events", _, 0) => "events removed",
                ("events", _, _) => "events changed",
                (other, _, _) => other,
            };
            AttributionLine { kind, label: b.label, baseline: Money::new(b.baseline, currency), scenario: Money::new(b.scenario, currency), delta: Money::new(b.scenario - b.baseline, currency) }
        })
        .collect();
    if !start_delta.is_zero() {
        attribution.push(AttributionLine { kind: "starting cash", label: "Reconciled starting cash".into(), baseline: baseline.start.money(), scenario: overlaid.start.money(), delta: start_delta });
    }
    attribution.sort_by_key(|a| std::cmp::Reverse(a.delta.minor().abs()));
    let mut attribution_total = Money::zero(currency);
    for line in &attribution {
        attribution_total = attribution_total.checked_add(line.delta)?;
    }
    let end_delta = overlaid.end.money().checked_sub(baseline.end.money())?;
    let attribution_verified = attribution_total == end_delta;
    if !attribution_verified {
        log::error!("F139 attribution does not sum to the end difference: {} vs {}", attribution_total.format(), end_delta.format());
    }

    // Merged path for charts.
    let dates: BTreeSet<NaiveDate> = baseline.path.iter().chain(overlaid.path.iter()).map(|p| p.date).collect();
    let merged_path = dates.into_iter().map(|d| (d, balance_on(&baseline.path, baseline.start.money(), d), balance_on(&overlaid.path, overlaid.start.money(), d))).collect();

    log::info!("compared {} scenario(s) on {:?}: end delta {}, attribution {}", scenarios.len(), boundary, end_delta.format_signed(), if attribution_verified { "verified" } else { "MISMATCH" });
    Ok(ScenarioComparison { scenarios: scenarios.to_vec(), boundary, baseline, overlaid, metrics, attribution, attribution_total, end_delta, attribution_verified, merged_path })
}

impl Household {
    /// Adds an explicit change to a scenario after checking it can be applied.
    pub fn add_scenario_change(&mut self, id: ScenarioId, change: ScenarioChange) -> EngineResult<()> {
        self.scenario(id).ok_or(EngineError::UnknownScenario(id))?;
        if let Some(series) = change.series() {
            self.series_by_id(series).ok_or(EngineError::UnknownSeries(series))?;
        }
        match &change {
            ScenarioChange::AddEmployment { company, .. } | ScenarioChange::RemoveCompany { company, .. } => {
                self.company(*company).ok_or(EngineError::UnknownCompany(*company))?;
            }
            ScenarioChange::EndSeries { series, last_on, .. } => {
                let s = self.series_by_id(*series).ok_or(EngineError::UnknownSeries(*series))?;
                if matches!(s.recurrence, Recurrence::OneTime { .. }) {
                    return Err(EngineError::Insufficient(format!("“{}” is a one-time event; remove it instead of ending it", s.name)));
                }
                if *last_on < self.as_of {
                    return Err(EngineError::Insufficient(format!("“{}” cannot end on {last_on}, before the reconciliation date {}", s.name, self.as_of)));
                }
            }
            _ => {}
        }
        let mut probe = self.clone();
        if let Some(s) = probe.scenarios.iter_mut().find(|s| s.id == id) {
            s.changes.push(change.clone());
        }
        probe.apply_scenarios(&[id])?;
        if let Some(s) = self.scenarios.iter_mut().find(|s| s.id == id) {
            s.changes.push(change);
        }
        Ok(())
    }

    /// Composes scenarios into a new one when they are compatible (§18.3).
    /// The composition is private when any member is private (§18.5) and gets
    /// its own access policy with `owner` (F162).
    pub fn compose_scenarios(&mut self, name: &str, members: &[ScenarioId], owner: PersonId) -> EngineResult<ScenarioId> {
        if members.len() < 2 {
            return Err(EngineError::Insufficient("a composition needs at least two scenarios".into()));
        }
        for member in members {
            self.scenario(*member).ok_or(EngineError::UnknownScenario(*member))?;
        }
        let problems = self.check_compatibility(members);
        if let Some(problem) = problems.first() {
            return Err(EngineError::Insufficient(format!(
                "“{}” and “{}” are incompatible: {}",
                self.scenario(problem.first).map(|s| s.name.clone()).unwrap_or_default(),
                self.scenario(problem.second).map(|s| s.name.clone()).unwrap_or_default(),
                problem.reason
            )));
        }
        let any_private = members.iter().filter_map(|m| self.scenario(*m)).any(|s| s.private_to.is_some());
        let id = self.next_scenario_id();
        let description = format!("Composition of {}", members.iter().filter_map(|m| self.scenario(*m)).map(|s| format!("“{}”", s.name)).collect::<Vec<_>>().join(" + "));
        let scenario = Scenario { id, name: name.into(), description, private_to: any_private.then_some(owner), changes: Vec::new(), composed_of: members.to_vec() };
        Ok(self.add_scenario(scenario, owner))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{self, ids, pkr};

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn overlay_is_deterministic_and_ends_the_salary() {
        let household = fixtures::plan_household();
        let once = household.apply_scenarios(&[ids::LEAVE_JOB]).unwrap();
        let twice = household.apply_scenarios(&[ids::LEAVE_JOB]).unwrap();
        assert_eq!(once, twice);
        // The salary ends after 30 Sep 2026: no October or November salary in the overlay.
        let salaries = |h: &Household| h.expand_all(h.as_of, fixtures::default_horizon(), Some(ids::LEAVE_JOB)).into_iter().filter(|o| o.series == ids::SALARY_A).count();
        assert_eq!(salaries(&household), 1, "the overlay applies through expand_all too");
        assert_eq!(salaries(&once), 1);
        assert_eq!(household.expand_all(household.as_of, fixtures::default_horizon(), None).into_iter().filter(|o| o.series == ids::SALARY_A).count(), 3);
        // The replacement hire is on Company Alpha's books inside the overlay only.
        assert_eq!(once.company(ids::ALPHA).unwrap().employees.len(), household.company(ids::ALPHA).unwrap().employees.len() + 1);
        // The applied scenario carries no changes any more, so nothing applies twice.
        assert!(once.scenario(ids::LEAVE_JOB).unwrap().changes.is_empty());
        // Buy car series are gone from the leave-job overlay; the car scenario keeps them.
        assert!(!once.series.iter().any(|s| s.id == ids::CAR_DOWN_PAYMENT));
        let car = household.apply_scenarios(&[ids::BUY_CAR]).unwrap();
        assert!(car.series.iter().any(|s| s.id == ids::CAR_DOWN_PAYMENT && s.scenario == Some(ids::BUY_CAR)));
    }

    #[test]
    fn composition_combines_compatible_scenarios_and_refuses_conflicts() {
        let mut household = fixtures::plan_household();
        assert!(household.check_compatibility(&[ids::LEAVE_JOB, ids::BUY_CAR]).is_empty());
        let both = household.apply_scenarios(&[ids::LEAVE_JOB_AND_CAR]).unwrap();
        assert!(both.series.iter().any(|s| s.id == ids::CAR_DOWN_PAYMENT && s.scenario == Some(ids::LEAVE_JOB_AND_CAR)));
        assert_eq!(both.expand_all(both.as_of, fixtures::default_horizon(), Some(ids::LEAVE_JOB_AND_CAR)).into_iter().filter(|o| o.series == ids::SALARY_A).count(), 1);
        // Funding rules of the car scenario now belong to the composition.
        assert!(both.rules.iter().any(|r| r.scenario == Some(ids::LEAVE_JOB_AND_CAR)));
        // A raise conflicts with leaving: both touch the salary series.
        let raise = household.next_scenario_id();
        household.scenarios.push(Scenario { id: raise, name: "Salary rise".into(), description: String::new(), private_to: None, changes: Vec::new(), composed_of: Vec::new() });
        household
            .add_scenario_change(raise, ScenarioChange::ChangeAmount { series: ids::SALARY_A, from: d(2026, 11, 1), amount: AmountSpec::Exact(pkr(600_000)), reason: "promotion".into() })
            .unwrap();
        let problems = household.check_compatibility(&[ids::LEAVE_JOB, raise]);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].reason.contains("both change"));
        let err = household.compose_scenarios("Leave + raise", &[ids::LEAVE_JOB, raise], ids::PERSON_A).unwrap_err();
        assert!(err.to_string().contains("incompatible"));
        let err = household.apply_scenarios(&[ids::LEAVE_JOB, raise]).unwrap_err();
        assert!(err.to_string().contains("incompatible"));
        // Compatible: the raise with the car.
        let id = household.compose_scenarios("Raise + car", &[raise, ids::BUY_CAR], ids::PERSON_A).unwrap();
        assert!(household.policy_for(ObjectRef::Scenario(id)).is_some(), "F162: a composition gets its own policy");
        assert_eq!(household.scenario(id).unwrap().composed_of, vec![raise, ids::BUY_CAR]);
        assert!(household.apply_scenarios(&[id]).is_ok());
        // Ending a one-time event is refused by name.
        let err = household.add_scenario_change(raise, ScenarioChange::EndSeries { series: ids::FREELANCE, last_on: d(2026, 12, 1), reason: String::new() }).unwrap_err();
        assert!(err.to_string().contains("one-time"));
    }

    #[test]
    fn comparison_metrics_and_attribution_sum_exactly() {
        let household = fixtures::plan_household();
        let comparison = compare(&household, Boundary::Household, &[ids::LEAVE_JOB], fixtures::default_horizon(), Case::Expected).unwrap();
        assert!(comparison.attribution_verified, "F139: buckets sum to the end difference");
        assert_eq!(comparison.attribution_total, comparison.end_delta);
        // Two salaries (Oct, Nov) of 500,000 gross are missing; the withholding they carried goes too.
        let salary = comparison.attribution.iter().find(|a| a.label.starts_with("Person A salary")).unwrap();
        assert_eq!(salary.kind, "events changed");
        assert_eq!(salary.delta, pkr(-1_000_000));
        let taxes = comparison.attribution.iter().find(|a| a.kind == "taxes").unwrap();
        assert!(taxes.delta.is_positive(), "less withholding is paid without the salary");
        assert!(comparison.end_delta.is_negative());
        let end_row = comparison.metrics.iter().find(|m| m.name.contains("end of window")).unwrap();
        assert_eq!(end_row.delta, Some(comparison.end_delta));
        assert!(comparison.metrics.iter().any(|m| m.name.starts_with("Cash runway")));
        assert!(comparison.metrics.iter().any(|m| m.name.contains("payroll coverage")));
        // The composed scenario adds the car purchase on top.
        let both = compare(&household, Boundary::Household, &[ids::LEAVE_JOB_AND_CAR], fixtures::default_horizon(), Case::Expected).unwrap();
        assert!(both.attribution_verified);
        assert!(both.attribution.iter().any(|a| a.kind == "events added" && a.label.starts_with("Car down payment")));
        assert!(both.end_delta.minor() < comparison.end_delta.minor());
        assert!(!both.merged_path.is_empty());
    }
}
