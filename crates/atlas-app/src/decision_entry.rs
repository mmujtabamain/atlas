//! The decision builder's form state (M9): one retained input per field of
//! the concrete plan, read back step by step into a [`PurchasePlan`]. The
//! builder runs as a stepper on the Decisions screen; every "Next" validates
//! the current step through the plan's own rules before moving on.

use atlas_core::decision::{CompanyRoute, ExtractionMethod, Financing, FundingSource, Objective, OtherCost, PurchasePlan, RunningCost};
use atlas_core::ids::*;
use atlas_core::model::Household;
use atlas_core::Money;
use chrono::{Days, Months, NaiveDate};
use gpui_kit::component::{
    IndexPath, WindowExt as _,
    date_picker::DatePickerState,
    input::InputState,
    select::SelectState,
};
use gpui_kit::*;

use crate::app::AtlasApp;

type Choice = Entity<SelectState<Vec<SharedString>>>;

/// Step titles, in order.
pub const STEPS: [&str; 5] = ["Purchase", "Down payment", "Recurring payment", "Other costs", "Result"];

/// Stateless-control values of the builder.
#[derive(Debug, Clone, Default)]
pub struct DecisionDraft {
    pub objective: usize,
    pub source_allowed: Vec<bool>,
    pub route_allowed: Vec<bool>,
    pub financing: bool,
    pub other_cost: bool,
    pub running_cost: bool,
}

/// Retained inputs of every step.
pub struct DecisionForm {
    pub draft: Entity<DecisionDraft>,
    // step 1
    pub name: Entity<InputState>,
    pub price: Entity<InputState>,
    pub purchase_on: Entity<DatePickerState>,
    pub window_from: Entity<DatePickerState>,
    pub window_to: Entity<DatePickerState>,
    pub reserve: Entity<InputState>,
    // step 2
    pub down_payment: Entity<InputState>,
    pub down_low: Entity<InputState>,
    pub down_high: Entity<InputState>,
    pub down_step: Entity<InputState>,
    pub source_floors: Vec<(AccountId, Entity<InputState>)>,
    pub routes: Vec<(CompanyId, Choice)>,
    pub max_tax: Entity<InputState>,
    // step 3
    pub months: Entity<InputState>,
    pub rate: Entity<InputState>,
    pub first_instalment: Entity<DatePickerState>,
    pub financing_account: Choice,
    // step 4
    pub other_label: Entity<InputState>,
    pub other_amount: Entity<InputState>,
    pub other_on: Entity<DatePickerState>,
    pub other_account: Choice,
    pub running_label: Entity<InputState>,
    pub running_monthly: Entity<InputState>,
    pub running_from: Entity<DatePickerState>,
    pub running_account: Choice,
    pub personal_accounts: Vec<AccountId>,
    pub all_accounts: Vec<AccountId>,
}

fn text(value: String, placeholder: &str, window: &mut Window, cx: &mut Context<AtlasApp>) -> Entity<InputState> {
    let placeholder = placeholder.to_string();
    cx.new(|cx| InputState::new(window, cx).placeholder(placeholder).default_value(value))
}

fn date(value: NaiveDate, window: &mut Window, cx: &mut Context<AtlasApp>) -> Entity<DatePickerState> {
    cx.new(|cx| {
        let mut state = DatePickerState::new(window, cx).date_format("%d %b %Y");
        state.set_date(value, window, cx);
        state
    })
}

fn choice(items: Vec<SharedString>, selected: usize, window: &mut Window, cx: &mut Context<AtlasApp>) -> Choice {
    cx.new(|cx| SelectState::new(items, Some(IndexPath::default().row(selected)), window, cx))
}

fn selected_row(state: &Choice, cx: &App) -> usize {
    state.read(cx).selected_index(cx).map(|p| p.row).unwrap_or(0)
}

fn read_money(state: &Entity<InputState>, currency: atlas_core::Currency, cx: &App, label: &str) -> Result<Option<Money>, String> {
    let value = state.read(cx).value().trim().to_string();
    if value.is_empty() {
        return Ok(None);
    }
    Money::parse(&value, currency).map(Some).map_err(|e| format!("{label}: {e}"))
}

