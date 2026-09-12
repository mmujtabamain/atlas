//! The three vocabularies every displayed value carries.
//!
//! The plan refuses one ambiguous "balance" (§6) and one opaque confidence
//! (§2.3). These enums are how the UI keeps that promise: a value is tagged
//! with *what kind of money it is* ([`MoneyClass`], §2.4), *how sure the
//! assumption behind it is* ([`Certainty`], §10.3) and *what the calculation
//! can legitimately claim* ([`ResultStrength`], §32.2).

use serde::{Deserialize, Serialize};

/// §2.4 — the five kinds of money the system distinguishes at minimum.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum MoneyClass {
    /// Already present and reconciled.
    ConfirmedCurrent,
    /// Present, but intentionally unavailable for general spending.
    ReservedCurrent,
    /// Present and not reserved.
    FreeCurrent,
    /// Not yet received; always assumption-bound.
    ExpectedFuture,
    /// Depends on an event, scenario, rule or uncertain assumption.
    ConditionalFuture,
}

impl MoneyClass {
    pub const ALL: [MoneyClass; 5] = [
        MoneyClass::ConfirmedCurrent,
        MoneyClass::ReservedCurrent,
        MoneyClass::FreeCurrent,
        MoneyClass::ExpectedFuture,
        MoneyClass::ConditionalFuture,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MoneyClass::ConfirmedCurrent => "Confirmed current",
            MoneyClass::ReservedCurrent => "Reserved current",
            MoneyClass::FreeCurrent => "Free current",
            MoneyClass::ExpectedFuture => "Expected future",
            MoneyClass::ConditionalFuture => "Conditional future",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            MoneyClass::ConfirmedCurrent => "Already present and reconciled.",
            MoneyClass::ReservedCurrent => "Present, but intentionally unavailable for general spending.",
            MoneyClass::FreeCurrent => "Present and not reserved.",
            MoneyClass::ExpectedFuture => "Not yet received and always assumption-bound.",
            MoneyClass::ConditionalFuture => "Depends on an event, scenario, rule or uncertain assumption.",
        }
    }

    /// Whether the money exists today. Future classes must never be shown as
    /// spendable (§2.4, V003).
    pub fn is_current(self) -> bool {
        matches!(
            self,
            MoneyClass::ConfirmedCurrent | MoneyClass::ReservedCurrent | MoneyClass::FreeCurrent
        )
    }
}

/// §10.3 — assumption classifications. "These labels should not imply
/// statistical probabilities unless probabilities are explicitly modelled."
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum Certainty {
    Confirmed,
    Contractual,
    Expected,
    UserEstimated,
    HistoricallyDerived,
    ScenarioOnly,
    Tentative,
}

impl Certainty {
    pub const ALL: [Certainty; 7] = [
        Certainty::Confirmed,
        Certainty::Contractual,
        Certainty::Expected,
        Certainty::UserEstimated,
        Certainty::HistoricallyDerived,
        Certainty::ScenarioOnly,
        Certainty::Tentative,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Certainty::Confirmed => "Confirmed",
            Certainty::Contractual => "Contractual",
            Certainty::Expected => "Expected",
            Certainty::UserEstimated => "User-estimated",
            Certainty::HistoricallyDerived => "Historically derived",
            Certainty::ScenarioOnly => "Scenario-only",
            Certainty::Tentative => "Tentative",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Certainty::Confirmed => "Recorded and reconciled; not an assumption.",
            Certainty::Contractual => "Agreed in a contract; still conditional until received.",
            Certainty::Expected => "Anticipated by the user without a contract.",
            Certainty::UserEstimated => "A figure the user typed in as a guess.",
            Certainty::HistoricallyDerived => "Computed from past reconciled payments by a shown formula.",
            Certainty::ScenarioOnly => "Exists only inside a named scenario.",
            Certainty::Tentative => "Under consideration; may never happen.",
        }
    }

    /// Only `Confirmed` describes money that exists today.
    pub fn is_confirmed(self) -> bool {
        self == Certainty::Confirmed
    }
}

