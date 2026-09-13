//! Sharing: the policies that decide what this viewer may see (with their
//! health, aspects and versions), the purpose-specific grants, and the
//! immutable audit of access changes. Ownership, visibility, use in
//! calculations and purpose stay four separate facts throughout.

use atlas_core::ids::ObjectRef;
use atlas_core::model::Household;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    accordion::Accordion,
    alert::Alert,
    button::{Button, ButtonVariants as _},
    description_list::{DescriptionItem, DescriptionList},
    h_flex,
    tag::Tag,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::common::workspace_header;
use crate::app::AtlasApp;
use crate::models::privacy::{AuditRow, GrantRow, PolicyRow, PrivacyModel};
use crate::nav::{Destination, Route};
use crate::widgets::labels;
use crate::widgets::record::{self, Lane};
use crate::widgets::states::{about_access_button, count_line, empty_state, fact, lanes, note, section};

fn date(d: chrono::NaiveDate) -> String {
    d.format("%d %b %Y").to_string()
}

fn stamp(at: chrono::NaiveDateTime) -> String {
    at.format("%d %b %Y %H:%M").to_string()
}

// ----- Policies ----------------------------------------------------------------

const POLICY_LANES: [(&str, Lane); 3] = [("Object", Lane::flex()), ("Preset", Lane::fixed(200.)), ("Version", Lane::fixed(100.))];

pub fn render_policies(app: &AtlasApp, model: &PrivacyModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let owns_something = model.viewer_is_owner_of > 0;
    let header = workspace_header(
        Destination::Sharing,
        Route::Policies,
        vec![Button::new("change-policy").small().outline().label("Change policy…").disabled(!owns_something).tooltip(if owns_something { "Change the policy of an object you own" } else { "Only an object's owner can change its policy" }).on_click(cx.listener(|this, _, window, cx| this.open_policy_editor(window, cx))).into_any_element()],
        cx,
    );
    let selected = app.selected_policy.filter(|o| model.policies.iter().any(|p| p.policy.object == *o)).or_else(|| model.policies.first().map(|p| p.policy.object));
    let rows: Vec<_> = model
        .policies
        .iter()
        .map(|row| {
            let object = row.policy.object;
            record::row(
                SharedString::from(format!("policy-{}", row.policy.id.raw())),
                selected == Some(object),
                vec![
                    (POLICY_LANES[0].1, record::stack(row.object_name.clone(), if row.owned { "You own this".to_string() } else { format!("Your disclosure: {}", row.viewer_disclosure.label()) }, cx)),
                    (POLICY_LANES[1].1, h_flex().child(Tag::secondary().xsmall().outline().child(preset_label(row))).into_any_element()),
                    (POLICY_LANES[2].1, record::muted(format!("v{}", row.policy.version), cx)),
                ],
                move |_, _, cx| crate::app::with_app(cx, |app, cx| app.select_policy_row(object, cx)),
            )
        })
        .collect();
    let detail = selected.and_then(|o| model.policies.iter().find(|p| p.policy.object == o)).map(|row| render_policy_detail(app, row, household, cx));
    let theme = cx.theme();
    let problems: AnyElement = if model.problems.is_empty() {
        if owns_something {
            note("No policy problem: every object you administer has an effective policy, and no grant points at something that no longer exists.", cx).into_any_element()
        } else {
            div().into_any_element()
        }
    } else {
        v_flex()
            .w_full()
            .gap_2()
            .child(Alert::warning("policy-problems", format!("{} policy problem{} need attention. Until each is resolved the object stays hidden and out of ordinary calculations.", model.problems.len(), if model.problems.len() == 1 { "" } else { "s" })).title("Fail-closed"))
            .children(model.problems.iter().enumerate().map(|(i, p)| {
                let object = p.object;
                let can_set = household.may_administer(app.viewer().person, object) || household.person(app.viewer().person).is_some_and(|x| x.role == atlas_core::model::HouseholdRole::Owner);
                h_flex()
                    .w_full()
                    .gap_2()
                    .items_center()
                    .child(div().flex_1().min_w_0().text_sm().whitespace_normal().child(p.text.clone()))
                    .child(Button::new(SharedString::from(format!("problem-review-{i}"))).xsmall().ghost().compact().label("Review problem…").on_click(cx.listener(move |this, _, _, cx| this.select_policy_row(object, cx))))
                    .when(can_set, |this| this.child(Button::new(SharedString::from(format!("problem-set-{i}"))).xsmall().outline().compact().label("Set initial policy…").on_click(cx.listener(move |this, _, window, cx| this.open_policy_editor_for(Some(object), window, cx)))))
            }))
            .into_any_element()
    };

    v_flex()
        .id("screen-policies")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(lanes([
            fact("Policies you may see", model.policies.len().to_string(), cx).into_any_element(),
            fact("Objects you own", model.viewer_is_owner_of.to_string(), cx).into_any_element(),
            fact("Not disclosed to you", model.hidden_policies.to_string(), cx).into_any_element(),
        ]))
        .child(problems)
        .child(count_line(model.policies.len(), model.policies.len() + model.hidden_policies, "policies", cx))
        .child(if model.policies.is_empty() {
            empty_state("policies-empty", "No sharing policies you may see", "Every person, account, company and scenario gets its policy when it is created. Nothing here is disclosed to you yet.", None, cx)
        } else {
            record::list("policies-list", record::header(&POLICY_LANES, cx), rows).into_any_element()
        })
        .children(detail)
        .child(
            section("policies-denial", "Why access is limited")
                .child(match &model.denial_example {
                    Some(example) => div().id("denial-example").test_support().text_sm().whitespace_normal().child(example.clone()).into_any_element(),
                    None => note("Nothing is withheld from you in this household.", cx).into_any_element(),
                })
                .child(h_flex().child(about_access_button("policies-about-access")))
                .child(div().text_xs().text_color(theme.muted_foreground).child("An object's owner can change its policy or issue a purpose-specific grant. Hidden objects are never listed here.")),
        )
        .into_any_element()
}

