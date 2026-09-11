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
use crate::EngineResult;
use crate::EngineError;
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

/// §7.4 — an explicitly modelled planning context a restricted resource may
/// be authorized for.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Purpose {
    HouseholdForecast,
    Scenario(ScenarioId),
    Decision,
    Tax,
    FundingSearch,
    Extraction,
}

impl Purpose {
    /// The tag stored in a policy's purpose list.
    pub fn tag(self) -> String {
        match self {
            Purpose::HouseholdForecast => "household-forecast".into(),
            Purpose::Scenario(id) => format!("scenario:{}", id.raw()),
            Purpose::Decision => "decision".into(),
            Purpose::Tax => "tax".into(),
            Purpose::FundingSearch => "funding-search".into(),
            Purpose::Extraction => "extraction".into(),
        }
    }

    pub fn from_tag(tag: &str) -> Option<Purpose> {
        match tag {
            "household-forecast" => Some(Purpose::HouseholdForecast),
            "decision" => Some(Purpose::Decision),
            "tax" => Some(Purpose::Tax),
            "funding-search" => Some(Purpose::FundingSearch),
            "extraction" => Some(Purpose::Extraction),
            other => other.strip_prefix("scenario:").and_then(|n| n.parse().ok()).map(|n| Purpose::Scenario(ScenarioId::new(n))),
        }
    }

    pub fn describe(self, household: &Household) -> String {
        match self {
            Purpose::HouseholdForecast => "household forecasts".into(),
            Purpose::Scenario(id) => household.scenario(id).map(|s| format!("scenario “{}”", s.name)).unwrap_or_else(|| format!("scenario {id}")),
            Purpose::Decision => "decisions and affordability".into(),
            Purpose::Tax => "tax calculations".into(),
            Purpose::FundingSearch => "funding searches".into(),
            Purpose::Extraction => "company-to-person extraction analysis".into(),
        }
    }

    /// The purpose a forecast serves, from its scenario option.
    pub fn of_forecast(scenario: Option<ScenarioId>) -> Purpose {
        match scenario {
            Some(id) => Purpose::Scenario(id),
            None => Purpose::HouseholdForecast,
        }
    }
}

/// §5.14 — who a grant is for.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Grantee {
    Person(PersonId),
    /// Everyone holding the household role.
    Role(crate::model::HouseholdRole),
}

impl Grantee {
    pub fn describe(&self, household: &Household) -> String {
        match self {
            Grantee::Person(id) => household.entity_name(EntityRef::Person(*id)),
            Grantee::Role(role) => format!("everyone with the role “{}”", role.label()),
        }
    }

    pub fn covers(&self, household: &Household, viewer: Viewer) -> bool {
        match self {
            Grantee::Person(id) => *id == viewer.person,
            Grantee::Role(role) => household.person(viewer.person).is_some_and(|p| p.role == *role),
        }
    }
}

/// §5.14 / §7.4 — a scoped grant: for one purpose, one grantee, an
/// effective range; it never widens what the object's owners can do and
/// never applies outside its purpose.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AccessGrant {
    pub id: GrantId,
    pub object: ObjectRef,
    pub grantee: Grantee,
    pub purpose: Purpose,
    /// How the object may appear to the grantee within the purpose.
    pub disclosure: Disclosure,
    /// Whether the object may contribute to calculations within the purpose.
    pub calculation: CalculationAccess,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
    pub granted_by: PersonId,
    pub granted_at: NaiveDateTime,
    pub revoked_on: Option<NaiveDate>,
    pub note: String,
}

impl AccessGrant {
    pub fn in_effect_on(&self, date: NaiveDate) -> bool {
        self.revoked_on.is_none_or(|r| date < r) && date >= self.effective_from && self.effective_to.is_none_or(|end| date <= end)
    }
}

/// §5.18 — what an audit record is about.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum AuditKind {
    PolicyChanged { from_version: u32, to_version: u32 },
    GrantAdded,
    GrantRevoked,
    AccessDenied,
    ViewerSwitched,
    SuppressionApplied,
}

