//! Typed identifiers. Every domain object has its own id type so an account
//! id can never be passed where a person id is expected, and so provenance
//! nodes can name the object they derive from ([`ObjectRef`]).

use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident, $prefix:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Serialize, Deserialize)]
        pub struct $name(u32);

        impl $name {
            /// Wraps a raw id.
            pub const fn new(raw: u32) -> Self {
                $name(raw)
            }

            /// The raw id.
            pub const fn raw(self) -> u32 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}-{}", $prefix, self.0)
            }
        }
    };
}

define_id!(/// A human participant (§5.2).
    PersonId, "person");
define_id!(/// A company, legally distinct from the people who own it (§5.3).
    CompanyId, "company");
define_id!(/// A container for money, investments, debt or another balance (§5.4).
    AccountId, "account");
define_id!(/// A claim on existing money (§5.8).
    ReservationId, "reservation");
define_id!(/// A template generating event occurrences (§5.6).
    SeriesId, "series");
define_id!(/// One generated or actual financial event (§5.5).
    OccurrenceId, "occurrence");
define_id!(/// A condition a forecast depends on (§5.9).
    AssumptionId, "assumption");
define_id!(/// An overlay over the baseline (§5.11).
    ScenarioId, "scenario");
define_id!(/// A deterministic user rule (§5.10).
    RuleId, "rule");
define_id!(/// One effective-dated tax rule (§12.1).
    TaxRuleId, "tax-rule");
define_id!(/// A versioned access policy (§5.13).
    PolicyId, "policy");
define_id!(/// A goal or proposed decision (§5.12).
    GoalId, "goal");
define_id!(/// An actual transaction (§5.7).
    TransactionId, "txn");

/// The legal/economic entity money belongs to or an event is attributed to.
/// A company is never "just another personal account" (§5.3, §12.6).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum EntityRef {
    Household,
    Person(PersonId),
    Company(CompanyId),
}

impl fmt::Display for EntityRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EntityRef::Household => f.write_str("household"),
            EntityRef::Person(id) => write!(f, "{id}"),
            EntityRef::Company(id) => write!(f, "{id}"),
        }
    }
}

/// Any financial object an access policy can attach to (§27) and a provenance
/// node can derive from (M55).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum ObjectRef {
    Person(PersonId),
    Company(CompanyId),
    Account(AccountId),
    Reservation(ReservationId),
    Series(SeriesId),
    Occurrence(OccurrenceId),
    Assumption(AssumptionId),
    Scenario(ScenarioId),
    Goal(GoalId),
}

impl fmt::Display for ObjectRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ObjectRef::Person(id) => write!(f, "{id}"),
            ObjectRef::Company(id) => write!(f, "{id}"),
            ObjectRef::Account(id) => write!(f, "{id}"),
            ObjectRef::Reservation(id) => write!(f, "{id}"),
            ObjectRef::Series(id) => write!(f, "{id}"),
            ObjectRef::Occurrence(id) => write!(f, "{id}"),
            ObjectRef::Assumption(id) => write!(f, "{id}"),
            ObjectRef::Scenario(id) => write!(f, "{id}"),
            ObjectRef::Goal(id) => write!(f, "{id}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_display_with_their_prefix() {
        assert_eq!(AccountId::new(17).to_string(), "account-17");
        assert_eq!(ObjectRef::Series(SeriesId::new(3)).to_string(), "series-3");
        assert_eq!(EntityRef::Household.to_string(), "household");
    }
}
