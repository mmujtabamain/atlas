//! The conditional projection (§2.1, §2.4, §11). M0 builds the household
//! chain the plan uses as its flagship example — starting cash plus every
//! future series, each labelled with its certainty — and M4 extends it to
//! per-account chronological paths with lowest balances and breaches.

use crate::ids::*;
use crate::liquidity::{household_exclusion_reason, household_liquidity};
use crate::model::{Assumption, Household, Liquidity};
use crate::money::Money;
use crate::provenance::{Calc, ProvNode};
use crate::timeline::{Direction, Occurrence};
use crate::vocab::{Certainty, MoneyClass, ResultStrength};
use crate::breach::PathPoint;
use crate::{EngineError, EngineResult};
use chrono::NaiveDate;

/// A household-level conditional projection with its full chain.
#[derive(Clone, Debug)]
pub struct Projection {
    pub as_of: NaiveDate,
    pub through: NaiveDate,
    pub scenario: Option<ScenarioId>,
    /// §2.1 "Conditional projected cash".
    pub conditional_cash: Calc<Money>,
    /// §2.1 "Conditional projected unreserved cash".
    pub unreserved_cash: Calc<Money>,
    /// Every occurrence that entered the chain, chronologically.
    pub occurrences: Vec<Occurrence>,
    /// The household liquid-cash path: the balance after each date's
    /// postings, starting from `as_of` (M13 input; M4 refines per account).
    pub path: Vec<PathPoint>,
    /// The assumptions the chain depends on (§10.2).
    pub assumptions: Vec<Assumption>,
}

fn is_liquid_included(household: &Household, account: AccountId) -> bool {
    household
        .account(account)
        .map(|a| {
            household_exclusion_reason(household, a).is_none() && a.kind.is_cash() && a.liquidity == Liquidity::Immediate
        })
        .unwrap_or(false)
}

