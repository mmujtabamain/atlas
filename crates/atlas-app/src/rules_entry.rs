//! The rule editor (M7, §14.1): one dialog that builds a deterministic,
//! effective-dated rule step by step — scope, trigger, conditions, action,
//! priority, effective range — and validates it through the engine (V054).

use atlas_core::ids::*;
use atlas_core::model::Household;
use atlas_core::rules::{Condition, Rule, RuleAction, RuleScope, Trigger};
use atlas_core::Money;
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
use gpui_kit::*;

use crate::alerting::{self, Level};
use crate::app::AtlasApp;

type Choice = Entity<SelectState<Vec<SharedString>>>;

/// Scope choices of the editor, in radio order.
pub const SCOPES: [&str; 3] = ["Whole household", "One category", "One account"];
/// Trigger choices, in radio order.
pub const TRIGGERS: [Trigger; 5] = [Trigger::AnyPosting, Trigger::Expense, Trigger::Income, Trigger::Transfer, Trigger::Funding];
/// Action choices, in radio order.
pub const ACTIONS: [&str; 6] = [
    "Add a percentage fee event",
    "Add a fixed fee event",
    "Classify under another category",
    "Funding: prefer an account (keep a floor)",
    "Funding: do not use an account (until a date)",
    "Bank selection: preferred account with a fallback",
];

/// Stateless-control values of the editor.
#[derive(Debug, Clone)]
pub struct RuleDraft {
    pub scope: usize,
    pub trigger: usize,
    pub action: usize,
    pub foreign_only: bool,
}

impl Default for RuleDraft {
    fn default() -> Self {
        RuleDraft { scope: 1, trigger: 1, action: 0, foreign_only: false }
    }
}

/// Retained state of the editor. Rebuilt when the household changes (the
/// selects snapshot categories and accounts).
pub struct RuleForm {
    pub draft: Entity<RuleDraft>,
    pub name: Entity<InputState>,
    pub category: Choice,
    pub account: Choice,
    pub amount_above: Entity<InputState>,
    pub percent: Entity<InputState>,
    pub fixed: Entity<InputState>,
    pub fee_label: Entity<InputState>,
    pub new_category: Entity<InputState>,
    pub target_account: Choice,
    pub fallback_account: Choice,
    pub floor: Entity<InputState>,
    pub unless_after: Entity<DatePickerState>,
    pub priority: Entity<InputState>,
    pub effective_from: Entity<DatePickerState>,
    pub effective_to: Entity<DatePickerState>,
    pub explanation: Entity<InputState>,
    categories: Vec<String>,
    accounts: Vec<AccountId>,
}

fn choice(items: Vec<SharedString>, window: &mut Window, cx: &mut Context<AtlasApp>) -> Choice {
    cx.new(|cx| SelectState::new(items, Some(IndexPath::default()), window, cx))
}

fn text(placeholder: &str, window: &mut Window, cx: &mut Context<AtlasApp>) -> Entity<InputState> {
    let placeholder = placeholder.to_string();
    cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
}

fn date(window: &mut Window, cx: &mut Context<AtlasApp>) -> Entity<DatePickerState> {
    cx.new(|cx| DatePickerState::new(window, cx).date_format("%d %b %Y"))
}

fn selected_row(state: &Choice, cx: &App) -> usize {
    state.read(cx).selected_index(cx).map(|p| p.row).unwrap_or(0)
}

