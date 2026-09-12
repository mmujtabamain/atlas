//! Decisions (§13, §19, §20, §26; M9, M25, M27): a concrete purchase plan
//! built step by step — the purchase, the down payment and its funding, the
//! recurring payment, other costs — evaluated deterministically:
//!
//! - [`gross_up`] finds the smallest gross extraction whose net proceeds
//!   cover the requirement after recomputed withholding and fees (M25, E02,
//!   V014).
//! - [`funding_strategies`] enumerates permitted funding routes (personal
//!   accounts in order, company extraction with the E07 ceiling and the M27
//!   legal-capacity caveat) and ranks them under the chosen objective, always
//!   as "best among the enumerated candidates" (§13.5).
//! - [`evaluate`] applies the plan as a temporary scenario, forecasts baseline
//!   and decision paths, and reports the §19.1 affordability metrics, the
//!   §19.2 parameter grid (E03), goal delays (§20), the conditional statement
//!   (§19.3) and the recommendation contract (§26).

use crate::assumptions::ConditionalStatement;
use crate::breach::{PathPoint, analyse};
use crate::forecast::{BoundaryForecast, Case, ForecastOptions, forecast};
use crate::authz::CalculationAccess;
use crate::ids::*;
use crate::liquidity::{Boundary, company_cash};
use crate::model::{Household, Scenario, TaxKind, TaxTiming};
use crate::money::Money;
use crate::provenance::{Calc, ProvNode};
use crate::timeline::{AmountSpec, DateSpec, Direction, EventSeries, InvalidDayPolicy, Recurrence, Until};
use crate::vocab::{Certainty, MoneyClass, ResultStrength};
use crate::{EngineError, EngineResult};
use chrono::{Datelike, Days, Months, NaiveDate};
use serde::{Deserialize, Serialize};

/// §13.2 — the objective the user chose. Never assumed.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Objective {
    MinimiseTaxAndFees,
    MaximiseLowestCash,
    MinimiseFinancingCost,
    FewestTransfers,
}

impl Objective {
    pub const ALL: [Objective; 4] = [Objective::MinimiseTaxAndFees, Objective::MaximiseLowestCash, Objective::MinimiseFinancingCost, Objective::FewestTransfers];

    pub fn label(self) -> &'static str {
        match self {
            Objective::MinimiseTaxAndFees => "Minimise immediate tax + fees",
            Objective::MaximiseLowestCash => "Maximise the lowest household cash",
            Objective::MinimiseFinancingCost => "Minimise total financing cost",
            Objective::FewestTransfers => "Fewest transfers",
        }
    }
}

/// §13.3 / §13.4 — a personal account the funding search may use, in order.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct FundingSource {
    pub account: AccountId,
    pub allowed: bool,
    /// "Never reduce Account A below 300,000" (§13.3).
    pub floor: Option<Money>,
}

/// M27 — permitted extraction methods; every one needs legal capacity.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ExtractionMethod {
    Salary,
    Dividend,
}

impl ExtractionMethod {
    pub fn label(self) -> &'static str {
        match self {
            ExtractionMethod::Salary => "owner salary",
            ExtractionMethod::Dividend => "dividend / distribution",
        }
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct CompanyRoute {
    pub company: CompanyId,
    pub method: ExtractionMethod,
    pub allowed: bool,
    /// The personal account that receives the extraction.
    pub to_account: AccountId,
}

/// The financed remainder: an annuity of `months` instalments.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Financing {
    pub months: u32,
    pub annual_rate_basis_points: u32,
    pub first_instalment: NaiveDate,
    pub account: AccountId,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct OtherCost {
    pub label: String,
    pub amount: Money,
    pub on: NaiveDate,
    pub account: AccountId,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct RunningCost {
    pub label: String,
    pub monthly: Money,
    pub from: NaiveDate,
    pub account: AccountId,
}

/// The concrete plan the stepper builds (§19: price, window, down payment
/// range, reserve; §13: sources, routes, objective, constraints).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct PurchasePlan {
    pub name: String,
    pub price: Money,
    pub purchase_on: NaiveDate,
    /// §19.2 grid: first and last purchase month (inclusive).
    pub window_from: NaiveDate,
    pub window_to: NaiveDate,
    pub down_payment: Money,
    /// §19.2 grid: down payments from `low` to `high` in `step`s.
    pub down_payment_low: Money,
    pub down_payment_high: Money,
    pub down_payment_step: Money,
    /// §19: household emergency reserve to keep throughout.
    pub reserve: Money,
    pub sources: Vec<FundingSource>,
    pub company_routes: Vec<CompanyRoute>,
    pub financing: Option<Financing>,
    pub other_costs: Vec<OtherCost>,
    pub running_cost: Option<RunningCost>,
    pub objective: Objective,
    /// §13.3 "Maximum tax cost".
    pub max_tax_and_fees: Option<Money>,
}

impl PurchasePlan {
    /// The financed amount: price minus the down payment, never negative.
    pub fn financed(&self) -> Money {
        self.price.checked_sub(self.down_payment).map(|m| m.clamped_at_zero()).unwrap_or(Money::zero(self.price.currency()))
    }

    /// The horizon the decision is judged over: the app's horizon or the
    /// purchase plus six months, whichever is later, extended to any goal due
    /// within a year of the purchase. Instalments beyond it count in the
    /// financing cost, not in the path (a three-year path would be dominated
    /// by everything else that changes in three years).
    pub fn horizon(&self, household: &Household, at_least: NaiveDate) -> NaiveDate {
        let mut through = at_least.max(self.purchase_on.checked_add_months(Months::new(6)).unwrap_or(self.purchase_on));
        let year_out = self.purchase_on.checked_add_months(Months::new(12)).unwrap_or(self.purchase_on);
        for goal in household.goals.iter().filter(|g| g.target_on <= year_out) {
            through = through.max(goal.target_on);
        }
        for cost in &self.other_costs {
            through = through.max(cost.on);
        }
        through
    }
}

// ----- M25: gross-up ------------------------------------------------------------

/// The result of a gross-up: what leaves the source, what the deductions take,
/// what arrives (M25).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NetProceeds {
    pub gross: Money,
    pub withholding: Money,
    pub fees: Money,
    pub net: Money,
}

/// M25 — the smallest gross `g` with `N(g) = g − W(g) − F(g) ≥ required_net`,
/// where `cost(g)` returns `(withholding, fees)` recomputed on `g`. Bisection
/// finds a candidate and a downward scan removes the slack; the result is
/// verified to deliver the net (V014) even when the cost is not monotone.
pub fn gross_up(required_net: Money, cost: &dyn Fn(Money) -> EngineResult<(Money, Money)>) -> EngineResult<NetProceeds> {
    let currency = required_net.currency();
    let proceeds = |gross: Money| -> EngineResult<NetProceeds> {
        let (withholding, fees) = cost(gross)?;
        let net = gross.checked_sub(withholding)?.checked_sub(fees)?;
        Ok(NetProceeds { gross, withholding, fees, net })
    };
    if !required_net.is_positive() {
        return proceeds(Money::zero(currency));
    }
    let mut low = required_net.minor();
    let mut high = required_net.minor();
    let mut guard = 0;
    while proceeds(Money::new(high, currency))?.net.minor() < required_net.minor() {
        high = high.saturating_mul(2);
        guard += 1;
        if guard > 40 {
            return Err(EngineError::Insufficient(format!("no gross amount delivers {} net: deductions grow as fast as the amount", required_net.format())));
        }
    }
    while high - low > 1 {
        let mid = low + (high - low) / 2;
        if proceeds(Money::new(mid, currency))?.net.minor() >= required_net.minor() { high = mid } else { low = mid }
    }
    // Downward scan for the non-monotone case (threshold rules): stop at the first failing step.
    let mut gross = high;
    for step in [1_000_000i64, 100_000, 10_000, 1_000, 100, 10, 1] {
        while gross - step >= required_net.minor() && proceeds(Money::new(gross - step, currency))?.net.minor() >= required_net.minor() {
            gross -= step;
        }
    }
    let result = proceeds(Money::new(gross, currency))?;
    if result.net.minor() < required_net.minor() {
        return Err(EngineError::Insufficient(format!("gross-up verification failed: {} gross delivers {} net, below {}", result.gross.format(), result.net.format(), required_net.format())));
    }
    Ok(result)
}

/// The monthly annuity instalment for `principal` over `months` at an annual
/// nominal rate in basis points, rounded to the minor unit (half up).
pub fn monthly_payment(principal: Money, months: u32, annual_rate_basis_points: u32) -> Money {
    let currency = principal.currency();
    if months == 0 || !principal.is_positive() {
        return Money::zero(currency);
    }
    if annual_rate_basis_points == 0 {
        return Money::new((principal.minor() + months as i64 - 1) / months as i64, currency);
    }
    let r = annual_rate_basis_points as f64 / 10_000.0 / 12.0;
    let n = months as f64;
    let factor = r / (1.0 - (1.0 + r).powf(-n));
    Money::new((principal.minor() as f64 * factor).round() as i64, currency)
}

/// E06 — the unrounded annuity payment (for display next to the contract's
/// rounded one).
pub fn annuity_payment_exact(principal: Money, months: u32, monthly_rate_basis_points: u32) -> f64 {
    if months == 0 {
        return 0.0;
    }
    let r = monthly_rate_basis_points as f64 / 10_000.0;
    if r == 0.0 {
        return principal.minor() as f64 / months as f64;
    }
    principal.minor() as f64 * r / (1.0 - (1.0 + r).powf(-(months as f64)))
}

/// One replayed instalment of a loan contract.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Instalment {
    pub number: u32,
    pub payment: Money,
    pub interest: Money,
    pub principal: Money,
    pub balance_after: Money,
}

