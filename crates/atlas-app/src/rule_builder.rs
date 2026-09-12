//! The create-rule flow's retained state: a full-screen stepper over scope,
//! trigger and conditions, action, then review. Every scope and condition the
//! engine supports is offered; nothing beyond it is invented.

use atlas_core::ids::{AccountId, CompanyId, PersonId, ScenarioId};
use atlas_core::model::Household;
use atlas_core::rules::{Condition, Rule, RuleAction, RuleScope, Trigger};
use atlas_core::Money;
use chrono::NaiveDate;
use gpui_kit::component::{
    IndexPath, WindowExt as _,
    date_picker::DatePickerState,
    input::InputState,
    select::SelectState,
};
use gpui_kit::*;

use crate::alerting::{self, Level};
use crate::app::AtlasApp;

type Choice = Entity<SelectState<Vec<SharedString>>>;

/// Step titles of the flow.
pub const STEPS: [&str; 4] = ["Scope", "Trigger & conditions", "Action", "Review"];

/// Scope kinds, in order.
pub const SCOPE_KINDS: [&str; 7] = ["Whole household", "Category", "Institution", "Person", "Company", "Account", "Scenario"];

/// Trigger kinds, in order.
pub const TRIGGERS: [Trigger; 5] = [Trigger::AnyPosting, Trigger::Expense, Trigger::Income, Trigger::Transfer, Trigger::Funding];

/// Condition kinds, in order.
pub const CONDITION_KINDS: [&str; 7] = [
    "Amount above",
    "Amount below",
    "Category is",
    "Account is",
    "Account currency differs from the household currency",
    "On or after",
    "On or before",
];

/// Action kinds, in order.
pub const ACTION_KINDS: [&str; 6] = [
    "Percentage fee",
    "Fixed fee",
    "Classify as category",
    "Prefer funding account",
    "Do not use funding account",
    "Choose expense account",
];

/// One condition row of the flow.
pub struct ConditionRow {
    pub kind: usize,
    pub money: Entity<InputState>,
    pub text: Entity<InputState>,
    pub account: Choice,
    pub date: Entity<DatePickerState>,
}

/// The retained inputs and choices of the flow.
pub struct RuleBuilder {
    pub step: usize,
    pub name: Entity<InputState>,
    pub priority: Entity<InputState>,
    pub scope_kind: usize,
    pub category: Choice,
    pub institution: Choice,
    pub person: Choice,
    pub company: Choice,
    pub account: Choice,
    pub scenario: Choice,
    pub trigger: usize,
    pub conditions: Vec<ConditionRow>,
    pub action_kind: usize,
    pub percent: Entity<InputState>,
    pub fixed: Entity<InputState>,
    pub fee_label: Entity<InputState>,
    pub new_category: Entity<InputState>,
    pub target_account: Choice,
    pub fallback_account: Choice,
    pub floor: Entity<InputState>,
    pub unless_after: Entity<DatePickerState>,
    pub effective_from: Entity<DatePickerState>,
    pub effective_to: Entity<DatePickerState>,
    pub explanation: Entity<InputState>,
    /// Snapshots behind the pickers.
    pub categories: Vec<String>,
    pub institutions: Vec<String>,
    pub people: Vec<PersonId>,
    pub companies: Vec<CompanyId>,
    pub accounts: Vec<AccountId>,
    pub scenarios: Vec<ScenarioId>,
}

fn text(placeholder: &str, window: &mut Window, cx: &mut App) -> Entity<InputState> {
    let placeholder = placeholder.to_string();
    cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
}

fn choice(items: Vec<SharedString>, window: &mut Window, cx: &mut App) -> Choice {
    cx.new(|cx| SelectState::new(items, Some(IndexPath::default()), window, cx))
}

fn date(value: Option<NaiveDate>, window: &mut Window, cx: &mut App) -> Entity<DatePickerState> {
    cx.new(|cx| {
        let mut state = DatePickerState::new(window, cx).date_format("%d %b %Y");
        if let Some(value) = value {
            state.set_date(value, window, cx);
        }
        state
    })
}

fn selected(state: &Choice, cx: &App) -> usize {
    state.read(cx).selected_index(cx).map(|p| p.row).unwrap_or(0)
}

fn read_money(state: &Entity<InputState>, currency: atlas_core::Currency, cx: &App, label: &str) -> Result<Option<Money>, String> {
    let value = state.read(cx).value().trim().to_string();
    if value.is_empty() {
        return Ok(None);
    }
    Money::parse(&value, currency).map(Some).map_err(|e| format!("{label}: {e}"))
}

