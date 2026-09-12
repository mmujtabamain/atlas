//! Privacy dialogs (M10, §7.2–§7.5): the policy editor (presets, per-aspect
//! grantees, calculation access, restricted disclosure, purpose scope,
//! effective date) and the purpose-specific grant dialog. Both go through
//! the engine's owner-only, audited mutations.

use atlas_core::authz::{AccessGrant, CalculationAccess, Grantee, Grantees, Purpose, VisibilityPreset};
use atlas_core::ids::*;
use atlas_core::model::Household;
use atlas_core::Disclosure;
use gpui_kit::component::{
    IndexPath, WindowExt as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    date_picker::{DatePicker, DatePickerState},
    dialog::DialogFooter,
    form::{Field, Form},
    input::{Input, InputState},
    radio::RadioGroup,
    select::{Select, SelectState},
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::alerting::{self, Level};
use crate::app::AtlasApp;

type Choice = Entity<SelectState<Vec<SharedString>>>;

pub const PRESETS: [VisibilityPreset; 4] = [VisibilityPreset::Private, VisibilityPreset::SharedSummary, VisibilityPreset::SharedBalance, VisibilityPreset::FullyShared];
pub const ACCESS: [CalculationAccess; 3] = [CalculationAccess::Excluded, CalculationAccess::RestrictedContribution, CalculationAccess::Full];
pub const RESTRICTED: [Disclosure; 2] = [Disclosure::Aggregate, Disclosure::Hidden];
pub const GRANT_DISCLOSURE: [Disclosure; 4] = [Disclosure::Aggregate, Disclosure::BalanceOnly, Disclosure::SelectedFields, Disclosure::Full];
pub const PURPOSES: [&str; 5] = ["Household forecasts", "One scenario", "Decisions and affordability", "Funding searches", "Tax calculations"];

#[derive(Debug, Clone, Default)]
pub struct PrivacyDraft {
    pub preset: usize,
    pub access: usize,
    pub restricted: usize,
    pub share_balance_with_person: bool,
    pub scope_to_scenario: bool,
    pub grant_purpose: usize,
    pub grant_disclosure: usize,
    pub grant_access: usize,
}

/// Retained state of both dialogs; rebuilt when the household changes.
pub struct PrivacyForms {
    pub draft: Entity<PrivacyDraft>,
    pub object: Choice,
    pub person: Choice,
    pub scenario: Choice,
    pub effective_from: Entity<DatePickerState>,
    pub effective_to: Entity<DatePickerState>,
    pub note: Entity<InputState>,
    objects: Vec<ObjectRef>,
    people: Vec<PersonId>,
    scenarios: Vec<ScenarioId>,
}

fn choice(items: Vec<SharedString>, window: &mut Window, cx: &mut Context<AtlasApp>) -> Choice {
    cx.new(|cx| SelectState::new(items, Some(IndexPath::default()), window, cx))
}

fn selected_row(state: &Choice, cx: &App) -> usize {
    state.read(cx).selected_index(cx).map(|p| p.row).unwrap_or(0)
}

impl PrivacyForms {
    /// Objects the viewer administers (owner of the policy), people other than the viewer, scenarios.
    pub fn new(household: &Household, viewer: PersonId, window: &mut Window, cx: &mut Context<AtlasApp>) -> Self {
        let mut objects: Vec<(ObjectRef, String)> = Vec::new();
        for account in &household.accounts {
            if household.may_administer(viewer, ObjectRef::Account(account.id)) {
                objects.push((ObjectRef::Account(account.id), format!("Account: {}", account.name)));
            }
        }
        for company in &household.companies {
            if household.may_administer(viewer, ObjectRef::Company(company.id)) {
                objects.push((ObjectRef::Company(company.id), format!("Company: {}", company.name)));
            }
        }
        for scenario in &household.scenarios {
            if household.may_administer(viewer, ObjectRef::Scenario(scenario.id)) {
                objects.push((ObjectRef::Scenario(scenario.id), format!("Scenario: {}", scenario.name)));
            }
        }
        let people: Vec<PersonId> = household.people.iter().filter(|p| p.id != viewer).map(|p| p.id).collect();
        let people_names: Vec<SharedString> = household.people.iter().filter(|p| p.id != viewer).map(|p| SharedString::from(p.name.clone())).collect();
        let scenarios: Vec<ScenarioId> = household.scenarios.iter().map(|s| s.id).collect();
        let scenario_names: Vec<SharedString> = household.scenarios.iter().map(|s| SharedString::from(s.name.clone())).collect();
        PrivacyForms {
            draft: cx.new(|_| PrivacyDraft { preset: 1, access: 1, ..Default::default() }),
            object: choice(objects.iter().map(|(_, n)| SharedString::from(n.clone())).collect(), window, cx),
            person: choice(people_names, window, cx),
            scenario: choice(scenario_names, window, cx),
            effective_from: cx.new(|cx| DatePickerState::new(window, cx).date_format("%d %b %Y")),
            effective_to: cx.new(|cx| DatePickerState::new(window, cx).date_format("%d %b %Y")),
            note: cx.new(|cx| InputState::new(window, cx).placeholder("why (goes on the audit log)")),
            objects: objects.into_iter().map(|(o, _)| o).collect(),
            people,
            scenarios,
        }
    }
}

impl AtlasApp {
    /// Opens the policy editor for the objects the viewer owns.
    pub fn open_policy_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.entity().downgrade();
        let f = &self.privacy_forms;
        let as_of = self.household.as_of;
        f.effective_from.update(cx, |s, cx| s.set_date(as_of, window, cx));
        let no_objects = f.objects.is_empty();
        let no_people = f.people.is_empty();
        let no_scenarios = f.scenarios.is_empty();
        let fields = (f.draft.clone(), f.object.clone(), f.person.clone(), f.scenario.clone(), f.effective_from.clone(), f.note.clone());
        window.open_dialog(cx, move |dialog, window, cx| {
            let this = this.clone();
            let wide_width = window.rem_size() * 52.;
            let (draft, object, person, scenario, effective_from, note) = fields.clone();
            let PrivacyDraft { preset, access, restricted, share_balance_with_person, scope_to_scenario, .. } = draft.read(cx).clone();
            let (d1, d2, d3, d4, d5) = (draft.clone(), draft.clone(), draft.clone(), draft.clone(), draft.clone());
            dialog
                .title("Access policy — a new, effective-dated version")
                .w(wide_width)
                .max_h(relative(0.9))
                .child(
                    v_flex().child(
                        Form::vertical()
                            .columns(2)
                            .child(Field::new().label("Object (only ones you own)").child(if no_objects { div().text_sm().child("You own no policed object.").into_any_element() } else { Select::new(&object).into_any_element() }))
                            .child(Field::new().label("Effective from").required(true).child(DatePicker::new(&effective_from)))
                            .child(
                                Field::new().label("Visibility").child(
                                    RadioGroup::vertical("policy-preset")
                                        .children(PRESETS.iter().map(|p| p.label()))
                                        .selected_index(Some(preset))
                                        .on_change(move |index, _, cx| d1.update(cx, |d, cx| { d.preset = *index; cx.notify(); })),
                                ),
                            )
                            .child(
                                Field::new().label("Use in calculations").child(
                                    RadioGroup::vertical("policy-access")
                                        .children(ACCESS.iter().map(|a| a.label()))
                                        .selected_index(Some(access))
                                        .on_change(move |index, _, cx| d2.update(cx, |d, cx| { d.access = *index; cx.notify(); })),
                                ),
                            )
                            .child(
                                Field::new().label("How a restricted contribution is shown").child(
                                    RadioGroup::horizontal("policy-restricted")
                                        .children(RESTRICTED.iter().map(|d| d.label()))
                                        .selected_index(Some(restricted))
                                        .on_change(move |index, _, cx| d3.update(cx, |d, cx| { d.restricted = *index; cx.notify(); })),
                                ),
                            )
                            .child(
                                Field::new().label_indent(false).child(
                                    v_flex()
                                        .gap_2()
                                        .child(Checkbox::new("policy-share-balance").label("Also share the balance with one person").checked(share_balance_with_person).on_change(move |v, _, cx| d4.update(cx, |d, cx| { d.share_balance_with_person = *v; cx.notify(); })))
                                        .when(share_balance_with_person && !no_people, |this| this.child(Select::new(&person)))
                                        .child(Checkbox::new("policy-scope-scenario").label("Usable only inside one scenario").checked(scope_to_scenario).on_change(move |v, _, cx| d5.update(cx, |d, cx| { d.scope_to_scenario = *v; cx.notify(); })))
                                        .when(scope_to_scenario && !no_scenarios, |this| this.child(Select::new(&scenario))),
                                ),
                            )
                            .child(Field::new().label("Note for the audit log").child(Input::new(&note).id("policy-note"))),
                    ),
                )
                .footer(
                    DialogFooter::new()
                        .child(Button::new("policy-cancel").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                        .child(Button::new("policy-save").primary().label("Set policy").on_click({
                            let this = this.clone();
                            move |_, window, cx| {
                                Self::confirm_policy(&this, window, cx);
                            }
                        })),
                )
                .on_ok(move |_, window, cx| Self::confirm_policy(&this, window, cx))
        });
    }

    fn confirm_policy(this: &WeakEntity<Self>, window: &mut Window, cx: &mut App) -> bool {
        match this.update(cx, |app, cx| app.submit_policy(cx)) {
            Ok(Ok(summary)) => {
                window.push_notification(summary, cx);
                window.close_dialog(cx);
                true
            }
            Ok(Err(message)) => {
                window.push_notification(message, cx);
                false
            }
            Err(_) => false,
        }
    }

    /// Builds the new policy version from the editor and sets it (owner-only, audited).
    pub fn submit_policy(&mut self, cx: &mut Context<Self>) -> Result<String, String> {
        let f = &self.privacy_forms;
        let draft = f.draft.read(cx).clone();
        let object = *f.objects.get(selected_row(&f.object, cx)).ok_or("You own no object whose policy you could change.")?;
        let current = self.household.policy_for(object).cloned();
        let owners = current.as_ref().map(|p| p.full_access.clone()).unwrap_or_else(|| vec![self.viewer.person]);
        let preset = PRESETS.get(draft.preset).copied().unwrap_or(VisibilityPreset::Private);
        let access = ACCESS.get(draft.access).copied().unwrap_or(CalculationAccess::RestrictedContribution);
        let effective_from = f.effective_from.read(cx).date().start().ok_or("Pick the effective date.")?;
        let at = chrono::Local::now().naive_local();
        let mut policy = atlas_core::authz::AccessPolicy::preset(current.as_ref().map(|p| p.id).unwrap_or_else(|| self.household.next_policy_id()), object, owners, preset, access, effective_from, at);
        policy.restricted_disclosure = RESTRICTED.get(draft.restricted).copied().unwrap_or(Disclosure::Aggregate);
        if draft.share_balance_with_person {
            let person = *f.people.get(selected_row(&f.person, cx)).ok_or("There is nobody else in the household to share with.")?;
            policy.existence = Grantees::Persons(vec![person]);
            policy.balance = Grantees::Persons(vec![person]);
        }
        if draft.scope_to_scenario {
            let scenario = *f.scenarios.get(selected_row(&f.scenario, cx)).ok_or("Create a scenario first.")?;
            policy.purposes = vec![Purpose::Scenario(scenario).tag()];
        }
        let note = f.note.read(cx).value().trim().to_string();
        match self.household.set_policy(policy, self.viewer.person, at) {
            Ok(version) => {
                if !note.is_empty() {
                    self.household.record_audit(self.viewer.person, Some(object), atlas_core::authz::AuditKind::PolicyChanged { from_version: version - 1, to_version: version }, format!("note: {note}"), Some(version), at);
                }
                log::info!("policy of {object} set to v{version} ({}) by {}", preset.label(), self.viewer.person);
                self.mark_dirty();
                self.rebuild_forms_after_policy(cx);
                cx.notify();
                Ok(format!("Policy of {object} is now version {version}: {} / {}. Every screen re-filtered.", preset.label(), access.label()))
            }
            Err(err) => {
                alerting::report(Level::Warning, format!("policy change refused for {object}: {err}"));
                Err(format!("Not changed — {err}"))
            }
        }
    }

    fn rebuild_forms_after_policy(&mut self, _cx: &mut Context<Self>) {
        self.refresh_derived();
    }

    /// Opens the purpose-specific grant dialog (§7.4).
    pub fn open_grant_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.entity().downgrade();
        let f = &self.privacy_forms;
        let as_of = self.household.as_of;
        f.effective_from.update(cx, |s, cx| s.set_date(as_of, window, cx));
        f.effective_to.update(cx, |s, cx| s.set_date(gpui_kit::base::Date::Single(None), window, cx));
        let no_objects = f.objects.is_empty();
        let no_people = f.people.is_empty();
        let fields = (f.draft.clone(), f.object.clone(), f.person.clone(), f.scenario.clone(), f.effective_from.clone(), f.effective_to.clone(), f.note.clone());
        window.open_dialog(cx, move |dialog, window, cx| {
            let this = this.clone();
            let wide_width = window.rem_size() * 52.;
            let (draft, object, person, scenario, effective_from, effective_to, note) = fields.clone();
            let PrivacyDraft { grant_purpose, grant_disclosure, grant_access, .. } = draft.read(cx).clone();
            let (d1, d2, d3) = (draft.clone(), draft.clone(), draft.clone());
            dialog
                .title("Grant access for one purpose — without sharing the object with everyone")
                .w(wide_width)
                .max_h(relative(0.9))
                .child(
                    v_flex().child(
                        Form::vertical()
                            .columns(2)
                            .child(Field::new().label("Object (only ones you own)").child(if no_objects { div().text_sm().child("You own no policed object.").into_any_element() } else { Select::new(&object).into_any_element() }))
                            .child(Field::new().label("Grantee").child(if no_people { div().text_sm().child("Nobody else in the household yet.").into_any_element() } else { Select::new(&person).into_any_element() }))
                            .child(
                                Field::new().label("Purpose").child(
                                    RadioGroup::vertical("grant-purpose")
                                        .children(PURPOSES)
                                        .selected_index(Some(grant_purpose))
                                        .on_change(move |index, _, cx| d1.update(cx, |d, cx| { d.grant_purpose = *index; cx.notify(); })),
                                ),
                            )
                            .child(if grant_purpose == 1 { Field::new().label("Scenario").child(Select::new(&scenario)) } else { Field::new().label("Scope").child(div().text_sm().child("applies to every calculation of that purpose")) })
                            .child(
                                Field::new().label("How the object may appear to the grantee").child(
                                    RadioGroup::vertical("grant-disclosure")
                                        .children(GRANT_DISCLOSURE.iter().map(|d| d.label()))
                                        .selected_index(Some(grant_disclosure))
                                        .on_change(move |index, _, cx| d2.update(cx, |d, cx| { d.grant_disclosure = *index; cx.notify(); })),
                                ),
                            )
                            .child(
                                Field::new().label("Use in calculations within the purpose").child(
                                    RadioGroup::vertical("grant-access")
                                        .children(ACCESS.iter().map(|a| a.label()))
                                        .selected_index(Some(grant_access))
                                        .on_change(move |index, _, cx| d3.update(cx, |d, cx| { d.grant_access = *index; cx.notify(); })),
                                ),
                            )
                            .child(Field::new().label("Effective from").required(true).child(DatePicker::new(&effective_from)))
                            .child(Field::new().label("Effective to (optional)").child(DatePicker::new(&effective_to)))
                            .child(Field::new().label("Note for the audit log").child(Input::new(&note).id("grant-note"))),
                    ),
                )
                .footer(
                    DialogFooter::new()
                        .child(Button::new("grant-cancel").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                        .child(Button::new("grant-save").primary().label("Grant").on_click({
                            let this = this.clone();
                            move |_, window, cx| {
                                Self::confirm_grant(&this, window, cx);
                            }
                        })),
                )
                .on_ok(move |_, window, cx| Self::confirm_grant(&this, window, cx))
        });
    }

    fn confirm_grant(this: &WeakEntity<Self>, window: &mut Window, cx: &mut App) -> bool {
        match this.update(cx, |app, cx| app.submit_grant(cx)) {
            Ok(Ok(summary)) => {
                window.push_notification(summary, cx);
                window.close_dialog(cx);
                true
            }
            Ok(Err(message)) => {
                window.push_notification(message, cx);
                false
            }
            Err(_) => false,
        }
    }

    pub fn submit_grant(&mut self, cx: &mut Context<Self>) -> Result<String, String> {
        let f = &self.privacy_forms;
        let draft = f.draft.read(cx).clone();
        let object = *f.objects.get(selected_row(&f.object, cx)).ok_or("You own no object to grant access to.")?;
        let person = *f.people.get(selected_row(&f.person, cx)).ok_or("There is nobody else in the household to grant to.")?;
        let purpose = match draft.grant_purpose {
            0 => Purpose::HouseholdForecast,
            1 => Purpose::Scenario(*f.scenarios.get(selected_row(&f.scenario, cx)).ok_or("Create a scenario first.")?),
            2 => Purpose::Decision,
            3 => Purpose::FundingSearch,
            _ => Purpose::Tax,
        };
        let effective_from = f.effective_from.read(cx).date().start().ok_or("Pick the effective date.")?;
        let effective_to = f.effective_to.read(cx).date().start();
        let at = chrono::Local::now().naive_local();
        let grant = AccessGrant {
            id: GrantId::new(0),
            object,
            grantee: Grantee::Person(person),
            purpose,
            disclosure: GRANT_DISCLOSURE.get(draft.grant_disclosure).copied().unwrap_or(Disclosure::Aggregate),
            calculation: ACCESS.get(draft.grant_access).copied().unwrap_or(CalculationAccess::RestrictedContribution),
            effective_from,
            effective_to,
            granted_by: self.viewer.person,
            granted_at: at,
            revoked_on: None,
            note: f.note.read(cx).value().trim().to_string(),
        };
        let described = format!("{} for {}", self.household.entity_name(EntityRef::Person(person)), purpose.describe(&self.household));
        match self.household.add_grant(grant, self.viewer.person, at) {
            Ok(id) => {
                log::info!("grant {id} on {object}: {described}");
                self.mark_dirty();
                self.refresh_derived();
                cx.notify();
                Ok(format!("Grant {id} added: {described}."))
            }
            Err(err) => {
                alerting::report(Level::Warning, format!("grant refused on {object}: {err}"));
                Err(format!("Not granted — {err}"))
            }
        }
    }

    pub fn revoke_grant(&mut self, id: GrantId, window: &mut Window, cx: &mut Context<Self>) {
        let at = chrono::Local::now().naive_local();
        match self.household.revoke_grant(id, self.viewer.person, self.household.as_of, at) {
            Ok(()) => {
                self.mark_dirty();
                self.refresh_derived();
                window.push_notification(format!("Grant {id} revoked from {}.", self.household.as_of.format("%d %b %Y")), cx);
            }
            Err(err) => {
                alerting::report(Level::Warning, format!("revoking grant {id} failed: {err}"));
                window.push_notification(format!("Not revoked — {err}"), cx);
            }
        }
        cx.notify();
    }
}
