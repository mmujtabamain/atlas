//! Privacy & authorization (§7, §5.13–§5.18, M10): the policies the viewer may
//! see (their own objects in full; others' only where existence is shared),
//! purpose-specific grants, the immutable audit log, fail-closed problems and
//! how a denial is explained without exposing the object. Ownership,
//! visibility, calculation access and disclosure are four separate things.

use atlas_core::authz::{AccessGrant, AccessPolicy, AuthorizationProblem, PrivacyAuditEvent, Purpose, Viewer};
use atlas_core::ids::ObjectRef;
use atlas_core::model::Household;
use atlas_core::Disclosure;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _,
    alert::Alert,
    button::{Button, ButtonVariants as _},
    group_box::GroupBox, h_flex,
    table::{Table, TableBody, TableCell, TableHead, TableHeader, TableRow},
    tag::Tag, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::AtlasApp;
use crate::widgets::labels;
use crate::widgets::master::page_header;
use crate::widgets::table::muted_cell;

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
            // V062: an object whose existence the viewer may not learn is not listed at all —
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

pub fn render(model: &PrivacyModel, household: &Household, viewer_name: &str, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    v_flex()
        .id("screen-privacy")
        .test_support()
        .w_full()
        .gap_6()
        .child(page_header(
            "Privacy & authorization",
            "Ownership, visibility, calculation access and disclosure are four separate things (§7.1). Every object carries a versioned, effective-dated policy; missing policies fail closed (F162). What you see here is filtered through your own policies too.",
            cx,
        ))
        .child(
            Alert::info("privacy-caveat", format!("Viewing as {viewer_name}. Objects whose existence you may not learn are not listed — not even as a count on their own row. Restricted contributions appear in explanations only as authorized aggregates, and a lone restricted term is suppressed rather than exposed as a difference (§7.6)."))
                .title("Fail-closed by design"),
        )
        .child(render_policies(model, household, cx))
        .child(render_grants(model, cx))
        .child(render_problems(model, cx))
        .child(render_audit(model, cx))
}

fn render_policies(model: &PrivacyModel, household: &Household, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    GroupBox::new().id("privacy-policies").title(format!("Access policies (§7.5) — {} visible to you, {} of them yours{}", model.policies.len(), model.viewer_is_owner_of, if model.hidden_policies > 0 { format!("; {} other objects are private to someone else", model.hidden_policies) } else { String::new() })).child(
        v_flex()
            .gap_4()
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_start()
                    .gap_4()
                    .child(div().flex_1().min_w_0().text_xs().text_color(theme.muted_foreground).child(
                        "Each policy lists who may learn the object exists, see its balance, its transactions, forecasts built on it, its assumptions and its explanations; whether it may be used in calculations; how a restricted contribution is disclosed; which purposes it serves; and its version history. Only owners change a policy — every change is a new version and an audit event.",
                    ))
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .gap_2()
                            .child(Button::new("privacy-edit-policy").small().outline().icon(IconName::Pencil).label("Set policy…").on_click(cx.listener(|this, _, window, cx| this.open_policy_editor(window, cx))))
                            .child(Button::new("privacy-add-grant").small().outline().icon(IconName::Plus).label("Purpose grant…").on_click(cx.listener(|this, _, window, cx| this.open_grant_dialog(window, cx)))),
                    ),
            )
            .children(model.policies.iter().enumerate().map(|(index, row)| {
                let p = &row.policy;
                v_flex()
                    .id(ElementId::Name(format!("policy-row-{}", p.id.raw()).into()))
                    .test_support()
                    .gap_1()
                    .px_3()
                    .py_2()
                    .rounded(theme.radius)
                    .when(index % 2 == 1, |c| c.bg(theme.table_even))
                    // Layout for taffy's sake (see perf.rs): the name and tags are one
                    // definite-width row, the provenance sentence and the policy lines
                    // are full-width rows. The old wrap row of sentences and tags cost
                    // ~2,000 measure callbacks per policy per frame; this costs ~130.
                    .child(
                        h_flex()
                            .w_full()
                            .gap_2()
                            .items_center()
                            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(row.object_name.clone()))
                            .child(Tag::secondary().xsmall().outline().child(p.preset_label()))
                            .child(Tag::secondary().xsmall().outline().child(format!("v{}", p.version)))
                            .child(if row.owned { Tag::secondary().xsmall().child("you own this") } else { labels::disclosure_tag(row.viewer_disclosure) }),
                    )
                    .child(div().w_full().text_xs().text_color(theme.muted_foreground).child(format!(
                        "{} · owners: {} · effective {} · changed by {} at {}{}",
                        p.id,
                        p.full_access.iter().map(|id| household.entity_name(atlas_core::ids::EntityRef::Person(*id))).collect::<Vec<_>>().join(", "),
                        p.effective_from.format("%d %b %Y"),
                        household.entity_name(atlas_core::ids::EntityRef::Person(p.changed_by)),
                        p.changed_at.format("%d %b %Y %H:%M"),
                        p.previous.as_ref().map(|prev| format!(" · previous: {prev}")).unwrap_or_default()
                    )))
                    .when(row.owned || matches!(row.viewer_disclosure, Disclosure::Full), |this| {
                        // One wrapping sentence, not a wrap row of nine chips: same
                        // density on screen, one text node for taffy instead of nine.
                        this.child(div().w_full().text_xs().text_color(theme.muted_foreground).child(row.lines.join(" · ")))
                    })
                    .when(!p.previous_versions.is_empty(), |this| {
                        this.child(div().text_xs().text_color(theme.muted_foreground).child(format!(
                            "history: {}",
                            p.previous_versions.iter().map(|v| format!("v{} “{}” effective {} (changed {})", v.version, v.preset_label(), v.effective_from.format("%d %b %Y"), v.changed_at.format("%d %b %Y"))).collect::<Vec<_>>().join("; ")
                        )))
                    })
            })),
    )
}

