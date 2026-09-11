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
use crate::EngineResult;
use crate::breach::PathPoint;
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
