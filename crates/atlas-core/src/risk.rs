//! Loss-distribution summaries for named, user-weighted outcomes (M10, M11,
//! E04): expected loss, VaR under the lower-quantile convention and CVaR
//! computed on the tail mass — never presented as a real confidence level,
//! because the weights are assumptions, not measured probabilities.

use crate::money::Money;
use crate::{EngineError, EngineResult};

/// One outcome: a loss and its assumed probability in basis points.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Outcome {
    pub loss: Money,
    pub probability_basis_points: u32,
}

fn checked(outcomes: &[Outcome]) -> EngineResult<()> {
    let total: u32 = outcomes.iter().map(|o| o.probability_basis_points).sum();
    if total != 10_000 {
        return Err(EngineError::Insufficient(format!("outcome probabilities sum to {total} basis points, not 10,000")));
    }
    Ok(())
}

/// Σ p·loss, rounded to the minor unit.
pub fn expected_loss(outcomes: &[Outcome]) -> EngineResult<Money> {
    checked(outcomes)?;
    let currency = outcomes.first().map(|o| o.loss.currency()).ok_or(EngineError::Insufficient("no outcomes".into()))?;
    let minor: i128 = outcomes.iter().map(|o| o.loss.minor() as i128 * o.probability_basis_points as i128).sum();
    Ok(Money::new(((minor + 5_000) / 10_000) as i64, currency))
}

/// Lower-quantile VaR: the smallest loss whose cumulative probability reaches
/// `confidence_basis_points` (E04: 100,000 at 90%).
pub fn value_at_risk(outcomes: &[Outcome], confidence_basis_points: u32) -> EngineResult<Money> {
    checked(outcomes)?;
    let mut sorted = outcomes.to_vec();
    sorted.sort_by_key(|o| o.loss.minor());
    let mut cumulative = 0;
    for outcome in &sorted {
        cumulative += outcome.probability_basis_points;
        if cumulative >= confidence_basis_points {
            return Ok(outcome.loss);
        }
    }
    Ok(sorted.last().map(|o| o.loss).unwrap_or(Money::zero(crate::Currency::USD)))
}

/// CVaR on the worst `1 − confidence` mass, taking part of the atom at the
/// VaR when needed (E04: (0.05·500,000 + 0.05·100,000)/0.10 = 300,000).
/// Averaging only losses strictly above the VaR would wrongly give 500,000.
pub fn conditional_value_at_risk(outcomes: &[Outcome], confidence_basis_points: u32) -> EngineResult<Money> {
    checked(outcomes)?;
    let currency = outcomes.first().map(|o| o.loss.currency()).ok_or(EngineError::Insufficient("no outcomes".into()))?;
    let tail = 10_000u32.saturating_sub(confidence_basis_points);
    if tail == 0 {
        return Err(EngineError::Insufficient("a 100% confidence level has no tail".into()));
    }
    let mut sorted = outcomes.to_vec();
    sorted.sort_by_key(|o| std::cmp::Reverse(o.loss.minor()));
    let mut remaining = tail;
    let mut weighted: i128 = 0;
    for outcome in &sorted {
        if remaining == 0 {
            break;
        }
        let take = outcome.probability_basis_points.min(remaining);
        weighted += outcome.loss.minor() as i128 * take as i128;
        remaining -= take;
    }
    Ok(Money::new(((weighted + tail as i128 / 2) / tail as i128) as i64, currency))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::pkr;

    #[test]
    fn e04_cvar_takes_part_of_the_var_atom() {
        let outcomes = [
            Outcome { loss: pkr(0), probability_basis_points: 8_000 },
            Outcome { loss: pkr(100_000), probability_basis_points: 1_500 },
            Outcome { loss: pkr(500_000), probability_basis_points: 500 },
        ];
        assert_eq!(expected_loss(&outcomes).unwrap(), pkr(40_000));
        assert_eq!(value_at_risk(&outcomes, 9_000).unwrap(), pkr(100_000));
        assert_eq!(conditional_value_at_risk(&outcomes, 9_000).unwrap(), pkr(300_000));
        // Equal failure frequency, very different severity: same 20% chance of any loss.
        let mild = [Outcome { loss: pkr(0), probability_basis_points: 8_000 }, Outcome { loss: pkr(10_000), probability_basis_points: 2_000 }];
        assert_eq!(conditional_value_at_risk(&mild, 9_000).unwrap(), pkr(10_000));
        assert!(checked(&[Outcome { loss: pkr(0), probability_basis_points: 5_000 }]).is_err());
    }
}