impl DecisionForm {
    /// Builds the form pre-filled from `plan`.
    pub fn new(household: &Household, plan: &PurchasePlan, viewer: atlas_core::authz::Viewer, window: &mut Window, cx: &mut Context<AtlasApp>) -> Self {
        // §7.3: only objects the viewer may discover are offered (V062).
        let visible = |a: &&atlas_core::model::Account| !matches!(household.disclosure_for(viewer, atlas_core::ids::ObjectRef::Account(a.id)), atlas_core::Disclosure::Hidden);
        let personal_accounts: Vec<AccountId> = household.accounts.iter().filter(visible).filter(|a| !a.holder.is_company()).map(|a| a.id).collect();
        let all_accounts: Vec<AccountId> = household.accounts.iter().filter(visible).map(|a| a.id).collect();
        let account_names: Vec<SharedString> = household.accounts.iter().filter(visible).map(|a| SharedString::from(a.name.clone())).collect();
        let index_of = |id: Option<AccountId>| id.and_then(|id| all_accounts.iter().position(|a| *a == id)).unwrap_or(0);
        let source_floors = personal_accounts
            .iter()
            .map(|id| {
                let floor = plan.sources.iter().find(|s| s.account == *id).and_then(|s| s.floor).map(|f| f.format()).unwrap_or_default();
                (*id, text(floor, "never below… (optional)", window, cx))
            })
            .collect();
        let routes = household
            .companies
            .iter()
            .map(|c| {
                let to = plan.company_routes.iter().find(|r| r.company == c.id).map(|r| r.to_account);
                (c.id, choice(account_names.clone(), index_of(to), window, cx))
            })
            .collect();
        let draft = DecisionDraft {
            objective: Objective::ALL.iter().position(|o| *o == plan.objective).unwrap_or(0),
            source_allowed: personal_accounts.iter().map(|id| plan.sources.iter().find(|s| s.account == *id).map(|s| s.allowed).unwrap_or(true)).collect(),
            route_allowed: household.companies.iter().map(|c| plan.company_routes.iter().find(|r| r.company == c.id).map(|r| r.allowed).unwrap_or(false)).collect(),
            financing: plan.financing.is_some(),
            other_cost: !plan.other_costs.is_empty(),
            running_cost: plan.running_cost.is_some(),
        };
        let financing = plan.financing.clone();
        let other = plan.other_costs.first().cloned();
        let running = plan.running_cost.clone();
        DecisionForm {
            draft: cx.new(|_| draft),
            name: text(plan.name.clone(), "e.g. Car", window, cx),
            price: text(plan.price.format(), "total price", window, cx),
            purchase_on: date(plan.purchase_on, window, cx),
            window_from: date(plan.window_from, window, cx),
            window_to: date(plan.window_to, window, cx),
            reserve: text(plan.reserve.format(), "household reserve to keep", window, cx),
            down_payment: text(plan.down_payment.format(), "down payment", window, cx),
            down_low: text(plan.down_payment_low.format(), "grid: lowest", window, cx),
            down_high: text(plan.down_payment_high.format(), "grid: highest", window, cx),
            down_step: text(plan.down_payment_step.format(), "grid: step", window, cx),
            source_floors,
            routes,
            max_tax: text(plan.max_tax_and_fees.map(|m| m.format()).unwrap_or_default(), "maximum tax + fees (optional)", window, cx),
            months: text(financing.as_ref().map(|f| f.months.to_string()).unwrap_or_else(|| "36".into()), "months", window, cx),
            rate: text(financing.as_ref().map(|f| format!("{}.{:02}", f.annual_rate_basis_points / 100, f.annual_rate_basis_points % 100)).unwrap_or_else(|| "12".into()), "annual rate %", window, cx),
            first_instalment: date(financing.as_ref().map(|f| f.first_instalment).unwrap_or(plan.purchase_on), window, cx),
            financing_account: choice(account_names.clone(), index_of(financing.as_ref().map(|f| f.account)), window, cx),
            other_label: text(other.as_ref().map(|c| c.label.clone()).unwrap_or_else(|| "Registration and insurance".into()), "what", window, cx),
            other_amount: text(other.as_ref().map(|c| c.amount.format()).unwrap_or_default(), "amount", window, cx),
            other_on: date(other.as_ref().map(|c| c.on).unwrap_or(plan.purchase_on), window, cx),
            other_account: choice(account_names.clone(), index_of(other.as_ref().map(|c| c.account)), window, cx),
            running_label: text(running.as_ref().map(|r| r.label.clone()).unwrap_or_else(|| "Running costs".into()), "what", window, cx),
            running_monthly: text(running.as_ref().map(|r| r.monthly.format()).unwrap_or_default(), "per month", window, cx),
            running_from: date(running.as_ref().map(|r| r.from).unwrap_or(plan.purchase_on), window, cx),
            running_account: choice(account_names, index_of(running.as_ref().map(|r| r.account)), window, cx),
            personal_accounts,
            all_accounts,
        }
    }
}

