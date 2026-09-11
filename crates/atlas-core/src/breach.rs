//! First-passage runway and minimum additional funding (M13, E08, V044, V045).
//!
//! For a dated balance path and a required floor `R`:
//!
//! ```text
//! τ  = inf { t ≤ H : b_t < R_t }          first breach
//! S_t = [R_t − b_t]₊                        shortfall on each date
//! K* = max_t S_t                            minimum immediate injection
//! ```
//!
//! The integrated shortfall `Σ S_t Δt` is in currency-days and measures
//! duration and severity; it is **not** a capital requirement (E08).

use crate::money::Money;
use crate::provenance::{Calc, ProvNode, ProvValue};
use crate::vocab::{MoneyClass, ResultStrength};
use crate::{EngineResult, MoneyError};
use chrono::NaiveDate;

/// One point of a balance path: the settled balance at the end of `date`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PathPoint {
    pub date: NaiveDate,
    pub balance: Money,
}

/// What a dated path does against a floor.
#[derive(Clone, Debug)]
pub struct BreachReport {
    pub floor: Money,
    pub horizon: NaiveDate,
    pub first_breach: Option<NaiveDate>,
    pub recovery: Option<NaiveDate>,
    pub worst_deficit: Money,
    pub worst_date: Option<NaiveDate>,
    pub days_below: i64,
    /// Currency-days: severity over time, not cash (E08).
    pub integrated_shortfall_currency_days: i128,
    /// `K*` with its chain.
    pub minimum_injection: Calc<Money>,
    /// Lowest balance on the path and when.
    pub lowest: Money,
    pub lowest_date: Option<NaiveDate>,
}

impl BreachReport {
    /// The M13 wording: never "infinite runway" (V044).
    pub fn summary(&self) -> String {
        match self.first_breach {
            None => format!("No breach of the {} floor through {}", self.floor.format(), self.horizon.format("%d %b %Y")),
            Some(date) => format!(
                "First breach on {}: {} below the {} floor at worst{}",
                date.format("%d %b %Y"),
                self.worst_deficit.format(),
                self.floor.format(),
                match self.recovery {
                    Some(back) => format!(", back above it on {}", back.format("%d %b %Y")),
                    None => ", not recovered within the horizon".to_string(),
                }
            ),
        }
    }
}

/// Analyses a chronologically ordered path (one point per date; consecutive
/// dates or not — the gap to the next point is the duration of each state)
/// against a fixed floor.
pub fn analyse(path: &[PathPoint], floor: Money, horizon: NaiveDate) -> EngineResult<BreachReport> {
    let currency = floor.currency();
    let mut first_breach = None;
    let mut recovery = None;
    let mut worst = Money::zero(currency);
    let mut worst_date = None;
    let mut days_below = 0i64;
    let mut integrated: i128 = 0;
    let mut lowest: Option<(Money, NaiveDate)> = None;
    let mut terms = Vec::new();

    for (index, point) in path.iter().enumerate() {
        if point.balance.currency() != currency {
            return Err(MoneyError::CurrencyMismatch { left: currency, right: point.balance.currency() }.into());
        }
        let next_date = path.get(index + 1).map(|p| p.date).unwrap_or(horizon.succ_opt().unwrap_or(horizon));
        let duration = (next_date - point.date).num_days().max(1);
        let shortfall = point.balance.shortfall_below(floor)?;
        if lowest.is_none_or(|(low, _)| point.balance.minor() < low.minor()) {
            lowest = Some((point.balance, point.date));
        }
        if shortfall.is_positive() {
            if first_breach.is_none() {
                first_breach = Some(point.date);
            }
            recovery = None;
            days_below += duration;
            integrated += shortfall.minor() as i128 * duration as i128;
            if shortfall.minor() > worst.minor() {
                worst = shortfall;
                worst_date = Some(point.date);
                terms.push(
                    ProvNode::formula(
                        format!("Shortfall on {}", point.date.format("%d %b %Y")),
                        shortfall,
                        format!("[{} − {}]₊", floor.format(), point.balance.format()),
                        Vec::new(),
                    )
                    .money_class(MoneyClass::ConditionalFuture),
                );
            }
        } else if first_breach.is_some() && recovery.is_none() {
            recovery = Some(point.date);
        }
    }

    let injection_node = ProvNode::formula(
        "Minimum immediate injection K*",
        worst,
        "max over dates of [R − b_t]₊ (M13); valid when flows are fixed, cash is one pool and the injection has no cost or downstream effect",
        terms,
    )
    .money_class(MoneyClass::ConditionalFuture)
    .strength(ResultStrength::ConditionalPath)
    .note(format!(
        "Integrated shortfall {} currency-days over {} day(s) below the floor measures duration and severity, not repeated new capital (E08).",
        (integrated / currency.minor_per_major() as i128),
        days_below
    ));

    Ok(BreachReport {
        floor,
        horizon,
        first_breach,
        recovery,
        worst_deficit: worst,
        worst_date,
        days_below,
        integrated_shortfall_currency_days: integrated / currency.minor_per_major() as i128,
        minimum_injection: Calc::new(worst, injection_node),
        lowest: lowest.map(|(m, _)| m).unwrap_or(Money::zero(currency)),
        lowest_date: lowest.map(|(_, d)| d),
    })
}