fn preset_label(row: &PolicyRow) -> String {
    // The stored policy has no preset field; its aspects say what it is.
    use atlas_core::authz::Grantees;
    let all_household = matches!(row.policy.existence, Grantees::Household) && matches!(row.policy.balance, Grantees::Household) && matches!(row.policy.transactions, Grantees::Household);
    let owners_only = matches!(row.policy.existence, Grantees::OwnersOnly);
    if all_household {
        "Fully shared".into()
    } else if owners_only {
        "Private".into()
    } else if matches!(row.policy.balance, Grantees::Household | Grantees::Persons(_)) {
        "Shared balance".into()
    } else if matches!(row.policy.existence, Grantees::Household | Grantees::Persons(_)) {
        "Shared summary".into()
    } else {
        "Custom".into()
    }
}

fn render_policy_detail(app: &AtlasApp, row: &PolicyRow, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let object = row.policy.object;
    let owned = row.owned;
    let may_read_aspects = owned || row.viewer_disclosure == atlas_core::Disclosure::Full;
    let owners: Vec<String> = row.policy.full_access.iter().map(|p| household.entity_name(atlas_core::ids::EntityRef::Person(*p))).collect();
    let open = app.policy_sections_open.clone();
    let aspects: Vec<String> = if may_read_aspects { row.lines.clone() } else { Vec::new() };
    let versions = row.policy.previous_versions.clone();
    let theme = cx.theme();
    let mut accordion = Accordion::new("policy-sections").bordered(true).multiple(true).on_toggle_click(cx.listener(|this, open: &[usize], _, cx| {
        this.policy_sections_open = open.to_vec();
        cx.notify();
    }));
    let aspects_open = open.contains(&0) || open.is_empty();
    accordion = accordion.item(move |item| {
        item.title("Who may see and use this").open(aspects_open).child(if aspects.is_empty() {
            div().text_xs().child("The aspects are shown to the object's owners and to a viewer with full disclosure.").into_any_element()
        } else {
            v_flex().gap_1().text_sm().children(aspects.iter().map(|l| div().whitespace_normal().child(l.clone()))).into_any_element()
        })
    });
    let versions_open = open.contains(&1);
    accordion = accordion.item(move |item| {
        item.title(format!("Version history ({})", versions.len())).open(versions_open).child(if versions.is_empty() {
            div().text_xs().child("This is the first version; nothing has changed it.").into_any_element()
        } else {
            v_flex()
                .gap_1()
                .text_sm()
                .children(versions.iter().map(|v| div().child(format!("v{} effective {} · changed {}", v.version, date(v.effective_from), stamp(v.changed_at)))))
                .into_any_element()
        })
    });

    section("policy-detail", row.object_name.clone())
        .action(
            h_flex()
                .gap_2()
                .child(Button::new("policy-view-object").small().ghost().label("View object").disabled(!object_is_navigable(object)).on_click(cx.listener(move |this, _, window, cx| this.open_policy_object(object, window, cx))))
                .when(owned, |this| {
                    this.child(Button::new("policy-change").small().outline().label("Change policy…").on_click(cx.listener(move |this, _, window, cx| this.open_policy_editor_for(Some(object), window, cx))))
                        .child(Button::new("policy-grant").small().ghost().label("Grant access…").on_click(cx.listener(move |this, _, window, cx| this.open_grant_editor_for(Some(object), window, cx))))
                }),
        )
        .child(
            DescriptionList::new()
                .columns(2)
                .child(DescriptionItem::new("Preset").value(preset_label(row)))
                .child(DescriptionItem::new("Version").value(format!("v{}", row.policy.version)))
                .child(DescriptionItem::new("Owners").value(if owners.is_empty() { "None recorded".to_string() } else { owners.join(", ") }))
                .child(DescriptionItem::new("Your access").value(if owned { "You own this object".to_string() } else { format!("Disclosure: {}", row.viewer_disclosure.label()) }))
                .child(DescriptionItem::new("Effective from").value(date(row.policy.effective_from)))
                .child(DescriptionItem::new("Changed").value(format!("{} by {}", stamp(row.policy.changed_at), household.entity_name(atlas_core::ids::EntityRef::Person(row.policy.changed_by)))))
                .child(DescriptionItem::new("Previous").value(row.policy.previous.clone().unwrap_or_else(|| "No earlier preset recorded".into()))),
        )
        .child(accordion)
        .child(div().text_xs().text_color(theme.muted_foreground).child("Ownership, visibility, use in calculations and purpose are separate: sharing a balance is not ownership, and an owner is not automatically every calculation's participant."))
        .into_any_element()
}

