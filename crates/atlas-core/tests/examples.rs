//! The plan's worked examples E01–E08 as regression tests over the public API
//! (M11.1). Each test names the models it exercises so a change to one of them
//! points back to the example it must keep reproducing. The numbers are the
//! plan's own; nothing here is a real jurisdiction's law or a real household.

use atlas_core::breach::{PathPoint, analyse};
use atlas_core::decision::{FundingSource, Objective, PurchasePlan, annuity_payment_exact, evaluate, gross_up, irr_roots, loan_schedule};
use atlas_core::fixtures::{self, e03_household, ids, pkr};
use atlas_core::forecast::{Case, ForecastOptions, forecast};
use atlas_core::ids::*;
use atlas_core::liquidity::{Boundary, account_liquidity, company_cash};
use atlas_core::model::Bracket;
use atlas_core::risk::{Outcome, conditional_value_at_risk, expected_loss, value_at_risk};
use atlas_core::tax::multi_year_comparison;
use atlas_core::{EngineResult, Money, ResultStrength};
use chrono::NaiveDate;

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

/// E01 — Reserving money is not spending it. Models: M01, M02, M24.
#[test]
fn e01_reserving_money_is_not_spending_it() {
    let mut household = fixtures::plan_household();
    let shared = ids::SHARED_SAVINGS;
    let before = account_liquidity(&household, shared).unwrap();
    assert_eq!(before.ledger_cash.money(), pkr(2_000_000));
    assert_eq!(before.reserved.money(), pkr(1_350_000), "800,000 + 300,000 + 250,000 disjoint");
    assert_eq!(before.free.money(), pkr(650_000));
    // Pay the 300,000 tax bill and release the earmark through the engine (returns the new balance).
    let balance = household.pay_and_release(ids::TAX_RESERVE, household.as_of).unwrap();
    assert_eq!(balance, pkr(1_700_000));
    let after = account_liquidity(&household, shared).unwrap();
    assert_eq!(after.ledger_cash.money(), pkr(1_700_000));
    assert_eq!(after.reserved.money(), pkr(1_050_000));
    assert_eq!(after.free.money(), pkr(650_000), "the paid liability never reduces spendability twice");
    assert!(after.free.node().verify_sums().is_empty());
}

/// E02 — Gross withdrawal must actually deliver the requested net amount. Models: M03, M25.
#[test]
fn e02_gross_withdrawal_delivers_the_requested_net() {
    // The plan's hypothetical strategy: 42,000 cash tax and 3,500 fees, assumed fixed.
    let cost = |_gross: Money| -> EngineResult<(Money, Money)> { Ok((pkr(42_000), pkr(3_500))) };
    let result = gross_up(pkr(1_000_000), &cost).unwrap();
    assert_eq!(result.gross, pkr(1_045_500));
    assert_eq!(result.net, pkr(1_000_000));
    // Withdrawing only 1,000,000 gross would deliver 954,500 net under these assumed costs.
    let (tax, fees) = cost(pkr(1_000_000)).unwrap();
    assert_eq!(pkr(1_000_000).checked_sub(tax).unwrap().checked_sub(fees).unwrap(), pkr(954_500));
    // In production the costs are recomputed on the chosen gross: a rate-based cost still verifies (V014).
    let proportional = |gross: Money| -> EngineResult<(Money, Money)> { Ok((Money::new((gross.minor() * 42_000 + 1_045_499) / 1_045_500, gross.currency()), pkr(3_500))) };
    let result = gross_up(pkr(1_000_000), &proportional).unwrap();
    assert!(result.net.minor() >= pkr(1_000_000).minor());
    assert_eq!(result.gross, pkr(1_045_500));
}

/// E03 — A dated, assumption-bound down-payment comparison. Models: M05, M13, M20 (conservative case).
#[test]
fn e03_down_payment_table_reproduces_under_the_conservative_case() {
    let cases = [
        (1_200_000, d(2026, 11, 15), 1_080_000, d(2026, 11, 15), 0),
        (1_300_000, d(2026, 11, 15), 980_000, d(2026, 11, 15), 20_000),
        (1_400_000, d(2026, 11, 15), 880_000, d(2026, 11, 15), 120_000),
        (1_200_000, d(2026, 11, 30), 1_180_000, d(2027, 1, 15), 0),
    ];
    for (down, on, lowest, lowest_on, breach) in cases {
        let household = e03_household(pkr(down), on);
        let result = forecast(&household, Boundary::Household, ForecastOptions { through: d(2027, 1, 31), scenario: None, case: Case::Conservative }).unwrap();
        assert_eq!(result.lowest.money(), pkr(lowest), "{down} on {on}");
        assert_eq!(result.lowest_date, Some(lowest_on));
        assert_eq!(result.breach.worst_deficit, pkr(breach));
        assert_eq!(result.end.node().result_strength(), ResultStrength::ScenarioTested, "V032: never a robust envelope");
    }
    // The decision builder's grid reproduces the same table from a plan.
    let mut household = e03_household(pkr(0), d(2026, 11, 15));
    household.series.retain(|s| s.name != "Down payment");
    let plan = PurchasePlan {
        name: "Car".into(),
        price: pkr(1_200_000),
        purchase_on: d(2026, 11, 15),
        window_from: d(2026, 11, 1),
        window_to: d(2026, 11, 30),
        down_payment: pkr(1_200_000),
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
    };
    let decision = evaluate(&household, &plan, d(2027, 1, 31)).unwrap();
    let cell = decision.grid.iter().find(|c| c.down_payment == pkr(1_300_000)).unwrap();
    assert_eq!(cell.lowest, pkr(980_000));
    assert_eq!(cell.shortfall, pkr(20_000));
}

