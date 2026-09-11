//! Authorization (§7.1–§7.6, §18.5, M55).
//!
//! Economic ownership, visibility, calculation access and disclosure are four
//! separate things. An [`AccessPolicy`] attaches to one financial object and
//! answers, per viewer: may they know it exists, see its balance, see its
//! transactions, see forecasts/assumptions built on it, and inspect its
//! provenance? Independently, may the object participate in calculations at
//! all ([`CalculationAccess`]) and, if it participates while restricted, how
//! is the contribution shown ([`Disclosure`])?
//!
//! Missing policies **fail closed** (F162): no policy means hidden and excluded.

use crate::ids::*;
use crate::model::Household;
use crate::provenance::Disclosure;
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

/// Who is looking (M55 `v`). Company roles arrive with M10.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct Viewer {
    pub person: PersonId,
}

impl Viewer {
    pub fn person(person: PersonId) -> Self {
        Viewer { person }
    }
}

/// §7.3 — whether an object may participate in a calculation, independently
/// of who can see it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum CalculationAccess {
    Excluded,
    RestrictedContribution,
    Full,
}

impl CalculationAccess {
    pub fn label(self) -> &'static str {
        match self {
            CalculationAccess::Excluded => "Excluded from calculations",
            CalculationAccess::RestrictedContribution => "May contribute under restricted disclosure",
            CalculationAccess::Full => "Fully available to calculations",
        }
    }
}

/// Who an aspect of an object is disclosed to.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Grantees {
    /// Only the policy's full-access people (the economic owners).
    OwnersOnly,
    /// The owners plus these people.
    Persons(Vec<PersonId>),
    /// Everyone in the household.
    Household,
}

impl Grantees {
    pub fn describe(&self, household: &Household) -> String {
        match self {
            Grantees::OwnersOnly => "owners only".into(),
            Grantees::Household => "whole household".into(),
            Grantees::Persons(people) => {
                let names: Vec<String> = people.iter().map(|p| household.entity_name(EntityRef::Person(*p))).collect();
                format!("owners + {}", names.join(", "))
            }
        }
    }
}

/// §7.1 — the visibility aspects a policy controls separately.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Aspect {
    Existence,
    Balance,
    Transactions,
    Forecasts,
    Assumptions,
    Provenance,
}

impl Aspect {
    pub const ALL: [Aspect; 6] = [
        Aspect::Existence,
        Aspect::Balance,
        Aspect::Transactions,
        Aspect::Forecasts,
        Aspect::Assumptions,
        Aspect::Provenance,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Aspect::Existence => "Existence",
            Aspect::Balance => "Balance / value",
            Aspect::Transactions => "Transactions / events",
            Aspect::Forecasts => "Forecasts",
            Aspect::Assumptions => "Assumptions",
            Aspect::Provenance => "Explanations / provenance",
        }
    }
}

/// §7.2 — the simple presets the UI exposes first. The model is not limited
/// to them: any combination of [`Grantees`] per aspect is representable.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum VisibilityPreset {
    Private,
    SharedSummary,
    SharedBalance,
    FullyShared,
}

impl VisibilityPreset {
    pub fn label(self) -> &'static str {
        match self {
            VisibilityPreset::Private => "Private",
            VisibilityPreset::SharedSummary => "Shared summary",
            VisibilityPreset::SharedBalance => "Shared balance",
            VisibilityPreset::FullyShared => "Fully shared",
        }
    }
}

/// §5.13 / §7.5 — a versioned, effective-dated policy on one object.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct AccessPolicy {
    pub id: PolicyId,
    pub object: ObjectRef,
    /// People who always receive full disclosure (the economic owners).
    pub full_access: Vec<PersonId>,
    pub existence: Grantees,
    pub balance: Grantees,
    pub transactions: Grantees,
    pub forecasts: Grantees,
    pub assumptions: Grantees,
    pub provenance: Grantees,
    pub calculation_access: CalculationAccess,
    /// How an authorized-but-restricted contribution is represented (§7.5
    /// "Disclosure of restricted contribution: aggregate only").
    pub restricted_disclosure: Disclosure,
    /// §7.4 purpose scope; empty means every purpose.
    pub purposes: Vec<String>,
    pub effective_from: NaiveDate,
    pub version: u32,
    pub changed_by: PersonId,
    pub changed_at: NaiveDateTime,
    pub previous: Option<String>,
}

