//! Scenario editing dialogs (M8, §18.2–§18.3): add an explicit change to a
//! scenario, and compose scenarios into a new one after the compatibility
//! check. Scenario creation itself is the generic entry dialog.

use atlas_core::ids::*;
use atlas_core::model::{Employee, Household};
use atlas_core::scenario::ScenarioChange;
use atlas_core::timeline::AmountSpec;
use atlas_core::Money;
use gpui_kit::component::{
    IndexPath, WindowExt as _,
    button::{Button, ButtonVariants as _},
    date_picker::{DatePicker, DatePickerState},
    dialog::DialogFooter,
    form::{Field, Form},
    input::{Input, InputState},
    radio::RadioGroup,
    select::{Select, SelectState},
    v_flex,
};
use gpui_kit::*;

use crate::alerting::{self, Level};
use crate::app::AtlasApp;

type Choice = Entity<SelectState<Vec<SharedString>>>;

/// Change kinds of the editor, in radio order.
pub const CHANGE_KINDS: [&str; 5] = ["End a stream after a date", "Remove the events", "Change the amount from a date", "Move the dates by days", "Add employment at a company"];

#[derive(Debug, Clone, Default)]
pub struct ScenarioDraft {
    pub kind: usize,
}

/// Retained state of the scenario dialogs; rebuilt when the household changes.
pub struct ScenarioForms {
    pub draft: Entity<ScenarioDraft>,
    pub scenario: Choice,
    pub series: Choice,
    pub company: Choice,
    pub date: Entity<DatePickerState>,
    pub amount: Entity<InputState>,
    pub days: Entity<InputState>,
    pub employee_name: Entity<InputState>,
    pub monthly_gross: Entity<InputState>,
    pub reason: Entity<InputState>,
    pub compose_name: Entity<InputState>,
    scenario_ids: Vec<ScenarioId>,
    series_ids: Vec<SeriesId>,
    company_ids: Vec<CompanyId>,
}

fn choice(items: Vec<SharedString>, window: &mut Window, cx: &mut Context<AtlasApp>) -> Choice {
    cx.new(|cx| SelectState::new(items, Some(IndexPath::default()), window, cx))
}

fn text(placeholder: &str, window: &mut Window, cx: &mut Context<AtlasApp>) -> Entity<InputState> {
    let placeholder = placeholder.to_string();
    cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
}

fn selected_row(state: &Choice, cx: &App) -> usize {
    state.read(cx).selected_index(cx).map(|p| p.row).unwrap_or(0)
}

impl ScenarioForms {
    pub fn new(household: &Household, window: &mut Window, cx: &mut Context<AtlasApp>) -> Self {
        let scenario_ids: Vec<ScenarioId> = household.scenarios.iter().map(|s| s.id).collect();
        let scenario_names: Vec<SharedString> = household.scenarios.iter().map(|s| SharedString::from(s.name.clone())).collect();
        let series_ids: Vec<SeriesId> = household.series.iter().filter(|s| s.scenario.is_none()).map(|s| s.id).collect();
        let series_names: Vec<SharedString> = household.series.iter().filter(|s| s.scenario.is_none()).map(|s| SharedString::from(s.name.clone())).collect();
        let company_ids: Vec<CompanyId> = household.companies.iter().map(|c| c.id).collect();
        let company_names: Vec<SharedString> = household.companies.iter().map(|c| SharedString::from(c.name.clone())).collect();
        ScenarioForms {
            draft: cx.new(|_| ScenarioDraft::default()),
            scenario: choice(scenario_names, window, cx),
            series: choice(series_names, window, cx),
            company: choice(company_names, window, cx),
            date: cx.new(|cx| DatePickerState::new(window, cx).date_format("%d %b %Y")),
            amount: text("new amount, e.g. 600,000", window, cx),
            days: text("e.g. 14 (later) or -7 (earlier)", window, cx),
            employee_name: text("e.g. Replacement hire", window, cx),
            monthly_gross: text("monthly gross, e.g. 400,000", window, cx),
            reason: text("why — shown in every overlay listing", window, cx),
            compose_name: text("e.g. Leave job + buy car", window, cx),
            scenario_ids,
            series_ids,
            company_ids,
        }
    }
}