impl AuditKind {
    pub fn label(&self) -> &'static str {
        match self {
            AuditKind::PolicyChanged { .. } => "policy changed",
            AuditKind::GrantAdded => "grant added",
            AuditKind::GrantRevoked => "grant revoked",
            AuditKind::AccessDenied => "access denied (fail-closed)",
            AuditKind::ViewerSwitched => "viewer switched",
            AuditKind::SuppressionApplied => "disclosure suppressed",
        }
    }
}

/// §5.18 — an immutable record of a material authorization event.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PrivacyAuditEvent {
    pub id: AuditId,
    pub at: NaiveDateTime,
    pub actor: PersonId,
    pub object: Option<ObjectRef>,
    pub kind: AuditKind,
    pub summary: String,
    pub policy_version: Option<u32>,
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
    /// §7.5 — every earlier version, so a historical calculation can be
    /// replayed under the policy that governed it (V071).
    #[serde(default)]
    pub previous_versions: Vec<AccessPolicy>,
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
            previous_versions: Vec::new(),
        }
    }

    /// One line per aspect, the §7.5 layout.
    pub fn describe(&self, household: &Household) -> Vec<String> {
        vec![
            format!("Existence: {}", self.existence.describe(household)),
            format!("Balance: {}", self.balance.describe(household)),
            format!("Transactions: {}", self.transactions.describe(household)),
            format!("Forecasts: {}", self.forecasts.describe(household)),
            format!("Assumptions: {}", self.assumptions.describe(household)),
            format!("Explanations: {}", self.provenance.describe(household)),
            format!("Use in calculations: {}", self.calculation_access.label()),
            format!("Disclosure of restricted contribution: {}", self.restricted_disclosure.label()),
            format!(
                "Purposes: {}",
                if self.purposes.is_empty() { "every purpose".to_string() } else { self.purposes.iter().map(|p| Purpose::from_tag(p).map(|p| p.describe(household)).unwrap_or_else(|| p.clone())).collect::<Vec<_>>().join(", ") }
            ),
        ]
    }

    /// Whether the policy allows the object to serve `purpose` at all (§7.4).
    pub fn allows_purpose(&self, purpose: Purpose) -> bool {
        self.purposes.is_empty() || self.purposes.contains(&purpose.tag())
    }

    /// The version of this policy in force on `date` (V071): the newest
    /// version whose effective date is not after `date`.
    pub fn version_on(&self, date: NaiveDate) -> Option<&AccessPolicy> {
        std::iter::once(self).chain(self.previous_versions.iter()).filter(|p| p.effective_from <= date).max_by_key(|p| p.version)
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

    /// M55 `A_i(c)` for a purpose (§7.4): the policy must allow the purpose,
    /// and grants in effect on `date` may add access within it. Missing
    /// policies fail closed.
    pub fn calculation_access_for_purpose(&self, object: ObjectRef, purpose: Purpose, date: NaiveDate) -> CalculationAccess {
        let governing = self.governing_object(object);
        let base = match self.policy_for(governing) {
            Some(policy) if policy.allows_purpose(purpose) => policy.calculation_access,
            Some(_) => CalculationAccess::Excluded,
            None => CalculationAccess::Excluded,
        };
        let granted = self
            .grants
            .iter()
            .filter(|g| g.object == governing && g.purpose == purpose && g.in_effect_on(date))
            .map(|g| g.calculation)
            .max_by_key(|a| access_rank(*a));
        match granted {
            Some(g) if access_rank(g) > access_rank(base) => g,
            _ => base,
        }
    }

    /// Disclosure for a viewer within a purpose: the object's policy, widened
    /// by any grant to the viewer for that purpose in effect on `date`.
    pub fn disclosure_for_purpose(&self, viewer: Viewer, object: ObjectRef, purpose: Purpose, date: NaiveDate) -> Disclosure {
        let base = self.disclosure_for(viewer, object);
        let governing = self.governing_object(object);
        self.grants
            .iter()
            .filter(|g| g.object == governing && g.purpose == purpose && g.in_effect_on(date) && g.grantee.covers(self, viewer))
            .map(|g| g.disclosure)
            .fold(base, |acc, d| acc.max(d))
    }

    /// The policy version that governed `object` on `date` (V071).
    pub fn policy_on(&self, object: ObjectRef, date: NaiveDate) -> Option<&AccessPolicy> {
        self.policy_for(self.governing_object(object)).and_then(|p| p.version_on(date))
    }

    pub fn next_policy_id(&self) -> PolicyId {
        PolicyId::new(self.policies.iter().map(|p| p.id.raw()).max().unwrap_or(0) + 1)
    }

    pub fn next_grant_id(&self) -> GrantId {
        GrantId::new(self.grants.iter().map(|g| g.id.raw()).max().unwrap_or(0) + 1)
    }

    fn next_audit_id(&self) -> AuditId {
        AuditId::new(self.audit.iter().map(|a| a.id.raw()).max().unwrap_or(0) + 1)
    }

    /// Appends an immutable audit record (§5.18).
    pub fn record_audit(&mut self, actor: PersonId, object: Option<ObjectRef>, kind: AuditKind, summary: impl Into<String>, policy_version: Option<u32>, at: NaiveDateTime) -> AuditId {
        let id = self.next_audit_id();
        let summary = summary.into();
        log::info!("audit {id}: {} by {actor} — {summary}", kind.label());
        self.audit.push(PrivacyAuditEvent { id, at, actor, object, kind, summary, policy_version });
        id
    }

    /// Whether `actor` may change the policy of `object`: an owner (full
    /// access) of the current policy, or the household owner when the
    /// object has none yet (F162: nobody else, ever).
    pub fn may_administer(&self, actor: PersonId, object: ObjectRef) -> bool {
        match self.policy_for(self.governing_object(object)) {
            Some(policy) => policy.full_access.contains(&actor),
            None => self.person(actor).is_some_and(|p| p.role == crate::model::HouseholdRole::Owner),
        }
    }

    /// §7.5 — replaces an object's policy with a new version; the old one is
    /// kept for replay, and the change is audited. Only an owner may do it.
    pub fn set_policy(&mut self, mut policy: AccessPolicy, actor: PersonId, at: NaiveDateTime) -> EngineResult<u32> {
        if !self.may_administer(actor, policy.object) {
            self.record_audit(actor, None, AuditKind::AccessDenied, "policy change refused: the actor is not an owner of the object", None, at);
            return Err(EngineError::Insufficient("only an owner of the object may change its access policy (F162)".into()));
        }
        if policy.full_access.is_empty() {
            return Err(EngineError::Insufficient("a policy needs at least one owner with full access".into()));
        }
        for p in &policy.purposes {
            if Purpose::from_tag(p).is_none() {
                return Err(EngineError::Insufficient(format!("unknown purpose “{p}”; purposes must be explicitly modelled contexts (§7.4)")));
            }
        }
        policy.changed_by = actor;
        policy.changed_at = at;
        let object = policy.object;
        match self.policies.iter_mut().find(|p| p.object == object) {
            Some(current) => {
                let mut old = current.clone();
                let from_version = old.version;
                old.previous_versions.clear();
                policy.id = current.id;
                policy.version = from_version + 1;
                policy.previous = Some(old.preset_label().to_string());
                policy.previous_versions = std::mem::take(&mut current.previous_versions);
                policy.previous_versions.push(old);
                let to_version = policy.version;
                *current = policy;
                let summary = format!("policy of {object} v{from_version} → v{to_version}: now “{}”, calculations {}, effective {}", self.policy_for(object).map(|p| p.preset_label()).unwrap_or("?"), self.policy_for(object).map(|p| p.calculation_access.label()).unwrap_or("?"), self.policy_for(object).map(|p| p.effective_from.to_string()).unwrap_or_default());
                self.record_audit(actor, Some(object), AuditKind::PolicyChanged { from_version, to_version }, summary, Some(to_version), at);
                Ok(to_version)
            }
            None => {
                policy.id = self.next_policy_id();
                policy.version = 1;
                policy.previous = None;
                policy.previous_versions.clear();
                self.policies.push(policy);
                self.record_audit(actor, Some(object), AuditKind::PolicyChanged { from_version: 0, to_version: 1 }, format!("first policy set on {object}"), Some(1), at);
                Ok(1)
            }
        }
    }

    /// §7.4 — adds a purpose-specific grant (owners only), audited.
    pub fn add_grant(&mut self, mut grant: AccessGrant, actor: PersonId, at: NaiveDateTime) -> EngineResult<GrantId> {
        if !self.may_administer(actor, grant.object) {
            self.record_audit(actor, None, AuditKind::AccessDenied, "grant refused: the actor is not an owner of the object", None, at);
            return Err(EngineError::Insufficient("only an owner of the object may grant access to it (F162)".into()));
        }
        if let Some(end) = grant.effective_to
            && end < grant.effective_from
        {
            return Err(EngineError::Insufficient("the grant ends before it starts".into()));
        }
        grant.id = self.next_grant_id();
        grant.granted_by = actor;
        grant.granted_at = at;
        let id = grant.id;
        let summary = format!("{} may use {} for {} as {} ({}), {} – {}", grant.grantee.describe(self), grant.object, grant.purpose.describe(self), grant.disclosure.label(), grant.calculation.label(), grant.effective_from, grant.effective_to.map(|d| d.to_string()).unwrap_or_else(|| "open".into()));
        let object = grant.object;
        self.grants.push(grant);
        self.record_audit(actor, Some(object), AuditKind::GrantAdded, summary, None, at);
        Ok(id)
    }

    /// Revokes a grant from `on` (the record stays for replay), audited.
    pub fn revoke_grant(&mut self, id: GrantId, actor: PersonId, on: NaiveDate, at: NaiveDateTime) -> EngineResult<()> {
        let object = self.grants.iter().find(|g| g.id == id).map(|g| g.object).ok_or(EngineError::Insufficient(format!("unknown grant {id}")))?;
        if !self.may_administer(actor, object) {
            return Err(EngineError::Insufficient("only an owner of the object may revoke a grant on it (F162)".into()));
        }
        if let Some(grant) = self.grants.iter_mut().find(|g| g.id == id) {
            grant.revoked_on = Some(on);
        }
        self.record_audit(actor, Some(object), AuditKind::GrantRevoked, format!("grant {id} on {object} revoked from {on}"), None, at);
        Ok(())
    }

    /// V077 — why a viewer cannot see an object, without exposing it: only
    /// the object's kind, the policy version and who may change it.
    pub fn explain_denial(&self, viewer: Viewer, object: ObjectRef) -> String {
        let kind = object_kind(object);
        match self.policy_for(self.governing_object(object)) {
            None => format!("This {kind} has no access policy, so access fails closed (F162): nobody but a household owner can see or use it until a policy is set."),
            Some(policy) => {
                let level = policy.disclosure(viewer);
                match level {
                    Disclosure::Hidden | Disclosure::Aggregate => format!(
                        "You are not authorized to see this {kind}. Its policy (version {}, effective {}) discloses its existence to: {}. An owner can change the policy or add a purpose-specific grant.",
                        policy.version,
                        policy.effective_from.format("%d %b %Y"),
                        policy.existence.describe(self)
                    ),
                    other => format!("You see this {kind} as “{}” under its policy version {}; the details you cannot see stay with its owners.", other.label(), policy.version),
                }
            }
        }
    }

    /// V077 — policy problems that make the engine fail closed: objects with
    /// no governing policy, duplicate policies, policies whose owners are not
    /// household people, and grants that reference unknown objects.
    pub fn authorization_problems(&self) -> Vec<AuthorizationProblem> {
        let mut problems = Vec::new();
        let objects: Vec<ObjectRef> = self
            .accounts
            .iter()
            .map(|a| ObjectRef::Account(a.id))
            .chain(self.companies.iter().map(|c| ObjectRef::Company(c.id)))
            .chain(self.scenarios.iter().map(|s| ObjectRef::Scenario(s.id)))
            .chain(self.people.iter().map(|p| ObjectRef::Person(p.id)))
            .collect();
        for object in objects {
            let count = self.policies.iter().filter(|p| p.object == object).count();
            match count {
                0 => problems.push(AuthorizationProblem { object, text: format!("{} has no access policy: hidden from everyone but a household owner and excluded from calculations (fails closed, F162)", object_kind(object)) }),
                1 => {
                    let policy = self.policy_for(object).expect("counted");
                    if policy.full_access.iter().any(|p| self.person(*p).is_none()) {
                        problems.push(AuthorizationProblem { object, text: format!("the policy of this {} names an owner who is not a household person; treated as fail-closed for that owner", object_kind(object)) });
                    }
                }
                n => problems.push(AuthorizationProblem { object, text: format!("{n} conflicting policies attached to the same {}; the engine uses the first and fails closed on the rest — merge them", object_kind(object)) }),
            }
        }
        for grant in &self.grants {
            if self.policy_for(grant.object).is_none() {
                problems.push(AuthorizationProblem { object: grant.object, text: format!("grant {} references a {} without a policy; the grant is ignored until a policy exists", grant.id, object_kind(grant.object)) });
            }
        }
        problems
    }
}