fn render_grants(model: &PrivacyModel, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    GroupBox::new().id("privacy-grants").title(format!("Purpose-specific grants (§7.4) — {}", model.grants.len())).child(
        v_flex()
            .gap_3()
            .child(div().text_xs().text_color(theme.muted_foreground).child(
                "A grant lets a restricted resource serve one explicitly modelled purpose — household forecasts, one scenario, decisions, funding searches, tax — for one person or role, between two dates, without sharing it globally. An account authorized only for “Buy home” stays unavailable to “Buy car”, the baseline and unrelated funding searches (V067).",
            ))
            .child(if model.grants.is_empty() {
                div().text_sm().text_color(theme.muted_foreground).child("No grants.").into_any_element()
            } else {
                Table::new()
                    .child(
                        TableHeader::new().child(
                            TableRow::new()
                                .child(TableHead::new().w_20().flex_shrink_0().child("Grant"))
                                .child(TableHead::new().w_64().flex_shrink_0().child("Object"))
                                .child(TableHead::new().w_40().flex_shrink_0().child("Grantee"))
                                .child(TableHead::new().w_56().flex_shrink_0().child("Purpose"))
                                .child(TableHead::new().w_40().flex_shrink_0().child("Disclosure · use"))
                                .child(TableHead::new().w_56().flex_shrink_0().child("Effective"))
                                .child(TableHead::new().min_w_0().child("Status")),
                        ),
                    )
                    .child(TableBody::new().children(model.grants.iter().enumerate().map(|(index, row)| {
                        let g = &row.grant;
                        let id = g.id;
                        TableRow::new()
                            .when(index % 2 == 1, |r| r.bg(theme.table_even))
                            .child(muted_cell(g.id.to_string(), cx).w_20().flex_shrink_0())
                            .child(TableCell::new().w_64().flex_shrink_0().overflow_hidden().text_ellipsis().child(row.object_name.clone()))
                            .child(TableCell::new().w_40().flex_shrink_0().overflow_hidden().text_ellipsis().child(row.grantee.clone()))
                            .child(TableCell::new().w_56().flex_shrink_0().overflow_hidden().text_ellipsis().child(row.purpose.clone()))
                            .child(muted_cell(format!("{} · {}", g.disclosure.label(), g.calculation.label()), cx).w_40().flex_shrink_0().overflow_hidden().text_ellipsis())
                            .child(muted_cell(format!("{} – {}", g.effective_from.format("%d %b %Y"), g.effective_to.map(|d| d.format("%d %b %Y").to_string()).unwrap_or_else(|| "open".into())), cx).w_56().flex_shrink_0())
                            .child(TableCell::new().min_w_0().child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(match g.revoked_on {
                                        Some(on) => Tag::warning().xsmall().outline().child(format!("revoked from {}", on.format("%d %b %Y"))),
                                        None if row.in_effect => Tag::secondary().xsmall().outline().child("in effect today"),
                                        None => Tag::secondary().xsmall().outline().child("not in effect today"),
                                    })
                                    .when(row.owned && g.revoked_on.is_none(), |c| {
                                        c.child(Button::new(ElementId::Name(format!("grant-revoke-{}", id.raw()).into())).xsmall().ghost().label("Revoke").on_click(cx.listener(move |this, _, window, cx| this.revoke_grant(id, window, cx))))
                                    }),
                            ))
                    })))
                    .into_any_element()
            }),
    )
}

