//! Cross-screen actions: the app handle that menus reach the view through,
//! contextual form openers (an account preselected), delete confirmations,
//! and the links between workspaces that carry a supported context.

use atlas_core::ids::{AccountId, CompanyId, ObjectRef, PersonId, SeriesId};
use atlas_core::liquidity::Boundary;
use atlas_core::Disclosure;
use gpui_kit::component::{IndexPath, WindowExt as _, input::InputState, select::SelectState};
use gpui_kit::*;

use crate::app::AtlasApp;
use crate::entry::{ACCOUNT_KINDS, Entry};
use crate::nav::Route;
use crate::screens::common::confirm_danger;
use crate::widgets::scope::{self, Choice};

/// The content view's handle, for closures that have no `this` (dropdown
/// menus, confirmation callbacks). Set once when the view is created.
pub struct AppHandle(pub WeakEntity<AtlasApp>);

impl Global for AppHandle {}

/// Runs `f` on the content view from anywhere with an `App`.
pub fn with_app(cx: &mut App, f: impl FnOnce(&mut AtlasApp, &mut Context<AtlasApp>)) {
    let handle = cx.try_global::<AppHandle>().map(|h| h.0.clone());
    if let Some(app) = handle.and_then(|h| h.upgrade()) {
        app.update(cx, f);
    }
}

/// Retained filter controls of the Accounts register (display-only filters).
pub struct AccountsControls {
    pub search: Entity<InputState>,
    pub holder: Choice,
    pub kind: Choice,
    /// The entities behind the holder filter's rows (after "All").
    pub holders: Vec<atlas_core::ids::EntityRef>,
}

impl AccountsControls {
    pub fn new(household: &atlas_core::model::Household, window: &mut Window, cx: &mut App) -> Self {
        use atlas_core::ids::EntityRef;
        let mut holders = Vec::new();
        holders.extend(household.people.iter().map(|p| EntityRef::Person(p.id)));
        holders.extend(household.companies.iter().map(|c| EntityRef::Company(c.id)));
        let mut holder_names: Vec<SharedString> = vec!["All holders".into()];
        holder_names.extend(holders.iter().map(|e| SharedString::from(household.entity_name(*e))));
        let mut kind_names: Vec<SharedString> = vec!["All types".into()];
        kind_names.extend(ACCOUNT_KINDS.iter().map(|k| SharedString::from(k.label())));
        AccountsControls {
            search: cx.new(|cx| InputState::new(window, cx).placeholder("name or institution")),
            holder: scope::choice(holder_names, 0, window, cx),
            kind: scope::choice(kind_names, 0, window, cx),
            holders,
        }
    }
}