/// E06 — replays a contract with each payment rounded to the minor unit
/// (half up) and the final payment adjusted so the principal reaches exactly
/// zero; interest accrues on the rounded balance each month.
pub fn loan_schedule(principal: Money, months: u32, monthly_rate_basis_points: u32) -> Vec<Instalment> {
    let currency = principal.currency();
    if months == 0 || !principal.is_positive() {
        return Vec::new();
    }
    let payment = Money::new(annuity_payment_exact(principal, months, monthly_rate_basis_points).round() as i64, currency);
    let mut balance = principal;
    let mut schedule = Vec::with_capacity(months as usize);
    for number in 1..=months {
        let interest = Money::new(((balance.minor() as i128 * monthly_rate_basis_points as i128 + 5_000) / 10_000) as i64, currency);
        let (pay, principal_part) = if number == months {
            let final_payment = balance.checked_add(interest).unwrap_or(balance);
            (final_payment, balance)
        } else {
            let principal_part = payment.checked_sub(interest).unwrap_or(payment).min(balance).unwrap_or(balance);
            (payment, principal_part)
        };
        balance = balance.checked_sub(principal_part).unwrap_or(balance);
        schedule.push(Instalment { number, payment: pay, interest, principal: principal_part, balance_after: balance });
    }
    schedule
}

/// E06 — every internal rate of return of a cash-flow pattern in
/// (−99%, 1000%), found by sign changes of the NPV on a fine grid and
/// bisection. A pattern can have several roots (−100, 230, −132 has 10% and
/// 20%), so a single-root routine cannot rank arbitrary patterns.
pub fn irr_roots(cash_flows: &[f64]) -> Vec<f64> {
    let npv = |rate: f64| cash_flows.iter().enumerate().map(|(t, cf)| cf / (1.0 + rate).powi(t as i32)).sum::<f64>();
    let mut roots = Vec::new();
    let steps = 10_990;
    let mut previous = -0.99;
    let mut previous_value = npv(previous);
    for i in 1..=steps {
        let rate = -0.99 + i as f64 * 0.001;
        let value = npv(rate);
        if previous_value == 0.0 {
            roots.push(previous);
        } else if previous_value.signum() != value.signum() {
            let (mut lo, mut hi) = (previous, rate);
            let (mut lo_v, _) = (previous_value, value);
            for _ in 0..60 {
                let mid = (lo + hi) / 2.0;
                let mid_v = npv(mid);
                if mid_v.signum() == lo_v.signum() {
                    lo = mid;
                    lo_v = mid_v;
                } else {
                    hi = mid;
                }
            }
            roots.push((lo + hi) / 2.0);
        }
        previous = rate;
        previous_value = value;
    }
    roots.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    roots
}

// ----- §13: funding strategies ---------------------------------------------------

/// One movement of a strategy.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FundingStep {
    pub source: String,
    pub account: AccountId,
    pub gross: Money,
    pub withholding: Money,
    pub fees: Money,
    pub net: Money,
    pub ending_balance: Money,
    pub floor: Money,
    pub note: String,
}

/// §13.5 — one enumerated strategy with everything the plan says a result must show.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Strategy {
    pub name: String,
    pub steps: Vec<FundingStep>,
    pub gross_total: Money,
    pub immediate_tax: Money,
    pub fees: Money,
    pub net: Money,
    /// M25: future incremental tax modelled separately, with its due date.
    pub future_tax: Money,
    pub future_tax_note: String,
    pub feasible: bool,
    pub violations: Vec<String>,
    pub caveats: Vec<String>,
    pub transfers: usize,
}

impl Strategy {
    pub fn tax_and_fees(&self) -> Money {
        self.immediate_tax.checked_add(self.fees).unwrap_or(self.immediate_tax)
    }
}

/// §13.5 — the enumeration with its status.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StrategyReport {
    pub objective: Objective,
    pub strategies: Vec<Strategy>,
    /// Index into `strategies` of the preferred feasible one.
    pub preferred: Option<usize>,
    pub status: String,
    pub search_space: String,
}

fn balance_on(path: &[PathPoint], start: Money, date: NaiveDate) -> Money {
    path.iter().filter(|p| p.date <= date).last().map(|p| p.balance).unwrap_or(start)
}

fn account_balance_on(household: &Household, account: AccountId, date: NaiveDate) -> EngineResult<Money> {
    let f = forecast(household, Boundary::Account(account), ForecastOptions { through: date, scenario: None, case: Case::Expected })?;
    Ok(balance_on(&f.path, f.start.money(), date))
}

/// The salary withholding rate (basis points) in force on `date`, if a
/// creditable-at-source flat rule for the "Salary" category exists.
fn salary_withholding_bp(household: &Household, date: NaiveDate) -> Option<(u32, String)> {
    crate::tax::effective_rules(household, date).into_iter().find_map(|(pack, rule)| match (&rule.kind, &rule.timing) {
        (TaxKind::FlatRate { rate_basis_points }, TaxTiming::WithheldAtSource { .. }) if rule.categories.iter().any(|c| c.eq_ignore_ascii_case("Salary")) => Some((*rate_basis_points, format!("{} · {}", pack.name, rule.name))),
        _ => None,
    })
}

/// Annual bracket tax on the person's taxable base in `year`, with and
/// without `extra` (M25: future incremental tax, modelled separately).
fn incremental_annual_tax(household: &Household, person: PersonId, year: i32, extra: Money, date: NaiveDate) -> EngineResult<(Money, String)> {
    let currency = household.base_currency;
    let brackets = crate::tax::effective_rules(household, date).into_iter().find_map(|(pack, rule)| match &rule.kind {
        TaxKind::AnnualBrackets { brackets } => Some((brackets.clone(), pack.name.clone(), rule.categories.clone())),
        _ => None,
    });
    let Some((brackets, pack, categories)) = brackets else { return Ok((Money::zero(currency), "no annual bracket rule in force; no future incremental tax modelled".into())) };
    let from = NaiveDate::from_ymd_opt(year, 1, 1).expect("valid");
    let to = NaiveDate::from_ymd_opt(year, 12, 31).expect("valid");
    let mut base = Money::zero(currency);
    for occurrence in household.expand_all(from.pred_opt().unwrap_or(from), to, None) {
        if occurrence.entity != EntityRef::Person(person) || occurrence.direction != Direction::Income {
            continue;
        }
        let Some(series) = household.series_by_id(occurrence.series) else { continue };
        if categories.iter().any(|c| c.eq_ignore_ascii_case(&series.category)) {
            base = base.checked_add(occurrence.amount.expected())?;
        }
    }
    let without = crate::tax::bracket_tax(&brackets, base, "annual tax without the extraction")?.money();
    let with = crate::tax::bracket_tax(&brackets, base.checked_add(extra)?, "annual tax with the extraction")?.money();
    let delta = with.checked_sub(without)?;
    Ok((delta, format!("{pack}: annual brackets on a {year} base of {} plus the extraction; payable with the annual assessment, held as a tax reserve", base.format())))
}

