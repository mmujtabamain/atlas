//! Privacy & authorization: the policies the viewer may see (their own
//! objects in full; others' only where existence is shared), purpose-specific
//! grants, the immutable audit log, fail-closed problems and how a denial is
//! explained without exposing the object. Ownership, visibility, calculation
//! access and disclosure are four separate things.

use atlas_core::authz::{AccessGrant, AccessPolicy, AuthorizationProblem, PrivacyAuditEvent, Viewer};
use atlas_core::ids::ObjectRef;
use atlas_core::model::Household;
use atlas_core::Disclosure;


#[derive(Clone, Debug)]
pub struct PolicyRow {
    pub policy: AccessPolicy,
    /// The object's name when the viewer may know it; otherwise its kind only.
    pub object_name: String,
    pub owned: bool,
    pub viewer_disclosure: Disclosure,
    pub lines: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct GrantRow {
    pub grant: AccessGrant,
    pub object_name: String,
    pub grantee: String,
    pub purpose: String,
    pub owned: bool,
    pub in_effect: bool,
}

#[derive(Clone, Debug)]
pub struct AuditRow {
    pub event: PrivacyAuditEvent,
    pub actor: String,
    pub object_name: String,
}

#[derive(Clone, Debug)]
pub struct PrivacyModel {
    pub policies: Vec<PolicyRow>,
    pub hidden_policies: usize,
    pub grants: Vec<GrantRow>,
    pub audit: Vec<AuditRow>,
    pub problems: Vec<AuthorizationProblem>,
    pub denial_example: Option<String>,
    pub viewer_is_owner_of: usize,
}

impl PrivacyModel {
    pub fn compute(household: &Household, viewer: Viewer) -> Self {
        let name_of = |object: ObjectRef| -> Option<String> {
            match object {
                ObjectRef::Account(id) => household.account(id).map(|a| format!("Account: {}", a.name)),
                ObjectRef::Company(id) => household.company(id).map(|c| format!("Company: {}", c.name)),
                ObjectRef::Scenario(id) => household.scenario(id).map(|s| format!("Scenario: {}", s.name)),
                ObjectRef::Person(id) => household.person(id).map(|p| format!("Person: {}", p.name)),
                ObjectRef::Goal(id) => household.goals.iter().find(|g| g.id == id).map(|g| format!("Goal: {}", g.name)),
                other => Some(other.to_string()),
            }
        };
        let kind_of = |object: ObjectRef| -> &'static str {
            match object {
                ObjectRef::Account(_) => "a private account",
                ObjectRef::Company(_) => "a private company",
                ObjectRef::Scenario(_) => "a private scenario",
                ObjectRef::Person(_) => "a person",
                ObjectRef::Goal(_) => "a private goal",
                _ => "a private object",
            }
        };
        let mut policies = Vec::new();
        let mut hidden_policies = 0;
        let mut viewer_is_owner_of = 0;
        for policy in &household.policies {
            let disclosure = policy.disclosure(viewer);
            let owned = policy.full_access.contains(&viewer.person);
            if owned {
                viewer_is_owner_of += 1;
            }
            // An object whose existence the viewer may not learn is not listed at all —
            // an aggregate-only contribution shows up inside calculations, never as a row here.
            if matches!(disclosure, Disclosure::Hidden | Disclosure::Aggregate) {
                hidden_policies += 1;
                continue;
            }
            policies.push(PolicyRow {
                policy: policy.clone(),
                object_name: name_of(policy.object).unwrap_or_else(|| kind_of(policy.object).into()),
                owned,
                viewer_disclosure: disclosure,
                lines: policy.describe(household),
            });
        }
        let grants = household
            .grants
            .iter()
            .filter(|g| !matches!(household.disclosure_for(viewer, g.object), Disclosure::Hidden | Disclosure::Aggregate) || g.grantee.covers(household, viewer))
            .map(|g| GrantRow {
                grant: g.clone(),
                object_name: if matches!(household.disclosure_for(viewer, g.object), Disclosure::Hidden | Disclosure::Aggregate) { kind_of(g.object).into() } else { name_of(g.object).unwrap_or_else(|| g.object.to_string()) },
                grantee: g.grantee.describe(household),
                purpose: g.purpose.describe(household),
                owned: household.may_administer(viewer.person, g.object),
                in_effect: g.in_effect_on(household.as_of),
            })
            .collect();
        let audit = household
            .audit
            .iter()
            .rev()
            .map(|e| AuditRow {
                event: e.clone(),
                actor: household.entity_name(atlas_core::ids::EntityRef::Person(e.actor)),
                object_name: match e.object {
                    None => "—".into(),
                    Some(object) if matches!(household.disclosure_for(viewer, object), Disclosure::Hidden | Disclosure::Aggregate) => kind_of(object).into(),
                    Some(object) => name_of(object).unwrap_or_else(|| object.to_string()),
                },
            })
            .collect();
        let problems = household.authorization_problems().into_iter().filter(|p| household.may_administer(viewer.person, p.object) || household.person(viewer.person).is_some_and(|p| p.role == atlas_core::model::HouseholdRole::Owner)).collect();
        let denial_example = household
            .policies
            .iter()
            .find(|p| matches!(p.disclosure(viewer), Disclosure::Hidden | Disclosure::Aggregate))
            .map(|p| household.explain_denial(viewer, p.object));
        log::info!("privacy screen for {}: {} policies visible ({hidden_policies} hidden), {} grants, {} audit events", viewer.person, policies.len(), household.grants.len(), household.audit.len());
        PrivacyModel { policies, hidden_policies, grants, audit, problems, denial_example, viewer_is_owner_of }
    }
}