fn object_is_navigable(object: ObjectRef) -> bool {
    matches!(object, ObjectRef::Account(_) | ObjectRef::Company(_) | ObjectRef::Person(_) | ObjectRef::Scenario(_))
}

// ----- Grants -------------------------------------------------------------------

const GRANT_LANES: [(&str, Lane); 5] = [("Object", Lane::fixed(260.)), ("Grantee", Lane::fixed(220.)), ("Purpose", Lane::flex()), ("Effective", Lane::fixed(200.)), ("Status", Lane::fixed(140.))];

pub fn render_grants(app: &AtlasApp, model: &PrivacyModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let owns_something = model.viewer_is_owner_of > 0;
    let header = workspace_header(
        Destination::Sharing,
        Route::Grants,
        vec![Button::new("grant-access").small().outline().icon(IconName::Plus).label("Grant access…").disabled(!owns_something).tooltip(if owns_something { "Give access for one purpose and period" } else { "Only an object owner can grant access" }).on_click(cx.listener(|this, _, window, cx| this.open_grant_dialog(window, cx))).into_any_element()],
        cx,
    );
    let selected = app.selected_grant;
    let rows: Vec<_> = model
        .grants
        .iter()
        .map(|row| {
            let id = row.grant.id;
            record::row(
                SharedString::from(format!("grant-{}", id.raw())),
                selected == Some(id),
                vec![
                    (GRANT_LANES[0].1, record::text(row.object_name.clone())),
                    (GRANT_LANES[1].1, record::text(row.grantee.clone())),
                    (GRANT_LANES[2].1, record::text(row.purpose.clone())),
                    (GRANT_LANES[3].1, record::muted(format!("{} – {}", date(row.grant.effective_from), row.grant.effective_to.map(date).unwrap_or_else(|| "no end".into())), cx)),
                    (GRANT_LANES[4].1, h_flex().child(status_tag(row, household)).into_any_element()),
                ],
                move |_, _, cx| crate::app::with_app(cx, |app, cx| app.select_grant(id, cx)),
            )
        })
        .collect();
    let detail = selected.and_then(|id| model.grants.iter().find(|g| g.grant.id == id)).map(|row| render_grant_detail(row, household, cx));
    v_flex()
        .id("screen-grants")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(format!("Status as of the reconciliation date, {}. A grant applies only to its stated purpose; the baseline and unrelated searches are unaffected.", date(household.as_of))))
        .child(count_line(model.grants.len(), model.grants.len(), "grants", cx))
        .child(if model.grants.is_empty() {
            empty_state(
                "grants-empty",
                "No purpose-specific grants",
                if owns_something { "The base policies still apply. A grant adds access for one purpose and period, nothing wider." } else { "Only an object owner can grant access." },
                owns_something.then(|| Button::new("grants-add-first").small().outline().icon(IconName::Plus).label("Grant access…").on_click(cx.listener(|this, _, window, cx| this.open_grant_dialog(window, cx))).into_any_element()),
                cx,
            )
        } else {
            record::list("grants-list", record::header(&GRANT_LANES, cx), rows).into_any_element()
        })
        .children(detail)
        .into_any_element()
}