/// The §2.1 chain for the household through `through`, optionally with one
/// scenario overlay applied (§18). Series that belong to other scenarios are
/// left out; company series are shown as excluded (§8.5).
pub fn household_projection(
    household: &Household,
    through: NaiveDate,
    scenario: Option<ScenarioId>,
) -> EngineResult<Projection> {
    let currency = household.base_currency;
    let liquidity = household_liquidity(household)?;
    let mut terms: Vec<ProvNode> = vec![
        ProvNode::sum(
            "Current reconciled liquid cash",
            liquidity.liquid_cash.money(),
            liquidity.liquid_cash.node().children().to_vec(),
        )
        .money_class(MoneyClass::ConfirmedCurrent)
        .certainty(Certainty::Confirmed)
        .note("The only term already received (§2.1)."),
    ];
    let mut total = liquidity.liquid_cash.money();
    let mut occurrences: Vec<Occurrence> = Vec::new();
    let mut signed_postings: Vec<(NaiveDate, Money)> = Vec::new();
    let mut included_series: Vec<SeriesId> = Vec::new();
    let mut company_counts: Vec<(CompanyId, usize, Money)> = Vec::new();

    for series in &household.series {
        if series.scenario.is_some() && series.scenario != scenario {
            continue;
        }
        let expanded: Vec<Occurrence> = household.expand_series(series, household.as_of, through).into_iter().filter(|o| o.is_live()).collect();
        if expanded.is_empty() {
            continue;
        }
        if let EntityRef::Company(company) = series.entity {
            let sum = Money::sum(currency, expanded.iter().map(|o| o.remaining_expected()))?;
            match company_counts.iter_mut().find(|(id, _, _)| *id == company) {
                Some(entry) => {
                    entry.1 += expanded.len();
                    entry.2 = entry.2.checked_add(sum)?;
                }
                None => company_counts.push((company, expanded.len(), sum)),
            }
            continue;
        }
        let from_included = is_liquid_included(household, series.account);
        let sign_is_minus = match series.direction {
            Direction::Income => {
                if !from_included {
                    continue;
                }
                false
            }
            Direction::Expense => {
                if !from_included {
                    continue;
                }
                true
            }
            Direction::Transfer { to } => {
                let to_included = is_liquid_included(household, to);
                match (from_included, to_included) {
                    (true, true) => {
                        let sum = Money::sum(currency, expanded.iter().map(|o| o.remaining_expected()))?;
                        terms.push(
                            ProvNode::excluded(
                                format!("{} ({} transfers)", series.name, expanded.len()),
                                sum,
                                "transfer between included liquid accounts — net zero for the household (§15)",
                            )
                            .subject(ObjectRef::Series(series.id)),
                        );
                        continue;
                    }
                    (true, false) => true,
                    (false, true) => false,
                    (false, false) => continue,
                }
            }
        };

        let sum = Money::sum(currency, expanded.iter().map(|o| o.remaining_expected()))?;
        let count = expanded.len();
        let amounts_equal = expanded.iter().all(|o| o.remaining_expected() == expanded[0].remaining_expected());
        let label = if count == 1 {
            format!("{} ({}, {})", series.name, expanded[0].due.format("%d %b"), series.certainty.label().to_lowercase())
        } else if amounts_equal {
            format!(
                "{} ({count} × {}, {})",
                series.name,
                expanded[0].remaining_expected().format(),
                series.certainty.label().to_lowercase()
            )
        } else {
            format!("{} ({count} occurrences, {})", series.name, series.certainty.label().to_lowercase())
        };
        let class = if series.scenario.is_some() || series.certainty == Certainty::ScenarioOnly {
            MoneyClass::ConditionalFuture
        } else {
            MoneyClass::ExpectedFuture
        };
        let mut node = ProvNode::input(label, sum, series.recurrence.describe())
            .money_class(class)
            .certainty(series.certainty)
            .subject(ObjectRef::Series(series.id));
        let low = expanded[0].amount.low();
        let high = expanded[0].amount.high();
        if low != high {
            node = node.note(format!("each occurrence allowed in {}–{} (§10.4); the expected value is used here", low.format(), high.format()));
        }
        if let Some(scenario_id) = series.scenario
            && let Some(sc) = household.scenario(scenario_id)
        {
            node = node.note(format!("only inside scenario “{}” (§18)", sc.name));
        }
        if let Direction::Transfer { to } = series.direction
            && let Some(target) = household.account(to)
        {
            node = node.note(format!("transfer to {} — leaves liquid cash but not net worth (§15.2)", target.name));
        }
        if sign_is_minus {
            total = total.checked_sub(sum)?;
            node = node.minus();
        } else {
            total = total.checked_add(sum)?;
        }
        if expanded.iter().any(|o| o.fulfilled.is_positive()) {
            let received = Money::sum(currency, expanded.iter().map(|o| o.fulfilled))?;
            node = node.note(format!("{} already received and reconciled; only the remainder is expected (§16, V012)", received.format()));
        }
        for occurrence in &expanded {
            let amount = occurrence.remaining_expected();
            signed_postings.push((occurrence.due, if sign_is_minus { amount.negated() } else { amount }));
        }
        terms.push(node);
        included_series.push(series.id);
        occurrences.extend(expanded);
    }

    for (company, count, sum) in company_counts {
        terms.push(
            ProvNode::excluded(
                format!("{} cash flows ({count} occurrences)", household.entity_name(EntityRef::Company(company))),
                sum,
                "business boundary: company money is not household money (§8.5); see the company view",
            )
            .subject(ObjectRef::Company(company)),
        );
    }

    occurrences.sort_by_key(|o| (o.due, o.series));
    signed_postings.sort_by_key(|(date, _)| *date);
    let mut path = vec![PathPoint { date: household.as_of, balance: liquidity.liquid_cash.money() }];
    let mut running = liquidity.liquid_cash.money();
    for (date, amount) in signed_postings {
        running = running.checked_add(amount)?;
        match path.last_mut() {
            Some(last) if last.date == date => last.balance = running,
            _ => path.push(PathPoint { date, balance: running }),
        }
    }

    let conditional = ProvNode::sum("Conditional projected cash", total, terms)
        .money_class(MoneyClass::ConditionalFuture)
        .strength(ResultStrength::ConditionalPath)
        .note(format!(
            "Expected values of every included occurrence from {} through {}; only the first term is money already received (§2.1, §2.4).",
            household.as_of.format("%d %b %Y"),
            through.format("%d %b %Y")
        ));
    let unreserved_total = total.checked_sub(liquidity.reserved.money())?;
    let unreserved = ProvNode::sum(
        "Conditional projected unreserved cash",
        unreserved_total,
        vec![conditional.clone(), liquidity.reserved.node().clone().minus()],
    )
    .money_class(MoneyClass::ConditionalFuture)
    .strength(ResultStrength::ConditionalPath)
    .note("An earmark reduces unreserved cash, not the bank balance; paying the obligation later reduces cash and releases the earmark (§2.1).");

    let assumptions = household
        .assumptions
        .iter()
        .filter(|a| {
            let scenario_ok = match &a.source {
                crate::model::AssumptionSource::Scenario(id) => Some(*id) == scenario,
                _ => true,
            };
            scenario_ok && (a.applies_to.is_empty() || a.applies_to.iter().any(|s| included_series.contains(s)))
        })
        .cloned()
        .collect();

    Ok(Projection {
        as_of: household.as_of,
        through,
        scenario,
        conditional_cash: Calc::new(total, conditional),
        unreserved_cash: Calc::new(unreserved_total, unreserved),
        occurrences,
        path,
        assumptions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{self, ids, pkr};

    fn horizon() -> NaiveDate {
        NaiveDate::from_ymd_opt(2027, 1, 31).unwrap()
    }

    #[test]
    fn baseline_projection_adds_up_and_keeps_current_money_separate() {
        let household = fixtures::plan_household();
        let projection = household_projection(&household, horizon(), None).unwrap();
        assert!(projection.conditional_cash.node().verify_sums().is_empty());
        assert!(projection.unreserved_cash.node().verify_sums().is_empty());

        // Salary A: Sep 30, Oct 31, Nov 30 → 3 × 500,000; salary B: 5 × 300,000 (Sep–Jan);
        // freelance 300,000; receivable 350,000 of which 150,000 already received → 200,000 (V012);
        // rent Oct–Jan 4 × 180,000; other Sep 15–Jan 15 5 × 130,000; school fees Dec 5 250,000;
        // Visa settlement 85,000 out of liquid cash. Card purchases hit the card, not liquid cash (V009);
        // the Sep 5 insurance premium is before as_of and fulfilled.
        let expected = 4_400_000 + 1_500_000 + 1_500_000 + 300_000 + 200_000 - 720_000 - 650_000 - 250_000 - 85_000;
        assert_eq!(projection.conditional_cash.money(), pkr(expected));
        assert_eq!(projection.unreserved_cash.money(), pkr(expected - 1_750_000));

        let text = projection.unreserved_cash.node().render_chain();
        assert!(text.starts_with("  Conditional projected cash"));
        assert!(text.contains("Current reconciled liquid cash"));
        assert!(text.contains("contractual)"));
        assert!(text.contains("(excluded) Company Alpha cash flows"));
        // The car down payment belongs to the Buy Car scenario and is absent from the baseline.
        assert!(!text.contains("Car down payment"));
        assert!(text.contains("150,000 already received"));
        assert!(!text.contains("Card purchases"), "card spending does not touch liquid cash (V009)");
        assert!(projection.occurrences.windows(2).all(|w| w[0].due <= w[1].due));
        // The path starts at today's liquid cash and ends at the conditional total.
        assert_eq!(projection.path.first().map(|p| p.balance), Some(pkr(4_400_000)));
        assert_eq!(projection.path.last().map(|p| p.balance), Some(projection.conditional_cash.money()));
        assert!(projection.path.windows(2).all(|w| w[0].date < w[1].date));
    }

    #[test]
    fn v001_a_transfer_between_included_accounts_conserves_household_cash() {
        use crate::timeline::{AmountSpec, DateSpec, Direction, EventSeries, Recurrence};
        let mut household = fixtures::plan_household();
        let before = household_projection(&household, horizon(), None).unwrap();
        household.series.push(EventSeries {
            id: SeriesId::new(99),
            name: "Move savings".into(),
            direction: Direction::Transfer { to: ids::SHARED_SAVINGS },
            amount: AmountSpec::Exact(pkr(500_000)),
            amount_changes: Vec::new(),
            exceptions: Vec::new(),
            recurrence: Recurrence::OneTime { on: DateSpec::Exact(NaiveDate::from_ymd_opt(2026, 10, 3).unwrap()) },
            settlement_lag_days: 0,
            availability_lag_days: 0,
            intraday_order: 10,
            account: ids::PERSON_B_CHECKING,
            linked_account: None,
            entity: EntityRef::Person(ids::PERSON_B),
            certainty: Certainty::Confirmed,
            category: "Transfer".into(),
            tax_treatment: String::new(),
            scenario: None,
            notes: String::new(),
        });
        let after = household_projection(&household, horizon(), None).unwrap();
        assert_eq!(after.conditional_cash.money(), before.conditional_cash.money(), "checking → savings is net zero (§15, V001)");
        assert!(after.conditional_cash.node().render_chain().contains("(excluded) Move savings"));
    }

    #[test]
    fn scenario_overlay_adds_its_events_and_assumptions() {
        let household = fixtures::plan_household();
        let baseline = household_projection(&household, horizon(), None).unwrap();
        let buy_car = household_projection(&household, horizon(), Some(ids::BUY_CAR)).unwrap();
        assert_eq!(baseline.conditional_cash.money() - buy_car.conditional_cash.money(), pkr(2_500_000));
        let text = buy_car.conditional_cash.node().render_chain();
        assert!(text.contains("Car down payment"));
        assert!(text.contains("scenario-only"));
        // The private Leave Job assumption is not part of the Buy Car projection.
        assert!(buy_car.assumptions.iter().all(|a| a.private_to.is_none()));
        assert!(!baseline.assumptions.is_empty());
    }

    #[test]
    fn projection_for_person_b_hides_person_a_private_account() {
        let household = fixtures::plan_household();
        let projection = household_projection(&household, horizon(), None).unwrap();
        let viewer = crate::authz::Viewer::person(ids::PERSON_B);
        let projected = projection.conditional_cash.node().project(&household.disclosure_fn(viewer));
        assert!(projected.verify_sums().is_empty());
        let text = projected.render_chain();
        assert!(!text.contains("Person A current account"));
        assert!(text.contains("Owner-authorized restricted contribution"));
        // Person A sees everything.
        let owner = crate::authz::Viewer::person(ids::PERSON_A);
        let full = projection.conditional_cash.node().project(&household.disclosure_fn(owner));
        assert!(full.render_chain().contains("Person A current account"));
    }
}

// ----- per-boundary chronological forecast (§11, M01, M05, §10.6) ----------------------

use crate::breach::{BreachReport, analyse};
use crate::liquidity::{Boundary, hard_floor_for_account};
use crate::timeline::{AmountSpec, DateSpec, Recurrence};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// §10.6 — a named case is a coherent path, not a probability statement.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Case {
    /// Low income, late receipts, high expenses.
    Conservative,
    /// The user-selected expected values.
    Expected,
    /// High income, early receipts, low expenses.
    Optimistic,
}

impl Case {
    pub const ALL: [Case; 3] = [Case::Conservative, Case::Expected, Case::Optimistic];

    pub fn label(self) -> &'static str {
        match self {
            Case::Conservative => "Conservative",
            Case::Expected => "Expected",
            Case::Optimistic => "Optimistic",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Case::Conservative => "low income, late receipts, high expenses",
            Case::Expected => "the user-selected expected values",
            Case::Optimistic => "high income, early receipts, low expenses",
        }
    }

    /// The amount of one occurrence under this case.
    pub fn amount(self, spec: &AmountSpec, direction: Direction) -> Money {
        match (self, direction) {
            (Case::Expected, _) => spec.expected(),
            (Case::Conservative, Direction::Income) | (Case::Optimistic, Direction::Expense) => spec.low(),
            (Case::Conservative, Direction::Expense) | (Case::Optimistic, Direction::Income) => spec.high(),
            (_, Direction::Transfer { .. }) => spec.expected(),
        }
    }

    /// The date of a one-time occurrence with a date range under this case.
    pub fn date(self, spec: &DateSpec, direction: Direction) -> NaiveDate {
        match (spec, self, direction) {
            (DateSpec::Exact(d), _, _) => *d,
            (_, Case::Expected, _) => spec.expected(),
            (DateSpec::Range { latest, .. }, Case::Conservative, Direction::Income) => *latest,
            (DateSpec::Range { earliest, .. }, Case::Conservative, _) => *earliest,
            (DateSpec::Range { earliest, .. }, Case::Optimistic, Direction::Income) => *earliest,
            (DateSpec::Range { latest, .. }, Case::Optimistic, _) => *latest,
        }
    }
}