impl AtlasApp {
    /// Reads step `step` of the form into the plan, validating it.
    pub fn apply_decision_step(&mut self, step: usize, cx: &mut Context<Self>) -> Result<(), String> {
        let currency = self.household.base_currency;
        let f = &self.decision_form;
        let draft = f.draft.read(cx).clone();
        let mut plan = self.decision_plan.clone();
        match step {
            0 => {
                let name = f.name.read(cx).value().trim().to_string();
                if name.is_empty() {
                    return Err("Name the purchase (e.g. Car).".into());
                }
                plan.name = name;
                plan.price = read_money(&f.price, currency, cx, "Price")?.ok_or("Enter the total price.")?;
                if !plan.price.is_positive() {
                    return Err("The price must be positive.".into());
                }
                plan.purchase_on = f.purchase_on.read(cx).date().start().ok_or("Pick the purchase date.")?;
                if plan.purchase_on < self.household.as_of {
                    return Err(format!("The purchase date is before the reconciliation date {}.", self.household.as_of.format("%d %b %Y")));
                }
                plan.window_from = f.window_from.read(cx).date().start().unwrap_or(plan.purchase_on);
                plan.window_to = f.window_to.read(cx).date().start().unwrap_or(plan.purchase_on);
                if plan.window_to < plan.window_from {
                    return Err("The purchase window ends before it starts.".into());
                }
                if plan.window_from.checked_add_months(Months::new(24)).is_some_and(|limit| plan.window_to > limit) {
                    return Err("Keep the purchase window within 24 months; the grid evaluates every month.".into());
                }
                plan.reserve = read_money(&f.reserve, currency, cx, "Reserve")?.unwrap_or(Money::zero(currency));
                plan.objective = Objective::ALL.get(draft.objective).copied().unwrap_or(Objective::MinimiseTaxAndFees);
            }
            1 => {
                plan.down_payment = read_money(&f.down_payment, currency, cx, "Down payment")?.ok_or("Enter the down payment.")?;
                if plan.down_payment.minor() > plan.price.minor() {
                    return Err("The down payment exceeds the price.".into());
                }
                plan.down_payment_low = read_money(&f.down_low, currency, cx, "Grid lowest")?.unwrap_or(plan.down_payment);
                plan.down_payment_high = read_money(&f.down_high, currency, cx, "Grid highest")?.unwrap_or(plan.down_payment);
                plan.down_payment_step = read_money(&f.down_step, currency, cx, "Grid step")?.unwrap_or(Money::zero(currency));
                if plan.down_payment_high.minor() < plan.down_payment_low.minor() {
                    return Err("The grid's highest down payment is below its lowest.".into());
                }
                if plan.down_payment_step.is_positive() && (plan.down_payment_high.minor() - plan.down_payment_low.minor()) / plan.down_payment_step.minor() > 30 {
                    return Err("The grid would have more than 30 down-payment steps; use a larger step.".into());
                }
                let mut sources = Vec::new();
                for (index, (account, floor)) in f.source_floors.iter().enumerate() {
                    let allowed = draft.source_allowed.get(index).copied().unwrap_or(true);
                    let floor = read_money(floor, currency, cx, "Floor")?;
                    sources.push(FundingSource { account: *account, allowed, floor });
                }
                if !sources.iter().any(|s| s.allowed) && !draft.route_allowed.iter().any(|r| *r) {
                    return Err("Allow at least one funding source.".into());
                }
                plan.sources = sources;
                let mut routes = Vec::new();
                for (index, (company, to)) in f.routes.iter().enumerate() {
                    let allowed = draft.route_allowed.get(index).copied().unwrap_or(false);
                    let to_account = *f.all_accounts.get(selected_row(to, cx)).ok_or("Pick the receiving account.")?;
                    routes.push(CompanyRoute { company: *company, method: ExtractionMethod::Salary, allowed, to_account });
                }
                plan.company_routes = routes;
                plan.max_tax_and_fees = read_money(&f.max_tax, currency, cx, "Maximum tax + fees")?;
            }
            2 => {
                if draft.financing {
                    let months_text = f.months.read(cx).value().trim().to_string();
                    let months: u32 = months_text.parse().map_err(|_| format!("Months {months_text:?} is not a whole number."))?;
                    if months == 0 || months > 480 {
                        return Err("Use between 1 and 480 months.".into());
                    }
                    let rate_text = f.rate.read(cx).value().trim().replace('%', "");
                    let rate: f64 = rate_text.parse().map_err(|_| format!("Rate {rate_text:?} is not a percentage."))?;
                    if !(0.0..=100.0).contains(&rate) {
                        return Err("The annual rate must be between 0 and 100 percent.".into());
                    }
                    let first_instalment = f.first_instalment.read(cx).date().start().unwrap_or(plan.purchase_on.checked_add_months(Months::new(1)).unwrap_or(plan.purchase_on));
                    if first_instalment < plan.purchase_on {
                        return Err("The first instalment is before the purchase.".into());
                    }
                    let account = *f.all_accounts.get(selected_row(&f.financing_account, cx)).ok_or("Pick the paying account.")?;
                    plan.financing = Some(Financing { months, annual_rate_basis_points: (rate * 100.0).round() as u32, first_instalment, account });
                } else {
                    plan.financing = None;
                    if plan.down_payment.minor() < plan.price.minor() {
                        return Err(format!("Without financing the down payment must cover the price ({} short).", plan.price.checked_sub(plan.down_payment).map(|m| m.format()).unwrap_or_default()));
                    }
                }
            }
            3 => {
                plan.other_costs.clear();
                if draft.other_cost {
                    let amount = read_money(&f.other_amount, currency, cx, "Other cost")?.ok_or("Enter the other cost's amount or untick it.")?;
                    let label = f.other_label.read(cx).value().trim().to_string();
                    let on = f.other_on.read(cx).date().start().unwrap_or(plan.purchase_on);
                    let account = *f.all_accounts.get(selected_row(&f.other_account, cx)).ok_or("Pick the account.")?;
                    plan.other_costs.push(OtherCost { label: if label.is_empty() { "Other cost".into() } else { label }, amount, on, account });
                }
                plan.running_cost = if draft.running_cost {
                    let monthly = read_money(&f.running_monthly, currency, cx, "Running cost")?.ok_or("Enter the monthly running cost or untick it.")?;
                    let label = f.running_label.read(cx).value().trim().to_string();
                    let from = f.running_from.read(cx).date().start().unwrap_or(plan.purchase_on.checked_add_days(Days::new(15)).unwrap_or(plan.purchase_on));
                    let account = *f.all_accounts.get(selected_row(&f.running_account, cx)).ok_or("Pick the account.")?;
                    Some(RunningCost { label: if label.is_empty() { "Running costs".into() } else { label }, monthly, from, account })
                } else {
                    None
                };
            }
            _ => {}
        }
        self.decision_plan = plan;
        Ok(())
    }