impl RuleBuilder {
    pub fn new(household: &Household, viewer: atlas_core::authz::Viewer, window: &mut Window, cx: &mut App) -> Self {
        use atlas_core::ids::ObjectRef;
        let categories = household.categories();
        let mut institutions: Vec<String> = household.accounts.iter().filter(|a| !a.institution.is_empty()).map(|a| a.institution.clone()).collect();
        institutions.sort();
        institutions.dedup();
        let people: Vec<PersonId> = household.people.iter().map(|p| p.id).collect();
        let companies: Vec<CompanyId> = household.companies.iter().filter(|c| household.disclosure_for(viewer, ObjectRef::Company(c.id)) != atlas_core::Disclosure::Hidden).map(|c| c.id).collect();
        let accounts: Vec<AccountId> = household.accounts.iter().filter(|a| household.disclosure_for(viewer, ObjectRef::Account(a.id)) != atlas_core::Disclosure::Hidden).map(|a| a.id).collect();
        let scenarios: Vec<ScenarioId> = household.scenarios.iter().filter(|s| household.disclosure_for(viewer, ObjectRef::Scenario(s.id)) != atlas_core::Disclosure::Hidden).map(|s| s.id).collect();
        let names = |items: Vec<String>| -> Vec<SharedString> { if items.is_empty() { vec!["None available".into()] } else { items.into_iter().map(SharedString::from).collect() } };
        let account_names = names(accounts.iter().filter_map(|id| household.account(*id)).map(|a| a.name.clone()).collect());
        RuleBuilder {
            step: 0,
            name: text("e.g. Foreign card fee", window, cx),
            priority: cx.new(|cx| InputState::new(window, cx).default_value("10")),
            scope_kind: 0,
            category: choice(names(categories.clone()), window, cx),
            institution: choice(names(institutions.clone()), window, cx),
            person: choice(names(people.iter().filter_map(|id| household.person(*id)).map(|p| p.name.clone()).collect()), window, cx),
            company: choice(names(companies.iter().filter_map(|id| household.company(*id)).map(|c| c.name.clone()).collect()), window, cx),
            account: choice(account_names.clone(), window, cx),
            scenario: choice(names(scenarios.iter().filter_map(|id| household.scenario(*id)).map(|s| s.name.clone()).collect()), window, cx),
            trigger: 1,
            conditions: Vec::new(),
            action_kind: 0,
            percent: text("e.g. 1.5", window, cx),
            fixed: text("e.g. 250", window, cx),
            fee_label: text("e.g. foreign transaction fee", window, cx),
            new_category: text("e.g. Housing", window, cx),
            target_account: choice(account_names.clone(), window, cx),
            fallback_account: choice(account_names, window, cx),
            floor: text("amount to preserve / threshold", window, cx),
            unless_after: date(None, window, cx),
            effective_from: date(Some(household.as_of), window, cx),
            effective_to: date(None, window, cx),
            explanation: text("why this rule exists (shown in every explanation)", window, cx),
            categories,
            institutions,
            people,
            companies,
            accounts,
            scenarios,
        }
    }

    /// The rule in words, for the preview and the review step.
    pub fn preview(&self, household: &Household, cx: &App) -> String {
        let scope = self.scope(household, cx).map(|s| s.describe(household)).unwrap_or_else(|_| "a scope still to choose".into());
        let trigger = TRIGGERS.get(self.trigger).copied().unwrap_or(Trigger::AnyPosting).label();
        let conditions = self.condition_texts(household, cx);
        let action = self.action(household, cx).map(|a| format!("{}.", a.describe(household))).unwrap_or_else(|_| "the action is not complete yet".into());
        if conditions.is_empty() {
            format!("When {trigger} in {scope}: {action}")
        } else {
            format!("When {trigger} in {scope}, if {}: {action}", conditions.join(" and "))
        }
    }

    pub fn condition_texts(&self, household: &Household, cx: &App) -> Vec<String> {
        self.conditions.iter().filter_map(|row| self.condition(row, household, cx).ok()).map(|c| c.describe(household)).collect()
    }

    fn scope(&self, _household: &Household, cx: &App) -> Result<RuleScope, String> {
        Ok(match self.scope_kind {
            0 => RuleScope::Household,
            1 => RuleScope::Category(self.categories.get(selected(&self.category, cx)).cloned().ok_or("No category exists yet; add a planned movement with one first.")?),
            2 => RuleScope::Institution(self.institutions.get(selected(&self.institution, cx)).cloned().ok_or("No account records an institution.")?),
            3 => RuleScope::Person(*self.people.get(selected(&self.person, cx)).ok_or("Add a person first.")?),
            4 => RuleScope::Company(*self.companies.get(selected(&self.company, cx)).ok_or("No company is disclosed to you.")?),
            5 => RuleScope::Account(*self.accounts.get(selected(&self.account, cx)).ok_or("Add an account first.")?),
            _ => RuleScope::Scenario(*self.scenarios.get(selected(&self.scenario, cx)).ok_or("Create a scenario first.")?),
        })
    }