impl RuleForm {
    pub fn new(household: &Household, window: &mut Window, cx: &mut Context<AtlasApp>) -> Self {
        let categories = household.categories();
        let category_names: Vec<SharedString> = categories.iter().map(|c| SharedString::from(c.clone())).collect();
        let accounts: Vec<AccountId> = household.accounts.iter().map(|a| a.id).collect();
        let account_names: Vec<SharedString> = household.accounts.iter().map(|a| SharedString::from(a.name.clone())).collect();
        RuleForm {
            draft: cx.new(|_| RuleDraft::default()),
            name: text("e.g. Foreign card fee", window, cx),
            category: choice(category_names, window, cx),
            account: choice(account_names.clone(), window, cx),
            amount_above: text("only when the amount exceeds… (optional)", window, cx),
            percent: text("e.g. 1.5", window, cx),
            fixed: text("e.g. 250", window, cx),
            fee_label: text("e.g. foreign transaction fee", window, cx),
            new_category: text("e.g. Housing", window, cx),
            target_account: choice(account_names.clone(), window, cx),
            fallback_account: choice(account_names, window, cx),
            floor: text("amount to preserve / threshold", window, cx),
            unless_after: date(window, cx),
            priority: cx.new(|cx| InputState::new(window, cx).default_value("10")),
            effective_from: date(window, cx),
            effective_to: date(window, cx),
            explanation: text("why this rule exists (shown in every explanation)", window, cx),
            categories,
            accounts,
        }
    }
}

fn read_money(state: &Entity<InputState>, currency: atlas_core::Currency, cx: &App, label: &str) -> Result<Option<Money>, String> {
    let value = state.read(cx).value().trim().to_string();
    if value.is_empty() {
        return Ok(None);
    }
    Money::parse(&value, currency).map(Some).map_err(|e| format!("{label}: {e}"))
}