impl AtlasApp {
    /// Opens the "Add change to scenario" dialog.
    pub fn open_scenario_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.entity().downgrade();
        let f = &self.scenario_forms;
        let as_of = self.household.as_of;
        f.date.update(cx, |s, cx| s.set_date(as_of, window, cx));
        let fields = (f.draft.clone(), f.scenario.clone(), f.series.clone(), f.company.clone(), f.date.clone(), f.amount.clone(), f.days.clone(), f.employee_name.clone(), f.monthly_gross.clone(), f.reason.clone());
        window.open_dialog(cx, move |dialog, window, cx| {
            let this = this.clone();
            let wide_width = window.rem_size() * 48.;
            let (draft, scenario, series, company, date, amount, days, employee_name, monthly_gross, reason) = fields.clone();
            let kind = draft.read(cx).kind;
            let d_kind = draft.clone();
            let detail: Vec<Field> = match kind {
                0 => vec![Field::new().label("Series").child(Select::new(&series)), Field::new().label("Last occurrence on or before").child(DatePicker::new(&date))],
                1 => vec![Field::new().label("Series").child(Select::new(&series))],
                2 => vec![
                    Field::new().label("Series").child(Select::new(&series)),
                    Field::new().label("From").child(DatePicker::new(&date)),
                    Field::new().label("New amount").required(true).child(Input::new(&amount).id("scenario-change-amount")),
                ],
                3 => vec![Field::new().label("Series").child(Select::new(&series)), Field::new().label("Days").required(true).child(Input::new(&days).id("scenario-change-days"))],
                _ => vec![
                    Field::new().label("Company").child(Select::new(&company)),
                    Field::new().label("Employee").required(true).child(Input::new(&employee_name).id("scenario-change-employee")),
                    Field::new().label("Monthly gross").required(true).child(Input::new(&monthly_gross).id("scenario-change-gross")),
                    Field::new().label("Starts").child(DatePicker::new(&date)),
                ],
            };
            dialog
                .title("Add a change to a scenario")
                .w(wide_width)
                .max_h(relative(0.9))
                .child(
                    v_flex().child(
                        Form::vertical()
                            .columns(2)
                            .child(Field::new().label("Scenario").child(Select::new(&scenario)))
                            .child(
                                Field::new().label("Kind of change").child(
                                    RadioGroup::vertical("scenario-change-kind")
                                        .children(CHANGE_KINDS)
                                        .selected_index(Some(kind))
                                        .on_change(move |index, _, cx| d_kind.update(cx, |d, cx| { d.kind = *index; cx.notify(); })),
                                ),
                            )
                            .children(detail)
                            .child(Field::new().label("Reason").child(Input::new(&reason).id("scenario-change-reason"))),
                    ),
                )
                .footer(
                    DialogFooter::new()
                        .child(Button::new("scenario-change-cancel").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                        .child(Button::new("scenario-change-save").primary().label("Add change").on_click({
                            let this = this.clone();
                            move |_, window, cx| {
                                Self::confirm_scenario_change(&this, window, cx);
                            }
                        })),
                )
                .on_ok(move |_, window, cx| Self::confirm_scenario_change(&this, window, cx))
        });
    }