/// §32.2 — required result-strength labels. Each carries the permitted claim
/// and, just as importantly, what the label does **not** establish.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum ResultStrength {
    ExactAccounting,
    ConditionalPath,
    ScenarioTested,
    RobustFeasible,
    ProbabilityModel,
    BestOnGrid,
    SolverCertified,
    FeasibleCandidate,
    Unresolved,
}

impl ResultStrength {
    pub const ALL: [ResultStrength; 9] = [
        ResultStrength::ExactAccounting,
        ResultStrength::ConditionalPath,
        ResultStrength::ScenarioTested,
        ResultStrength::RobustFeasible,
        ResultStrength::ProbabilityModel,
        ResultStrength::BestOnGrid,
        ResultStrength::SolverCertified,
        ResultStrength::FeasibleCandidate,
        ResultStrength::Unresolved,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ResultStrength::ExactAccounting => "Exact accounting calculation",
            ResultStrength::ConditionalPath => "Conditional path calculation",
            ResultStrength::ScenarioTested => "Scenario-tested",
            ResultStrength::RobustFeasible => "Robust-feasible under the declared uncertainty",
            ResultStrength::ProbabilityModel => "Probability-model result",
            ResultStrength::BestOnGrid => "Best on specified grid",
            ResultStrength::SolverCertified => "Solver-certified to stated tolerances",
            ResultStrength::FeasibleCandidate => "Feasible candidate / approximation",
            ResultStrength::Unresolved => "Unresolved / incomplete model",
        }
    }

    /// The claim the label permits (§32.2, column "Permitted claim").
    pub fn permitted_claim(self) -> &'static str {
        match self {
            ResultStrength::ExactAccounting => "Reconciled postings and configured rounding produce the stated balance.",
            ResultStrength::ConditionalPath => "A specified path produces the stated outcome.",
            ResultStrength::ScenarioTested => "Every named path in a finite set passed.",
            ResultStrength::RobustFeasible => "All model constraints hold for the complete declared uncertainty set, under a validated robust method.",
            ResultStrength::ProbabilityModel => "A probability or tail metric follows from the disclosed probability model.",
            ResultStrength::BestOnGrid => "No tested combination on that finite grid was better.",
            ResultStrength::SolverCertified => "The mathematical program meets the recorded feasibility/optimality criteria.",
            ResultStrength::FeasibleCandidate => "A candidate passed the replayed checks.",
            ResultStrength::Unresolved => "Inputs or supported rules are insufficient for a claimed answer.",
        }
    }

    /// What the label does not establish (§32.2, last column).
    pub fn does_not_establish(self) -> &'static str {
        match self {
            ResultStrength::ExactAccounting => "The imported data is complete or the future events will occur.",
            ResultStrength::ConditionalPath => "Other paths are safe.",
            ResultStrength::ScenarioTested => "Unexamined paths or a continuum of values passed.",
            ResultStrength::RobustFeasible => "Reality will remain in that set.",
            ResultStrength::ProbabilityModel => "The model is the true future distribution.",
            ResultStrength::BestOnGrid => "A continuous or untested combination cannot be better.",
            ResultStrength::SolverCertified => "Exact arithmetic proof or correct economic/legal assumptions.",
            ResultStrength::FeasibleCandidate => "Global optimality or robust feasibility outside the checked set.",
            ResultStrength::Unresolved => "That the real-world decision is necessarily impossible.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_current_classes_are_current() {
        let current: Vec<_> = MoneyClass::ALL.iter().filter(|c| c.is_current()).collect();
        assert_eq!(current.len(), 3);
        assert!(!MoneyClass::ExpectedFuture.is_current());
        assert!(!MoneyClass::ConditionalFuture.is_current());
    }

    #[test]
    fn every_strength_names_its_limit() {
        for strength in ResultStrength::ALL {
            assert!(!strength.permitted_claim().is_empty());
            assert!(!strength.does_not_establish().is_empty());
        }
    }
}
