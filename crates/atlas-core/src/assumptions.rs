//! Assumption-bound forecasting (§2.5, §10): deterministic derivation of an
//! assumption from reconciled history with a disclosed formula and sample
//! (§10.7), and the §10.1 conditional statement every future-facing
//! conclusion is phrased with.

use crate::ids::{AssumptionId, SeriesId};
use crate::model::{Assumption, Household};
use crate::money::Money;
use crate::provenance::{Calc, ProvNode, ProvValue};
use crate::timeline::AmountSpec;
use crate::vocab::{Certainty, MoneyClass, ResultStrength};
use crate::{EngineError, EngineResult};
use chrono::NaiveDate;

/// §10.7 — the deterministic formulas a derived assumption may use.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Derivation {
    /// Minimum and maximum of the last N payments; expected = the latest payment.
    MinMax { last_n: usize },
    /// Median of the last N payments, as an exact amount.
    Median { last_n: usize },
    /// Arithmetic mean of the last N payments (rounded half away from zero).
    Mean { last_n: usize },
}

impl Derivation {
    pub const ALL: [Derivation; 3] = [Derivation::MinMax { last_n: 6 }, Derivation::Median { last_n: 6 }, Derivation::Mean { last_n: 6 }];

    pub fn describe(self) -> String {
        match self {
            Derivation::MinMax { last_n } => format!("minimum and maximum of the last {last_n} reconciled payments"),
            Derivation::Median { last_n } => format!("median of the last {last_n} reconciled payments"),
            Derivation::Mean { last_n } => format!("arithmetic mean of the last {last_n} reconciled payments"),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Derivation::MinMax { .. } => "Min–max range",
            Derivation::Median { .. } => "Median",
            Derivation::Mean { .. } => "Mean",
        }
    }
}

/// A derived amount assumption with the sample it came from.
#[derive(Clone, Debug)]
pub struct DerivedAssumption {
    pub series: SeriesId,
    pub derivation: Derivation,
    pub amount: AmountSpec,
    pub sample_size: usize,
    pub sample_from: NaiveDate,
    pub sample_to: NaiveDate,
    /// The chain: one input node per sample payment, the formula on top.
    pub calc: Calc<Money>,
    /// The §10.7 wording, including what the range does *not* mean.
    pub statement: String,
}

/// Derives an amount assumption for a series from its reconciled history.
pub fn derive(household: &Household, series: SeriesId, derivation: Derivation) -> EngineResult<DerivedAssumption> {
    let series_ref = household.series_by_id(series).ok_or(EngineError::UnknownSeries(series))?;
    let history = household.history_of(series);
    let last_n = match derivation {
        Derivation::MinMax { last_n } | Derivation::Median { last_n } | Derivation::Mean { last_n } => last_n,
    };
    if history.len() < last_n.min(2) || history.is_empty() {
        return Err(EngineError::Insufficient(format!(
            "{} has {} reconciled payment(s); the formula needs {last_n}. Zero observations mean unknown, not zero risk (M15).",
            series_ref.name,
            history.len()
        )));
    }
    let sample: Vec<_> = history.iter().rev().take(last_n).rev().collect();
    let currency = sample[0].amount.currency();
    let inputs: Vec<ProvNode> = sample
        .iter()
        .map(|h| {
            ProvNode::input(format!("Reconciled payment {}", h.date.format("%d %b %Y")), h.amount, "reconciled history")
                .money_class(MoneyClass::ConfirmedCurrent)
                .certainty(Certainty::Confirmed)
        })
        .collect();
    let mut amounts: Vec<Money> = sample.iter().map(|h| h.amount).collect();
    amounts.sort_by_key(|m| m.minor());
    let low = amounts[0];
    let high = *amounts.last().expect("non-empty");
    let latest = sample.last().expect("non-empty").amount;
    let (amount, value, formula) = match derivation {
        Derivation::MinMax { .. } => (
            AmountSpec::Range { low, expected: latest, high },
            ProvValue::Text(format!("{}–{} (expected: latest {})", low.format(), high.format(), latest.format())),
            format!("range = [min, max] of the sample; expected = the most recent payment ({})", latest.format()),
        ),
        Derivation::Median { .. } => {
            let n = amounts.len();
            let median = if n % 2 == 1 {
                amounts[n / 2]
            } else {
                Money::new((amounts[n / 2 - 1].minor() + amounts[n / 2].minor()) / 2, currency)
            };
            (AmountSpec::Exact(median), ProvValue::Money(median), format!("median of {n} sorted payments"))
        }
        Derivation::Mean { .. } => {
            let n = amounts.len() as i128;
            let total: i128 = amounts.iter().map(|m| m.minor() as i128).sum();
            let rounded = (total * 2 + n) / (2 * n);
            let mean = Money::new(rounded as i64, currency);
            (AmountSpec::Exact(mean), ProvValue::Money(mean), format!("sum of {n} payments ÷ {n}, rounded half away from zero"))
        }
    };
    let sample_from = sample[0].date;
    let sample_to = sample.last().expect("non-empty").date;
    let node = ProvNode::formula(
        format!("{} — {}", series_ref.name, derivation.describe()),
        value,
        formula,
        inputs,
    )
    .money_class(MoneyClass::ExpectedFuture)
    .certainty(Certainty::HistoricallyDerived)
    .strength(ResultStrength::ExactAccounting)
    .note(format!(
        "Sample: {} payments from {} to {}. A historical range is a descriptive sample statistic, not a bound on the next payment and not proof that employment continues (§10.7).",
        sample.len(),
        sample_from.format("%d %b %Y"),
        sample_to.format("%d %b %Y")
    ));
    let statement = format!(
        "{} assumption: {}. Derived from the {} ({} payments, {} – {}). It records what was paid, not what will be.",
        series_ref.name,
        amount.describe(),
        derivation.describe(),
        sample.len(),
        sample_from.format("%d %b %Y"),
        sample_to.format("%d %b %Y")
    );
    Ok(DerivedAssumption {
        series,
        derivation,
        amount,
        sample_size: sample.len(),
        sample_from,
        sample_to,
        calc: Calc::new(amount.expected(), node),
        statement,
    })
}