/// §13 — enumerates the permitted funding strategies for the down payment on
/// the purchase date and ranks them under the objective (§13.5 wording:
/// best among the enumerated candidates, never a global optimum).
pub fn funding_strategies(household: &Household, plan: &PurchasePlan) -> EngineResult<StrategyReport> {
    let currency = household.base_currency;
    let required = plan.down_payment;
    let on = plan.purchase_on;
    let mut strategies: Vec<Strategy> = Vec::new();
    let name_of = |id: AccountId| household.account(id).map(|a| a.name.clone()).unwrap_or_else(|| id.to_string());

    // Capacity of every allowed personal source on the purchase date.
    struct Capacity {
        account: AccountId,
        balance: Money,
        floor: Money,
        capacity: Money,
    }
    let mut capacities: Vec<Capacity> = Vec::new();
    let mut refused: Vec<String> = Vec::new();
    for source in plan.sources.iter().filter(|s| s.allowed) {
        let Some(account) = household.account(source.account) else { continue };
        if account.holder.is_company() {
            continue;
        }
        // §7.3 / V067: authorization is evaluated before an object becomes a funding source.
        if household.calculation_access_for_purpose(ObjectRef::Account(source.account), crate::authz::Purpose::FundingSearch, on) == CalculationAccess::Excluded {
            refused.push(format!("{} is not authorized for funding searches; it was not considered", account.name));
            continue;
        }
        let balance = account_balance_on(household, source.account, on)?;
        let hard = crate::liquidity::hard_floor_for_account(household, source.account).unwrap_or(Money::zero(currency));
        let floor = match source.floor { Some(f) => f.max(hard)?, None => hard };
        let capacity = balance.checked_sub(floor)?.clamped_at_zero();
        capacities.push(Capacity { account: source.account, balance, floor, capacity });
    }

    // Strategy: personal accounts in the configured order, greedy, fees recomputed per step.
    let personal = |capacities: &[Capacity], label: &str| -> EngineResult<Strategy> {
        let mut steps = Vec::new();
        let mut remaining = required;
        let mut fees_total = Money::zero(currency);
        let mut violations = Vec::new();
        for cap in capacities {
            if !remaining.is_positive() {
                break;
            }
            let take_net = remaining.min(cap.capacity)?;
            if !take_net.is_positive() {
                continue;
            }
            // Fees on the movement are recomputed on the gross amount (M25).
            let account = cap.account;
            let proceeds = gross_up(take_net, &|gross| {
                let fees = crate::rules::fee_quote(household, account, Direction::Expense, gross, on, "Major purchase")?.iter().fold(Money::zero(currency), |acc, f| acc.checked_add(f.amount.abs()).unwrap_or(acc));
                Ok((Money::zero(currency), fees))
            })?;
            let ending = cap.balance.checked_sub(proceeds.gross)?;
            if ending.minor() < cap.floor.minor() {
                violations.push(format!("{} would end at {} below its floor {} once fees are included", name_of(account), ending.format(), cap.floor.format()));
            }
            fees_total = fees_total.checked_add(proceeds.fees)?;
            remaining = remaining.checked_sub(proceeds.net)?;
            steps.push(FundingStep {
                source: name_of(account),
                account,
                gross: proceeds.gross,
                withholding: Money::zero(currency),
                fees: proceeds.fees,
                net: proceeds.net,
                ending_balance: ending,
                floor: cap.floor,
                note: format!("balance {} on {} − floor {} = capacity {}", cap.balance.format(), on.format("%d %b %Y"), cap.floor.format(), cap.capacity.format()),
            });
        }
        if remaining.is_positive() {
            violations.push(format!("{} short: the allowed personal accounts cannot cover the down payment above their floors", remaining.format()));
        }
        let gross_total = steps.iter().fold(Money::zero(currency), |acc, s| acc.checked_add(s.gross).unwrap_or(acc));
        let net = steps.iter().fold(Money::zero(currency), |acc, s| acc.checked_add(s.net).unwrap_or(acc));
        Ok(Strategy {
            name: label.into(),
            transfers: steps.len(),
            steps,
            gross_total,
            immediate_tax: Money::zero(currency),
            fees: fees_total,
            net,
            future_tax: Money::zero(currency),
            future_tax_note: "personal cash is already taxed; no incremental tax".into(),
            feasible: violations.is_empty(),
            violations,
            caveats: Vec::new(),
        })
    };
    if !capacities.is_empty() {
        let mut strategy = personal(&capacities, "Personal accounts in the configured order")?;
        strategy.caveats.extend(refused.iter().cloned());
        strategies.push(strategy);
    } else if !refused.is_empty() {
        strategies.push(Strategy {
            name: "Personal accounts in the configured order".into(),
            steps: Vec::new(),
            gross_total: Money::zero(currency),
            immediate_tax: Money::zero(currency),
            fees: Money::zero(currency),
            net: Money::zero(currency),
            future_tax: Money::zero(currency),
            future_tax_note: String::new(),
            feasible: false,
            violations: refused.clone(),
            caveats: Vec::new(),
            transfers: 0,
        });
    }

    // Company routes (M27): each independently, with the E07 ceiling and the legal-capacity caveat.
    for route in plan.company_routes.iter().filter(|r| r.allowed) {
        let Some(company) = household.company(route.company) else { continue };
        let owner = company.owners.first().map(|o| o.person);
        let cash = company_cash(household, route.company)?;
        let ceiling = cash.ceiling.money();
        let mut violations = Vec::new();
        let mut caveats = vec![format!("{} cash ceiling {} is not proof of lawful distributable profit; legal capacity for a {} must be established before this route can be relied on.", company.name, ceiling.format(), route.method.label())];
        match route.method {
            ExtractionMethod::Salary => {
                let Some((rate_bp, rule_name)) = salary_withholding_bp(household, on) else {
                    strategies.push(Strategy {
                        name: format!("{} → owner salary", company.name),
                        steps: Vec::new(),
                        gross_total: Money::zero(currency),
                        immediate_tax: Money::zero(currency),
                        fees: Money::zero(currency),
                        net: Money::zero(currency),
                        future_tax: Money::zero(currency),
                        future_tax_note: String::new(),
                        feasible: false,
                        violations: vec!["no salary withholding rule is in force on the purchase date; the route's cash cost is unknown, so it cannot be ranked".into()],
                        caveats,
                        transfers: 0,
                    });
                    continue;
                };
                let proceeds = gross_up(required, &|gross| {
                    let withholding = gross.share_basis_points(rate_bp);
                    let fees = crate::rules::fee_quote(household, route.to_account, Direction::Expense, gross.checked_sub(withholding)?, on, "Major purchase")?.iter().fold(Money::zero(currency), |acc, f| acc.checked_add(f.amount.abs()).unwrap_or(acc));
                    Ok((withholding, fees))
                })?;
                if proceeds.gross.minor() > ceiling.minor() {
                    violations.push(format!("gross salary {} exceeds the {} ceiling {} (committed payroll, tax remittance and operating buffer stay funded)", proceeds.gross.format(), company.name, ceiling.format()));
                }
                let (future_tax, future_note) = match owner {
                    Some(person) => incremental_annual_tax(household, person, on.year(), proceeds.gross, on)?,
                    None => (Money::zero(currency), "no owner on record".into()),
                };
                caveats.push(format!("withholding {} at {}.{:02}% under {} is creditable against the annual assessment", proceeds.withholding.format(), rate_bp / 100, rate_bp % 100, rule_name));
                let feasible = violations.is_empty();
                strategies.push(Strategy {
                    name: format!("{} → owner salary → {}", company.name, name_of(route.to_account)),
                    steps: vec![FundingStep {
                        source: format!("{} (salary)", company.name),
                        account: route.to_account,
                        gross: proceeds.gross,
                        withholding: proceeds.withholding,
                        fees: proceeds.fees,
                        net: proceeds.net,
                        ending_balance: ceiling.checked_sub(proceeds.gross)?,
                        floor: cash.committed.money(),
                        note: format!("gross-up: {} gross − {} withholding − {} fees = {} net", proceeds.gross.format(), proceeds.withholding.format(), proceeds.fees.format(), proceeds.net.format()),
                    }],
                    gross_total: proceeds.gross,
                    immediate_tax: proceeds.withholding,
                    fees: proceeds.fees,
                    net: proceeds.net,
                    future_tax,
                    future_tax_note: future_note,
                    feasible,
                    violations,
                    caveats,
                    transfers: 2,
                });
            }
            ExtractionMethod::Dividend => {
                strategies.push(Strategy {
                    name: format!("{} → dividend", company.name),
                    steps: Vec::new(),
                    gross_total: Money::zero(currency),
                    immediate_tax: Money::zero(currency),
                    fees: Money::zero(currency),
                    net: Money::zero(currency),
                    future_tax: Money::zero(currency),
                    future_tax_note: String::new(),
                    feasible: false,
                    violations: vec!["no verified distribution-tax rule and no distributable-profit evidence: eligibility unknown, so the route cannot be ranked".into()],
                    caveats,
                    transfers: 0,
                });
            }
        }
    }

    // Mixed: personal first, the shortfall from the first feasible company salary route.
    if let Some(personal_first) = strategies.first().filter(|s| s.name.starts_with("Personal") && !s.feasible).cloned()
        && let Some(route) = plan.company_routes.iter().find(|r| r.allowed && r.method == ExtractionMethod::Salary)
        && let Some(company) = household.company(route.company)
        && let Some((rate_bp, _)) = salary_withholding_bp(household, on)
    {
        let covered = personal_first.net;
        let shortfall = required.checked_sub(covered)?.clamped_at_zero();
        if shortfall.is_positive() {
            let cash = company_cash(household, route.company)?;
            let ceiling = cash.ceiling.money();
            let proceeds = gross_up(shortfall, &|gross| Ok((gross.share_basis_points(rate_bp), Money::zero(currency))))?;
            let mut violations: Vec<String> = personal_first.violations.iter().filter(|v| !v.contains("short:")).cloned().collect();
            if proceeds.gross.minor() > ceiling.minor() {
                violations.push(format!("gross salary {} exceeds the {} ceiling {}", proceeds.gross.format(), company.name, ceiling.format()));
            }
            let (future_tax, future_note) = match company.owners.first().map(|o| o.person) {
                Some(person) => incremental_annual_tax(household, person, on.year(), proceeds.gross, on)?,
                None => (Money::zero(currency), "no owner on record".into()),
            };
            let mut steps = personal_first.steps.clone();
            steps.push(FundingStep {
                source: format!("{} (salary, for the shortfall)", company.name),
                account: route.to_account,
                gross: proceeds.gross,
                withholding: proceeds.withholding,
                fees: proceeds.fees,
                net: proceeds.net,
                ending_balance: ceiling.checked_sub(proceeds.gross)?,
                floor: cash.committed.money(),
                note: format!("gross-up of the {} shortfall at {}.{:02}% withholding", shortfall.format(), rate_bp / 100, rate_bp % 100),
            });
            let feasible = violations.is_empty();
            strategies.push(Strategy {
                name: format!("Personal accounts, then {} salary for the shortfall", company.name),
                transfers: steps.len() + 1,
                gross_total: personal_first.gross_total.checked_add(proceeds.gross)?,
                immediate_tax: proceeds.withholding,
                fees: personal_first.fees,
                net: personal_first.net.checked_add(proceeds.net)?,
                future_tax,
                future_tax_note: future_note,
                feasible,
                violations,
                caveats: vec![format!("{} legal capacity for an owner salary must be established.", company.name)],
                steps,
            });
        }
    }

    // §13.3: maximum tax cost.
    if let Some(cap) = plan.max_tax_and_fees {
        for strategy in strategies.iter_mut() {
            if strategy.tax_and_fees().minor() > cap.minor() {
                strategy.feasible = false;
                strategy.violations.push(format!("tax + fees {} exceed the maximum {}", strategy.tax_and_fees().format(), cap.format()));
            }
        }
    }

    // Ranking under the objective, feasible strategies only.
    let key = |s: &Strategy| -> i128 {
        match plan.objective {
            Objective::MinimiseTaxAndFees => s.tax_and_fees().minor() as i128 + s.future_tax.minor() as i128,
            Objective::MaximiseLowestCash => -(s.steps.iter().map(|st| st.ending_balance.checked_sub(st.floor).map(|m| m.minor()).unwrap_or(0)).min().unwrap_or(0) as i128),
            Objective::MinimiseFinancingCost => s.tax_and_fees().minor() as i128,
            Objective::FewestTransfers => s.transfers as i128 * 1_000_000_000_000 + s.tax_and_fees().minor() as i128,
        }
    };
    let preferred = strategies.iter().enumerate().filter(|(_, s)| s.feasible).min_by_key(|(_, s)| key(s)).map(|(i, _)| i);
    let feasible_count = strategies.iter().filter(|s| s.feasible).count();
    let status = match preferred {
        Some(i) => format!("Best among {} enumerated candidates ({} feasible) under “{}”: {} — not a global optimum", strategies.len(), feasible_count, plan.objective.label(), strategies[i].name),
        None => format!("No feasible strategy among {} enumerated candidates under the constraints; the decision cannot be funded as specified", strategies.len()),
    };
    let search_space = format!(
        "{} personal source{} in configured order, {} company route{}, mixed personal+salary when personal falls short; fees and withholding recomputed on gross amounts; no account order search",
        capacities.len(),
        if capacities.len() == 1 { "" } else { "s" },
        plan.company_routes.iter().filter(|r| r.allowed).count(),
        if plan.company_routes.iter().filter(|r| r.allowed).count() == 1 { "" } else { "s" }
    );
    log::info!("funding strategies for “{}”: {} evaluated, {} feasible, preferred {:?}", plan.name, strategies.len(), feasible_count, preferred);
    Ok(StrategyReport { objective: plan.objective, strategies, preferred, status, search_space })
}