/// One signed cash posting on an account.
#[derive(Clone, Debug, PartialEq)]
pub struct Posting {
    pub date: NaiveDate,
    pub intraday_order: u16,
    pub account: AccountId,
    pub amount: Money,
    pub label: String,
    /// The series behind the posting (the base series for a tax posting).
    pub series: SeriesId,
    pub certainty: Certainty,
    /// Set for tax postings (§12.3): the rule that produced it.
    pub tax_rule: Option<TaxRuleId>,
    /// Set for fee postings from user rules (§14.4).
    pub fee_rule: Option<RuleId>,
}

/// The dated path of one account.
#[derive(Clone, Debug)]
pub struct AccountPath {
    pub account: AccountId,
    /// The boundary's share of the account (joint accounts split, §7).
    pub share_basis_points: u32,
    pub start: Money,
    pub end: Money,
    /// One point per posting (intraday granularity, V010), starting at `as_of`.
    pub points: Vec<PathPoint>,
    pub postings: Vec<Posting>,
    pub lowest: Money,
    pub lowest_date: Option<NaiveDate>,
    /// Hard earmarks and uncovered bank minimum on the account.
    pub floor: Money,
    pub breach: BreachReport,
    /// First date the balance itself goes below zero, if any (§11.3).
    pub negative_from: Option<NaiveDate>,
}

