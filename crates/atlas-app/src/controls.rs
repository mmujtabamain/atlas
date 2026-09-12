//! Retained scope controls shared by several screens: the `Plan` selectors
//! (Baseline / With the first visible scenario), the `Whose money` choice of
//! the earmarks screen, and the one place every control subscription is
//! (re)built when the household or the viewer changes.

use atlas_core::authz::Viewer;
use atlas_core::liquidity::Boundary;
use atlas_core::model::Household;
use gpui_kit::component::{input::{InputEvent, InputState}, select::SelectEvent, table::TableEvent};
use gpui_kit::*;

use crate::app::AtlasApp;
use crate::widgets::scope::{self, Choice};

/// One `Plan` selector per analysis that accepts the overlay.
pub struct PlanChoices {
    pub forecast: Choice,
    pub upcoming: Choice,
    pub series: Choice,
    pub sensitivity: Choice,
    pub taxes: Choice,
    pub rules: Choice,
    pub funding: Choice,
    /// The scenario the second option stands for, if any.
    pub overlay: Option<atlas_core::ids::ScenarioId>,
}

impl PlanChoices {
    pub fn items(household: &Household, viewer: Viewer) -> (Vec<SharedString>, Option<atlas_core::ids::ScenarioId>) {
        let overlay = crate::models::overlay_scenario(household, viewer);
        let mut items: Vec<SharedString> = vec!["Baseline".into()];
        if let Some(name) = overlay.and_then(|id| household.scenario(id)).map(|s| s.name.clone()) {
            items.push(SharedString::from(format!("With {name}")));
        }
        (items, overlay)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(household: &Household, viewer: Viewer, forecast_on: bool, upcoming_on: bool, sensitivity_on: bool, taxes_on: bool, rules_on: bool, window: &mut Window, cx: &mut App) -> Self {
        let (items, overlay) = Self::items(household, viewer);
        let row = |on: bool| if on && items.len() > 1 { 1 } else { 0 };
        PlanChoices {
            forecast: scope::choice(items.clone(), row(forecast_on), window, cx),
            upcoming: scope::choice(items.clone(), row(upcoming_on), window, cx),
            series: scope::choice(items.clone(), row(upcoming_on), window, cx),
            sensitivity: scope::choice(items.clone(), row(sensitivity_on), window, cx),
            taxes: scope::choice(items.clone(), row(taxes_on), window, cx),
            rules: scope::choice(items.clone(), row(rules_on), window, cx),
            funding: scope::choice(items.clone(), row(rules_on), window, cx),
            overlay,
        }
    }
}

/// Retained controls of the Activity workspace: the series name search and
/// the account filter of the actual-transactions register.
pub struct ActivityControls {
    pub series_search: Entity<InputState>,
    pub actuals_account: Choice,
    /// The accounts behind the `actuals_account` rows (row 0 is all).
    pub accounts: Vec<atlas_core::ids::AccountId>,
}

impl ActivityControls {
    pub fn new(household: &Household, viewer: Viewer, current: Option<atlas_core::ids::AccountId>, window: &mut Window, cx: &mut App) -> Self {
        use atlas_core::ids::ObjectRef;
        let accounts: Vec<atlas_core::ids::AccountId> = household
            .accounts
            .iter()
            .filter(|a| matches!(household.disclosure_for(viewer, ObjectRef::Account(a.id)), atlas_core::Disclosure::Full | atlas_core::Disclosure::SelectedFields))
            .map(|a| a.id)
            .collect();
        let mut items: Vec<SharedString> = vec!["All disclosed accounts".into()];
        items.extend(accounts.iter().filter_map(|id| household.account(*id)).map(|a| SharedString::from(a.name.clone())));
        let selected = current.and_then(|id| accounts.iter().position(|a| *a == id)).map(|p| p + 1).unwrap_or(0);
        ActivityControls {
            series_search: cx.new(|cx| InputState::new(window, cx).placeholder("name")),
            actuals_account: scope::choice(items, selected, window, cx),
            accounts,
        }
    }
}

/// Retained controls of Forecast / Assumptions, Derive and Sensitivity.
pub struct AssumptionControls {
    /// All / This run / one series per row.
    pub applies: Choice,
    pub applies_series: Vec<atlas_core::ids::SeriesId>,
    /// All / Fresh / Stale / Not accepted / Expired.
    pub freshness: Choice,
    /// Series with derivation history the viewer may see.
    pub derive_series: Choice,
    pub derive_series_ids: Vec<atlas_core::ids::SeriesId>,
    /// Household or one personal cash account.
    pub sensitivity_path: Choice,
    pub sensitivity_paths: Vec<Boundary>,
}

impl AssumptionControls {
    pub fn new(household: &Household, viewer: Viewer, derivation_series: Option<atlas_core::ids::SeriesId>, sensitivity: Boundary, window: &mut Window, cx: &mut App) -> Self {
        use atlas_core::ids::ObjectRef;
        let visible_series = |id: atlas_core::ids::SeriesId| !matches!(household.disclosure_for(viewer, ObjectRef::Series(id)), atlas_core::Disclosure::Hidden | atlas_core::Disclosure::Aggregate);
        let applies_series: Vec<atlas_core::ids::SeriesId> = household.series.iter().filter(|s| visible_series(s.id)).map(|s| s.id).collect();
        let mut applies_items: Vec<SharedString> = vec!["All".into(), "This forecast run".into()];
        applies_items.extend(applies_series.iter().filter_map(|id| household.series_by_id(*id)).map(|s| SharedString::from(s.name.clone())));
        let freshness_items: Vec<SharedString> = ["All", "Fresh", "Stale", "Not accepted", "Expired"].into_iter().map(SharedString::from).collect();
        let mut derive_series_ids: Vec<atlas_core::ids::SeriesId> = household.series.iter().filter(|s| !household.history_of(s.id).is_empty()).filter(|s| visible_series(s.id)).map(|s| s.id).collect();
        derive_series_ids.sort_by_key(|id| id.raw());
        let derive_items: Vec<SharedString> = if derive_series_ids.is_empty() {
            vec!["No series has history".into()]
        } else {
            derive_series_ids.iter().filter_map(|id| household.series_by_id(*id)).map(|s| SharedString::from(s.name.clone())).collect()
        };
        let derive_selected = derivation_series.and_then(|id| derive_series_ids.iter().position(|s| *s == id)).unwrap_or(0);
        let mut sensitivity_paths = vec![Boundary::Household];
        sensitivity_paths.extend(
            household
                .accounts
                .iter()
                .filter(|a| a.kind.is_cash() && !a.is_company_account())
                .filter(|a| matches!(household.disclosure_for(viewer, ObjectRef::Account(a.id)), atlas_core::Disclosure::Full | atlas_core::Disclosure::SelectedFields))
                .map(|a| Boundary::Account(a.id)),
        );
        let path_items: Vec<SharedString> = sensitivity_paths.iter().map(|b| SharedString::from(b.label(household))).collect();
        let path_selected = sensitivity_paths.iter().position(|b| *b == sensitivity).unwrap_or(0);
        AssumptionControls {
            applies: scope::choice(applies_items, 0, window, cx),
            applies_series,
            freshness: scope::choice(freshness_items, 0, window, cx),
            derive_series: scope::choice(derive_items, derive_selected, window, cx),
            derive_series_ids,
            sensitivity_path: scope::choice(path_items, path_selected, window, cx),
            sensitivity_paths,
        }
    }
}

/// The `Case` choice: conservative / expected / optimistic.
pub fn case_choice(current: atlas_core::forecast::Case, window: &mut Window, cx: &mut App) -> Choice {
    use atlas_core::forecast::Case;
    let items: Vec<SharedString> = Case::ALL.iter().map(|c| SharedString::from(c.label())).collect();
    let selected = Case::ALL.iter().position(|c| *c == current).unwrap_or(1);
    scope::choice(items, selected, window, cx)
}

/// The boundaries the earmarks screen offers this viewer.
pub fn boundaries_for(household: &Household, viewer: Viewer) -> Vec<Boundary> {
    use atlas_core::ids::ObjectRef;
    let mut boundaries = vec![Boundary::Household];
    boundaries.extend(household.people.iter().map(|p| Boundary::Person(p.id)));
    boundaries.extend(household.companies.iter().filter(|c| household.disclosure_for(viewer, ObjectRef::Company(c.id)) == atlas_core::Disclosure::Full).map(|c| Boundary::Company(c.id)));
    boundaries
}

/// Builds the `Whose money` choice with `current` selected.
pub fn boundary_choice(household: &Household, viewer: Viewer, current: Boundary, window: &mut Window, cx: &mut App) -> (Choice, Vec<Boundary>) {
    let boundaries = boundaries_for(household, viewer);
    let items: Vec<SharedString> = boundaries.iter().map(|b| SharedString::from(b.label(household))).collect();
    let selected = boundaries.iter().position(|b| *b == current).unwrap_or(0);
    (scope::choice(items, selected, window, cx), boundaries)
}

impl AtlasApp {
    /// Rebuilds every subscription behind the retained controls. Called once
    /// with the window and again whenever the controls are recreated.
    pub(crate) fn subscribe_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut subscriptions: Vec<Subscription> = self
            .timeline_controls
            .all()
            .iter()
            .map(|state| {
                cx.subscribe_in(state, window, |this, _, event: &SelectEvent<Vec<SharedString>>, _, cx| {
                    let SelectEvent::Confirm(_) = event;
                    this.apply_timeline_filters(cx);
                })
            })
            .collect();
        subscriptions.push(cx.subscribe_in(&self.tax_controls.e05_amount, window, |this, state, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                let text = state.read(cx).value().to_string();
                this.set_e05_amount_text(&text, cx);
            }
        }));
        // Display filters only re-render.
        subscriptions.push(cx.subscribe_in(&self.accounts_controls.search, window, |_, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        }));
        for choice in [&self.accounts_controls.holder, &self.accounts_controls.kind] {
            subscriptions.push(cx.subscribe_in(choice, window, |_, _, event: &SelectEvent<Vec<SharedString>>, _, cx| {
                let SelectEvent::Confirm(_) = event;
                cx.notify();
            }));
        }
        subscriptions.push(cx.subscribe_in(&self.activity_controls.series_search, window, |_, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        }));
        subscriptions.push(cx.subscribe_in(&self.activity_controls.actuals_account, window, |this, state, event: &SelectEvent<Vec<SharedString>>, _, cx| {
            let SelectEvent::Confirm(_) = event;
            let row = state.read(cx).selected_index(cx).map(|p| p.row).unwrap_or(0);
            let account = if row == 0 { None } else { this.activity_controls.accounts.get(row - 1).copied() };
            this.set_actuals_account(account, cx);
        }));
        // Row selection on the virtualised registers opens the inspectors.
        subscriptions.push(cx.subscribe_in(&self.grids.timeline_occurrences, window, |this, _, event: &TableEvent, _, cx| {
            if let TableEvent::SelectRow(row) = event {
                this.select_occurrence_row(*row, cx);
            }
        }));
        subscriptions.push(cx.subscribe_in(&self.grids.timeline_actuals, window, |this, _, event: &TableEvent, _, cx| {
            if let TableEvent::SelectRow(row) = event {
                this.select_actual_row(*row, cx);
            }
        }));
        // Forecast / Assumptions, Derive, Sensitivity.
        for choice in [&self.assumption_controls.applies, &self.assumption_controls.freshness] {
            subscriptions.push(cx.subscribe_in(choice, window, |_, _, event: &SelectEvent<Vec<SharedString>>, _, cx| {
                let SelectEvent::Confirm(_) = event;
                cx.notify();
            }));
        }
        subscriptions.push(cx.subscribe_in(&self.assumption_controls.derive_series, window, |this, state, event: &SelectEvent<Vec<SharedString>>, _, cx| {
            let SelectEvent::Confirm(_) = event;
            let row = state.read(cx).selected_index(cx).map(|p| p.row).unwrap_or(0);
            if let Some(series) = this.assumption_controls.derive_series_ids.get(row).copied() {
                this.select_derivation_series(series, cx);
            }
        }));
        subscriptions.push(cx.subscribe_in(&self.assumption_controls.sensitivity_path, window, |this, state, event: &SelectEvent<Vec<SharedString>>, _, cx| {
            let SelectEvent::Confirm(_) = event;
            let row = state.read(cx).selected_index(cx).map(|p| p.row).unwrap_or(0);
            if let Some(boundary) = this.assumption_controls.sensitivity_paths.get(row).copied() {
                this.select_sensitivity_boundary(boundary, cx);
            }
        }));
        // Forecast / Path scope and chart states.
        subscriptions.push(cx.subscribe_in(&self.forecast_boundary_choice, window, |this, state, event: &SelectEvent<Vec<SharedString>>, _, cx| {
            let SelectEvent::Confirm(_) = event;
            let row = state.read(cx).selected_index(cx).map(|p| p.row).unwrap_or(0);
            if let Some(boundary) = this.forecast_boundaries.get(row).copied() {
                this.forecast_selected_account = None;
                this.select_projection_boundary(boundary, cx);
            }
        }));
        subscriptions.push(cx.subscribe_in(&self.forecast_case_choice, window, |this, state, event: &SelectEvent<Vec<SharedString>>, _, cx| {
            let SelectEvent::Confirm(_) = event;
            let row = state.read(cx).selected_index(cx).map(|p| p.row).unwrap_or(1);
            if let Some(case) = atlas_core::forecast::Case::ALL.get(row).copied() {
                this.select_projection_case(case, cx);
            }
        }));
        for state in [&self.forecast_path_state, &self.account_path_state] {
            subscriptions.push(cx.observe(state, |_, _, cx| cx.notify()));
        }
        subscriptions.push(cx.subscribe_in(&self.grids.forecast_values, window, |this, _, event: &TableEvent, _, cx| {
            if let TableEvent::SelectRow(row) = event {
                let row = *row;
                this.forecast_path_state.update(cx, |s, cx| {
                    s.selected = Some(row);
                    cx.notify();
                });
            }
        }));
        // Whose money on the earmarks screen.
        subscriptions.push(cx.subscribe_in(&self.boundary_choice, window, |this, state, event: &SelectEvent<Vec<SharedString>>, _, cx| {
            let SelectEvent::Confirm(_) = event;
            let row = state.read(cx).selected_index(cx).map(|p| p.row).unwrap_or(0);
            if let Some(boundary) = this.boundaries.get(row).copied() {
                this.select_boundary(boundary, cx);
            }
        }));
        // Plan selectors: row 1 is "With <scenario>".
        let plans: [(Choice, fn(&mut AtlasApp, bool, &mut Context<AtlasApp>)); 7] = [
            (self.plan_choices.forecast.clone(), |app, on, cx| app.set_projection_scenario(on, cx)),
            (self.plan_choices.upcoming.clone(), |app, on, cx| app.set_timeline_scenario(on, cx)),
            (self.plan_choices.series.clone(), |app, on, cx| app.set_timeline_scenario(on, cx)),
            (self.plan_choices.sensitivity.clone(), |app, on, cx| app.set_sensitivity_scenario(on, cx)),
            (self.plan_choices.taxes.clone(), |app, on, cx| app.set_tax_scenario(on, cx)),
            (self.plan_choices.rules.clone(), |app, on, cx| app.set_rules_scenario(on, cx)),
            (self.plan_choices.funding.clone(), |app, on, cx| app.set_rules_scenario(on, cx)),
        ];
        for (choice, apply) in plans {
            subscriptions.push(cx.subscribe_in(&choice, window, move |this, state, event: &SelectEvent<Vec<SharedString>>, _, cx| {
                let SelectEvent::Confirm(_) = event;
                let on = state.read(cx).selected_index(cx).map(|p| p.row).unwrap_or(0) == 1;
                apply(this, on, cx);
            }));
        }
        self._subscriptions = subscriptions;
    }
}