// ----- §19: the decision -----------------------------------------------------------

/// A goal date, baseline versus decision (§20).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GoalImpact {
    pub goal: GoalId,
    pub name: String,
    pub amount: Money,
    pub target_on: NaiveDate,
    pub baseline_reached: Option<NaiveDate>,
    pub decision_reached: Option<NaiveDate>,
    pub delay_days: Option<i64>,
    pub text: String,
}

/// §19.1 — one affordability metric with its value and how it was obtained.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AffordabilityMetric {
    pub name: String,
    pub value: String,
    pub money: Option<Money>,
    pub how: String,
}

/// §19.2 — one cell of the purchase-month × down-payment grid.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GridCell {
    pub purchase_on: NaiveDate,
    pub down_payment: Money,
    pub lowest: Money,
    pub lowest_on: Option<NaiveDate>,
    pub reserve_ok: bool,
    pub shortfall: Money,
    pub financing_cost: Money,
    pub best: bool,
}

/// §26 — the deterministic recommendation contract.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Recommendation {
    pub action: String,
    pub objective: String,
    pub candidates_evaluated: usize,
    pub feasible: usize,
    pub constraints: Vec<String>,
    pub winning_strategy: String,
    pub metrics: Vec<String>,
    pub alternatives: Vec<String>,
    pub assumptions: Vec<String>,
    pub applied_rules: Vec<String>,
    pub explanation: String,
}

impl Recommendation {
    pub fn render(&self) -> String {
        let mut text = format!("Recommendation:\n{}\n\nObjective:\n{}\n\nWhy:\n", self.action, self.objective);
        for m in &self.metrics {
            text.push_str(&format!("- {m}\n"));
        }
        text.push_str(&format!("\nAlternatives tested: {}\nFeasible alternatives: {}\n", self.candidates_evaluated, self.feasible));
        if !self.alternatives.is_empty() {
            text.push_str("\nAlternatives:\n");
            for a in &self.alternatives {
                text.push_str(&format!("- {a}\n"));
            }
        }
        text.push_str("\nConstraints:\n");
        for c in &self.constraints {
            text.push_str(&format!("- {c}\n"));
        }
        text.push_str("\nKey assumptions:\n");
        for a in &self.assumptions {
            text.push_str(&format!("- {a}\n"));
        }
        text.push_str("\nApplied rules:\n");
        for r in &self.applied_rules {
            text.push_str(&format!("- {r}\n"));
        }
        text.push_str(&format!("\n{}\n", self.explanation));
        text
    }
}

/// Everything the result step shows.
#[derive(Clone, Debug)]
pub struct Decision {
    pub plan: PurchasePlan,
    pub through: NaiveDate,
    pub strategies: StrategyReport,
    pub baseline: BoundaryForecast,
    pub with_decision: BoundaryForecast,
    pub with_decision_conservative: BoundaryForecast,
    pub monthly_payment: Money,
    pub total_financing_cost: Money,
    pub metrics: Vec<AffordabilityMetric>,
    pub goals: Vec<GoalImpact>,
    pub grid: Vec<GridCell>,
    pub grid_status: String,
    pub statement: ConditionalStatement,
    pub recommendation: Recommendation,
    /// The chain behind "immediate cash after purchase".
    pub immediate_cash: Calc<Money>,
    /// Balances of both paths on the union of their dates, for the chart.
    pub merged_path: Vec<(NaiveDate, Money, Money)>,
    pub company_consequences: Vec<String>,
}

/// The series the plan adds, tagged with `scenario` (the down payment per
/// funding step, the salary extraction if any, instalments, other and
/// running costs).
pub fn plan_series(household: &Household, plan: &PurchasePlan, strategy: Option<&Strategy>, scenario: ScenarioId, first_id: u32) -> Vec<EventSeries> {
    let currency = household.base_currency;
    let mut next = first_id;
    let mut id = || {
        next += 1;
        SeriesId::new(next - 1)
    };
    let base = |id: SeriesId, name: String, direction: Direction, amount: Money, recurrence: Recurrence, account: AccountId, order: u16, category: &str| EventSeries {
        id,
        name,
        direction,
        amount: AmountSpec::Exact(amount),
        amount_changes: Vec::new(),
        exceptions: Vec::new(),
        recurrence,
        settlement_lag_days: 0,
        availability_lag_days: 0,
        intraday_order: order,
        account,
        linked_account: None,
        entity: household.account(account).map(|a| a.holder.primary_entity()).unwrap_or(EntityRef::Household),
        certainty: Certainty::ScenarioOnly,
        category: category.into(),
        tax_treatment: String::new(),
        scenario: Some(scenario),
        notes: format!("Decision “{}”", plan.name),
    };
    let mut out = Vec::new();
    let fallback_account = plan.sources.iter().find(|s| s.allowed).map(|s| s.account).or_else(|| household.accounts.first().map(|a| a.id));
    match strategy {
        Some(strategy) if !strategy.steps.is_empty() => {
            for step in &strategy.steps {
                if step.withholding.is_positive() {
                    // Company salary extraction: gross income on the receiving account, linked to the company.
                    let mut salary = base(id(), format!("{}: {}", plan.name, step.source), Direction::Income, step.gross, Recurrence::OneTime { on: DateSpec::Exact(plan.purchase_on) }, step.account, 20, "Salary");
                    salary.linked_account = plan.company_routes.iter().find(|r| r.allowed).and_then(|r| household.company_accounts(r.company).next().map(|a| a.id));
                    salary.tax_treatment = "Salary (withholding at source)".into();
                    out.push(salary);
                    // The net is then paid out for the purchase.
                    out.push(base(id(), format!("{} down payment from {}", plan.name, step.source), Direction::Expense, step.net.checked_add(step.fees).unwrap_or(step.net), Recurrence::OneTime { on: DateSpec::Exact(plan.purchase_on) }, step.account, 30, "Major purchase"));
                } else {
                    out.push(base(id(), format!("{} down payment from {}", plan.name, step.source), Direction::Expense, step.net, Recurrence::OneTime { on: DateSpec::Exact(plan.purchase_on) }, step.account, 30, "Major purchase"));
                }
            }
        }
        _ => {
            if let Some(account) = fallback_account {
                out.push(base(id(), format!("{} down payment", plan.name), Direction::Expense, plan.down_payment, Recurrence::OneTime { on: DateSpec::Exact(plan.purchase_on) }, account, 30, "Major purchase"));
            }
        }
    }
    if let Some(f) = &plan.financing
        && plan.financed().is_positive()
        && f.months > 0
    {
        let schedule = loan_schedule(plan.financed(), f.months, f.annual_rate_basis_points / 12);
        let payment = schedule.first().map(|i| i.payment).unwrap_or_else(|| monthly_payment(plan.financed(), f.months, f.annual_rate_basis_points));
        let mut instalments = base(
            id(),
            format!("{} instalment ({} × {})", plan.name, f.months, payment.format()),
            Direction::Expense,
            payment,
            Recurrence::Monthly { every_n_months: 1, day: f.first_instalment.day() as u8, from: f.first_instalment, until: Until::Count(f.months), invalid_day: InvalidDayPolicy::ClampToMonthEnd },
            f.account,
            10,
            "Financing",
        );
        // E06: the contract's rounded payments are replayed; the final one is adjusted so the
        // principal reaches exactly zero instead of assuming the unrounded formula does.
        if let (Some(last), Some(final_on)) = (schedule.last(), f.first_instalment.checked_add_months(Months::new(f.months.saturating_sub(1))))
            && last.payment != payment
        {
            instalments.change_amount_from(final_on, AmountSpec::Exact(last.payment));
            instalments.notes.push_str(&format!(" · final instalment {} (rounded schedule replayed)", last.payment.format()));
        }
        out.push(instalments);
    }
    for cost in &plan.other_costs {
        out.push(base(id(), format!("{}: {}", plan.name, cost.label), Direction::Expense, cost.amount, Recurrence::OneTime { on: DateSpec::Exact(cost.on) }, cost.account, 30, "Major purchase"));
    }
    if let Some(r) = &plan.running_cost {
        out.push(base(id(), format!("{}: {}", plan.name, r.label), Direction::Expense, r.monthly, Recurrence::Monthly { every_n_months: 1, day: r.from.day() as u8, from: r.from, until: Until::Indefinite, invalid_day: InvalidDayPolicy::ClampToMonthEnd }, r.account, 10, "Running costs"));
    }
    let _ = currency;
    out
}