/// §11.3 — a date on which an account needs cash from elsewhere in the boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct TransferPoint {
    pub date: NaiveDate,
    pub account: AccountId,
    pub shortfall: Money,
    /// Whether other accounts of the boundary had at least that much headroom on the date.
    pub coverable: bool,
}

/// §11.4 / §24 — what a forecast was computed from, enough to reproduce it.
#[derive(Clone, Debug)]
pub struct ForecastRecord {
    pub algorithm: &'static str,
    pub as_of: NaiveDate,
    pub through: NaiveDate,
    pub boundary: Boundary,
    pub case: Case,
    pub scenario: Option<ScenarioId>,
    pub starting_snapshot: Vec<(AccountId, Money)>,
    pub included_series: Vec<SeriesId>,
    pub excluded_accounts: Vec<(AccountId, String)>,
    pub rules_applied: Vec<String>,
    pub tax_rule_packs: Vec<String>,
    pub assumptions: Vec<AssumptionId>,
    pub policy_versions: Vec<(PolicyId, u32)>,
    /// Deterministic hash of every input above (§24 "input hash").
    pub input_hash: u64,
}

/// A complete forecast for one boundary under one case.
#[derive(Clone, Debug)]
pub struct BoundaryForecast {
    pub boundary: Boundary,
    pub case: Case,
    pub scenario: Option<ScenarioId>,
    pub as_of: NaiveDate,
    pub through: NaiveDate,
    pub start: Calc<Money>,
    pub end: Calc<Money>,
    pub lowest: Calc<Money>,
    pub lowest_date: Option<NaiveDate>,
    /// The boundary path: sum of the account paths, one point per posting.
    pub path: Vec<PathPoint>,
    pub floor: Money,
    pub breach: BreachReport,
    pub accounts: Vec<AccountPath>,
    pub transfer_points: Vec<TransferPoint>,
    pub assumptions: Vec<Assumption>,
    pub record: ForecastRecord,
}

/// Options for [`forecast`].
#[derive(Clone, Copy, Debug)]
pub struct ForecastOptions {
    pub through: NaiveDate,
    pub scenario: Option<ScenarioId>,
    pub case: Case,
}

/// The accounts whose cash a boundary's path is made of, with the share applied.
fn boundary_accounts(household: &Household, boundary: Boundary) -> Vec<(AccountId, u32)> {
    match boundary {
        Boundary::Household => household
            .accounts
            .iter()
            .filter(|a| household_exclusion_reason(household, a).is_none() && a.kind.is_cash() && a.liquidity == Liquidity::Immediate)
            .map(|a| (a.id, 10_000))
            .collect(),
        Boundary::Person(person) => household
            .accounts_of(person)
            .filter(|a| a.kind.is_cash() && a.liquidity == Liquidity::Immediate && a.currency == household.base_currency)
            .map(|a| (a.id, a.holder.share_of(person)))
            .collect(),
        Boundary::Company(company) => household.company_accounts(company).filter(|a| a.kind.is_cash()).map(|a| (a.id, 10_000)).collect(),
        Boundary::Account(id) => vec![(id, 10_000)],
    }
}