/// Applies a derived assumption: the series takes the derived amount and the
/// linked assumption records the formula, sample and (pending) acceptance.
pub fn apply_derived(household: &mut Household, derived: &DerivedAssumption, assumption: AssumptionId) -> EngineResult<()> {
    let series = household.series.iter_mut().find(|s| s.id == derived.series).ok_or(EngineError::UnknownSeries(derived.series))?;
    series.amount = derived.amount;
    series.certainty = if series.certainty == Certainty::Confirmed { Certainty::HistoricallyDerived } else { series.certainty };
    let record = household.assumptions.iter_mut().find(|a| a.id == assumption).ok_or(EngineError::UnknownAssumption(assumption))?;
    record.text = derived.statement.clone();
    record.certainty = Certainty::HistoricallyDerived;
    record.source = crate::model::AssumptionSource::DerivedFromHistory {
        formula: derived.derivation.describe(),
        sample_size: derived.sample_size as u32,
        sample_from: derived.sample_from,
        sample_to: derived.sample_to,
    };
    record.accepted_on = None;
    Ok(())
}

/// §10.1 / V031 — the shape every future-facing conclusion must take: a
/// conditional claim with its horizon, coverage label, assumptions and the
/// shocks it did not consider.
#[derive(Clone, Debug, PartialEq)]
pub struct ConditionalStatement {
    /// "You could purchase the car in November while retaining the 1,000,000 reserve"
    pub claim: String,
    pub horizon: NaiveDate,
    pub coverage: ResultStrength,
    pub assumptions: Vec<String>,
    pub excluded_shocks: Vec<String>,
}

impl ConditionalStatement {
    /// The plan's preferred wording (§10.1): never unconditional.
    pub fn render(&self) -> String {
        let mut text = format!("{} through {}, under the following assumptions:\n", self.claim, self.horizon.format("%d %b %Y"));
        for (index, assumption) in self.assumptions.iter().enumerate() {
            text.push_str(&format!("{}. {}\n", index + 1, assumption));
        }
        text.push_str(&format!("Coverage: {} — {}\n", self.coverage.label(), self.coverage.permitted_claim()));
        text.push_str(&format!("Not established: {}\n", self.coverage.does_not_establish()));
        if !self.excluded_shocks.is_empty() {
            text.push_str(&format!("Outside this analysis: {}.", self.excluded_shocks.join("; ")));
        }
        text
    }