/// E04 — Equal failure frequency can hide very different severity. Models: M10, M11.
#[test]
fn e04_cvar_on_the_tail_mass() {
    let outcomes = [
        Outcome { loss: pkr(0), probability_basis_points: 8_000 },
        Outcome { loss: pkr(100_000), probability_basis_points: 1_500 },
        Outcome { loss: pkr(500_000), probability_basis_points: 500 },
    ];
    assert_eq!(expected_loss(&outcomes).unwrap(), pkr(40_000));
    assert_eq!(value_at_risk(&outcomes, 9_000).unwrap(), pkr(100_000));
    assert_eq!(conditional_value_at_risk(&outcomes, 9_000).unwrap(), pkr(300_000), "not 500,000: the 100,000 atom is partly in the tail");
}

/// E05 — A multi-year tax comparison can beat the lowest-immediate-tax instinct. Models: M23, M26, M29.
#[test]
fn e05_splitting_an_extraction_reduces_incremental_tax() {
    let brackets = vec![
        Bracket { lower: pkr(0), upper: Some(pkr(100_000)), rate_basis_points: 1_000 },
        Bracket { lower: pkr(100_000), upper: None, rate_basis_points: 3_000 },
    ];
    let baseline = vec![pkr(60_000), pkr(60_000)];
    let strategies = vec![("Extract 100,000 in year 1".to_string(), vec![pkr(100_000), pkr(0)]), ("Extract 50,000 in each year".to_string(), vec![pkr(50_000), pkr(50_000)])];
    let (baseline_total, results) = multi_year_comparison(&brackets, &baseline, &strategies).unwrap();
    assert_eq!(baseline_total, pkr(12_000));
    assert_eq!(results[0].tax_by_year.iter().map(|c| c.money().minor()).collect::<Vec<_>>(), vec![pkr(28_000).minor(), pkr(6_000).minor()]);
    assert_eq!(results[0].incremental.money(), pkr(22_000));
    assert_eq!(results[1].incremental.money(), pkr(14_000));
    assert!(results[1].incremental.node().verify_sums().is_empty());
}

/// E06 — A lower monthly payment is not the same as cheaper borrowing. Models: M30–M32.
#[test]
fn e06_rounded_contract_replay_and_multiple_irr_roots() {
    let exact = annuity_payment_exact(pkr(1_000_000), 12, 100) / 100.0;
    assert!((exact - 88_848.78867834).abs() < 1e-6);
    let schedule = loan_schedule(pkr(1_000_000), 12, 100);
    assert_eq!(schedule[0].payment, Money::new(8_884_879, atlas_core::Currency::PKR));
    assert!(schedule.last().unwrap().balance_after.is_zero());
    assert_ne!(schedule.last().unwrap().payment, schedule[0].payment);
    let roots = irr_roots(&[-100.0, 230.0, -132.0]);
    assert_eq!(roots.len(), 2);
    assert!((roots[0] - 0.10).abs() < 1e-6 && (roots[1] - 0.20).abs() < 1e-6);
}

/// E07 — Company cash is not free household money. Models: M02, M04, M27, M41.
#[test]
fn e07_company_ceiling_is_not_distributable_and_extraction_nets_to_zero() {
    let household = fixtures::plan_household();
    let alpha = company_cash(&household, ids::ALPHA).unwrap();
    assert_eq!(alpha.cash.money(), pkr(2_350_000));
    assert_eq!(alpha.committed.money(), pkr(1_500_000), "payroll 700,000 + tax 200,000 + buffer 600,000");
    assert_eq!(alpha.ceiling.money(), pkr(850_000));
    assert_eq!(alpha.extractable.node().result_strength(), ResultStrength::Unresolved, "legal capacity is never inferred from cash");
    // Household cash excludes the company's; the owner salary moves cash between them without creating wealth.
    let options = ForecastOptions { through: d(2026, 10, 31), scenario: None, case: Case::Expected };
    let household_side = forecast(&household, Boundary::Household, options).unwrap();
    assert!(household_side.record.excluded_accounts.iter().any(|(id, why)| *id == ids::ALPHA_OPERATING && why.contains("business cash")));
    let company_side = forecast(&household, Boundary::Company(ids::ALPHA), options).unwrap();
    let salary_out: i64 = company_side.accounts.iter().flat_map(|a| a.postings.iter()).filter(|p| p.series == ids::SALARY_A && p.tax_rule.is_none()).map(|p| p.amount.minor()).sum();
    let salary_in: i64 = household_side.accounts.iter().flat_map(|a| a.postings.iter()).filter(|p| p.series == ids::SALARY_A && p.tax_rule.is_none()).map(|p| p.amount.minor()).sum();
    assert_eq!(salary_out + salary_in, 0, "the internal movement cancels across the combined boundary");
}

/// E08 — A funding gap is not multiplied by its duration. Models: M13, E08.
#[test]
fn e08_currency_days_measure_severity_not_capital() {
    let mut path: Vec<PathPoint> = (1..=10).map(|day| PathPoint { date: d(2026, 10, day), balance: pkr(900_000) }).collect();
    path.push(PathPoint { date: d(2026, 10, 11), balance: pkr(1_200_000) });
    let report = analyse(&path, pkr(1_000_000), d(2026, 10, 31)).unwrap();
    assert_eq!(report.minimum_injection.money(), pkr(100_000), "K*: one injection, not 10 × 100,000");
    assert_eq!(report.integrated_shortfall_currency_days, 1_000_000);
    assert_eq!(report.days_below, 10);
    assert_eq!(report.recovery, Some(d(2026, 10, 11)));
}