/// Every posting a series makes in the window under a case, on its own
/// account and — for linked movements (§8.4) and transfers (§15) — on the
/// other side. Inflows post at their availability date (M05: two-day
/// settlement cannot fund today's payment); outflows at their due date.
fn postings_for(household: &Household, series: &crate::timeline::EventSeries, options: ForecastOptions) -> EngineResult<Vec<Posting>> {
    let mut adjusted = series.clone();
    if let Recurrence::OneTime { on } = &series.recurrence {
        adjusted.recurrence = Recurrence::OneTime { on: DateSpec::Exact(options.case.date(on, series.direction)) };
    }
    let mut postings = Vec::new();
    for occurrence in household.expand_series(&adjusted, household.as_of, options.through) {
        if !occurrence.is_live() {
            continue;
        }
        let full = options.case.amount(&occurrence.amount, series.direction);
        let amount = full.checked_sub(occurrence.fulfilled)?.clamped_at_zero();
        if amount.is_zero() {
            continue;
        }
        let label = occurrence.label.clone();
        match series.direction {
            Direction::Income => {
                postings.push(Posting { date: occurrence.availability, intraday_order: occurrence.intraday_order, account: series.account, amount, label: label.clone(), series: series.id, certainty: series.certainty, tax_rule: None, fee_rule: None });
                if let Some(linked) = series.linked_account {
                    postings.push(Posting { date: occurrence.due, intraday_order: occurrence.intraday_order, account: linked, amount: amount.negated(), label: format!("{label} (paid)"), series: series.id, certainty: series.certainty, tax_rule: None, fee_rule: None });
                }
            }
            Direction::Expense => {
                postings.push(Posting { date: occurrence.due, intraday_order: occurrence.intraday_order, account: series.account, amount: amount.negated(), label: label.clone(), series: series.id, certainty: series.certainty, tax_rule: None, fee_rule: None });
                if let Some(linked) = series.linked_account {
                    postings.push(Posting { date: occurrence.availability, intraday_order: occurrence.intraday_order, account: linked, amount, label: format!("{label} (received)"), series: series.id, certainty: series.certainty, tax_rule: None, fee_rule: None });
                }
            }
            Direction::Transfer { to } => {
                postings.push(Posting { date: occurrence.due, intraday_order: occurrence.intraday_order, account: series.account, amount: amount.negated(), label: format!("{label} (out)"), series: series.id, certainty: series.certainty, tax_rule: None, fee_rule: None });
                postings.push(Posting { date: occurrence.availability, intraday_order: occurrence.intraday_order, account: to, amount, label: format!("{label} (in)"), series: series.id, certainty: series.certainty, tax_rule: None, fee_rule: None });
            }
        }
    }
    Ok(postings)
}