/// Chain node for a headroom figure: the signed headroom and, when negative,
/// the separately reported deficit (§6.6).
pub fn headroom_node(label: &str, balance: Money, floor: Money) -> EngineResult<Calc<Money>> {
    let headroom = balance.checked_sub(floor)?;
    let mut node = ProvNode::sum(
        label.to_string(),
        headroom,
        vec![
            ProvNode::input("Settled cash", balance, "reconciled balance").money_class(MoneyClass::ConfirmedCurrent),
            ProvNode::input("Hard floors", floor, "hard earmarks and bank minimums (§17)").money_class(MoneyClass::ReservedCurrent).minus(),
        ],
    )
    .money_class(MoneyClass::FreeCurrent);
    if headroom.is_negative() {
        node = node.note(format!(
            "Deficit of {}: a displayed spendable amount of 0 is only permitted with this deficit shown next to it (§6.6).",
            headroom.abs().format()
        ));
    }
    let _ = ProvValue::Empty;
    Ok(Calc::new(headroom, node))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::pkr;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    #[test]
    fn e08_a_gap_is_not_multiplied_by_its_duration() {
        // 10 consecutive days 100,000 below a 1,000,000 floor, then recovery.
        let mut path: Vec<PathPoint> = (1..=10).map(|day| PathPoint { date: d(day), balance: pkr(900_000) }).collect();
        path.push(PathPoint { date: d(11), balance: pkr(1_200_000) });
        let report = analyse(&path, pkr(1_000_000), d(31)).unwrap();
        assert_eq!(report.first_breach, Some(d(1)));
        assert_eq!(report.recovery, Some(d(11)));
        assert_eq!(report.minimum_injection.money(), pkr(100_000));
        assert_eq!(report.integrated_shortfall_currency_days, 1_000_000);
        assert_eq!(report.days_below, 10);
        assert_eq!(report.lowest, pkr(900_000));
        assert!(report.summary().starts_with("First breach on 01 Oct 2026: 100,000 below"));
        assert!(report.minimum_injection.node().notes()[0].contains("not repeated new capital"));
    }

    #[test]
    fn no_breach_is_not_infinite_runway() {
        let path = vec![PathPoint { date: d(1), balance: pkr(1_500_000) }, PathPoint { date: d(15), balance: pkr(1_100_000) }];
        let report = analyse(&path, pkr(1_000_000), d(31)).unwrap();
        assert_eq!(report.first_breach, None);
        assert_eq!(report.minimum_injection.money(), pkr(0));
        assert_eq!(report.summary(), "No breach of the 1,000,000 floor through 31 Oct 2026");
        assert_eq!(report.lowest_date, Some(d(15)));
    }

    #[test]
    fn worst_deficit_uses_intermediate_dates() {
        // V044: the dip in the middle counts even though the ends are fine.
        let path = vec![
            PathPoint { date: d(1), balance: pkr(1_200_000) },
            PathPoint { date: d(5), balance: pkr(980_000) },
            PathPoint { date: d(6), balance: pkr(1_300_000) },
        ];
        let report = analyse(&path, pkr(1_000_000), d(31)).unwrap();
        assert_eq!(report.first_breach, Some(d(5)));
        assert_eq!(report.worst_deficit, pkr(20_000));
        assert_eq!(report.days_below, 1);
    }

    #[test]
    fn headroom_keeps_the_sign() {
        let calc = headroom_node("Headroom", pkr(900_000), pkr(1_000_000)).unwrap();
        assert_eq!(calc.money(), pkr(-100_000));
        assert!(calc.node().notes()[0].contains("Deficit of 100,000"));
        assert!(calc.node().verify_sums().is_empty());
    }
}