    /// Moves to `target` after applying the current step; evaluates on the result step.
    pub fn go_to_decision_step(&mut self, target: usize, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.decision_step;
        if target > current {
            for step in current..target.min(4) {
                if let Err(message) = self.apply_decision_step(step, cx) {
                    log::warn!("decision step {step} refused: {message}");
                    window.push_notification(message, cx);
                    self.decision_step = step;
                    cx.notify();
                    return;
                }
            }
        } else if let Err(message) = self.apply_decision_step(current, cx) {
            log::debug!("leaving decision step {current} with unapplied values: {message}");
        }
        self.decision_step = target.min(4);
        if self.decision_step == 4 {
            self.evaluate_decision();
        }
        cx.notify();
    }

    /// Runs the engine on the current plan.
    pub fn evaluate_decision(&mut self) {
        log::info!("evaluating decision “{}”: price {} on {}, down payment {}", self.decision_plan.name, self.decision_plan.price.format(), self.decision_plan.purchase_on, self.decision_plan.down_payment.format());
        let result = crate::perf::timed(&format!("evaluate decision “{}” (§19.2 grid)", self.decision_plan.name), || {
            atlas_core::decision::evaluate(&self.household, &self.decision_plan, self.horizon)
        });
        if let Err(err) = &result {
            crate::alerting::report(crate::alerting::Level::Error, format!("decision evaluation failed for “{}”: {err}", self.decision_plan.name));
        }
        self.decision = Some(result);
    }

    /// Resets the builder to the default plan.
    pub fn reset_decision(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.decision_plan = atlas_core::decision::default_plan_for(&self.household, self.household.as_of, self.viewer);
        self.decision_form = DecisionForm::new(&self.household, &self.decision_plan, self.viewer, window, cx);
        self.decision_step = 0;
        self.decision = None;
        cx.notify();
    }

    /// Saves the evaluated decision as a scenario (M9 ↔ M8).
    pub fn save_decision_as_scenario(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Ok(decision)) = &self.decision else { return };
        let strategy = decision.strategies.preferred.map(|i| decision.strategies.strategies[i].clone());
        let plan = decision.plan.clone();
        let id = self.household.save_decision_as_scenario(&plan, strategy.as_ref(), self.viewer.person);
        log::info!("decision “{}” saved as scenario {id}", plan.name);
        self.scenario_selection = vec![id];
        self.mark_dirty();
        self.rebuild_forms(window, cx);
        self.refresh_derived();
        window.push_notification(format!("Saved as scenario “Decision: {}” ({id}); compare it on the Scenarios screen.", plan.name), cx);
        cx.notify();
    }
}