/// Runs the chronological forecast for a boundary (§11.1): reconciled
/// starting cash plus every signed posting through `through`, in date and
/// intraday order; taxes and fees enter once as postings; reservations
/// constrain spendability and never post.
pub fn forecast(household: &Household, boundary: Boundary, options: ForecastOptions) -> EngineResult<BoundaryForecast> {
    // §18 — a scenario with explicit changes or members runs as an overlay:
    // the overlaid household carries the applied changes, tagged to the scenario.
    if let Some(id) = options.scenario
        && household.scenario(id).is_some_and(|s| s.has_overlay())
    {
        let overlaid = household.apply_scenarios(&[id])?;
        return forecast(&overlaid, boundary, options);
    }
    let currency = household.base_currency;
    let members = boundary_accounts(household, boundary);
    let member_ids: Vec<AccountId> = members.iter().map(|(id, _)| *id).collect();

    // 1. Collect postings on member accounts.
    let mut postings: Vec<Posting> = Vec::new();
    let mut included_series = Vec::new();
    for series in &household.series {
        if series.scenario.is_some() && series.scenario != options.scenario {
            continue;
        }
        let mine: Vec<Posting> = postings_for(household, series, options)?.into_iter().filter(|p| member_ids.contains(&p.account)).collect();
        if !mine.is_empty() {
            included_series.push(series.id);
            postings.extend(mine);
        }
    }
    // 1b. Tax postings (§12.3): each tax enters cash once, on its cash date,
    // right after the base transaction of the same day (intraday order + 1).
    let tax_postings = crate::tax::cash_postings(household, options.through, options.scenario, options.case)?;
    let mut tax_rules_applied: Vec<String> = Vec::new();
    for tax in tax_postings.into_iter().filter(|t| member_ids.contains(&t.account)) {
        let base_order = tax
            .base_series
            .and_then(|id| household.series_by_id(id))
            .map(|s| s.intraday_order + 1)
            .unwrap_or(15);
        if !tax_rules_applied.contains(&tax.label) {
            tax_rules_applied.push(tax.label.clone());
        }
        postings.push(Posting {
            date: tax.date,
            intraday_order: base_order,
            account: tax.account,
            amount: tax.amount,
            label: tax.label,
            series: tax.base_series.unwrap_or(SeriesId::new(0)),
            certainty: Certainty::Contractual,
            tax_rule: Some(tax.rule),
            fee_rule: None,
        });
    }
    // 1c. Fee postings from user rules (§14.4): each fee enters cash once.
    let mut rules_applied: Vec<String> = Vec::new();
    for fee in crate::rules::fee_postings(household, options.through, options.scenario, options.case)?.into_iter().filter(|f| member_ids.contains(&f.account)) {
        if !rules_applied.contains(&fee.label) {
            rules_applied.push(fee.label.clone());
        }
        postings.push(Posting {
            date: fee.date,
            intraday_order: fee.intraday_order,
            account: fee.account,
            amount: fee.amount,
            label: fee.label,
            series: fee.base_series,
            certainty: Certainty::Contractual,
            tax_rule: None,
            fee_rule: Some(fee.rule),
        });
    }
    postings.sort_by_key(|p| (p.date, p.intraday_order, p.series, p.account));

    // 2. Per-account paths with intraday granularity.
    let mut accounts = Vec::new();
    for (id, share) in &members {
        let account = household.account(*id).ok_or(EngineError::UnknownAccount(*id))?;
        let start = account.settled_balance.share_basis_points(*share);
        let mut points = vec![PathPoint { date: household.as_of, balance: start }];
        let mut running = start;
        let mut lowest = start;
        let mut lowest_date = None;
        let mut negative_from = None;
        let mut own_postings = Vec::new();
        for posting in postings.iter().filter(|p| p.account == *id) {
            let amount = posting.amount.share_basis_points(*share);
            running = running.checked_add(amount)?;
            points.push(PathPoint { date: posting.date, balance: running });
            if running.minor() < lowest.minor() {
                lowest = running;
                lowest_date = Some(posting.date);
            }
            if running.is_negative() && negative_from.is_none() {
                negative_from = Some(posting.date);
            }
            own_postings.push(Posting { amount, ..posting.clone() });
        }
        let floor = hard_floor_for_account(household, *id)?.share_basis_points(*share);
        let breach = analyse(&points, floor, options.through)?;
        accounts.push(AccountPath { account: *id, share_basis_points: *share, start, end: running, points, postings: own_postings, lowest, lowest_date, floor, breach, negative_from });
    }

    // 3. Boundary path: sum of member balances at every posting instant.
    let start_total = Money::sum(currency, accounts.iter().map(|a| a.start))?;
    let mut path = vec![PathPoint { date: household.as_of, balance: start_total }];
    let mut running = start_total;
    let mut lowest = start_total;
    let mut lowest_date = None;
    for posting in &postings {
        let share = members.iter().find(|(id, _)| *id == posting.account).map(|(_, s)| *s).unwrap_or(10_000);
        running = running.checked_add(posting.amount.share_basis_points(share))?;
        path.push(PathPoint { date: posting.date, balance: running });
        if running.minor() < lowest.minor() {
            lowest = running;
            lowest_date = Some(posting.date);
        }
    }
    let floor = Money::sum(currency, accounts.iter().map(|a| a.floor))?;
    let breach = analyse(&path, floor, options.through)?;

    // 4. Transfer points: an account below its floor while the rest of the boundary has headroom.
    let mut transfer_points = Vec::new();
    for account in &accounts {
        for point in account.points.iter().skip(1) {
            let shortfall = point.balance.shortfall_below(account.floor)?;
            if !shortfall.is_positive() {
                continue;
            }
            if transfer_points.iter().any(|t: &TransferPoint| t.account == account.account && t.date == point.date) {
                continue;
            }
            // Headroom of the other accounts at the end of that date.
            let mut others = Money::zero(currency);
            for other in accounts.iter().filter(|o| o.account != account.account) {
                let balance = other.points.iter().filter(|p| p.date <= point.date).last().map(|p| p.balance).unwrap_or(other.start);
                others = others.checked_add(balance.checked_sub(other.floor)?.clamped_at_zero())?;
            }
            transfer_points.push(TransferPoint { date: point.date, account: account.account, shortfall, coverable: others.minor() >= shortfall.minor() });
        }
    }

    // 5. The §2.1 chain for the boundary, one term per series.
    let mut terms = vec![
        ProvNode::sum(
            "Reconciled starting cash",
            start_total,
            accounts
                .iter()
                .filter_map(|a| household.account(a.account).map(|acc| (a, acc)))
                .map(|(a, acc)| ProvNode::input(acc.name.clone(), a.start, "settled balance at as_of").money_class(MoneyClass::ConfirmedCurrent).certainty(Certainty::Confirmed).subject(ObjectRef::Account(acc.id)))
                .collect(),
        )
        .money_class(MoneyClass::ConfirmedCurrent)
        .certainty(Certainty::Confirmed)
        .note("The only term already received (§2.4)."),
    ];
    for series_id in &included_series {
        let Some(series) = household.series_by_id(*series_id) else { continue };
        let mut sum = Money::zero(currency);
        let mut count = 0usize;
        for posting in postings.iter().filter(|p| p.series == *series_id && p.tax_rule.is_none() && p.fee_rule.is_none()) {
            let share = members.iter().find(|(id, _)| *id == posting.account).map(|(_, s)| *s).unwrap_or(10_000);
            sum = sum.checked_add(posting.amount.share_basis_points(share))?;
            count += 1;
        }
        if sum.is_zero() {
            terms.push(ProvNode::excluded(format!("{} ({count} postings)", series.name), Money::zero(currency), "nets to zero inside this boundary (§15)").subject(ObjectRef::Series(*series_id)));
            continue;
        }
        let class = if series.scenario.is_some() { MoneyClass::ConditionalFuture } else { MoneyClass::ExpectedFuture };
        let mut node = ProvNode::input(
            format!("{} ({count} posting{}, {})", series.name, if count == 1 { "" } else { "s" }, series.certainty.label().to_lowercase()),
            sum.abs(),
            format!("{} · {} case", series.recurrence.describe(), options.case.label().to_lowercase()),
        )
        .money_class(class)
        .certainty(series.certainty)
        .subject(ObjectRef::Series(*series_id));
        if sum.is_negative() {
            node = node.minus();
        }
        terms.push(node);
    }
    let tax_total = {
        let mut total = Money::zero(currency);
        for posting in postings.iter().filter(|p| p.tax_rule.is_some()) {
            let share = members.iter().find(|(id, _)| *id == posting.account).map(|(_, s)| *s).unwrap_or(10_000);
            total = total.checked_add(posting.amount.share_basis_points(share))?;
        }
        total
    };
    if !tax_total.is_zero() {
        let count = postings.iter().filter(|p| p.tax_rule.is_some()).count();
        terms.push(
            ProvNode::input(
                format!("Taxes withheld or paid inside the window ({count} postings, contractual)"),
                tax_total.abs(),
                tax_rules_applied.join(" · "),
            )
            .money_class(MoneyClass::ExpectedFuture)
            .certainty(Certainty::Contractual)
            .note("Each tax enters cash once as a posting on its cash date (§12.3, M01); assessment balances payable after the horizon are a tax reserve, not a posting (§12.4).")
            .minus(),
        );
    }
    let fee_total = {
        let mut total = Money::zero(currency);
        for posting in postings.iter().filter(|p| p.fee_rule.is_some()) {
            let share = members.iter().find(|(id, _)| *id == posting.account).map(|(_, s)| *s).unwrap_or(10_000);
            total = total.checked_add(posting.amount.share_basis_points(share))?;
        }
        total
    };
    if !fee_total.is_zero() {
        let count = postings.iter().filter(|p| p.fee_rule.is_some()).count();
        terms.push(
            ProvNode::input(format!("Fees from user rules ({count} postings, contractual)"), fee_total.abs(), rules_applied.join(" · "))
                .money_class(MoneyClass::ExpectedFuture)
                .certainty(Certainty::Contractual)
                .note("Fee events added by enabled rules in force on each date; conflicts resolved by priority, then scope specificity (§14.4, §14.7).")
                .minus(),
        );
    }
    let end_node = ProvNode::sum(format!("Conditional projected cash — {} case", options.case.label().to_lowercase()), running, terms)
        .money_class(MoneyClass::ConditionalFuture)
        .strength(ResultStrength::ScenarioTested)
        .note(format!(
            "One named path ({}); scenario-tested, not a robust envelope and not a probability (§10.6, V032).",
            options.case.description()
        ));
    let lowest_node = ProvNode::formula(
        format!("Lowest projected cash — {} case", options.case.label().to_lowercase()),
        lowest,
        "min over every posting instant of the boundary balance (§11.3)",
        vec![end_node.clone()],
    )
    .money_class(MoneyClass::ConditionalFuture)
    .strength(ResultStrength::ScenarioTested)
    .note(match lowest_date {
        Some(date) => format!("Reached on {} after that day's postings in intraday order (§11.1).", date.format("%d %b %Y")),
        None => "No posting lowered the balance below its start.".to_string(),
    });
    let start_node = ProvNode::sum("Reconciled starting cash", start_total, Vec::new()).money_class(MoneyClass::ConfirmedCurrent).certainty(Certainty::Confirmed);

    // 6. Assumptions and the provenance record.
    let assumptions: Vec<Assumption> = household
        .assumptions
        .iter()
        .filter(|a| match &a.source {
            crate::model::AssumptionSource::Scenario(id) => Some(*id) == options.scenario,
            _ => true,
        })
        .filter(|a| a.applies_to.is_empty() || a.applies_to.iter().any(|s| included_series.contains(s)))
        .cloned()
        .collect();
    let excluded_accounts: Vec<(AccountId, String)> = household
        .accounts
        .iter()
        .filter(|a| !member_ids.contains(&a.id))
        .map(|a| (a.id, household_exclusion_reason(household, a).unwrap_or_else(|| "outside this boundary".into())))
        .collect();
    let policy_versions: Vec<(PolicyId, u32)> = household.policies.iter().map(|p| (p.id, p.version)).collect();
    let mut hasher = DefaultHasher::new();
    for account in &accounts {
        account.account.hash(&mut hasher);
        account.start.minor().hash(&mut hasher);
    }
    for posting in &postings {
        posting.date.hash(&mut hasher);
        posting.intraday_order.hash(&mut hasher);
        posting.account.hash(&mut hasher);
        posting.amount.minor().hash(&mut hasher);
        posting.series.hash(&mut hasher);
    }
    options.through.hash(&mut hasher);
    options.case.hash(&mut hasher);
    options.scenario.hash(&mut hasher);
    for (id, version) in &policy_versions {
        id.hash(&mut hasher);
        version.hash(&mut hasher);
    }
    let record = ForecastRecord {
        algorithm: "atlas-core forecast/1 (chronological postings, intraday order, availability-dated inflows)",
        as_of: household.as_of,
        through: options.through,
        boundary,
        case: options.case,
        scenario: options.scenario,
        starting_snapshot: accounts.iter().map(|a| (a.account, a.start)).collect(),
        included_series: included_series.clone(),
        excluded_accounts,
        rules_applied: {
            let mut rules: Vec<String> = rules_applied.iter().map(|r| format!("user rule: {r}")).collect();
            if rules.is_empty() {
                rules.push("no user rule produced a posting in this window".into());
            }
            rules.extend(tax_rules_applied.iter().map(|r| format!("tax posting: {r}")));
            rules
        },
        tax_rule_packs: household.tax_packs.iter().map(|p| format!("{} ({}, {})", p.name, p.version, if p.verified { "verified" } else { "unverified" })).collect(),
        assumptions: assumptions.iter().map(|a| a.id).collect(),
        policy_versions,
        input_hash: hasher.finish(),
    };

    log::debug!("forecast {:?} {:?}: start {} end {} lowest {} hash {:016x}", boundary, options.case, start_total.format(), running.format(), lowest.format(), record.input_hash);

    Ok(BoundaryForecast {
        boundary,
        case: options.case,
        scenario: options.scenario,
        as_of: household.as_of,
        through: options.through,
        start: Calc::new(start_total, start_node),
        end: Calc::new(running, end_node),
        lowest: Calc::new(lowest, lowest_node),
        lowest_date,
        path,
        floor,
        breach,
        accounts,
        transfer_points,
        assumptions,
        record,
    })
}