impl AccessPolicy {
    /// A preset policy (§7.2) — the starting point every fixture object gets.
    pub fn preset(
        id: PolicyId,
        object: ObjectRef,
        owners: Vec<PersonId>,
        preset: VisibilityPreset,
        calculation_access: CalculationAccess,
        effective_from: NaiveDate,
        changed_at: NaiveDateTime,
    ) -> Self {
        let changed_by = owners.first().copied().expect("a policy has at least one owner");
        let (existence, balance, transactions, forecasts, assumptions, provenance) = match preset {
            VisibilityPreset::Private => (
                Grantees::OwnersOnly,
                Grantees::OwnersOnly,
                Grantees::OwnersOnly,
                Grantees::OwnersOnly,
                Grantees::OwnersOnly,
                Grantees::OwnersOnly,
            ),
            VisibilityPreset::SharedSummary => (
                Grantees::OwnersOnly,
                Grantees::OwnersOnly,
                Grantees::OwnersOnly,
                Grantees::Household,
                Grantees::OwnersOnly,
                Grantees::OwnersOnly,
            ),
            VisibilityPreset::SharedBalance => (
                Grantees::Household,
                Grantees::Household,
                Grantees::OwnersOnly,
                Grantees::Household,
                Grantees::OwnersOnly,
                Grantees::OwnersOnly,
            ),
            VisibilityPreset::FullyShared => (
                Grantees::Household,
                Grantees::Household,
                Grantees::Household,
                Grantees::Household,
                Grantees::Household,
                Grantees::Household,
            ),
        };
        AccessPolicy {
            id,
            object,
            full_access: owners,
            existence,
            balance,
            transactions,
            forecasts,
            assumptions,
            provenance,
            calculation_access,
            restricted_disclosure: Disclosure::Aggregate,
            purposes: Vec::new(),
            effective_from,
            version: 1,
            changed_by,
            changed_at,
            previous: None,
        }
    }

    /// The preset this policy matches, if any (for the summary column).
    pub fn preset_label(&self) -> &'static str {
        let all_household = [&self.existence, &self.balance, &self.transactions, &self.provenance]
            .iter()
            .all(|g| **g == Grantees::Household);
        let all_owners = [&self.existence, &self.balance, &self.transactions, &self.provenance]
            .iter()
            .all(|g| **g == Grantees::OwnersOnly);
        if all_household {
            "Fully shared"
        } else if all_owners && self.forecasts == Grantees::OwnersOnly {
            "Private"
        } else if all_owners {
            "Shared summary"
        } else if self.balance != Grantees::OwnersOnly && self.transactions == Grantees::OwnersOnly {
            "Shared balance"
        } else {
            "Custom"
        }
    }

    fn grantees(&self, aspect: Aspect) -> &Grantees {
        match aspect {
            Aspect::Existence => &self.existence,
            Aspect::Balance => &self.balance,
            Aspect::Transactions => &self.transactions,
            Aspect::Forecasts => &self.forecasts,
            Aspect::Assumptions => &self.assumptions,
            Aspect::Provenance => &self.provenance,
        }
    }

    /// Whether the viewer is one of the object's full-access people.
    pub fn is_full_viewer(&self, viewer: Viewer) -> bool {
        self.full_access.contains(&viewer.person)
    }

    /// Whether `viewer` may see `aspect` of the object.
    pub fn grants(&self, aspect: Aspect, viewer: Viewer) -> bool {
        if self.is_full_viewer(viewer) {
            return true;
        }
        match self.grantees(aspect) {
            Grantees::OwnersOnly => false,
            Grantees::Persons(people) => people.contains(&viewer.person),
            Grantees::Household => true,
        }
    }

    /// M55 `D_i(v, c)` for this object and viewer.
    pub fn disclosure(&self, viewer: Viewer) -> Disclosure {
        if self.is_full_viewer(viewer) {
            return Disclosure::Full;
        }
        let existence = self.grants(Aspect::Existence, viewer);
        let balance = self.grants(Aspect::Balance, viewer);
        let transactions = self.grants(Aspect::Transactions, viewer);
        let provenance = self.grants(Aspect::Provenance, viewer);
        if existence && balance && transactions && provenance {
            Disclosure::Full
        } else if existence && balance && transactions {
            Disclosure::SelectedFields
        } else if existence && balance {
            Disclosure::BalanceOnly
        } else if self.calculation_access == CalculationAccess::RestrictedContribution || existence {
            // Existence without balance, or a restricted contribution: only an
            // authorized aggregate may appear (§7.2 "Private but usable").
            self.restricted_disclosure.min(Disclosure::Aggregate)
        } else {
            Disclosure::Hidden
        }
    }
}

