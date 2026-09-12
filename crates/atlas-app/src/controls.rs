//! Retained scope controls shared by several screens: the `Plan` selectors
//! (Baseline / With the first visible scenario), the `Whose money` choice of
//! the earmarks screen, and the one place every control subscription is
//! (re)built when the household or the viewer changes.

use atlas_core::authz::Viewer;
use atlas_core::liquidity::Boundary;
use atlas_core::model::Household;
use gpui_kit::component::{input::InputEvent, select::SelectEvent};
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