#[cfg(test)]
mod forecast_tests {
    use super::*;
    use crate::fixtures::{self, ids, pkr};
    use crate::timeline::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    use crate::fixtures::e03_household;

    #[test]
    fn e03_conservative_case_reproduces_the_plan_table() {
        let cases = [
            (1_200_000, d(2026, 11, 15), 1_080_000, d(2026, 11, 15)),
            (1_300_000, d(2026, 11, 15), 980_000, d(2026, 11, 15)),
            (1_400_000, d(2026, 11, 15), 880_000, d(2026, 11, 15)),
            (1_200_000, d(2026, 11, 30), 1_180_000, d(2027, 1, 15)),
            (1_300_000, d(2026, 11, 30), 1_080_000, d(2027, 1, 15)),
            (1_400_000, d(2026, 11, 30), 980_000, d(2027, 1, 15)),
        ];
        for (down, on, lowest, lowest_on) in cases {
            let household = e03_household(pkr(down), on);
            let result = forecast(&household, Boundary::Household, ForecastOptions { through: d(2027, 1, 31), scenario: None, case: Case::Conservative }).unwrap();
            assert_eq!(result.lowest.money(), pkr(lowest), "down payment {down} on {on}");
            assert_eq!(result.lowest_date, Some(lowest_on), "down payment {down} on {on}");
            assert_eq!(result.floor, pkr(1_000_000));
            let breached = result.breach.first_breach.is_some();
            assert_eq!(breached, lowest < 1_000_000, "reserve condition for {down} on {on}");
            if breached {
                assert_eq!(result.breach.worst_deficit, pkr(1_000_000 - lowest));
            }
            assert!(result.end.node().verify_sums().is_empty());
            assert_eq!(result.end.node().result_strength(), ResultStrength::ScenarioTested, "V032: a named case is scenario-tested, never robust");
        }
    }