    fn condition(&self, row: &ConditionRow, household: &Household, cx: &App) -> Result<Condition, String> {
        let currency = household.base_currency;
        Ok(match row.kind {
            0 => Condition::AmountAbove(read_money(&row.money, currency, cx, "Amount above")?.ok_or("Enter the amount the posting must exceed.")?),
            1 => Condition::AmountBelow(read_money(&row.money, currency, cx, "Amount below")?.ok_or("Enter the amount the posting must stay under.")?),
            2 => {
                let category = row.text.read(cx).value().trim().to_string();
                if category.is_empty() {
                    return Err("Enter the category the posting must have.".into());
                }
                Condition::CategoryIs(category)
            }
            3 => Condition::AccountIs(*self.accounts.get(selected(&row.account, cx)).ok_or("Pick the account.")?),
            4 => Condition::ForeignCurrency,
            5 => Condition::OnOrAfter(row.date.read(cx).date().start().ok_or("Pick the date the condition starts.")?),
            _ => Condition::OnOrBefore(row.date.read(cx).date().start().ok_or("Pick the date the condition ends.")?),
        })
    }

    fn action(&self, household: &Household, cx: &App) -> Result<RuleAction, String> {
        let currency = household.base_currency;
        let label = {
            let l = self.fee_label.read(cx).value().trim().to_string();
            if l.is_empty() { "fee".to_string() } else { l }
        };
        let target = || self.accounts.get(selected(&self.target_account, cx)).copied().ok_or_else(|| "Add an account first.".to_string());
        Ok(match self.action_kind {
            0 => {
                let text = self.percent.read(cx).value().trim().replace('%', "");
                let percent: f64 = text.parse().map_err(|_| format!("Fee rate {text:?} is not a percentage."))?;
                if !(0.0..=100.0).contains(&percent) {
                    return Err("The fee rate must be between 0 and 100 percent.".into());
                }
                RuleAction::AddFee { basis_points: (percent * 100.0).round() as u32, fixed: None, label }
            }
            1 => RuleAction::AddFee { basis_points: 0, fixed: Some(read_money(&self.fixed, currency, cx, "Fixed fee")?.ok_or("Enter the fixed fee amount.")?), label },
            2 => {
                let category = self.new_category.read(cx).value().trim().to_string();
                if category.is_empty() {
                    return Err("Enter the category to classify under.".into());
                }
                RuleAction::Classify { category }
            }
            3 => RuleAction::PreferAccount { account: target()?, preserve: read_money(&self.floor, currency, cx, "Never below")? },
            4 => RuleAction::ForbidAccount { account: target()?, unless_after: self.unless_after.read(cx).date().start() },
            _ => RuleAction::BankSelection {
                prefer: target()?,
                fallback: self.accounts.get(selected(&self.fallback_account, cx)).copied().ok_or("Pick the fallback account.")?,
                when_below: read_money(&self.floor, currency, cx, "Balance threshold")?.ok_or("Enter the balance below which the fallback is used.")?,
            },
        })
    }
}

