//! Deterministic sensitivity and reverse stress (§10.8, M14): which single
//! assumption would have to fail for a path to breach its floor — with the
//! explicit caveat that separate one-at-a-time limits never guarantee joint
//! safety (V043).

use crate::forecast::{ForecastOptions, forecast};
use crate::ids::SeriesId;
use crate::liquidity::Boundary;
use crate::model::Household;
use crate::money::Money;
use crate::timeline::{AmountSpec, DateSpec, Direction, Recurrence};
use crate::vocab::ResultStrength;
use crate::EngineResult;
use chrono::{Days, NaiveDate};

/// One single-assumption breakpoint, everything else held at the case values.
#[derive(Clone, Debug, PartialEq)]
pub struct Breakpoint {
    pub series: SeriesId,
    pub label: String,
    /// What was varied and where the floor breaks, in the plan's wording.
    pub statement: String,
    /// The per-occurrence amount (or date) at which the breach starts, if it lies inside the declared range.
    pub breaking_amount: Option<Money>,
    pub breaking_date: Option<NaiveDate>,
    /// The declared range that was searched.
    pub searched: String,
    /// Whether the search found a breach inside the declared range.
    pub breaks_within_range: bool,
}

/// The §10.8 report.
#[derive(Clone, Debug)]
pub struct SensitivityReport {
    pub boundary: Boundary,
    pub through: NaiveDate,
    pub floor: Money,
    pub baseline_lowest: Money,
    pub baseline_breaches: bool,
    /// Headroom at the lowest point: the largest one-off unplanned expense the
    /// path absorbs on its worst day.
    pub unplanned_spending_limit: Money,
    pub breakpoints: Vec<Breakpoint>,
    pub coverage: ResultStrength,
    /// The V043 caveat, verbatim on every report.
    pub caveat: &'static str,
}

pub const JOINT_CAVEAT: &str = "These separate limits do NOT guarantee safety when several assumptions change at once; combinations and shared causes need a joint stress test.";

fn lowest_with(household: &Household, series: SeriesId, amount: Money, boundary: Boundary, options: ForecastOptions) -> EngineResult<Money> {
    let mut trial = household.clone();
    if let Some(s) = trial.series.iter_mut().find(|s| s.id == series) {
        s.amount = AmountSpec::Exact(amount);
        s.amount_changes.clear();
    }
    Ok(forecast(&trial, boundary, options)?.lowest.money())
}

fn lowest_with_date(household: &Household, series: SeriesId, on: NaiveDate, boundary: Boundary, options: ForecastOptions) -> EngineResult<Money> {
    let mut trial = household.clone();
    if let Some(s) = trial.series.iter_mut().find(|s| s.id == series) {
        s.recurrence = Recurrence::OneTime { on: DateSpec::Exact(on) };
    }
    Ok(forecast(&trial, boundary, options)?.lowest.money())
}