/// The household with the decision applied as a scenario overlay.
fn with_plan(household: &Household, plan: &PurchasePlan, strategy: Option<&Strategy>) -> (Household, ScenarioId) {
    let mut out = household.clone();
    let scenario = out.next_scenario_id();
    out.scenarios.push(Scenario { id: scenario, name: format!("Decision: {}", plan.name), description: String::new(), private_to: None, changes: Vec::new(), composed_of: Vec::new() });
    let first_id = out.series.iter().map(|s| s.id.raw()).max().unwrap_or(0) + 1;
    out.series.extend(plan_series(household, plan, strategy, scenario, first_id));
    (out, scenario)
}

/// §20 — the first date from which the path holds `reserve + amount` for the
/// rest of the window (the goal's money is there and stays there). `None`
/// when the window ends below that level.
pub fn goal_reach_date(path: &[PathPoint], start: Money, as_of: NaiveDate, reserve: Money, amount: Money) -> Option<NaiveDate> {
    let needed = reserve.checked_add(amount).ok()?;
    let last_below = path.iter().filter(|p| p.balance.minor() < needed.minor()).map(|p| p.date).max();
    match last_below {
        None => {
            if start.minor() >= needed.minor() { Some(as_of) } else { path.first().map(|p| p.date) }
        }
        Some(last) => path.iter().find(|p| p.date >= last && p.balance.minor() >= needed.minor() && !path.iter().any(|q| q.date > p.date && q.balance.minor() < needed.minor())).map(|p| p.date),
    }
}