impl AtlasApp {
    /// Clears the Accounts register's display filters.
    pub fn clear_account_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.accounts_controls.search.update(cx, |s, cx| s.set_value("", window, cx));
        for choice in [&self.accounts_controls.holder, &self.accounts_controls.kind] {
            choice.update(cx, |s, cx| s.set_selected_index(Some(IndexPath::default()), window, cx));
        }
        cx.notify();
    }

    /// Asks before deleting an object, naming it and the consequence; the
    /// engine still refuses referenced deletions.
    pub fn confirm_delete(&mut self, object: ObjectRef, window: &mut Window, cx: &mut Context<Self>) {
        let household = &self.household;
        let (title, body, ok) = match object {
            ObjectRef::Account(id) => {
                let name = household.account(id).map(|a| a.name.clone()).unwrap_or_default();
                (format!("Delete “{name}”?"), "This cannot be undone. Planned movements, earmarks or recorded transactions that reference the account block the deletion.".to_string(), "Delete account")
            }
            ObjectRef::Series(id) => {
                let name = household.series_by_id(id).map(|s| s.name.clone()).unwrap_or_default();
                (format!("Delete “{name}”?"), "This cannot be undone. Its planned movements leave the forecast; reconciliation links or assumptions that reference it block the deletion.".to_string(), "Delete series")
            }
            ObjectRef::Reservation(id) => {
                let name = household.reservation(id).map(|r| r.name.clone()).unwrap_or_default();
                (format!("Delete “{name}”?"), "This removes the earmark without recording a payment. The settled balance will not change.".to_string(), "Delete earmark")
            }
            ObjectRef::Person(id) => {
                let name = household.person(id).map(|p| p.name.clone()).unwrap_or_default();
                if id == self.viewer.person {
                    window.push_notification("Choose another viewer before deleting the person who is looking.", cx);
                    return;
                }
                (format!("Delete {name}?"), "This cannot be undone. Accounts, companies or planned movements that reference the person block the deletion.".to_string(), "Delete person")
            }
            other => (format!("Delete {other}?"), "This cannot be undone.".to_string(), "Delete"),
        };
        confirm_danger(window, cx, title, body, ok, move |window, cx| with_app(cx, |app, cx| app.delete_object(object, window, cx)));
    }

    /// Asks before deleting a rule (rules are not household objects).
    pub fn confirm_delete_rule(&mut self, id: atlas_core::ids::RuleId, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.household.rule(id).map(|r| r.name.clone()).unwrap_or_else(|| id.to_string());
        confirm_danger(window, cx, format!("Delete “{name}”?"), "This cannot be undone; the rule's versions go with it. Forecasts are recomputed without it.", "Delete rule", move |window, cx| {
            with_app(cx, |app, cx| app.delete_rule(id, window, cx))
        });
    }

    /// Opens Sharing / Policies with `object`'s policy selected.
    pub fn open_policy_for(&mut self, object: ObjectRef, cx: &mut Context<Self>) {
        self.selected_policy = Some(object);
        self.navigate(Route::Policies, cx);
    }

    /// Opens the policy editor for an owned object (preselected when given).
    pub fn open_policy_editor_for(&mut self, object: Option<ObjectRef>, window: &mut Window, cx: &mut Context<Self>) {
        self.policy_target = object;
        self.open_policy_editor(window, cx);
    }

    /// The reservation form with `account` preselected.
    pub fn open_new_reservation_for(&mut self, account: Option<AccountId>, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = account
            && let Some(row) = self.reservation_form.account_row(id)
        {
            self.reservation_form.account.update(cx, |s, cx| s.set_selected_index(Some(IndexPath::new(row)), window, cx));
        }
        self.open_new_reservation(window, cx);
    }

    /// An entry form (series, actual) with `account` preselected.
    pub fn open_entry_with_account(&mut self, entry: Entry, account: AccountId, window: &mut Window, cx: &mut Context<Self>) {
        let row = self.entry_forms.account_row(account);
        if let Some(row) = row {
            let choice: Option<&Choice> = match entry {
                Entry::Series => Some(&self.entry_forms.series_account),
                Entry::Actual => Some(&self.entry_forms.actual_account),
                _ => None,
            };
            if let Some(choice) = choice {
                choice.update(cx, |s, cx| s.set_selected_index(Some(IndexPath::new(row)), window, cx));
            }
        }
        self.open_entry(entry, window, cx);
    }

    /// An entry form (account, series) with `person` preselected as owner/entity.
    pub fn open_entry_for_person(&mut self, entry: Entry, person: PersonId, window: &mut Window, cx: &mut Context<Self>) {
        match entry {
            Entry::Account => {
                if let Some(row) = self.entry_forms.person_row(person) {
                    self.entry_forms.account_holder.update(cx, |s, cx| s.set_selected_index(Some(IndexPath::new(row)), window, cx));
                    self.entry_forms.draft.update(cx, |d, _| d.account_company = false);
                }
            }
            Entry::Series => {
                if let Some(row) = self.entry_forms.entity_row(atlas_core::ids::EntityRef::Person(person)) {
                    self.entry_forms.series_entity.update(cx, |s, cx| s.set_selected_index(Some(IndexPath::new(row)), window, cx));
                    self.entry_forms.draft.update(cx, |d, _| d.series_direction = 0);
                }
            }
            _ => {}
        }
        self.open_entry(entry, window, cx);
    }

    /// The account form with `company` preselected as holder.
    pub fn open_account_for_company(&mut self, company: CompanyId, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(row) = self.entry_forms.company_row(company) {
            self.entry_forms.account_company.update(cx, |s, cx| s.set_selected_index(Some(IndexPath::new(row)), window, cx));
            self.entry_forms.draft.update(cx, |d, _| d.account_company = true);
        }
        self.open_entry(Entry::Account, window, cx);
    }

    /// Activity / Actuals filtered to one account.
    pub fn open_actuals_for(&mut self, account: AccountId, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(row) = self.activity_controls.accounts.iter().position(|a| *a == account) {
            Self::set_choice(&self.activity_controls.actuals_account, row + 1, window, cx);
        }
        self.set_actuals_account(Some(account), cx);
        self.navigate(Route::Actuals, cx);
    }

    /// Rules & taxes / Taxes with the entity display filter set.
    pub fn open_taxes_for_entity(&mut self, entity: atlas_core::ids::EntityRef, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_tax_entity_filter(entity, window, cx);
        self.navigate(Route::Taxes, cx);
    }

    /// Activity / Upcoming narrowed to one series.
    pub fn open_upcoming_for_series(&mut self, series: SeriesId, cx: &mut Context<Self>) {
        self.set_timeline_series(Some(series), cx);
        self.navigate(Route::Upcoming, cx);
    }

    /// Forecast / Derive from history on a series.
    pub fn open_derive_for_series(&mut self, series: SeriesId, cx: &mut Context<Self>) {
        self.derivation_series = Some(series);
        self.refresh_assumptions();
        self.navigate(Route::Derive, cx);
    }

    /// The assumption form with `series` preselected as what it applies to.
    pub fn open_assumption_for_series(&mut self, series: SeriesId, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(row) = self.entry_forms.series_row(series) {
            Self::set_choice(&self.entry_forms.assumption_series, row + 1, window, cx);
        }
        self.open_entry(Entry::Assumption, window, cx);
    }

    /// Forecast / Path with the account's path selected: the household path
    /// for a personal account, the company's path for a company account.
    pub fn open_forecast_for_account(&mut self, account: AccountId, company: Option<CompanyId>, cx: &mut Context<Self>) {
        let boundary = match company {
            Some(id) if self.household.disclosure_for(self.viewer, ObjectRef::Company(id)) == Disclosure::Full => Boundary::Company(id),
            _ => Boundary::Household,
        };
        self.projection_scenario = false;
        self.select_projection_boundary(boundary, cx);
        self.forecast_selected_account = Some(account);
        self.forecast_report_tab = 0;
        self.navigate(Route::ForecastPath, cx);
    }

    /// Forecast / Sensitivity on this personal cash account's path.
    pub fn open_sensitivity_for_account(&mut self, account: AccountId, cx: &mut Context<Self>) {
        self.select_sensitivity_boundary(Boundary::Account(account), cx);
        self.navigate(Route::Sensitivity, cx);
    }

    /// Forecast / Path on a person's or company's boundary.
    pub fn open_forecast_for_boundary(&mut self, boundary: Boundary, cx: &mut Context<Self>) {
        self.projection_scenario = false;
        self.select_projection_boundary(boundary, cx);
        self.navigate(Route::ForecastPath, cx);
    }

    /// Accounts / Earmarks on a boundary.
    pub fn open_earmarks_for_boundary(&mut self, boundary: Boundary, cx: &mut Context<Self>) {
        self.select_boundary(boundary, cx);
        self.navigate(Route::Earmarks, cx);
    }

    /// The boundaries the sensitivity tool accepts for this viewer.
    pub fn sensitivity_boundaries(&self) -> Vec<Boundary> {
        let household = &self.household;
        let viewer = self.viewer;
        let mut boundaries = vec![Boundary::Household];
        boundaries.extend(
            household
                .accounts
                .iter()
                .filter(|a| a.kind.is_cash() && !a.is_company_account())
                .filter(|a| matches!(household.disclosure_for(viewer, ObjectRef::Account(a.id)), Disclosure::Full | Disclosure::SelectedFields))
                .map(|a| Boundary::Account(a.id)),
        );
        boundaries
    }

    /// Selects a row of a select control (tests and contextual openers).
    pub fn set_choice(choice: &Entity<SelectState<Vec<SharedString>>>, row: usize, window: &mut Window, cx: &mut App) {
        choice.update(cx, |s, cx| s.set_selected_index(Some(IndexPath::new(row)), window, cx));
    }
}