impl AtlasApp {
    /// Opens the rule editor.
    pub fn open_rule_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.entity().downgrade();
        let f = &self.rule_form;
        let as_of = self.household.as_of;
        f.name.update(cx, |s, cx| s.set_value("", window, cx));
        f.effective_from.update(cx, |s, cx| s.set_date(as_of, window, cx));
        f.effective_to.update(cx, |s, cx| s.set_date(gpui_kit::base::Date::Single(None), window, cx));
        f.unless_after.update(cx, |s, cx| s.set_date(gpui_kit::base::Date::Single(None), window, cx));
        let fields = (
            f.draft.clone(),
            f.name.clone(),
            f.category.clone(),
            f.account.clone(),
            f.amount_above.clone(),
            f.percent.clone(),
            f.fixed.clone(),
            f.fee_label.clone(),
            f.new_category.clone(),
            f.target_account.clone(),
            f.fallback_account.clone(),
            f.floor.clone(),
            f.unless_after.clone(),
            f.priority.clone(),
            f.effective_from.clone(),
            f.effective_to.clone(),
            f.explanation.clone(),
        );
        window.open_dialog(cx, move |dialog, window, cx| {
            let this = this.clone();
            let wide_width = window.rem_size() * 56.;
            let (draft, name, category, account, amount_above, percent, fixed, fee_label, new_category, target_account, fallback_account, floor, unless_after, priority, effective_from, effective_to, explanation) = fields.clone();
            let RuleDraft { scope, trigger, action, foreign_only } = draft.read(cx).clone();
            let (d_scope, d_trigger, d_action, d_foreign) = (draft.clone(), draft.clone(), draft.clone(), draft.clone());
            let action_fields: Vec<AnyElement> = match action {
                0 => vec![
                    Field::new().label("Fee rate (%)").required(true).child(Input::new(&percent).id("rule-percent")).into_any_element(),
                    Field::new().label("Fee label").child(Input::new(&fee_label).id("rule-fee-label")).into_any_element(),
                ],
                1 => vec![
                    Field::new().label("Fixed fee amount").required(true).child(Input::new(&fixed).id("rule-fixed")).into_any_element(),
                    Field::new().label("Fee label").child(Input::new(&fee_label).id("rule-fee-label")).into_any_element(),
                ],
                2 => vec![Field::new().label("Classify as category").required(true).child(Input::new(&new_category).id("rule-new-category")).into_any_element()],
                3 => vec![
                    Field::new().label("Account to use").child(Select::new(&target_account)).into_any_element(),
                    Field::new().label("Preserve at least (optional)").child(Input::new(&floor).id("rule-floor")).into_any_element(),
                ],
                4 => vec![
                    Field::new().label("Account not to use").child(Select::new(&target_account)).into_any_element(),
                    Field::new().label("Unless the date is after (optional)").child(DatePicker::new(&unless_after)).into_any_element(),
                ],
                _ => vec![
                    Field::new().label("Preferred account").child(Select::new(&target_account)).into_any_element(),
                    Field::new().label("Fallback account").child(Select::new(&fallback_account)).into_any_element(),
                    Field::new().label("Use the fallback when the preferred account would fall below").required(true).child(Input::new(&floor).id("rule-floor")).into_any_element(),
                ],
            };
            dialog
                .title("New rule")
                .w(wide_width)
                .max_h(relative(0.9))
                .child(
                    v_flex().child(
                        Form::vertical()
                            .columns(2)
                            .child(Field::new().label("Name").required(true).child(Input::new(&name).id("rule-name")))
                            .child(Field::new().label("Priority (higher wins)").child(Input::new(&priority).id("rule-priority")))
                            .child(
                                Field::new().label("Scope (more specific scopes win ties)").child(
                                    RadioGroup::horizontal("rule-scope")
                                        .children(SCOPES)
                                        .selected_index(Some(scope))
                                        .on_change(move |index, _, cx| d_scope.update(cx, |d, cx| { d.scope = *index; cx.notify(); })),
                                ),
                            )
                            .child(match scope {
                                1 => Field::new().label("Category").child(Select::new(&category)),
                                2 => Field::new().label("Account").child(Select::new(&account)),
                                _ => Field::new().label("Applies to").child(div().text_sm().child("Every posting of the household")),
                            })
                            .child(
                                Field::new().label("Trigger").child(
                                    RadioGroup::horizontal("rule-trigger")
                                        .children(TRIGGERS.iter().map(|t| t.label()))
                                        .selected_index(Some(trigger))
                                        .on_change(move |index, _, cx| d_trigger.update(cx, |d, cx| { d.trigger = *index; cx.notify(); })),
                                ),
                            )
                            .child(
                                Field::new().label("Conditions").child(
                                    v_flex()
                                        .gap_2()
                                        .child(Input::new(&amount_above).id("rule-amount-above"))
                                        .child(Checkbox::new("rule-foreign-only").label("Only accounts in a currency other than the base currency").checked(foreign_only).on_change(move |v, _, cx| d_foreign.update(cx, |d, cx| { d.foreign_only = *v; cx.notify(); }))),
                                ),
                            )
                            .child(
                                Field::new().label("Action").child(
                                    RadioGroup::vertical("rule-action")
                                        .children(ACTIONS)
                                        .selected_index(Some(action))
                                        .on_change(move |index, _, cx| d_action.update(cx, |d, cx| { d.action = *index; cx.notify(); })),
                                ),
                            )
                            .child(Field::new().label("Action details").child(v_flex().gap_2().children(action_fields)))
                            .child(Field::new().label("Effective from").required(true).child(DatePicker::new(&effective_from)))
                            .child(Field::new().label("Effective to (optional)").child(DatePicker::new(&effective_to)))
                            .child(Field::new().label("Explanation").child(Input::new(&explanation).id("rule-explanation"))),
                    ),
                )
                .footer(
                    DialogFooter::new()
                        .child(Button::new("rule-cancel").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                        .child(Button::new("rule-save").primary().label("Add rule").on_click({
                            let this = this.clone();
                            move |_, window, cx| {
                                Self::confirm_rule(&this, window, cx);
                            }
                        })),
                )
                .on_ok(move |_, window, cx| Self::confirm_rule(&this, window, cx))
        });
    }