impl AtlasApp {
    /// Opens the create-rule flow at step 1 with a fresh draft.
    pub fn start_rule_flow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.rule_builder = RuleBuilder::new(&self.household, self.viewer, window, cx);
        self.navigate(crate::nav::Route::CreateRule, cx);
    }

    pub fn set_rule_step(&mut self, step: usize, cx: &mut Context<Self>) {
        self.rule_builder.step = step.min(3);
        cx.notify();
    }

    /// Validates the current step and moves on.
    pub fn advance_rule_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let step = self.rule_builder.step;
        if let Err(message) = self.validate_rule_step(step, cx) {
            window.push_notification(message, cx);
            return;
        }
        self.rule_builder.step = (step + 1).min(3);
        cx.notify();
    }

    fn validate_rule_step(&self, step: usize, cx: &App) -> Result<(), String> {
        let b = &self.rule_builder;
        match step {
            0 => {
                if b.name.read(cx).value().trim().is_empty() {
                    return Err("Give the rule a name.".into());
                }
                let priority = b.priority.read(cx).value().trim().to_string();
                if !priority.is_empty() && priority.parse::<i32>().is_err() {
                    return Err(format!("Priority {priority:?} is not a whole number."));
                }
                b.scope(&self.household, cx).map(|_| ())
            }
            1 => {
                for row in &b.conditions {
                    b.condition(row, &self.household, cx)?;
                }
                // An inverted amount or date band is caught before the engine sees it.
                let mut above: Option<Money> = None;
                let mut below: Option<Money> = None;
                let mut after: Option<NaiveDate> = None;
                let mut before: Option<NaiveDate> = None;
                for row in &b.conditions {
                    match b.condition(row, &self.household, cx)? {
                        Condition::AmountAbove(m) => above = Some(m),
                        Condition::AmountBelow(m) => below = Some(m),
                        Condition::OnOrAfter(d) => after = Some(d),
                        Condition::OnOrBefore(d) => before = Some(d),
                        _ => {}
                    }
                }
                if let (Some(a), Some(b_)) = (above, below)
                    && a.minor() >= b_.minor()
                {
                    return Err("The amount band is inverted: “above” must be less than “below”.".into());
                }
                if let (Some(a), Some(b_)) = (after, before)
                    && a > b_
                {
                    return Err("The date band is inverted: “on or after” must not be later than “on or before”.".into());
                }
                Ok(())
            }
            2 => b.action(&self.household, cx).map(|_| ()),
            _ => Ok(()),
        }
    }

    /// Adds one condition row of `kind`.
    pub fn add_rule_condition(&mut self, kind: usize, window: &mut Window, cx: &mut Context<Self>) {
        let account_names: Vec<SharedString> = self.rule_builder.accounts.iter().filter_map(|id| self.household.account(*id)).map(|a| SharedString::from(a.name.clone())).collect();
        let account_names = if account_names.is_empty() { vec!["None available".into()] } else { account_names };
        let row = ConditionRow {
            kind,
            money: text("amount", window, cx),
            text: text("category", window, cx),
            account: choice(account_names, window, cx),
            date: date(Some(self.household.as_of), window, cx),
        };
        self.rule_builder.conditions.push(row);
        cx.notify();
    }

    pub fn remove_rule_condition(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.rule_builder.conditions.len() {
            self.rule_builder.conditions.remove(index);
            cx.notify();
        }
    }

    pub fn set_rule_condition_kind(&mut self, index: usize, kind: usize, cx: &mut Context<Self>) {
        if let Some(row) = self.rule_builder.conditions.get_mut(index) {
            row.kind = kind;
            cx.notify();
        }
    }

    /// Commits the flow: the only mutation it makes.
    pub fn submit_rule_flow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for step in 0..3 {
            if let Err(message) = self.validate_rule_step(step, cx) {
                self.rule_builder.step = step;
                window.push_notification(message, cx);
                cx.notify();
                return;
            }
        }
        let b = &self.rule_builder;
        let name = b.name.read(cx).value().trim().to_string();
        let priority_text = b.priority.read(cx).value().trim().to_string();
        let priority: i32 = if priority_text.is_empty() { 10 } else { priority_text.parse().unwrap_or(10) };
        let effective_from = match b.effective_from.read(cx).date().start() {
            Some(d) => d,
            None => {
                window.push_notification("Pick the date the rule takes effect.", cx);
                return;
            }
        };
        let effective_to = b.effective_to.read(cx).date().start();
        if let Some(end) = effective_to
            && end < effective_from
        {
            window.push_notification("The rule cannot end before it starts.", cx);
            return;
        }
        let scope = match b.scope(&self.household, cx) {
            Ok(s) => s,
            Err(message) => {
                window.push_notification(message, cx);
                return;
            }
        };
        let scenario = match scope {
            RuleScope::Scenario(id) => Some(id),
            _ => None,
        };
        let conditions: Vec<Condition> = match b.conditions.iter().map(|row| b.condition(row, &self.household, cx)).collect::<Result<_, _>>() {
            Ok(c) => c,
            Err(message) => {
                window.push_notification(message, cx);
                return;
            }
        };
        let action = match b.action(&self.household, cx) {
            Ok(a) => a,
            Err(message) => {
                window.push_notification(message, cx);
                return;
            }
        };
        let explanation = b.explanation.read(cx).value().trim().to_string();
        let trigger = TRIGGERS.get(b.trigger).copied().unwrap_or(Trigger::AnyPosting);
        let rule = Rule {
            id: self.household.next_rule_id(),
            name: name.clone(),
            scope,
            trigger,
            conditions,
            action,
            priority,
            effective_from,
            effective_to,
            enabled: true,
            scenario,
            explanation: if explanation.is_empty() { "user-authored rule".into() } else { explanation },
            version: 1,
            history: Vec::new(),
        };
        let described = rule.action.describe(&self.household);
        match self.household.add_rule(rule) {
            Ok(id) => {
                log::info!("rule {id} “{name}” added: {described}");
                self.mark_dirty();
                self.refresh_derived();
                self.note_result(format!("Rule “{name}” added ({id}) as version 1: {described}. Forecasts recomputed."));
                self.rule_builder = RuleBuilder::new(&self.household, self.viewer, window, cx);
                self.selected_rule = Some(id);
                self.navigate(crate::nav::Route::Rule(id), cx);
            }
            Err(err) => {
                alerting::report(Level::Warning, format!("rule “{name}” refused: {err}"));
                window.push_notification(format!("Not added — {err}"), cx);
            }
        }
        cx.notify();
    }
}