    fn confirm_scenario_change(this: &WeakEntity<Self>, window: &mut Window, cx: &mut App) -> bool {
        match this.update(cx, |app, cx| app.submit_scenario_change(cx)) {
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

    /// Builds the change from the dialog and adds it through the engine.
    pub fn submit_scenario_change(&mut self, cx: &mut Context<Self>) -> Result<String, String> {
        let currency = self.household.base_currency;
        let f = &self.scenario_forms;
        let scenario = *f.scenario_ids.get(selected_row(&f.scenario, cx)).ok_or("Create a scenario first.")?;
        let series = || f.series_ids.get(selected_row(&f.series, cx)).copied().ok_or_else(|| "Add a baseline series first.".to_string());
        let date = f.date.read(cx).date().start().unwrap_or(self.household.as_of);
        let reason = {
            let r = f.reason.read(cx).value().trim().to_string();
            if r.is_empty() { "no reason given".to_string() } else { r }
        };
        let change = match f.draft.read(cx).kind {
            0 => ScenarioChange::EndSeries { series: series()?, last_on: date, reason },
            1 => ScenarioChange::RemoveSeries { series: series()?, reason },
            2 => {
                let text = f.amount.read(cx).value().trim().to_string();
                let amount = Money::parse(&text, currency).map_err(|e| format!("New amount: {e}"))?;
                ScenarioChange::ChangeAmount { series: series()?, from: date, amount: AmountSpec::Exact(amount), reason }
            }
            3 => {
                let text = f.days.read(cx).value().trim().to_string();
                let days: i64 = text.parse().map_err(|_| format!("Days {text:?} is not a whole number."))?;
                if days == 0 {
                    return Err("Moving by 0 days changes nothing.".into());
                }
                ScenarioChange::MoveDates { series: series()?, days, reason }
            }
            _ => {
                let company = *f.company_ids.get(selected_row(&f.company, cx)).ok_or("Add a company first.")?;
                let name = f.employee_name.read(cx).value().trim().to_string();
                if name.is_empty() {
                    return Err("Name the employee.".into());
                }
                let gross = Money::parse(f.monthly_gross.read(cx).value().trim(), currency).map_err(|e| format!("Monthly gross: {e}"))?;
                ScenarioChange::AddEmployment { company, employee: Employee { name, person: None, monthly_gross: gross, start: date, end: None } }
            }
        };
        let described = change.describe(&self.household);
        let scenario_name = self.household.scenario(scenario).map(|s| s.name.clone()).unwrap_or_default();
        match self.household.add_scenario_change(scenario, change) {
            Ok(()) => {
                log::info!("scenario {scenario} “{scenario_name}”: change added — {described}");
                self.mark_dirty();
                self.refresh_derived();
                cx.notify();
                Ok(format!("“{scenario_name}”: {described}. Comparisons recomputed."))
            }
            Err(err) => {
                alerting::report(Level::Warning, format!("scenario change refused for {scenario}: {err}"));
                Err(format!("Not added — {err}"))
            }
        }
    }

    /// Opens the "Compose scenarios" dialog over the current selection.
    pub fn open_compose_scenarios(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.entity().downgrade();
        let name = self.scenario_forms.compose_name.clone();
        let members: Vec<String> = self.scenario_selection.iter().filter_map(|id| self.household.scenario(*id)).map(|s| s.name.clone()).collect();
        let problems = self.household.check_compatibility(&self.scenario_selection);
        let problem_text: Vec<String> = problems
            .iter()
            .map(|p| format!("{} × {}: {}", self.household.scenario(p.first).map(|s| s.name.clone()).unwrap_or_default(), self.household.scenario(p.second).map(|s| s.name.clone()).unwrap_or_default(), p.reason))
            .collect();
        window.open_dialog(cx, move |dialog, _, _| {
            let this = this.clone();
            dialog
                .title("Combine the selected scenarios")
                .child(
                    v_flex()
                        .gap_3()
                        .child(div().text_sm().child(format!("Members: {}", if members.is_empty() { "none selected".to_string() } else { members.join(" + ") })))
                        .child(if problem_text.is_empty() {
                            div().text_sm().child("Compatible: no two changes touch the same series, company or tax rule.").into_any_element()
                        } else {
                            v_flex().gap_1().text_sm().children(problem_text.iter().map(|p| div().child(format!("Incompatible — {p}")))).into_any_element()
                        })
                        .child(Form::vertical().child(Field::new().label("Name of the composition").required(true).child(Input::new(&name).id("compose-name")))),
                )
                .footer(
                    DialogFooter::new()
                        .child(Button::new("compose-cancel").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                        .child(Button::new("compose-save").primary().label("Compose").on_click({
                            let this = this.clone();
                            move |_, window, cx| {
                                Self::confirm_compose(&this, window, cx);
                            }
                        })),
                )
                .on_ok(move |_, window, cx| Self::confirm_compose(&this, window, cx))
        });
    }

    fn confirm_compose(this: &WeakEntity<Self>, window: &mut Window, cx: &mut App) -> bool {
        match this.update(cx, |app, cx| app.submit_compose(cx)) {
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

    pub fn submit_compose(&mut self, cx: &mut Context<Self>) -> Result<String, String> {
        let name = self.scenario_forms.compose_name.read(cx).value().trim().to_string();
        if name.is_empty() {
            return Err("Name the composition.".into());
        }
        let members = self.scenario_selection.clone();
        match self.household.compose_scenarios(&name, &members, self.viewer.person) {
            Ok(id) => {
                log::info!("composed scenario {id} “{name}” from {members:?}");
                self.scenario_selection = vec![id];
                self.mark_dirty();
                self.refresh_derived();
                cx.notify();
                Ok(format!("“{name}” composed ({id}); it is now the compared scenario."))
            }
            Err(err) => {
                alerting::report(Level::Warning, format!("composition “{name}” refused: {err}"));
                Err(format!("Not composed — {err}"))
            }
        }
    }
}