    #[test]
    fn v010_intraday_rent_before_salary_shows_the_dip() {
        let mut household = e03_household(pkr(0), d(2026, 12, 1));
        household.series.retain(|s| s.id.raw() <= 2);
        household.accounts[0].settled_balance = pkr(100_000);
        // Rent (order 10) and salary (order 20) both on 31 Oct: move rent onto the salary date.
        household.series[1].recurrence = Recurrence::LastDayOfMonth { every_n_months: 1, from: d(2026, 10, 1), until: Until::Date(d(2026, 10, 31)) };
        let result = forecast(&household, Boundary::Account(AccountId::new(1)), ForecastOptions { through: d(2026, 10, 31), scenario: None, case: Case::Expected }).unwrap();
        let account = &result.accounts[0];
        // 100,000 + 500,000 (30 Sep) = 600,000; 31 Oct: −190,000 → 410,000 then +500,000 → 910,000.
        assert_eq!(account.end, pkr(910_000));
        assert_eq!(account.lowest, pkr(100_000));
        // Now make the dip real: no September salary.
        household.series[0].recurrence = Recurrence::LastDayOfMonth { every_n_months: 1, from: d(2026, 10, 1), until: Until::Date(d(2026, 10, 31)) };
        let result = forecast(&household, Boundary::Account(AccountId::new(1)), ForecastOptions { through: d(2026, 10, 31), scenario: None, case: Case::Expected }).unwrap();
        let account = &result.accounts[0];
        assert_eq!(account.lowest, pkr(-90_000), "the 09:00 rent debit overdraws before the 17:00 salary arrives (V010)");
        assert_eq!(account.negative_from, Some(d(2026, 10, 31)));
        assert_eq!(account.end, pkr(410_000));
    }

    #[test]
    fn v003_expected_income_never_changes_current_money() {
        let household = fixtures::plan_household();
        let today = crate::liquidity::household_liquidity(&household).unwrap();
        let result = forecast(&household, Boundary::Household, ForecastOptions { through: fixtures::default_horizon(), scenario: None, case: Case::Optimistic }).unwrap();
        assert_eq!(result.start.money(), today.liquid_cash.money(), "the optimistic path starts from the same reconciled cash");
        assert_eq!(result.path[0].balance, today.liquid_cash.money());
        assert!(result.end.money().minor() > result.start.money().minor());
        assert_eq!(result.start.node().money_class_label(), Some(MoneyClass::ConfirmedCurrent));
        assert_eq!(result.end.node().money_class_label(), Some(MoneyClass::ConditionalFuture));
    }

    #[test]
    fn cases_are_ordered_and_linked_movements_post_on_both_sides() {
        let household = fixtures::plan_household();
        let run = |case| forecast(&household, Boundary::Household, ForecastOptions { through: fixtures::default_horizon(), scenario: None, case }).unwrap();
        let (c, e, o) = (run(Case::Conservative), run(Case::Expected), run(Case::Optimistic));
        assert!(c.end.money().minor() <= e.end.money().minor() && e.end.money().minor() <= o.end.money().minor());
        assert!(c.lowest.money().minor() <= e.lowest.money().minor());
        // Company Alpha: the owner salary leaves the payroll account as one linked movement (§8.4).
        let alpha = forecast(&household, Boundary::Company(ids::ALPHA), ForecastOptions { through: d(2026, 10, 15), scenario: None, case: Case::Expected }).unwrap();
        let payroll = alpha.accounts.iter().find(|a| a.account == ids::ALPHA_PAYROLL).unwrap();
        assert!(payroll.postings.iter().any(|p| p.label.contains("Person A salary") && p.amount == pkr(-500_000)));
        assert!(alpha.record.input_hash != e.record.input_hash);
        // Reproducible: the same inputs give the same hash.
        assert_eq!(run(Case::Expected).record.input_hash, e.record.input_hash);
    }

    #[test]
    fn transfer_points_name_the_account_that_needs_cash() {
        let mut household = fixtures::plan_household();
        // Drain Person B's checking so its ordinary expenses breach a hard floor.
        household.accounts.iter_mut().find(|a| a.id == ids::PERSON_B_CHECKING).unwrap().minimum_balance = Some(pkr(800_000));
        let result = forecast(&household, Boundary::Household, ForecastOptions { through: d(2026, 10, 31), scenario: None, case: Case::Expected }).unwrap();
        let point = result.transfer_points.iter().find(|t| t.account == ids::PERSON_B_CHECKING).expect("a transfer point");
        assert_eq!(point.date, d(2026, 9, 15));
        assert_eq!(point.shortfall, pkr(30_000));
        assert!(point.coverable, "shared savings has headroom on that date");
    }
}