/// Evaluates the whole plan (§19).
pub fn evaluate(household: &Household, plan: &PurchasePlan, at_least: NaiveDate) -> EngineResult<Decision> {
    let currency = household.base_currency;
    let through = plan.horizon(household, at_least);
    let strategies = funding_strategies(household, plan)?;
    let strategy = strategies.preferred.map(|i| &strategies.strategies[i]).or_else(|| strategies.strategies.first());
    let (overlaid, scenario) = with_plan(household, plan, strategy);
    let baseline = forecast(household, Boundary::Household, ForecastOptions { through, scenario: None, case: Case::Expected })?;
    let with_decision = forecast(&overlaid, Boundary::Household, ForecastOptions { through, scenario: Some(scenario), case: Case::Expected })?;
    let with_decision_conservative = forecast(&overlaid, Boundary::Household, ForecastOptions { through, scenario: Some(scenario), case: Case::Conservative })?;

    let monthly_payment = plan.financing.as_ref().map(|f| monthly_payment(plan.financed(), f.months, f.annual_rate_basis_points)).unwrap_or(Money::zero(currency));
    let total_financing_cost = match &plan.financing {
        Some(f) if plan.financed().is_positive() => Money::new(monthly_payment.minor() * f.months as i64, currency).checked_sub(plan.financed())?.clamped_at_zero(),
        _ => Money::zero(currency),
    };

    // §19.1 metrics.
    let immediate = balance_on(&with_decision.path, with_decision.start.money(), plan.purchase_on);
    let before_purchase = balance_on(&baseline.path, baseline.start.money(), plan.purchase_on);
    let strategy_cost = strategy.map(|s| s.tax_and_fees()).unwrap_or(Money::zero(currency));
    let immediate_node = ProvNode::sum(
        format!("Household cash right after the purchase on {}", plan.purchase_on.format("%d %b %Y")),
        immediate,
        vec![
            ProvNode::input("Baseline household cash on the purchase date", before_purchase, "expected case, after that day's postings").money_class(MoneyClass::ExpectedFuture).certainty(Certainty::Expected),
            ProvNode::input("Down payment (net delivered)", plan.down_payment, "the plan").money_class(MoneyClass::ConditionalFuture).certainty(Certainty::ScenarioOnly).minus(),
            ProvNode::input("Tax and fees triggered by funding", strategy_cost, strategy.map(|s| s.name.clone()).unwrap_or_default()).money_class(MoneyClass::ConditionalFuture).certainty(Certainty::ScenarioOnly).minus(),
            ProvNode::input(
                "Other costs on the purchase date",
                plan.other_costs.iter().filter(|c| c.on == plan.purchase_on).fold(Money::zero(currency), |acc, c| acc.checked_add(c.amount).unwrap_or(acc)),
                "the plan",
            )
            .money_class(MoneyClass::ConditionalFuture)
            .certainty(Certainty::ScenarioOnly)
            .minus(),
        ],
    )
    .strength(ResultStrength::ScenarioTested)
    .note("Salary extraction, if any, enters as gross income on the same day and its withholding as a tax posting; the terms above are the decision's own figures, the total is the forecast's.");
    let after = |f: &BoundaryForecast| -> (Money, Option<NaiveDate>) {
        f.path.iter().filter(|p| p.date >= plan.purchase_on).min_by_key(|p| (p.balance.minor(), p.date)).map(|p| (p.balance, Some(p.date))).unwrap_or((f.end.money(), None))
    };
    let (lowest_after, lowest_after_on) = after(&with_decision);
    let (lowest_after_cons, lowest_after_cons_on) = after(&with_decision_conservative);
    let reserve_breach = analyse(&with_decision.path, plan.reserve, through)?;
    let months_after = ((through - plan.purchase_on).num_days() / 30).max(1);
    let free_cash_flow = with_decision.end.money().checked_sub(immediate)?;
    let monthly_free = Money::new(free_cash_flow.minor() / months_after, currency);
    let recovery = with_decision.path.iter().find(|p| p.date > plan.purchase_on && p.balance.minor() >= before_purchase.minor()).map(|p| p.date);
    let mut metrics = vec![
        AffordabilityMetric { name: "Immediate cash after purchase".into(), value: immediate.format(), money: Some(immediate), how: "household cash after the purchase date's postings, expected case".into() },
        AffordabilityMetric {
            name: "Lowest projected cash after purchase".into(),
            value: format!("{} (conservative case {})", lowest_after.format(), lowest_after_cons.format()),
            money: Some(lowest_after),
            how: "minimum of the household path from the purchase date, after intraday ordering".into(),
        },
        AffordabilityMetric {
            name: "Date of lowest balance".into(),
            value: format!("{} (conservative {})", lowest_after_on.map(|d| d.format("%d %b %Y").to_string()).unwrap_or_else(|| "–".into()), lowest_after_cons_on.map(|d| d.format("%d %b %Y").to_string()).unwrap_or_else(|| "–".into())),
            money: None,
            how: String::new(),
        },
        AffordabilityMetric {
            name: "Emergency reserve remaining".into(),
            value: lowest_after.checked_sub(plan.reserve)?.format_signed(),
            money: Some(lowest_after.checked_sub(plan.reserve)?),
            how: format!("lowest cash after purchase minus the {} reserve; negative means the reserve is breached", plan.reserve.format()),
        },
        AffordabilityMetric {
            name: "Future shortfalls caused by the purchase".into(),
            value: match reserve_breach.first_breach {
                None => format!("none through {}", through.format("%d %b %Y")),
                Some(first) => format!("reserve crossed {} · worst {} below · {} days · recovery {}", first.format("%d %b %Y"), reserve_breach.worst_deficit.format(), reserve_breach.days_below, reserve_breach.recovery.map(|d| d.format("%d %b %Y").to_string()).unwrap_or_else(|| "not within the window".into())),
            },
            money: None,
            how: "first day the decision path falls below the reserve".into(),
        },
        AffordabilityMetric { name: "Monthly repayment".into(), value: monthly_payment.format(), money: Some(monthly_payment), how: plan.financing.as_ref().map(|f| format!("annuity on {} over {} months at {}.{:02}% nominal", plan.financed().format(), f.months, f.annual_rate_basis_points / 100, f.annual_rate_basis_points % 100)).unwrap_or_else(|| "no financing".into()) },
        AffordabilityMetric { name: "Total financing cost".into(), value: total_financing_cost.format(), money: Some(total_financing_cost), how: "instalments × months − financed amount".into() },
        AffordabilityMetric { name: "Free cash flow after purchase".into(), value: format!("{} per month ({} over {} months)", monthly_free.format_signed(), free_cash_flow.format_signed(), months_after), money: Some(monthly_free), how: "end-of-window cash minus cash after purchase, per 30 days".into() },
        AffordabilityMetric {
            name: "Time to recover the pre-purchase cash level".into(),
            value: match recovery {
                Some(d) => format!("{} ({} days)", d.format("%d %b %Y"), (d - plan.purchase_on).num_days()),
                None => format!("not within the window (baseline level {} on the purchase date)", before_purchase.format()),
            },
            money: None,
            how: "first date after the purchase on which household cash is back at its baseline level of the purchase date".into(),
        },
        AffordabilityMetric { name: "Taxes and fees triggered by funding".into(), value: strategy.map(|s| format!("{} now{}", s.tax_and_fees().format(), if s.future_tax.is_positive() { format!(" + {} future incremental tax", s.future_tax.format()) } else { String::new() })).unwrap_or_else(|| "no strategy".into()), money: Some(strategy_cost), how: strategy.map(|s| s.name.clone()).unwrap_or_default() },
    ];

    // §20 goals.
    let goals: Vec<GoalImpact> = household
        .goals
        .iter()
        .map(|g| {
            let baseline_reached = goal_reach_date(&baseline.path, baseline.start.money(), household.as_of, plan.reserve, g.amount);
            let decision_reached = goal_reach_date(&with_decision.path, with_decision.start.money(), household.as_of, plan.reserve, g.amount);
            let delay_days = match (baseline_reached, decision_reached) {
                (Some(b), Some(d)) => Some((d - b).num_days()),
                _ => None,
            };
            let text = match (baseline_reached, decision_reached) {
                (Some(b), Some(d)) if d > b => format!("delayed from {} to {} ({} days){}", b.format("%d %b %Y"), d.format("%d %b %Y"), (d - b).num_days(), if d > g.target_on { " — past its target" } else { "" }),
                (Some(b), Some(_)) => format!("unchanged: reachable {}{}", b.format("%d %b %Y"), if b > g.target_on { " (after its target even without the purchase)" } else { "" }),
                (Some(b), None) => format!("made infeasible within the window (baseline {})", b.format("%d %b %Y")),
                (None, _) => "not reachable within the window even without the purchase".into(),
            };
            GoalImpact { goal: g.id, name: g.name.clone(), amount: g.amount, target_on: g.target_on, baseline_reached, decision_reached, delay_days, text }
        })
        .collect();
    metrics.push(AffordabilityMetric {
        name: "Goals delayed or made infeasible".into(),
        value: if goals.is_empty() { "no goals defined".into() } else { goals.iter().map(|g| format!("{}: {}", g.name, g.text)).collect::<Vec<_>>().join(" · ") },
        money: None,
        how: "first date the household path holds reserve + goal amount, baseline vs decision".into(),
    });

    // Company consequences.
    let mut company_consequences = Vec::new();
    for route in plan.company_routes.iter().filter(|r| r.allowed) {
        if let Some(company) = household.company(route.company) {
            let base = forecast(household, Boundary::Company(route.company), ForecastOptions { through, scenario: None, case: Case::Expected })?;
            let over = forecast(&overlaid, Boundary::Company(route.company), ForecastOptions { through, scenario: Some(scenario), case: Case::Expected })?;
            let delta = over.end.money().checked_sub(base.end.money())?;
            company_consequences.push(format!("{}: cash at the end {} → {} ({}); lowest {} → {}", company.name, base.end.money().format(), over.end.money().format(), delta.format_signed(), base.lowest.money().format(), over.lowest.money().format()));
        }
    }
    metrics.push(AffordabilityMetric { name: "Company cash consequences".into(), value: if company_consequences.is_empty() { "no company route considered".into() } else { company_consequences.join(" · ") }, money: None, how: "company forecast with vs without the extraction".into() });

    // §19.2 grid (conservative case, E03 logic: the worst stated path).
    let mut grid = Vec::new();
    let mut month = NaiveDate::from_ymd_opt(plan.window_from.year(), plan.window_from.month(), 1).expect("valid");
    let last = NaiveDate::from_ymd_opt(plan.window_to.year(), plan.window_to.month(), 1).expect("valid");
    let day = plan.purchase_on.day();
    let mut guard = 0;
    while month <= last && guard < 24 {
        guard += 1;
        let purchase_on = clamp_day(month, day);
        let mut dp = plan.down_payment_low;
        let step = if plan.down_payment_step.is_positive() { plan.down_payment_step } else { plan.down_payment_high.checked_sub(plan.down_payment_low)?.max(Money::new(1, currency))? };
        let mut steps_guard = 0;
        while dp.minor() <= plan.down_payment_high.minor() && steps_guard < 40 {
            steps_guard += 1;
            let variant = PurchasePlan { purchase_on, down_payment: dp, ..plan.clone() };
            let (h, sc) = with_plan(household, &variant, None);
            let through_cell = variant.horizon(household, at_least);
            let f = forecast(&h, Boundary::Household, ForecastOptions { through: through_cell, scenario: Some(sc), case: Case::Conservative })?;
            let (lowest, lowest_on) = f.path.iter().filter(|p| p.date >= purchase_on).min_by_key(|p| (p.balance.minor(), p.date)).map(|p| (p.balance, Some(p.date))).unwrap_or((f.end.money(), None));
            let shortfall = plan.reserve.checked_sub(lowest)?.clamped_at_zero();
            let cost = match &plan.financing {
                Some(fin) if variant.financed().is_positive() => Money::new(monthly_payment_of(variant.financed(), fin).minor() * fin.months as i64, currency).checked_sub(variant.financed())?.clamped_at_zero(),
                _ => Money::zero(currency),
            };
            grid.push(GridCell { purchase_on, down_payment: dp, lowest, lowest_on, reserve_ok: shortfall.is_zero(), shortfall, financing_cost: cost, best: false });
            dp = dp.checked_add(step)?;
        }
        month = month.checked_add_months(Months::new(1)).expect("valid");
    }
    let feasible_cells = grid.iter().filter(|c| c.reserve_ok).count();
    let best_index = grid
        .iter()
        .enumerate()
        .filter(|(_, c)| c.reserve_ok)
        .min_by_key(|(_, c)| match plan.objective {
            Objective::MaximiseLowestCash => (-(c.lowest.minor() as i128), c.purchase_on.num_days_from_ce() as i128),
            Objective::MinimiseFinancingCost => (c.financing_cost.minor() as i128, -(c.lowest.minor() as i128)),
            Objective::MinimiseTaxAndFees | Objective::FewestTransfers => (c.financing_cost.minor() as i128, -(c.lowest.minor() as i128)),
        })
        .map(|(i, _)| i);
    if let Some(i) = best_index {
        grid[i].best = true;
    }
    let grid_status = match best_index {
        Some(i) => format!(
            "Best on the specified grid under “{}”: {} with a {} down payment — {} combinations evaluated, {} keep the {} reserve (conservative case)",
            plan.objective.label(),
            grid[i].purchase_on.format("%d %b %Y"),
            grid[i].down_payment.format(),
            grid.len(),
            feasible_cells,
            plan.reserve.format()
        ),
        None => format!("No combination on the specified grid keeps the {} reserve in the conservative case ({} evaluated)", plan.reserve.format(), grid.len()),
    };

    // §19.3 conditional statement from the ranged series in the window.
    let mut assumptions_text: Vec<String> = Vec::new();
    for occurrence in household.expand_all(household.as_of, through, None) {
        if let AmountSpec::Range { low, high, .. } = occurrence.amount
            && let Some(series) = household.series_by_id(occurrence.series)
        {
            let text = match series.direction {
                Direction::Income => format!("{} on {} ≥ {}", series.name, occurrence.due.format("%d %b %Y"), low.format()),
                _ => format!("{} on {} ≤ {}", series.name, occurrence.due.format("%d %b %Y"), high.format()),
            };
            if !assumptions_text.contains(&text) {
                assumptions_text.push(text);
            }
        }
    }
    for series in household.series.iter().filter(|s| s.scenario.is_none()) {
        if let Recurrence::OneTime { on: DateSpec::Range { latest, .. } } = &series.recurrence
            && series.direction == Direction::Income
        {
            assumptions_text.push(format!("{} arrives by {}", series.name, latest.format("%d %b %Y")));
        }
    }
    for a in &with_decision.assumptions {
        if !assumptions_text.contains(&a.text) {
            assumptions_text.push(a.text.clone());
        }
    }
    for pack in &with_decision.record.tax_rule_packs {
        assumptions_text.push(format!("{pack} remains applicable"));
    }
    let keeps_reserve = lowest_after_cons.minor() >= plan.reserve.minor();
    let statement = ConditionalStatement {
        claim: if keeps_reserve {
            format!("{} with a {} down payment on {} keeps the {} household reserve", plan.name, plan.down_payment.format(), plan.purchase_on.format("%d %b %Y"), plan.reserve.format())
        } else {
            format!("{} with a {} down payment on {} breaches the {} household reserve by {} in the conservative case", plan.name, plan.down_payment.format(), plan.purchase_on.format("%d %b %Y"), plan.reserve.format(), plan.reserve.checked_sub(lowest_after_cons)?.format())
        },
        horizon: through,
        coverage: ResultStrength::ScenarioTested,
        assumptions: assumptions_text.clone(),
        excluded_shocks: vec!["no additional unplanned expense".into(), "no late or missing planned inflow beyond the stated ranges".into(), "no change in the effective tax packs or rules".into()],
    };

    // §26 recommendation.
    let feasible = strategies.strategies.iter().filter(|s| s.feasible).count();
    let winner = strategies.preferred.map(|i| &strategies.strategies[i]);
    let mut constraints = vec![format!("household reserve ≥ {}", plan.reserve.format())];
    for s in plan.sources.iter() {
        let name = household.account(s.account).map(|a| a.name.clone()).unwrap_or_default();
        match (s.allowed, s.floor) {
            (false, _) => constraints.push(format!("do not use {name}")),
            (true, Some(f)) => constraints.push(format!("never reduce {name} below {}", f.format())),
            _ => {}
        }
    }
    if let Some(cap) = plan.max_tax_and_fees {
        constraints.push(format!("tax + fees ≤ {}", cap.format()));
    }
    let recommendation = Recommendation {
        action: match (winner, keeps_reserve) {
            (Some(w), true) => format!("Consider {} on {} with a {} down payment funded via “{}”.", plan.name.to_lowercase(), plan.purchase_on.format("%d %b %Y"), plan.down_payment.format(), w.name),
            (Some(w), false) => format!("{} on {} with a {} down payment via “{}” does not keep the reserve in the conservative case; see the purchase month × down payment table for combinations that do.", plan.name, plan.purchase_on.format("%d %b %Y"), plan.down_payment.format(), w.name),
            (None, _) => format!("{} on {} with a {} down payment cannot be funded under the constraints.", plan.name, plan.purchase_on.format("%d %b %Y"), plan.down_payment.format()),
        },
        objective: format!("{} while maintaining ≥ {} household reserve", plan.objective.label(), plan.reserve.format()),
        candidates_evaluated: strategies.strategies.len() + grid.len(),
        feasible: feasible + feasible_cells,
        constraints,
        winning_strategy: winner.map(|w| w.name.clone()).unwrap_or_else(|| "none".into()),
        metrics: vec![
            format!("Lowest projected household cash after purchase: {} (conservative {})", lowest_after.format(), lowest_after_cons.format()),
            format!("Total funding tax/fees: {}{}", strategy_cost.format(), winner.filter(|w| w.future_tax.is_positive()).map(|w| format!(" + {} future incremental tax", w.future_tax.format())).unwrap_or_default()),
            format!("Monthly repayment {} · total financing cost {}", monthly_payment.format(), total_financing_cost.format()),
            match reserve_breach.first_breach {
                None => "The reserve is never crossed in the window".into(),
                Some(d) => format!("The reserve is crossed on {}", d.format("%d %b %Y")),
            },
            if company_consequences.is_empty() { "No company cash used".into() } else { company_consequences.join("; ") },
        ],
        alternatives: strategies.strategies.iter().enumerate().filter(|(i, _)| Some(*i) != strategies.preferred).map(|(_, s)| format!("{} — {}{}", s.name, if s.feasible { "feasible" } else { "infeasible" }, if s.violations.is_empty() { String::new() } else { format!(": {}", s.violations.join("; ")) })).collect(),
        assumptions: assumptions_text,
        applied_rules: with_decision.record.rules_applied.clone(),
        explanation: "This is not an AI recommendation. It is the reported result of deterministic search under the inputs above: enumerated strategies and grid cells, ranked under the stated objective.".into(),
    };

    let mut dates: std::collections::BTreeSet<NaiveDate> = baseline.path.iter().map(|p| p.date).collect();
    dates.extend(with_decision.path.iter().map(|p| p.date));
    let merged_path = dates.into_iter().map(|d| (d, balance_on(&baseline.path, baseline.start.money(), d), balance_on(&with_decision.path, with_decision.start.money(), d))).collect();

    log::info!("decision “{}” evaluated: immediate {}, lowest after {}, reserve {}, grid {}/{} feasible", plan.name, immediate.format(), lowest_after.format(), if keeps_reserve { "kept" } else { "breached" }, feasible_cells, grid.len());
    Ok(Decision {
        plan: plan.clone(),
        through,
        strategies,
        baseline,
        with_decision,
        with_decision_conservative,
        monthly_payment,
        total_financing_cost,
        metrics,
        goals,
        grid,
        grid_status,
        statement,
        recommendation,
        immediate_cash: Calc::new(immediate, immediate_node),
        merged_path,
        company_consequences,
    })
}