    /// The assumption texts a set of [`Assumption`]s contributes.
    pub fn assumption_texts(assumptions: &[Assumption]) -> Vec<String> {
        assumptions.iter().map(|a| format!("{} ({})", a.text, a.certainty.label().to_lowercase())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{self, ids, pkr};

    #[test]
    fn min_max_derivation_discloses_formula_and_sample() {
        let household = fixtures::plan_household();
        let derived = derive(&household, ids::SALARY_A, Derivation::MinMax { last_n: 6 }).unwrap();
        assert_eq!(derived.amount.low(), pkr(480_000));
        assert_eq!(derived.amount.high(), pkr(520_000));
        assert_eq!(derived.amount.expected(), pkr(500_000));
        assert_eq!(derived.sample_size, 6);
        assert_eq!(derived.calc.node().children().len(), 6, "one input node per reconciled payment");
        let text = derived.calc.node().render_chain();
        assert!(text.contains("Reconciled payment 30 Apr 2026"));
        assert!(text.contains("formula: range = [min, max]"));
        assert!(derived.statement.contains("not what will be"));
        assert_eq!(derived.calc.node().certainty_label(), Some(Certainty::HistoricallyDerived));
    }

    #[test]
    fn median_and_mean_are_exact_and_rounded() {
        let household = fixtures::plan_household();
        let median = derive(&household, ids::SALARY_A, Derivation::Median { last_n: 6 }).unwrap();
        // sorted: 480, 495, 500, 500, 505, 520 → median (500+500)/2.
        assert_eq!(median.amount.expected(), pkr(500_000));
        let mean = derive(&household, ids::SALARY_A, Derivation::Mean { last_n: 6 }).unwrap();
        // 3,000,000 / 6 = 500,000.
        assert_eq!(mean.amount.expected(), pkr(500_000));
        let mean4 = derive(&household, ids::SALARY_A, Derivation::Mean { last_n: 4 }).unwrap();
        // last 4: 505 + 520 + 495 + 500 = 2,020,000 / 4 = 505,000.
        assert_eq!(mean4.amount.expected(), pkr(505_000));
    }

    #[test]
    fn insufficient_history_is_unknown_not_zero() {
        let household = fixtures::plan_household();
        let err = derive(&household, ids::FREELANCE, Derivation::MinMax { last_n: 6 }).unwrap_err();
        assert!(err.to_string().contains("unknown, not zero risk"));
    }

    #[test]
    fn applying_a_derivation_records_provenance_and_needs_acceptance() {
        let mut household = fixtures::plan_household();
        let derived = derive(&household, ids::SALARY_A, Derivation::Mean { last_n: 4 }).unwrap();
        apply_derived(&mut household, &derived, AssumptionId::new(1)).unwrap();
        let assumption = household.assumption(AssumptionId::new(1)).unwrap();
        assert_eq!(assumption.accepted_on, None, "a re-derived assumption must be accepted again (§2.5)");
        assert_eq!(assumption.freshness(household.as_of), crate::model::Freshness::NotAccepted);
        assert!(assumption.source.describe().contains("arithmetic mean of the last 4"));
        assert_eq!(household.series_by_id(ids::SALARY_A).unwrap().amount.expected(), pkr(505_000));
        household.accept_assumption(AssumptionId::new(1), household.as_of).unwrap();
        assert_eq!(household.assumption(AssumptionId::new(1)).unwrap().freshness(household.as_of), crate::model::Freshness::Fresh);
    }

    #[test]
    fn freshness_flags_stale_and_expired() {
        let household = fixtures::plan_household();
        let rent = household.assumption(AssumptionId::new(3)).unwrap();
        assert_eq!(rent.freshness(household.as_of), crate::model::Freshness::Stale, "accepted 1 May, more than 90 days ago");
        let receivable = household.assumption(AssumptionId::new(4)).unwrap();
        assert_eq!(receivable.freshness(NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()), crate::model::Freshness::Expired);
    }

    #[test]
    fn v031_conditional_statement_lists_everything() {
        let statement = ConditionalStatement {
            claim: "You could purchase the car in November while retaining the 1,000,000 reserve".into(),
            horizon: NaiveDate::from_ymd_opt(2027, 1, 31).unwrap(),
            coverage: ResultStrength::ScenarioTested,
            assumptions: vec!["Salary ≥ 480,000 in Sep–Nov (contractual)".into()],
            excluded_shocks: vec!["car ownership costs".into(), "unplanned expenses above 150,000".into()],
        };
        let text = statement.render();
        assert!(text.contains("through 31 Jan 2027"));
        assert!(text.contains("1. Salary"));
        assert!(text.contains("Coverage: Scenario-tested"));
        assert!(text.contains("Not established: Unexamined paths"));
        assert!(text.contains("Outside this analysis: car ownership costs; unplanned expenses"));
        assert!(!text.to_lowercase().contains("guaranteed"));
    }
}