impl Household {
    /// The policy attached directly to an object.
    pub fn policy_for(&self, object: ObjectRef) -> Option<&AccessPolicy> {
        self.policies.iter().find(|p| p.object == object)
    }

    /// The object whose policy governs `object` when it has none of its own:
    /// reservations and series inherit from their account (§7.1 "inherited
    /// from an authorized boundary").
    pub fn governing_object(&self, object: ObjectRef) -> ObjectRef {
        if self.policy_for(object).is_some() {
            return object;
        }
        match object {
            ObjectRef::Reservation(id) => self
                .reservation(id)
                .map(|r| ObjectRef::Account(r.account))
                .unwrap_or(object),
            ObjectRef::Series(id) => self
                .series
                .iter()
                .find(|s| s.id == id)
                .map(|s| ObjectRef::Account(s.account))
                .unwrap_or(object),
            other => other,
        }
    }

    /// M55 `D_i(v, c)`; a missing policy fails closed to `Hidden`.
    pub fn disclosure_for(&self, viewer: Viewer, object: ObjectRef) -> Disclosure {
        match self.policy_for(self.governing_object(object)) {
            Some(policy) => policy.disclosure(viewer),
            None => Disclosure::Hidden,
        }
    }

    /// M55 `A_i(c)`; a missing policy fails closed to `Excluded`.
    pub fn calculation_access_for(&self, object: ObjectRef) -> CalculationAccess {
        match self.policy_for(self.governing_object(object)) {
            Some(policy) => policy.calculation_access,
            None => CalculationAccess::Excluded,
        }
    }

    /// The disclosure function [`crate::provenance::ProvNode::project`] takes.
    pub fn disclosure_fn(&self, viewer: Viewer) -> impl Fn(&ObjectRef) -> Disclosure + '_ {
        move |object| self.disclosure_for(viewer, *object)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;

    fn when() -> (NaiveDate, NaiveDateTime) {
        let day = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
        (day, day.and_hms_opt(18, 3, 0).unwrap())
    }

    #[test]
    fn presets_map_to_disclosure_levels() {
        let a = PersonId::new(1);
        let b = PersonId::new(2);
        let object = ObjectRef::Account(AccountId::new(17));
        let (from, at) = when();
        let policy = |preset, access| AccessPolicy::preset(PolicyId::new(1), object, vec![a], preset, access, from, at);

        assert_eq!(policy(VisibilityPreset::FullyShared, CalculationAccess::Full).disclosure(Viewer::person(b)), Disclosure::Full);
        assert_eq!(policy(VisibilityPreset::SharedBalance, CalculationAccess::Full).disclosure(Viewer::person(b)), Disclosure::BalanceOnly);
        assert_eq!(policy(VisibilityPreset::Private, CalculationAccess::Excluded).disclosure(Viewer::person(b)), Disclosure::Hidden);
        assert_eq!(
            policy(VisibilityPreset::Private, CalculationAccess::RestrictedContribution).disclosure(Viewer::person(b)),
            Disclosure::Aggregate
        );
        // The owner always sees everything, whatever the preset.
        assert_eq!(policy(VisibilityPreset::Private, CalculationAccess::Excluded).disclosure(Viewer::person(a)), Disclosure::Full);
        assert_eq!(policy(VisibilityPreset::Private, CalculationAccess::Excluded).preset_label(), "Private");
        assert_eq!(policy(VisibilityPreset::SharedBalance, CalculationAccess::Full).preset_label(), "Shared balance");
    }
}