fn monthly_payment_of(principal: Money, f: &Financing) -> Money {
    monthly_payment(principal, f.months, f.annual_rate_basis_points)
}

fn clamp_day(month_start: NaiveDate, day: u32) -> NaiveDate {
    let mut d = day;
    loop {
        if let Some(date) = NaiveDate::from_ymd_opt(month_start.year(), month_start.month(), d) {
            return date;
        }
        d -= 1;
        if d == 0 {
            return month_start;
        }
    }
}

impl Household {
    /// Saves the decision as a scenario with its series (so it can be compared
    /// and its events appear on the timeline), with an access policy.
    pub fn save_decision_as_scenario(&mut self, plan: &PurchasePlan, strategy: Option<&Strategy>, owner: PersonId) -> ScenarioId {
        let id = self.next_scenario_id();
        let first_id = self.series.iter().map(|s| s.id.raw()).max().unwrap_or(0) + 1;
        let series = plan_series(self, plan, strategy, id, first_id);
        let description = format!(
            "Decision: {} on {} with a {} down payment{}{}",
            plan.name,
            plan.purchase_on.format("%d %b %Y"),
            plan.down_payment.format(),
            plan.financing.as_ref().map(|f| format!(", {} instalments", f.months)).unwrap_or_default(),
            strategy.map(|s| format!(", funded via “{}”", s.name)).unwrap_or_default()
        );
        self.series.extend(series);
        self.add_scenario(Scenario { id, name: format!("Decision: {}", plan.name), description, private_to: None, changes: Vec::new(), composed_of: Vec::new() }, owner)
    }

    pub fn next_goal_id(&self) -> GoalId {
        GoalId::new(self.goals.iter().map(|g| g.id.raw()).max().unwrap_or(0) + 1)
    }
}

fn purchase_on_for(as_of: NaiveDate) -> NaiveDate {
    let purchase_on = as_of.checked_add_months(Months::new(2)).unwrap_or(as_of);
    NaiveDate::from_ymd_opt(purchase_on.year(), purchase_on.month(), 15).unwrap_or(purchase_on)
}

/// [`default_plan`] restricted to what `viewer` may discover (§7.3, V062):
/// hidden accounts and companies are never offered as sources or routes.
pub fn default_plan_for(household: &Household, as_of: NaiveDate, viewer: crate::authz::Viewer) -> PurchasePlan {
    let mut plan = default_plan(household, as_of);
    let visible = |object: ObjectRef| !matches!(household.disclosure_for(viewer, object), crate::provenance::Disclosure::Hidden);
    plan.sources.retain(|s| visible(ObjectRef::Account(s.account)));
    plan.company_routes.retain(|r| visible(ObjectRef::Company(r.company)) && visible(ObjectRef::Account(r.to_account)));
    let first = plan.sources.iter().find(|s| s.allowed).map(|s| s.account).or_else(|| plan.sources.first().map(|s| s.account));
    if let Some(account) = first {
        if let Some(f) = plan.financing.as_mut() && !visible(ObjectRef::Account(f.account)) {
            f.account = account;
        }
        for cost in plan.other_costs.iter_mut() {
            if !visible(ObjectRef::Account(cost.account)) {
                cost.account = account;
            }
        }
        if let Some(r) = plan.running_cost.as_mut() && !visible(ObjectRef::Account(r.account)) {
            r.account = account;
        }
    } else {
        plan.financing = None;
        plan.other_costs.clear();
        plan.running_cost = None;
    }
    plan
}

