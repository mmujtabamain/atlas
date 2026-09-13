//! Atlas Financer core engine — the deterministic half of `plan.md` (v3.2).
//!
//! This crate holds the domain model and every calculation the UI shows. It
//! deliberately has **no UI, no I/O and no floating-point money**:
//!
//! - money is integer minor units plus a currency (§32.1);
//! - every derived figure is a [`provenance::Calc`], i.e. a value *and* the
//!   calculation graph that produced it, so the UI can always answer
//!   "why is this number this number?" (§2.1);
//! - future money never becomes current money (§2.4); the vocabularies in
//!   [`vocab`] make the distinction explicit on every value;
//! - authorization is part of the model from the start (§7.1, §22.2): a
//!   [`provenance::ProvNode`] can be projected for a viewer (M55).
//!
//! Module map (plan sections in brackets):
//!
//! | module | contents |
//! |---|---|
//! | [`money`] | `Currency`, `Money`, currency-checked arithmetic, plan-style formatting |
//! | [`ids`] | typed identifiers, `EntityRef`, `ObjectRef` |
//! | [`vocab`] | `MoneyClass` (§2.4), `Certainty` (§10.3), `ResultStrength` (§32.2) |
//! | [`provenance`] | `Calc<T>`, `ProvNode`, chain rendering, viewer projection (M55) |
//! | [`model`] | household, people, companies, accounts, ownership, reservations, assumptions, scenarios, tax rules (§5–§8, §17) |
//! | [`authz`] | access policies, viewers, disclosure evaluation (§7.1–§7.6) |
//! | [`timeline`] | event series, recurrence expansion, occurrences (§9, M05 — M3 completes it) |
//! | [`liquidity`] | money definitions §6, earmarks §17, E01, boundaries |
//! | [`breach`] | first breach, worst deficit, minimum injection (M13, E08) |
//! | [`forecast`] | the §2.1 conditional projection chain (M4 completes it) |
//! | [`fixtures`] | the fictitious plan household used by the UI and the tests |

pub mod assumptions;
pub mod authz;
pub mod breach;
pub mod decision;
pub mod fixtures;
pub mod forecast;
pub mod ids;
pub mod liquidity;
pub mod model;
pub mod money;
pub mod mutations;
pub mod provenance;
pub mod risk;
pub mod rules;
pub mod scenario;
pub mod sensitivity;
pub mod tax;
pub mod timeline;
pub mod vocab;

pub use money::{Currency, Money, MoneyError};
pub use provenance::{Calc, Disclosure, ProvNode};
pub use vocab::{Certainty, MoneyClass, ResultStrength};

use thiserror::Error;

/// Every failure the engine can report. Failures are values, never panics:
/// the UI shows them and the alerting hook forwards them to the team.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    #[error("unknown account {0}")]
    UnknownAccount(ids::AccountId),
    #[error("unknown person {0}")]
    UnknownPerson(ids::PersonId),
    #[error("unknown company {0}")]
    UnknownCompany(ids::CompanyId),
    #[error("unknown reservation {0}")]
    UnknownReservation(ids::ReservationId),
    #[error("{0}")]
    Money(#[from] MoneyError),
    #[error("no access policy for {0}: hidden and excluded until an owner sets one")]
    MissingPolicy(ids::ObjectRef),
    #[error("unknown assumption {0}")]
    UnknownAssumption(ids::AssumptionId),
    #[error("unknown series {0}")]
    UnknownSeries(ids::SeriesId),
    #[error("unknown rule {0}")]
    UnknownRule(ids::RuleId),
    #[error("unknown scenario {0}")]
    UnknownScenario(ids::ScenarioId),
    #[error("{0}")]
    Insufficient(String),
}

/// Result alias used across the engine.
pub type EngineResult<T> = Result<T, EngineError>;