    fn confirm_rule(this: &WeakEntity<Self>, window: &mut Window, cx: &mut App) -> bool {
        match this.update(cx, |app, cx| app.submit_rule(cx)) {
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

    /// Builds the rule from the editor and adds it through the engine (V054 validation).
    pub fn submit_rule(&mut self, cx: &mut Context<Self>) -> Result<String, String> {
        let currency = self.household.base_currency;
        let f = &self.rule_form;
        let draft = f.draft.read(cx).clone();
        let name = f.name.read(cx).value().trim().to_string();
        if name.is_empty() {
            return Err("Give the rule a name.".into());
        }
        let scope = match draft.scope {
            0 => RuleScope::Household,
            1 => RuleScope::Category(f.categories.get(selected_row(&f.category, cx)).cloned().ok_or("Add a series first; there is no category to scope the rule to.")?),
            _ => RuleScope::Account(*f.accounts.get(selected_row(&f.account, cx)).ok_or("Add an account first.")?),
        };
        let trigger = TRIGGERS.get(draft.trigger).copied().unwrap_or(Trigger::AnyPosting);
        let mut conditions = Vec::new();
        if let Some(above) = read_money(&f.amount_above, currency, cx, "Amount above")? {
            conditions.push(Condition::AmountAbove(above));
        }
        if draft.foreign_only {
            conditions.push(Condition::ForeignCurrency);
        }
        let label = {
            let l = f.fee_label.read(cx).value().trim().to_string();
            if l.is_empty() { "fee".to_string() } else { l }
        };
        let target = || f.accounts.get(selected_row(&f.target_account, cx)).copied().ok_or_else(|| "Add an account first.".to_string());
        let action = match draft.action {
            0 => {
                let percent_text = f.percent.read(cx).value().trim().replace('%', "");
                let percent: f64 = percent_text.parse().map_err(|_| format!("Fee rate {percent_text:?} is not a percentage."))?;
                if !(0.0..=100.0).contains(&percent) {
                    return Err("The fee rate must be between 0 and 100 percent.".into());
                }
                RuleAction::AddFee { basis_points: (percent * 100.0).round() as u32, fixed: None, label }
            }
            1 => RuleAction::AddFee { basis_points: 0, fixed: Some(read_money(&f.fixed, currency, cx, "Fixed fee")?.ok_or("Enter the fixed fee amount.")?), label },
            2 => {
                let category = f.new_category.read(cx).value().trim().to_string();
                if category.is_empty() {
                    return Err("Enter the category to classify under.".into());
                }
                RuleAction::Classify { category }
            }
            3 => RuleAction::PreferAccount { account: target()?, preserve: read_money(&f.floor, currency, cx, "Preserve")? },
            4 => RuleAction::ForbidAccount { account: target()?, unless_after: f.unless_after.read(cx).date().start() },
            _ => RuleAction::BankSelection {
                prefer: target()?,
                fallback: f.accounts.get(selected_row(&f.fallback_account, cx)).copied().ok_or("Pick the fallback account.")?,
                when_below: read_money(&f.floor, currency, cx, "Threshold")?.ok_or("Enter the balance below which the fallback is used.")?,
            },
        };
        let priority_text = f.priority.read(cx).value().trim().to_string();
        let priority: i32 = if priority_text.is_empty() { 10 } else { priority_text.parse().map_err(|_| format!("Priority {priority_text:?} is not a whole number."))? };
        let effective_from = f.effective_from.read(cx).date().start().ok_or("Pick the date the rule takes effect.")?;
        let effective_to = f.effective_to.read(cx).date().start();
        let explanation = f.explanation.read(cx).value().trim().to_string();
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
            scenario: None,
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
                cx.notify();
                Ok(format!("Rule “{name}” added ({id}): {described}. Forecasts recomputed."))
            }
            Err(err) => {
                alerting::report(Level::Warning, format!("rule “{name}” refused: {err}"));
                Err(format!("Not added — {err}"))
            }
        }
    }
}