/// A sensible default plan for a household: the usable personal accounts as
/// sources, every company as a (not yet allowed) salary route, financing over
/// 36 months.
pub fn default_plan(household: &Household, as_of: NaiveDate) -> PurchasePlan {
    let currency = household.base_currency;
    // Sources: personal cash that can actually pay on the purchase date — no cards, loans or
    // locked deposits; the user can still allow them on the step.
    let personal: Vec<AccountId> = household.accounts.iter().filter(|a| !a.holder.is_company() && a.include_in_household).map(|a| a.id).collect();
    let usable = |id: &AccountId| {
        household.account(*id).is_some_and(|a| {
            !matches!(a.kind, crate::model::AccountKind::CreditCard | crate::model::AccountKind::Loan | crate::model::AccountKind::CorporateCard)
                && match a.liquidity {
                    crate::model::Liquidity::LockedUntil(date) => date <= purchase_on_for(as_of),
                    _ => true,
                }
                && a.settled_balance.is_positive()
        })
    };
    let purchase_on = purchase_on_for(as_of);
    let first_account = personal.iter().find(|id| usable(id)).copied().or_else(|| household.accounts.first().map(|a| a.id));
    PurchasePlan {
        name: "Car".into(),
        price: Money::from_major(8_000_000, currency),
        purchase_on,
        window_from: as_of.checked_add_months(Months::new(1)).unwrap_or(as_of),
        window_to: as_of.checked_add_months(Months::new(6)).unwrap_or(as_of),
        down_payment: Money::from_major(2_500_000, currency),
        down_payment_low: Money::from_major(2_000_000, currency),
        down_payment_high: Money::from_major(5_000_000, currency),
        down_payment_step: Money::from_major(500_000, currency),
        reserve: Money::from_major(1_000_000, currency),
        sources: personal.iter().map(|id| FundingSource { account: *id, allowed: usable(id), floor: None }).collect(),
        company_routes: household.companies.iter().filter_map(|c| first_account.map(|to| CompanyRoute { company: c.id, method: ExtractionMethod::Salary, allowed: false, to_account: to })).collect(),
        financing: first_account.map(|account| Financing { months: 36, annual_rate_basis_points: 1_200, first_instalment: purchase_on.checked_add_months(Months::new(1)).unwrap_or(purchase_on), account }),
        other_costs: first_account.map(|account| vec![OtherCost { label: "Registration and insurance".into(), amount: Money::from_major(150_000, currency), on: purchase_on, account }]).unwrap_or_default(),
        running_cost: first_account.map(|account| RunningCost { label: "Fuel, service and parking".into(), monthly: Money::from_major(25_000, currency), from: purchase_on.checked_add_days(Days::new(15)).unwrap_or(purchase_on), account }),
        objective: Objective::MinimiseTaxAndFees,
        max_tax_and_fees: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{self, e03_household, ids, pkr};

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn e02_gross_up_delivers_the_requested_net_exactly() {
        // E02's hypothetical strategy: 42,000 cash tax and 3,500 fees on the gross.
        // With a flat 4.0172…% the plan's own figures reproduce: 1,045,500 gross → 1,000,000 net.
        // Tax rounds up to the minor unit, as a real assessment would.
        let cost = |gross: Money| -> EngineResult<(Money, Money)> { Ok((Money::new((gross.minor() * 42_000 + 1_045_499) / 1_045_500, gross.currency()), pkr(3_500))) };
        let result = gross_up(pkr(1_000_000), &cost).unwrap();
        assert!(result.net.minor() >= pkr(1_000_000).minor(), "at least the requested net");
        assert_eq!(result.gross, pkr(1_045_500));
        assert_eq!(result.withholding, pkr(42_000));
        assert_eq!(result.net, pkr(1_000_000));
        // Withdrawing only the net gross would fall short (E02).
        let (tax, fees) = cost(pkr(1_000_000)).unwrap();
        assert!(pkr(1_000_000).minor() - tax.minor() - fees.minor() < pkr(1_000_000).minor());
        // Non-monotone threshold costs still verify: 5% once the gross passes 1,020,000.
        let threshold = |gross: Money| -> EngineResult<(Money, Money)> { Ok((if gross.minor() > pkr(1_020_000).minor() { gross.share_basis_points(500) } else { Money::zero(gross.currency()) }, pkr(0))) };
        let result = gross_up(pkr(1_000_000), &threshold).unwrap();
        assert!(result.net.minor() >= pkr(1_000_000).minor());
        assert_eq!(result.gross, pkr(1_000_000), "below the threshold no deduction applies");
        let result = gross_up(pkr(1_010_000), &threshold).unwrap();
        assert_eq!(result.gross, pkr(1_010_000));
        // Just above the threshold the smallest valid gross is found and verified.
        let result = gross_up(pkr(1_019_000), &threshold).unwrap();
        assert!(result.net.minor() >= pkr(1_019_000).minor());
        assert_eq!(result.gross, pkr(1_019_000));
    }

    #[test]
    fn e06_rounded_schedule_replays_to_zero_and_irr_has_two_roots() {
        let exact = annuity_payment_exact(pkr(1_000_000), 12, 100);
        assert!((exact / 100.0 - 88_848.78867834).abs() < 1e-6, "{exact}");
        let schedule = loan_schedule(pkr(1_000_000), 12, 100);
        assert_eq!(schedule.len(), 12);
        assert_eq!(schedule[0].payment, Money::new(8_884_879, crate::Currency::PKR), "88,848.79 rounded");
        assert_eq!(schedule[0].interest, pkr(10_000));
        assert!(schedule.last().unwrap().balance_after.is_zero(), "the final payment clears the principal exactly");
        assert_ne!(schedule.last().unwrap().payment, schedule[0].payment, "the final payment is adjusted, not assumed");
        let total: i64 = schedule.iter().map(|i| i.payment.minor()).sum();
        assert!((total - 12 * 8_884_879).abs() < 100, "adjustment stays within rounding drift");
        let roots = irr_roots(&[-100.0, 230.0, -132.0]);
        assert_eq!(roots.len(), 2, "{roots:?}");
        assert!((roots[0] - 0.10).abs() < 1e-6 && (roots[1] - 0.20).abs() < 1e-6, "{roots:?}");
    }

    #[test]
    fn annuity_payment_and_cost() {
        let payment = monthly_payment(pkr(5_500_000), 36, 1_200);
        // Standard annuity: 5,500,000 at 1%/month over 36 months ≈ 182,678.
        assert!((payment.minor() - pkr(182_678).minor()).abs() <= 100, "{}", payment.format());
        assert_eq!(monthly_payment(pkr(1_200_000), 12, 0), pkr(100_000));
        assert_eq!(monthly_payment(pkr(0), 12, 1_000), pkr(0));
    }

    fn e03_plan(down_payment: i64, on: NaiveDate) -> PurchasePlan {
        PurchasePlan {
            name: "Car".into(),
            price: pkr(down_payment),
            purchase_on: on,
            window_from: d(2026, 11, 1),
            window_to: d(2026, 11, 30),
            down_payment: pkr(down_payment),
            down_payment_low: pkr(1_200_000),
            down_payment_high: pkr(1_400_000),
            down_payment_step: pkr(100_000),
            reserve: pkr(1_000_000),
            sources: vec![FundingSource { account: AccountId::new(1), allowed: true, floor: None }],
            company_routes: Vec::new(),
            financing: None,
            other_costs: Vec::new(),
            running_cost: None,
            objective: Objective::MaximiseLowestCash,
            max_tax_and_fees: None,
        }
    }

    #[test]
    fn e03_grid_reproduces_the_plan_table() {
        // The E03 household without its own down-payment series; the plan supplies it.
        let mut household = e03_household(pkr(0), d(2026, 11, 15));
        household.series.retain(|s| s.name != "Down payment");
        let decision = evaluate(&household, &e03_plan(1_200_000, d(2026, 11, 15)), d(2027, 1, 31)).unwrap();
        let cell = |dp: i64, on: NaiveDate| decision.grid.iter().find(|c| c.down_payment == pkr(dp) && c.purchase_on == on).unwrap().clone();
        let c = cell(1_200_000, d(2026, 11, 15));
        assert_eq!(c.lowest, pkr(1_080_000));
        assert_eq!(c.lowest_on, Some(d(2026, 11, 15)));
        assert!(c.reserve_ok);
        let c = cell(1_300_000, d(2026, 11, 15));
        assert_eq!(c.lowest, pkr(980_000));
        assert_eq!(c.shortfall, pkr(20_000));
        let c = cell(1_400_000, d(2026, 11, 15));
        assert_eq!(c.lowest, pkr(880_000));
        assert_eq!(c.shortfall, pkr(120_000));
        // The best cell under "maximise lowest cash" is the smallest down payment.
        assert!(decision.grid.iter().find(|c| c.best).unwrap().down_payment == pkr(1_200_000));
        assert!(decision.grid_status.starts_with("Best on the specified grid"));
        // The 30 Nov variant from the table.
        let decision = evaluate(&household, &e03_plan(1_200_000, d(2026, 11, 30)), d(2027, 1, 31)).unwrap();
        let c = decision.grid.iter().find(|c| c.down_payment == pkr(1_200_000) && c.purchase_on == d(2026, 11, 30)).unwrap();
        assert_eq!(c.lowest, pkr(1_180_000));
        assert_eq!(c.lowest_on, Some(d(2027, 1, 15)));
        // The conditional statement lists the ranged assumptions and never claims certainty.
        let text = decision.statement.render();
        assert!(text.contains("Salary on 30 Sep 2026 ≥ 480,000"));
        assert!(text.contains("Client receipt arrives by 10 Dec 2026"));
        assert!(decision.recommendation.render().contains("not an AI recommendation"));
    }

    #[test]
    fn strategies_gross_up_company_salary_and_reject_reserve_violations() {
        let household = fixtures::plan_household();
        let mut plan = default_plan(&household, household.as_of);
        plan.purchase_on = d(2026, 11, 15);
        plan.down_payment = pkr(2_500_000);
        plan.sources = vec![
            FundingSource { account: ids::SHARED_SAVINGS, allowed: true, floor: Some(pkr(1_000_000)) },
            FundingSource { account: ids::PERSON_A_CURRENT, allowed: true, floor: Some(pkr(300_000)) },
        ];
        plan.company_routes = vec![CompanyRoute { company: ids::ALPHA, method: ExtractionMethod::Salary, allowed: true, to_account: ids::PERSON_A_CURRENT }];
        let report = funding_strategies(&household, &plan).unwrap();
        assert!(report.strategies.len() >= 2);
        let personal = &report.strategies[0];
        assert!(personal.name.starts_with("Personal"));
        // Every step delivers at least what it takes net (V014) and respects its floor.
        for step in &personal.steps {
            assert!(step.net.minor() <= step.gross.minor());
            assert!(step.ending_balance.minor() >= step.floor.minor());
        }
        let salary = report.strategies.iter().find(|s| s.name.contains("owner salary")).unwrap();
        // 7% withholding: gross ≈ 2,500,000 / 0.93 plus the fee, net exactly the down payment.
        assert_eq!(salary.net, pkr(2_500_000));
        assert!(salary.gross_total.minor() > pkr(2_688_000).minor());
        assert_eq!(salary.immediate_tax, salary.gross_total.share_basis_points(700));
        assert!(salary.caveats.iter().any(|c| c.contains("legal capacity")));
        // 2,688,000+ gross exceeds Alpha's E07 ceiling (500,000): cheapest-tax-looking or not, it is rejected.
        assert!(!salary.feasible);
        assert!(salary.violations[0].contains("ceiling"));
        assert!(salary.future_tax.is_positive(), "the annual brackets see the extra salary (modelled separately)");
        // Preferred is the feasible personal strategy; status says "among enumerated".
        assert_eq!(report.preferred, Some(0));
        assert!(report.status.contains("not a global optimum"));
        // V035: raising an independent reserve cannot improve the feasible optimum.
        let mut tighter = plan.clone();
        tighter.sources[0].floor = Some(pkr(2_000_000));
        let tight = funding_strategies(&household, &tighter).unwrap();
        let cost = |r: &StrategyReport| r.preferred.map(|i| r.strategies[i].tax_and_fees().minor());
        match (cost(&report), cost(&tight)) {
            (Some(a), Some(b)) => assert!(b >= a, "a tighter reserve must not lower the cost: {b} < {a}"),
            (Some(_), None) => {}
            (None, Some(_)) => panic!("a tighter reserve made an infeasible problem feasible"),
            (None, None) => {}
        }
        // A maximum tax cost below the fees makes every strategy infeasible.
        let mut capped = plan.clone();
        capped.max_tax_and_fees = Some(pkr(0));
        let capped = funding_strategies(&household, &capped).unwrap();
        assert!(capped.strategies.iter().any(|s| s.violations.iter().any(|v| v.contains("exceed the maximum"))));
    }

    #[test]
    fn decision_metrics_goals_and_save_as_scenario() {
        let mut household = fixtures::plan_household();
        let mut plan = default_plan(&household, household.as_of);
        plan.purchase_on = d(2026, 11, 15);
        let decision = evaluate(&household, &plan, fixtures::default_horizon()).unwrap();
        assert_eq!(decision.through, d(2027, 6, 30), "purchase + 6 months, extended to the house goal due within a year");
        assert_eq!(decision.monthly_payment, monthly_payment(pkr(5_500_000), 36, 1_200));
        assert!(decision.metrics.iter().any(|m| m.name == "Immediate cash after purchase"));
        assert!(decision.immediate_cash.node().verify_sums().is_empty());
        assert_eq!(decision.goals.len(), household.goals.len());
        assert!(!decision.grid.is_empty());
        assert!(decision.statement.render().contains("under the following assumptions"));
        // Saving creates a comparable scenario with the plan's series.
        let series_before = household.series.len();
        let strategy = decision.strategies.preferred.map(|i| decision.strategies.strategies[i].clone());
        let id = household.save_decision_as_scenario(&plan, strategy.as_ref(), ids::PERSON_A);
        assert!(household.series.len() > series_before);
        assert!(household.series.iter().filter(|s| s.scenario == Some(id)).count() >= 3);
        let comparison = crate::scenario::compare(&household, Boundary::Household, &[id], decision.through, Case::Expected).unwrap();
        assert!(comparison.attribution_verified);
        assert!(comparison.end_delta.is_negative());
    }
}