/// V077 — one fail-closed situation, named by object kind, never by identity.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AuthorizationProblem {
    pub object: ObjectRef,
    pub text: String,
}

fn object_kind(object: ObjectRef) -> &'static str {
    match object {
        ObjectRef::Person(_) => "person",
        ObjectRef::Company(_) => "company",
        ObjectRef::Account(_) => "account",
        ObjectRef::Reservation(_) => "reservation",
        ObjectRef::Series(_) => "event series",
        ObjectRef::Occurrence(_) => "event",
        ObjectRef::Assumption(_) => "assumption",
        ObjectRef::Scenario(_) => "scenario",
        ObjectRef::Goal(_) => "goal",
    }
}

fn access_rank(access: CalculationAccess) -> u8 {
    match access {
        CalculationAccess::Excluded => 0,
        CalculationAccess::RestrictedContribution => 1,
        CalculationAccess::Full => 2,
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

    fn at(day: NaiveDate) -> NaiveDateTime {
        day.and_hms_opt(12, 0, 0).unwrap()
    }

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn v061_visibility_changes_never_change_ownership_or_postings() {
        use crate::fixtures::{self, ids};
        use crate::forecast::{Case, ForecastOptions, forecast};
        use crate::liquidity::{Boundary, person_attribution};
        let mut household = fixtures::plan_household();
        let options = ForecastOptions { through: fixtures::default_horizon(), scenario: None, case: Case::Expected };
        let before = forecast(&household, Boundary::Household, options).unwrap();
        let before_b = forecast(&household, Boundary::Person(ids::PERSON_B), options).unwrap();
        let attribution_a = person_attribution(&household, ids::PERSON_A).unwrap();
        let attribution_b = person_attribution(&household, ids::PERSON_B).unwrap();
        // Person B makes their checking account private (still usable in calculations).
        let mut policy = household.policy_for(ObjectRef::Account(ids::PERSON_B_CHECKING)).unwrap().clone();
        policy.existence = Grantees::OwnersOnly;
        policy.balance = Grantees::OwnersOnly;
        policy.transactions = Grantees::OwnersOnly;
        policy.provenance = Grantees::OwnersOnly;
        policy.calculation_access = CalculationAccess::RestrictedContribution;
        household.set_policy(policy, ids::PERSON_B, at(d(2026, 9, 11))).unwrap();
        let after = forecast(&household, Boundary::Household, options).unwrap();
        let after_b = forecast(&household, Boundary::Person(ids::PERSON_B), options).unwrap();
        assert_eq!(after.path, before.path, "postings and the household path are unchanged");
        assert_eq!(after.end.money(), before.end.money());
        assert_eq!(after_b.end.money(), before_b.end.money(), "Person B's economic share is unchanged");
        assert_eq!(person_attribution(&household, ids::PERSON_A).unwrap().money(), attribution_a.money());
        assert_eq!(person_attribution(&household, ids::PERSON_B).unwrap().money(), attribution_b.money());
        assert_eq!(household.account(ids::SHARED_SAVINGS).unwrap().holder.share_of(ids::PERSON_B), 5_000, "joint 50/50 ownership is untouched and counted once");
        // What changed is disclosure only: A no longer sees B's account, B still does.
        assert_eq!(household.disclosure_for(Viewer::person(ids::PERSON_A), ObjectRef::Account(ids::PERSON_B_CHECKING)), Disclosure::Aggregate);
        assert_eq!(household.disclosure_for(Viewer::person(ids::PERSON_B), ObjectRef::Account(ids::PERSON_B_CHECKING)), Disclosure::Full);
    }

    #[test]
    fn v062_v063_v064_private_accounts_and_authorized_contributions() {
        use crate::fixtures::{self, ids};
        use crate::forecast::{Case, ForecastOptions, forecast};
        use crate::liquidity::Boundary;
        use crate::model::*;
        let mut household = fixtures::plan_household();
        // V062: a fully private, excluded account of Person A.
        let secret = household.next_account_id();
        let mut account = household.account(ids::PERSON_B_CHECKING).unwrap().clone();
        account.id = secret;
        account.name = "Person A secret savings".into();
        account.institution = "Bank Z".into();
        account.holder = Holder::Persons(vec![OwnershipShare { person: ids::PERSON_A, basis_points: 10_000 }]);
        account.settled_balance = fixtures::pkr(777_000);
        household.add_account(account, VisibilityPreset::Private, CalculationAccess::Excluded).unwrap();
        let b = Viewer::person(ids::PERSON_B);
        assert_eq!(household.disclosure_for(b, ObjectRef::Account(secret)), Disclosure::Hidden, "not discoverable");
        let options = ForecastOptions { through: fixtures::default_horizon(), scenario: None, case: Case::Expected };
        let result = forecast(&household, Boundary::Household, options).unwrap();
        assert!(!result.accounts.iter().any(|a| a.account == secret), "not a calculation input");
        assert!(result.record.excluded_accounts.iter().any(|(id, why)| *id == secret && why.contains("access policy")));
        let projected = result.end.node().project(&household.disclosure_fn(b));
        let text = projected.render_chain();
        assert!(!text.contains("secret savings") && !text.contains("Bank Z") && !text.contains("777,000"));

        // V063: private but usable — contributes without revealing institution, id, balance or transactions.
        let liquid_before = crate::liquidity::household_liquidity(&household).unwrap().liquid_cash;
        let mut policy = household.policy_for(ObjectRef::Account(secret)).unwrap().clone();
        policy.calculation_access = CalculationAccess::RestrictedContribution;
        policy.forecasts = Grantees::Household;
        household.set_policy(policy, ids::PERSON_A, at(d(2026, 9, 11))).unwrap();
        let with = forecast(&household, Boundary::Household, options).unwrap();
        assert_eq!(with.start.money(), result.start.money().checked_add(fixtures::pkr(777_000)).unwrap(), "the contribution counts");
        let liquid = crate::liquidity::household_liquidity(&household).unwrap().liquid_cash;
        assert_eq!(liquid.money(), liquid_before.money().checked_add(fixtures::pkr(777_000)).unwrap());
        let projected = liquid.node().project(&household.disclosure_fn(b));
        let text = projected.render_chain();
        assert!(!text.contains("secret savings") && !text.contains("Bank Z") && !text.contains(&secret.to_string()));
        assert_eq!(household.disclosure_for(b, ObjectRef::Account(secret)), Disclosure::Aggregate);

        // V064: the owner inspects complete provenance; B receives only the authorized aggregate explanation.
        let owner_text = liquid.node().project(&household.disclosure_fn(Viewer::person(ids::PERSON_A))).render_chain();
        assert!(owner_text.contains("secret savings"), "{owner_text}");
        assert!(owner_text.contains("777,000"));
        assert!(text.contains("restricted") || text.contains("suppressed"));
        assert!(projected.verify_sums().is_empty(), "the aggregate explanation still adds up");
    }

    #[test]
    fn v067_purpose_scoped_access_and_grants() {
        use crate::decision::{FundingSource, PurchasePlan, default_plan, funding_strategies};
        use crate::fixtures::{self, ids};
        use crate::forecast::{Case, ForecastOptions, forecast};
        use crate::liquidity::Boundary;
        let mut household = fixtures::plan_household();
        let object = ObjectRef::Account(ids::PERSON_B_CHECKING);
        let buy_home = household.next_scenario_id();
        household.add_scenario(crate::model::Scenario { id: buy_home, name: "Buy home".into(), description: String::new(), private_to: None, changes: Vec::new(), composed_of: Vec::new() }, ids::PERSON_A);
        let mut policy = household.policy_for(object).unwrap().clone();
        policy.purposes = vec![Purpose::Scenario(buy_home).tag()];
        household.set_policy(policy, ids::PERSON_B, at(d(2026, 9, 11))).unwrap();
        let today = household.as_of;
        assert_eq!(household.calculation_access_for_purpose(object, Purpose::Scenario(buy_home), today), CalculationAccess::Full);
        assert_eq!(household.calculation_access_for_purpose(object, Purpose::Scenario(ids::BUY_CAR), today), CalculationAccess::Excluded, "unavailable to Buy Car");
        assert_eq!(household.calculation_access_for_purpose(object, Purpose::HouseholdForecast, today), CalculationAccess::Excluded, "unavailable to the baseline");
        assert_eq!(household.calculation_access_for_purpose(object, Purpose::FundingSearch, today), CalculationAccess::Excluded, "unavailable to funding searches");
        // The baseline and Buy car forecasts exclude it with the purpose reason; Buy home includes it.
        let baseline = forecast(&household, Boundary::Household, ForecastOptions { through: fixtures::default_horizon(), scenario: None, case: Case::Expected }).unwrap();
        assert!(baseline.record.excluded_accounts.iter().any(|(id, why)| *id == ids::PERSON_B_CHECKING && why.contains("purpose")));
        let car = forecast(&household, Boundary::Household, ForecastOptions { through: fixtures::default_horizon(), scenario: Some(ids::BUY_CAR), case: Case::Expected }).unwrap();
        assert!(car.record.excluded_accounts.iter().any(|(id, _)| *id == ids::PERSON_B_CHECKING));
        let home = forecast(&household, Boundary::Household, ForecastOptions { through: fixtures::default_horizon(), scenario: Some(buy_home), case: Case::Expected }).unwrap();
        assert!(!home.record.excluded_accounts.iter().any(|(id, _)| *id == ids::PERSON_B_CHECKING));
        // A funding search cannot use it either, even when listed as a source.
        let mut plan: PurchasePlan = default_plan(&household, household.as_of);
        plan.sources = vec![FundingSource { account: ids::PERSON_B_CHECKING, allowed: true, floor: None }];
        plan.company_routes.clear();
        let report = funding_strategies(&household, &plan).unwrap();
        assert!(report.strategies.iter().all(|s| !s.steps.iter().any(|st| st.account == ids::PERSON_B_CHECKING)), "purpose-scoped account is not a funding source");
        // A grant for funding searches re-enables it for that purpose only.
        let grant = AccessGrant {
            id: GrantId::new(0),
            object,
            grantee: Grantee::Person(ids::PERSON_A),
            purpose: Purpose::FundingSearch,
            disclosure: Disclosure::Aggregate,
            calculation: CalculationAccess::Full,
            effective_from: today,
            effective_to: None,
            granted_by: ids::PERSON_B,
            granted_at: at(today),
            revoked_on: None,
            note: String::new(),
        };
        household.add_grant(grant, ids::PERSON_B, at(today)).unwrap();
        assert_eq!(household.calculation_access_for_purpose(object, Purpose::FundingSearch, today), CalculationAccess::Full);
        assert_eq!(household.calculation_access_for_purpose(object, Purpose::HouseholdForecast, today), CalculationAccess::Excluded);
        assert_eq!(household.calculation_access_for_purpose(object, Purpose::FundingSearch, today.pred_opt().unwrap()), CalculationAccess::Excluded, "not before its effective date");
        assert!(household.audit.iter().any(|e| matches!(e.kind, AuditKind::GrantAdded)));
    }

    #[test]
    fn v071_effective_dated_versions_are_replayable_and_audited() {
        use crate::fixtures::{self, ids};
        use crate::forecast::{Case, ForecastOptions, forecast};
        use crate::liquidity::Boundary;
        let mut household = fixtures::plan_household();
        let object = ObjectRef::Account(ids::PERSON_B_CHECKING);
        let v1 = household.policy_for(object).unwrap().clone();
        assert_eq!(v1.version, 1);
        let mut v2 = v1.clone();
        v2.transactions = Grantees::OwnersOnly;
        v2.effective_from = d(2026, 10, 1);
        let version = household.set_policy(v2, ids::PERSON_B, at(d(2026, 9, 11))).unwrap();
        assert_eq!(version, 2);
        let current = household.policy_for(object).unwrap();
        assert_eq!(current.version, 2);
        assert_eq!(current.previous_versions.len(), 1);
        assert_eq!(current.previous.as_deref(), Some("Fully shared"));
        // Replay: on 15 Sep the v1 policy governs; from 1 Oct v2 does.
        assert_eq!(household.policy_on(object, d(2026, 9, 15)).unwrap().version, 1);
        assert_eq!(household.policy_on(object, d(2026, 10, 1)).unwrap().version, 2);
        assert_eq!(household.policy_on(object, d(2026, 12, 1)).unwrap().transactions, Grantees::OwnersOnly);
        // The forecast record pins the versions in force.
        let result = forecast(&household, Boundary::Household, ForecastOptions { through: fixtures::default_horizon(), scenario: None, case: Case::Expected }).unwrap();
        assert!(result.record.policy_versions.iter().any(|(id, v)| *id == current.id && *v == 2));
        // Audited, with the version.
        let event = household.audit.iter().rev().find(|e| matches!(e.kind, AuditKind::PolicyChanged { from_version: 1, to_version: 2 })).unwrap();
        assert_eq!(event.policy_version, Some(2));
        assert_eq!(event.actor, ids::PERSON_B);
        assert_eq!(event.object, Some(object));
    }

    #[test]
    fn v077_missing_conflicting_or_unauthorized_policy_fails_closed() {
        use crate::fixtures::{self, ids};
        let mut household = fixtures::plan_household();
        assert!(household.authorization_problems().is_empty(), "the fixture is fully policed");
        // Missing policy: hidden and excluded for everyone, explained without exposing the object.
        let orphan = household.next_account_id();
        let mut account = household.account(ids::PERSON_B_CHECKING).unwrap().clone();
        account.id = orphan;
        account.name = "Orphan account at Bank Q".into();
        household.accounts.push(account);
        let b = Viewer::person(ids::PERSON_B);
        assert_eq!(household.disclosure_for(b, ObjectRef::Account(orphan)), Disclosure::Hidden);
        assert_eq!(household.calculation_access_for(ObjectRef::Account(orphan)), CalculationAccess::Excluded);
        let why = household.explain_denial(b, ObjectRef::Account(orphan));
        assert!(why.contains("fails closed"));
        assert!(!why.contains("Orphan") && !why.contains("Bank Q"), "the explanation names the kind, never the object");
        let problems = household.authorization_problems();
        assert_eq!(problems.len(), 1);
        assert!(problems[0].text.contains("no access policy"));
        // Conflicting policies on one object are reported.
        let duplicate = household.policy_for(ObjectRef::Account(ids::PERSON_B_CHECKING)).unwrap().clone();
        household.policies.push(AccessPolicy { id: household.next_policy_id(), ..duplicate });
        assert!(household.authorization_problems().iter().any(|p| p.text.contains("conflicting")));
        household.policies.pop();
        // A non-owner cannot change a policy; the refusal is audited and explained.
        let mut policy = household.policy_for(ObjectRef::Account(ids::PERSON_A_CURRENT)).unwrap().clone();
        policy.existence = Grantees::Household;
        let err = household.set_policy(policy, ids::PERSON_B, at(d(2026, 9, 11))).unwrap_err();
        assert!(err.to_string().contains("owner"));
        assert!(household.audit.iter().any(|e| matches!(e.kind, AuditKind::AccessDenied)));
        assert_eq!(household.policy_for(ObjectRef::Account(ids::PERSON_A_CURRENT)).unwrap().version, 2, "unchanged");
        // Denial text for a hidden object names its kind and policy version only.
        let why = household.explain_denial(b, ObjectRef::Account(ids::PERSON_A_CURRENT));
        assert!(why.contains("account") && why.contains("version 2"));
        assert!(!why.contains("Person A current"));
        // An unknown purpose tag is refused.
        let mut policy = household.policy_for(ObjectRef::Account(ids::SHARED_SAVINGS)).unwrap().clone();
        policy.purposes = vec!["whatever".into()];
        assert!(household.set_policy(policy, ids::PERSON_A, at(d(2026, 9, 11))).unwrap_err().to_string().contains("unknown purpose"));
    }
}