fn render_problems(model: &PrivacyModel, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    GroupBox::new().id("privacy-problems").title("Fail-closed checks (F162, V077)").child(
        v_flex()
            .gap_2()
            .child(div().text_xs().text_color(theme.muted_foreground).child("Objects without a policy, conflicting policies and dangling grants are listed here for owners. The engine never guesses: such objects are hidden and excluded until an owner fixes the policy."))
            .child(if model.problems.is_empty() {
                div().id("privacy-no-problems").test_support().text_sm().child("Every object has exactly one policy with household owners; no grant dangles.").into_any_element()
            } else {
                v_flex().gap_1().children(model.problems.iter().map(|p| h_flex().gap_2().items_start().child(Tag::danger().xsmall().outline().child("fails closed")).child(div().text_sm().child(p.text.clone())))).into_any_element()
            })
            .when_some(model.denial_example.clone(), |this, example| {
                this.child(div().text_xs().text_color(theme.muted_foreground).child("How a denial is explained to you, without exposing the object:")).child(div().id("privacy-denial-example").test_support().text_sm().child(example))
            }),
    )
}

fn render_audit(model: &PrivacyModel, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    GroupBox::new().id("privacy-audit").title(format!("Privacy audit log (§5.18) — {} events, newest first", model.audit.len())).child(
        v_flex()
            .gap_3()
            .child(div().text_xs().text_color(theme.muted_foreground).child("Immutable: policy changes with their versions, grants, revocations, refused changes, viewer switches and applied suppressions. Objects you may not see appear by kind only."))
            .child(if model.audit.is_empty() {
                div().text_sm().text_color(theme.muted_foreground).child("No events yet.").into_any_element()
            } else {
                Table::new()
                    .child(
                        TableHeader::new().child(
                            TableRow::new()
                                .child(TableHead::new().w_40().flex_shrink_0().child("When"))
                                .child(TableHead::new().w_32().flex_shrink_0().child("Who"))
                                .child(TableHead::new().w_40().flex_shrink_0().child("What"))
                                .child(TableHead::new().w_56().flex_shrink_0().child("Object"))
                                .child(TableHead::new().min_w_0().child("Summary"))
                                .child(TableHead::new().w_20().flex_shrink_0().text_right().child("Version")),
                        ),
                    )
                    .child(TableBody::new().children(model.audit.iter().enumerate().map(|(index, row)| {
                        let e = &row.event;
                        TableRow::new()
                            .when(index % 2 == 1, |r| r.bg(theme.table_even))
                            .child(muted_cell(e.at.format("%d %b %Y %H:%M").to_string(), cx).w_40().flex_shrink_0())
                            .child(TableCell::new().w_32().flex_shrink_0().overflow_hidden().text_ellipsis().child(row.actor.clone()))
                            .child(TableCell::new().w_40().flex_shrink_0().child(h_flex().child(Tag::secondary().xsmall().outline().child(e.kind.label()))))
                            .child(muted_cell(row.object_name.clone(), cx).w_56().flex_shrink_0().overflow_hidden().text_ellipsis())
                            .child(TableCell::new().min_w_0().overflow_hidden().text_ellipsis().text_xs().child(e.summary.clone()))
                            .child(muted_cell(e.policy_version.map(|v| format!("v{v}")).unwrap_or_else(|| "–".into()), cx).w_20().flex_shrink_0().text_right())
                    })))
                    .into_any_element()
            }),
    )
}

#[allow(dead_code)]
fn _purpose_used(_: Purpose) {}