fn status_tag(row: &GrantRow, household: &Household) -> Tag {
    let as_of = household.as_of;
    if let Some(revoked) = row.grant.revoked_on {
        Tag::danger().xsmall().outline().child(format!("Revoked from {}", date(revoked)))
    } else if row.in_effect {
        Tag::secondary().xsmall().outline().child("In effect")
    } else if row.grant.effective_from > as_of {
        Tag::info().xsmall().outline().child("Future")
    } else {
        Tag::warning().xsmall().outline().child("Expired")
    }
}

fn render_grant_detail(row: &GrantRow, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let id = row.grant.id;
    let object = row.grant.object;
    let revoked = row.grant.revoked_on.is_some();
    let can_revoke = row.owned && !revoked;
    section("grant-detail", format!("{} — {}", row.object_name, row.purpose))
        .action(
            h_flex()
                .gap_2()
                .child(Button::new("grant-view-policy").small().ghost().label("View policy").disabled(!object_is_navigable(object)).on_click(cx.listener(move |this, _, _, cx| this.open_policy_for(object, cx))))
                .when(can_revoke, |this| this.child(Button::new("grant-revoke").small().danger().outline().label("Revoke grant…").on_click(cx.listener(move |this, _, window, cx| this.confirm_revoke_grant(id, window, cx))))),
        )
        .child(
            DescriptionList::new()
                .columns(2)
                .child(DescriptionItem::new("Grantee").value(row.grantee.clone()))
                .child(DescriptionItem::new("Purpose").value(row.purpose.clone()))
                .child(DescriptionItem::new("How the object may appear").value(row.grant.disclosure.label()))
                .child(DescriptionItem::new("Use in calculations").value(row.grant.calculation.label()))
                .child(DescriptionItem::new("From").value(date(row.grant.effective_from)))
                .child(DescriptionItem::new("To").value(row.grant.effective_to.map(date).unwrap_or_else(|| "No end".into())))
                .child(DescriptionItem::new("Revoked").value(row.grant.revoked_on.map(date).unwrap_or_else(|| "Not revoked".into())))
                .child(DescriptionItem::new("Granted").value(format!("{} by {}", stamp(row.grant.granted_at), household.entity_name(atlas_core::ids::EntityRef::Person(row.grant.granted_by)))))
                .child(DescriptionItem::new("Note").value(if row.grant.note.is_empty() { "No note".to_string() } else { row.grant.note.clone() })),
        )
        .child(note(if revoked { "A revoked grant is kept with its date; calculations made while it applied keep their recorded context." } else { "The grant widens access only inside its purpose, and only while it is in effect." }, cx))
        .into_any_element()
}

// ----- Audit ---------------------------------------------------------------------

const AUDIT_LANES: [(&str, Lane); 5] = [("When", Lane::fixed(170.)), ("Who", Lane::fixed(150.)), ("What", Lane::fixed(230.)), ("Object", Lane::flex()), ("Version", Lane::fixed(120.))];