/// One-at-a-time breakpoints for every ranged series of the boundary, plus the
/// one-off spending limit. Bisection is valid because, with every other input
/// fixed, the lowest balance is monotone in a single amount or arrival date in
/// this additive cash model (E03's argument); rules that break monotonicity
/// (thresholds, cliffs) need enumeration instead and are labelled so when they
/// arrive with M6/M7.
pub fn one_at_a_time(household: &Household, boundary: Boundary, options: ForecastOptions) -> EngineResult<SensitivityReport> {
    let baseline = forecast(household, boundary, options)?;
    let floor = baseline.floor;
    let currency = household.base_currency;
    let baseline_lowest = baseline.lowest.money();
    let baseline_breaches = baseline_lowest.minor() < floor.minor();
    let headroom = baseline_lowest.checked_sub(floor)?.clamped_at_zero();
    let mut breakpoints = Vec::new();

    for series in &household.series {
        if !baseline.record.included_series.contains(&series.id) {
            continue;
        }
        let (low, high) = (series.amount.low(), series.amount.high());
        if low != high && low.currency() == currency {
            // Adverse direction: incomes fall, expenses rise.
            let (safe_end, adverse_end) = match series.direction {
                Direction::Income => (high, low),
                _ => (low, high),
            };
            let adverse_lowest = lowest_with(household, series.id, adverse_end, boundary, options)?;
            let safe_lowest = lowest_with(household, series.id, safe_end, boundary, options)?;
            let searched = format!("{}–{} per occurrence", low.format(), high.format());
            if safe_lowest.minor() < floor.minor() {
                breakpoints.push(Breakpoint {
                    series: series.id,
                    label: series.name.clone(),
                    statement: format!("{} breaches the floor at every value in its declared range, even its favourable extreme ({}); no single change of this amount restores the path.", series.name, safe_end.format()),
                    breaking_amount: None,
                    breaking_date: None,
                    searched,
                    breaks_within_range: true,
                });
                continue;
            }
            if adverse_lowest.minor() >= floor.minor() {
                breakpoints.push(Breakpoint {
                    series: series.id,
                    label: series.name.clone(),
                    statement: format!("{} at its {} extreme ({}) keeps the path above the floor; no single-amount breach inside the declared range.", series.name, if series.direction == Direction::Income { "low" } else { "high" }, adverse_end.format()),
                    breaking_amount: None,
                    breaking_date: None,
                    searched,
                    breaks_within_range: false,
                });
                continue;
            }
            // Bisect between the safe end and the adverse end for the first breaching amount.
            // Exact to the minor unit: `breaking` is the first breaching amount,
            // one minor unit before it the floor still holds.
            let mut safe = safe_end;
            let mut breaking = adverse_end;
            while (breaking.minor() - safe.minor()).abs() > 1 {
                let mid = Money::new((safe.minor() + breaking.minor()) / 2, currency);
                let lowest = lowest_with(household, series.id, mid, boundary, options)?;
                if lowest.minor() < floor.minor() {
                    breaking = mid;
                } else {
                    safe = mid;
                }
            }
            let statement = match series.direction {
                Direction::Income => format!("{} below {} per occurrence causes a floor breach.", series.name, breaking.format()),
                _ => format!("{} above {} per occurrence causes a floor breach.", series.name, breaking.format()),
            };
            breakpoints.push(Breakpoint { series: series.id, label: series.name.clone(), statement, breaking_amount: Some(breaking), breaking_date: None, searched, breaks_within_range: true });
        }
        if let Recurrence::OneTime { on: DateSpec::Range { earliest, latest, .. } } = &series.recurrence
            && series.direction == Direction::Income
        {
            let searched = format!("arrival {} – {}", earliest.format("%d %b %Y"), latest.format("%d %b %Y"));
            let at_latest = lowest_with_date(household, series.id, *latest, boundary, options)?;
            if at_latest.minor() >= floor.minor() {
                breakpoints.push(Breakpoint {
                    series: series.id,
                    label: series.name.clone(),
                    statement: format!("{} arriving as late as {} keeps the path above the floor.", series.name, latest.format("%d %b %Y")),
                    breaking_amount: None,
                    breaking_date: None,
                    searched,
                    breaks_within_range: false,
                });
            } else {
                let mut day = *earliest;
                let mut breaking = None;
                while day <= *latest {
                    if lowest_with_date(household, series.id, day, boundary, options)?.minor() < floor.minor() {
                        breaking = Some(day);
                        break;
                    }
                    day = day.checked_add_days(Days::new(1)).unwrap_or(*latest);
                }
                breakpoints.push(Breakpoint {
                    series: series.id,
                    label: series.name.clone(),
                    statement: match breaking {
                        Some(date) => format!("{} later than {} causes a floor breach.", series.name, date.pred_opt().unwrap_or(date).format("%d %b %Y")),
                        None => format!("{} breaches the floor at every arrival date in its range.", series.name),
                    },
                    breaking_amount: None,
                    breaking_date: breaking,
                    searched,
                    breaks_within_range: true,
                });
            }
        }
    }

    Ok(SensitivityReport {
        boundary,
        through: options.through,
        floor,
        baseline_lowest,
        baseline_breaches,
        unplanned_spending_limit: headroom,
        breakpoints,
        coverage: ResultStrength::ConditionalPath,
        caveat: JOINT_CAVEAT,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{self, ids, pkr};
    use crate::forecast::Case;

    fn options() -> ForecastOptions {
        ForecastOptions { through: fixtures::default_horizon(), scenario: Some(ids::BUY_CAR), case: Case::Expected }
    }

    #[test]
    fn breakpoints_are_labelled_one_at_a_time_with_the_joint_caveat() {
        let household = fixtures::plan_household();
        // Shared savings alone: the car down payment makes the account's path tight.
        let report = one_at_a_time(&household, Boundary::Account(ids::SHARED_SAVINGS), options()).unwrap();
        assert_eq!(report.caveat, JOINT_CAVEAT);
        assert!(report.caveat.contains("do NOT guarantee safety"), "the joint caveat is always present");
        assert_eq!(report.coverage, ResultStrength::ConditionalPath);
        assert!(!report.breakpoints.is_empty());
        for point in &report.breakpoints {
            assert!(!point.statement.is_empty());
            assert!(!point.searched.is_empty());
        }
    }

    #[test]
    fn a_ranged_expense_has_an_amount_breakpoint_when_headroom_is_thin() {
        let mut household = fixtures::plan_household();
        // Make the shared-savings path tight: raise its hard floor so rent's range straddles the breach.
        // Floor 660,000 + 300,000 = 960,000 against a baseline lowest of 1,030,000 (Jan):
        // four rent payments share the 70,000 headroom, so rent breaks at 197,500.
        household.reservations.iter_mut().find(|r| r.id == ids::EMERGENCY_RESERVE).unwrap().amount = pkr(660_000);
        let plain = ForecastOptions { through: fixtures::default_horizon(), scenario: None, case: Case::Expected };
        let baseline = forecast(&household, Boundary::Account(ids::SHARED_SAVINGS), plain).unwrap();
        let report = one_at_a_time(&household, Boundary::Account(ids::SHARED_SAVINGS), plain).unwrap();
        assert_eq!(report.baseline_lowest, baseline.lowest.money());
        assert!(!report.baseline_breaches);
        let rent = report.breakpoints.iter().find(|b| b.series == ids::RENT).expect("rent is ranged");
        assert!(rent.breaks_within_range);
        let breaking = rent.breaking_amount.unwrap();
        assert_eq!(breaking, Money::new(pkr(197_500).minor() + 1, breaking.currency()), "first breaching amount, exact to the minor unit");
        assert!(rent.statement.contains("above"));
        let below = lowest_with(&household, ids::RENT, Money::new(breaking.minor() - 1, breaking.currency()), Boundary::Account(ids::SHARED_SAVINGS), plain).unwrap();
        let at = lowest_with(&household, ids::RENT, breaking, Boundary::Account(ids::SHARED_SAVINGS), plain).unwrap();
        assert!(below.minor() >= report.floor.minor() && at.minor() < report.floor.minor());
        assert_eq!(report.unplanned_spending_limit, pkr(70_000));
    }

    #[test]
    fn a_late_receipt_has_a_date_breakpoint() {
        let household = fixtures::plan_household();
        let report = one_at_a_time(&household, Boundary::Account(ids::PERSON_A_CURRENT), ForecastOptions { through: fixtures::default_horizon(), scenario: None, case: Case::Expected }).unwrap();
        let receivable = report.breakpoints.iter().filter(|b| b.series == ids::CLIENT_RECEIVABLE).collect::<Vec<_>>();
        assert!(!receivable.is_empty(), "the receivable has a date range and yields a date breakpoint entry");
        assert!(receivable.iter().all(|b| b.searched.starts_with("arrival")));
    }
}