pub fn render_audit(app: &AtlasApp, model: &PrivacyModel, cx: &mut Context<AtlasApp>) -> AnyElement {
    let header = workspace_header(Destination::Sharing, Route::Audit, vec![], cx);
    let expanded = app.audit_expanded;
    let rows: Vec<AnyElement> = model
        .audit
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let is_open = expanded == Some(i);
            let row_el = record::row(
                SharedString::from(format!("audit-{i}")),
                is_open,
                vec![
                    (AUDIT_LANES[0].1, record::text(stamp(row.event.at))),
                    (AUDIT_LANES[1].1, record::muted(row.actor.clone(), cx)),
                    (AUDIT_LANES[2].1, h_flex().child(audit_tag(row)).into_any_element()),
                    (AUDIT_LANES[3].1, record::stack(row.object_name.clone(), row.event.summary.clone(), cx)),
                    (AUDIT_LANES[4].1, record::muted(row.event.policy_version.map(|v| format!("v{v}")).unwrap_or_else(|| "—".into()), cx)),
                ],
                move |_, _, cx| {
                    crate::app::with_app(cx, |app, cx| {
                        app.audit_expanded = if app.audit_expanded == Some(i) { None } else { Some(i) };
                        cx.notify();
                    })
                },
            );
            if !is_open {
                return row_el.into_any_element();
            }
            let versions = match row.event.kind {
                atlas_core::authz::AuditKind::PolicyChanged { from_version, to_version } => Some(format!("v{from_version} → v{to_version}")),
                _ => None,
            };
            v_flex()
                .w_full()
                .child(row_el)
                .child(
                    v_flex().w_full().gap_2().px_3().py_2().child(
                        DescriptionList::new()
                            .columns(1)
                            .child(DescriptionItem::new("When").value(stamp(row.event.at)))
                            .child(DescriptionItem::new("Who").value(row.actor.clone()))
                            .child(DescriptionItem::new("What").value(row.event.kind.label()))
                            .child(DescriptionItem::new("Object").value(row.object_name.clone()))
                            .child(DescriptionItem::new("Summary").value(row.event.summary.clone()))
                            .child(DescriptionItem::new("Policy version").value(versions.or_else(|| row.event.policy_version.map(|v| format!("v{v}"))).unwrap_or_else(|| "Not applicable".into()))),
                    ),
                )
                .into_any_element()
        })
        .collect();
    v_flex()
        .id("screen-audit")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(div().text_xs().text_color(cx.theme().muted_foreground).child("Newest first. Sharing activity cannot be edited, and a hidden object's name is redacted in both the reference and the summary."))
        .child(count_line(model.audit.len(), model.audit.len(), "sharing events", cx))
        .child(if model.audit.is_empty() {
            empty_state("audit-empty", "No sharing activity yet", "Policy changes, grants, denials and viewer switches appear here as they happen.", None, cx)
        } else {
            v_flex().w_full().gap_0p5().child(record::header(&AUDIT_LANES, cx)).children(rows).into_any_element()
        })
        .into_any_element()
}

fn audit_tag(row: &AuditRow) -> Tag {
    use atlas_core::authz::AuditKind;
    match row.event.kind {
        AuditKind::AccessDenied => Tag::danger().xsmall().outline().child(row.event.kind.label()),
        AuditKind::GrantRevoked | AuditKind::SuppressionApplied => Tag::warning().xsmall().outline().child(row.event.kind.label()),
        _ => Tag::secondary().xsmall().outline().child(row.event.kind.label()),
    }
}

impl AtlasApp {
    pub fn select_policy_row(&mut self, object: ObjectRef, cx: &mut Context<Self>) {
        self.selected_policy = Some(object);
        self.navigate(Route::Policies, cx);
        cx.notify();
    }

    pub fn select_grant(&mut self, id: atlas_core::ids::GrantId, cx: &mut Context<Self>) {
        self.selected_grant = Some(id);
        cx.notify();
    }

    /// Follows a policy's object to its canonical detail.
    pub fn open_policy_object(&mut self, object: ObjectRef, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        match object {
            ObjectRef::Account(id) => self.navigate(Route::Account(id), cx),
            ObjectRef::Company(id) => self.navigate(Route::Company(id), cx),
            ObjectRef::Person(id) => self.navigate(Route::Person(id), cx),
            ObjectRef::Scenario(id) => {
                self.scenario_detail = Some(id);
                self.navigate(Route::Scenarios, cx);
            }
            _ => {}
        }
    }

    /// The grant sheet with `object` preselected.
    pub fn open_grant_editor_for(&mut self, object: Option<ObjectRef>, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(object) = object
            && let Some(row) = self.privacy_forms.object_row(object)
        {
            Self::set_choice(&self.privacy_forms.object, row, window, cx);
        }
        self.open_grant_dialog(window, cx);
    }

    /// Confirms before revoking, naming the object, grantee and purpose.
    pub fn confirm_revoke_grant(&mut self, id: atlas_core::ids::GrantId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.privacy().grants.iter().find(|g| g.grant.id == id).cloned() else { return };
        let lines = vec![
            format!("{} — {}", row.object_name, row.purpose),
            format!("Grantee: {}", row.grantee),
            format!("Revoked from the reconciliation date, {}.", date(self.household.as_of)),
            "The grant is kept with its revocation date; calculations made while it applied keep their context.".to_string(),
        ];
        super::common::confirm_primary(window, cx, "Revoke this grant?", lines, "Revoke grant", move |window, cx| {
            crate::app::with_app(cx, |app, cx| app.revoke_grant(id, window, cx));
        });
    }

    /// A policy label for other screens (tests read it too).
    pub fn policy_preset_label(row: &PolicyRow) -> String {
        preset_label(row)
    }

    /// The disclosure tag of a policy row, shared with other screens.
    pub fn policy_disclosure_tag(row: &PolicyRow) -> Tag {
        labels::disclosure_tag(row.viewer_disclosure)
    }
}
