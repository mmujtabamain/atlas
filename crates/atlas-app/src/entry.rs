//! Data-entry dialogs: people, companies, accounts, event
//! series, assumptions, scenarios, actual transactions with reconciliation,
//! and account reconciliation. Every form keeps its values in retained
//! entities (the dialog builder runs during render and must not read the
//! app), validates through the engine's mutation API, and marks the household
//! dirty on success.

use atlas_core::authz::{CalculationAccess, VisibilityPreset};
use atlas_core::ids::*;
use atlas_core::model::*;
use atlas_core::timeline::{AmountSpec, DateSpec, Direction, EventSeries, InvalidDayPolicy, Recurrence, Until};
use atlas_core::vocab::Certainty;
use atlas_core::Money;
use gpui_kit::component::{
    ActiveTheme as _, IndexPath, WindowExt as _,
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

/// Stateless-control values of the entry dialogs.
#[derive(Debug, Clone, Default)]
pub struct EntryDraft {
    pub person_role: usize,
    pub account_joint: bool,
    pub account_company: bool,
    pub account_include: bool,
    pub series_direction: usize,
    pub series_ranged: bool,
    pub scenario_private: bool,
}

/// Retained state of every entry dialog. Rebuilt whenever the household changes
/// (the Select option lists snapshot people, companies and accounts).
pub struct EntryForms {
    pub draft: Entity<EntryDraft>,
    // person
    pub person_name: Entity<InputState>,
    // company
    pub company_name: Entity<InputState>,
    pub company_jurisdiction: Entity<InputState>,
    pub company_owner: Choice,
    // account
    pub account_name: Entity<InputState>,
    pub account_institution: Entity<InputState>,
    pub account_kind: Choice,
    pub account_holder: Choice,
    pub account_second_holder: Choice,
    pub account_company: Choice,
    pub account_balance: Entity<InputState>,
    pub account_minimum: Entity<InputState>,
    pub account_liquidity: Choice,
    pub account_visibility: Choice,
    pub account_access: Choice,
    // series
    pub series_name: Entity<InputState>,
    pub series_amount: Entity<InputState>,
    pub series_low: Entity<InputState>,
    pub series_high: Entity<InputState>,
    pub series_recurrence: Choice,
    pub series_day: Entity<InputState>,
    pub series_from: Entity<DatePickerState>,
    pub series_until: Entity<DatePickerState>,
    pub series_account: Choice,
    pub series_transfer_to: Choice,
    pub series_entity: Choice,
    pub series_certainty: Choice,
    pub series_category: Entity<InputState>,
    // assumption
    pub assumption_text: Entity<InputState>,
    pub assumption_certainty: Choice,
    pub assumption_series: Choice,
    pub assumption_expires: Entity<DatePickerState>,
    // scenario
    pub scenario_name: Entity<InputState>,
    pub scenario_description: Entity<InputState>,
    // actual + link
    pub actual_date: Entity<DatePickerState>,
    pub actual_account: Choice,
    pub actual_amount: Entity<InputState>,
    pub actual_description: Entity<InputState>,
    pub actual_link_series: Choice,
    pub actual_link_due: Entity<DatePickerState>,
    // reconcile
    pub reconcile_balance: Entity<InputState>,
    pub reconcile_date: Entity<DatePickerState>,
    // snapshots behind the Selects
    people: Vec<PersonId>,
    companies: Vec<CompanyId>,
    accounts: Vec<AccountId>,
    entities: Vec<EntityRef>,
    series_ids: Vec<SeriesId>,
}

pub const ACCOUNT_KINDS: [AccountKind; 11] = [
    AccountKind::Checking,
    AccountKind::Savings,
    AccountKind::CashWallet,
    AccountKind::CreditCard,
    AccountKind::Loan,
    AccountKind::Brokerage,
    AccountKind::FixedDeposit,
    AccountKind::TaxReserve,
    AccountKind::CompanyOperating,
    AccountKind::CompanyPayroll,
    AccountKind::CorporateCard,
];
const LIQUIDITY: [&str; 3] = ["Immediate", "Delayed — 2 business days", "Locked for 6 months"];
const VISIBILITY: [VisibilityPreset; 4] = [VisibilityPreset::FullyShared, VisibilityPreset::SharedBalance, VisibilityPreset::SharedSummary, VisibilityPreset::Private];
const ACCESS: [CalculationAccess; 3] = [CalculationAccess::Full, CalculationAccess::RestrictedContribution, CalculationAccess::Excluded];
const RECURRENCES: [&str; 6] = ["Monthly on a day", "One time", "Last day of every month", "Weekly", "Every N weeks (fortnightly = 2)", "Yearly on the start date"];
const ROLES: [HouseholdRole; 5] = [HouseholdRole::Owner, HouseholdRole::Member, HouseholdRole::Dependent, HouseholdRole::Adviser, HouseholdRole::ReadOnly];

impl EntryForms {
    /// Select rows behind the option lists, for contextual preselection.
    pub fn account_row(&self, account: AccountId) -> Option<usize> {
        self.accounts.iter().position(|id| *id == account)
    }
    pub fn person_row(&self, person: PersonId) -> Option<usize> {
        self.people.iter().position(|id| *id == person)
    }
    pub fn company_row(&self, company: CompanyId) -> Option<usize> {
        self.companies.iter().position(|id| *id == company)
    }
    pub fn entity_row(&self, entity: EntityRef) -> Option<usize> {
        self.entities.iter().position(|e| *e == entity)
    }
    /// The series' row among the series options (the `none` row not counted).
    pub fn series_row(&self, series: SeriesId) -> Option<usize> {
        self.series_ids.iter().position(|id| *id == series)
    }
    pub fn series_id_at(&self, row: usize) -> Option<SeriesId> {
        self.series_ids.get(row).copied()
    }
    /// The row of a recurrence option by its label prefix.
    pub fn recurrence_row(&self, label: &str) -> Option<usize> {
        RECURRENCES.iter().position(|r| r.starts_with(label))
    }

    pub fn new(household: &Household, window: &mut Window, cx: &mut Context<AtlasApp>) -> Self {
        let people: Vec<PersonId> = household.people.iter().map(|p| p.id).collect();
        let people_names: Vec<SharedString> = household.people.iter().map(|p| SharedString::from(p.name.clone())).collect();
        let companies: Vec<CompanyId> = household.companies.iter().map(|c| c.id).collect();
        let company_names: Vec<SharedString> = household.companies.iter().map(|c| SharedString::from(c.name.clone())).collect();
        let accounts: Vec<AccountId> = household.accounts.iter().map(|a| a.id).collect();
        let account_names: Vec<SharedString> = household.accounts.iter().map(|a| SharedString::from(a.name.clone())).collect();
        let mut entities = vec![EntityRef::Household];
        entities.extend(people.iter().map(|p| EntityRef::Person(*p)));
        entities.extend(companies.iter().map(|c| EntityRef::Company(*c)));
        let entity_names: Vec<SharedString> = entities.iter().map(|e| SharedString::from(household.entity_name(*e))).collect();
        let series_ids: Vec<SeriesId> = household.series.iter().map(|s| s.id).collect();
        let series_names: Vec<SharedString> = household.series.iter().map(|s| SharedString::from(s.name.clone())).collect();
        let certainties: Vec<SharedString> = Certainty::ALL.iter().map(|c| SharedString::from(c.label())).collect();
        let mut none_or_series: Vec<SharedString> = vec!["No particular series".into()];
        none_or_series.extend(series_names.iter().cloned());
        let mut none_or_series_link: Vec<SharedString> = vec!["Not reconciled to a planned occurrence".into()];
        none_or_series_link.extend(series_names.iter().cloned());

        EntryForms {
            draft: cx.new(|_| EntryDraft { account_include: true, ..EntryDraft::default() }),
            person_name: text("name", window, cx),
            company_name: text("company name", window, cx),
            company_jurisdiction: text("jurisdiction (free text; nothing is inferred)", window, cx),
            company_owner: choice(people_names.clone(), window, cx),
            account_name: text("e.g. Joint current account", window, cx),
            account_institution: text("bank or institution", window, cx),
            account_kind: choice(ACCOUNT_KINDS.iter().map(|k| SharedString::from(k.label())).collect(), window, cx),
            account_holder: choice(people_names.clone(), window, cx),
            account_second_holder: choice(people_names.clone(), window, cx),
            account_company: choice(company_names, window, cx),
            account_balance: text("settled balance today, e.g. 12,500.00 (negative for debt)", window, cx),
            account_minimum: text("minimum balance the bank requires (optional)", window, cx),
            account_liquidity: choice(LIQUIDITY.iter().map(|l| SharedString::from(*l)).collect(), window, cx),
            account_visibility: choice(VISIBILITY.iter().map(|v| SharedString::from(v.label())).collect(), window, cx),
            account_access: choice(ACCESS.iter().map(|a| SharedString::from(a.label())).collect(), window, cx),
            series_name: text("e.g. Salary, Rent, Car loan repayment", window, cx),
            series_amount: text("expected amount per occurrence", window, cx),
            series_low: text("lowest plausible (optional range)", window, cx),
            series_high: text("highest plausible", window, cx),
            series_recurrence: choice(RECURRENCES.iter().map(|r| SharedString::from(*r)).collect(), window, cx),
            series_day: text("day of month (1–31) or every N weeks", window, cx),
            series_from: date(window, cx),
            series_until: date(window, cx),
            series_account: choice(account_names.clone(), window, cx),
            series_transfer_to: choice(account_names.clone(), window, cx),
            series_entity: choice(entity_names, window, cx),
            series_certainty: choice(certainties.clone(), window, cx),
            series_category: text("category, e.g. Salary, Housing, Living", window, cx),
            assumption_text: text("what must hold for the forecast", window, cx),
            assumption_certainty: choice(certainties, window, cx),
            assumption_series: choice(none_or_series, window, cx),
            assumption_expires: date(window, cx),
            scenario_name: text("e.g. Buy a home", window, cx),
            scenario_description: text("what changes in this scenario", window, cx),
            actual_date: date(window, cx),
            actual_account: choice(account_names, window, cx),
            actual_amount: text("signed amount: +income, −payment", window, cx),
            actual_description: text("description as on the statement", window, cx),
            actual_link_series: choice(none_or_series_link, window, cx),
            actual_link_due: date(window, cx),
            reconcile_balance: text("settled balance on the statement", window, cx),
            reconcile_date: date(window, cx),
            people,
            companies,
            accounts,
            entities,
            series_ids,
        }
    }
}

/// Which entry dialog to open.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Entry {
    Person,
    Company,
    Account,
    Series,
    Assumption,
    Scenario,
    Actual,
    Reconcile(AccountId),
}

fn read_money(state: &Entity<InputState>, currency: atlas_core::Currency, cx: &App, label: &str) -> Result<Option<Money>, String> {
    let value = state.read(cx).value().trim().to_string();
    if value.is_empty() {
        return Ok(None);
    }
    Money::parse(&value, currency).map(Some).map_err(|e| format!("{label}: {e}"))
}

impl AtlasApp {
    /// Opens one of the entry dialogs.
    pub fn open_entry(&mut self, entry: Entry, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.entity().downgrade();
        let forms = &self.entry_forms;
        let draft = forms.draft.clone();
        let (title, body, confirm_id): (String, Box<dyn Fn(&App) -> AnyElement>, &'static str) = match entry {
            Entry::Person => {
                let name = forms.person_name.clone();
                name.update(cx, |s, cx| s.set_value("", window, cx));
                let draft = draft.clone();
                (
                    "New person".into(),
                    Box::new(move |cx: &App| {
                        let role = draft.read(cx).person_role;
                        let draft = draft.clone();
                        Form::vertical()
                            .child(Field::new().label("Name").required(true).child(Input::new(&name).id("entry-person-name")))
                            .child(
                                Field::new().label("Household role").child(
                                    RadioGroup::vertical("entry-person-role")
                                        .children(ROLES.iter().map(|r| r.label()))
                                        .selected_index(Some(role))
                                        .on_change(move |index, _, cx| draft.update(cx, |d, cx| { d.person_role = *index; cx.notify(); })),
                                ),
                            )
                            .into_any_element()
                    }),
                    "entry-save-person",
                )
            }
            Entry::Company => {
                let name = forms.company_name.clone();
                let jurisdiction = forms.company_jurisdiction.clone();
                let owner = forms.company_owner.clone();
                name.update(cx, |s, cx| s.set_value("", window, cx));
                (
                    "New company".into(),
                    Box::new(move |_| {
                        Form::vertical()
                            .child(Field::new().label("Name").required(true).child(Input::new(&name).id("entry-company-name")))
                            .child(Field::new().label("Jurisdiction (free text; nothing is inferred from it)").child(Input::new(&jurisdiction)))
                            .child(Field::new().label("Owner (100 %; add co-owners later)").child(Select::new(&owner)))
                            .into_any_element()
                    }),
                    "entry-save-company",
                )
            }
            Entry::Account => {
                let f = (
                    forms.account_name.clone(),
                    forms.account_institution.clone(),
                    forms.account_kind.clone(),
                    forms.account_holder.clone(),
                    forms.account_second_holder.clone(),
                    forms.account_company.clone(),
                    forms.account_balance.clone(),
                    forms.account_minimum.clone(),
                    forms.account_liquidity.clone(),
                    forms.account_visibility.clone(),
                    forms.account_access.clone(),
                );
                f.0.update(cx, |s, cx| s.set_value("", window, cx));
                f.6.update(cx, |s, cx| s.set_value("", window, cx));
                let draft = draft.clone();
                (
                    "New account".into(),
                    Box::new(move |cx: &App| {
                        let EntryDraft { account_joint, account_company, account_include, .. } = draft.read(cx).clone();
                        let d1 = draft.clone();
                        let d2 = draft.clone();
                        let d3 = draft.clone();
                        Form::vertical()
                            .columns(2)
                            .child(Field::new().label("Name").required(true).child(Input::new(&f.0).id("entry-account-name")))
                            .child(Field::new().label("Institution").child(Input::new(&f.1)))
                            .child(Field::new().label("Type").child(Select::new(&f.2)))
                            .child(Field::new().label_indent(false).child(Checkbox::new("entry-account-company").label("Held by a company (business cash, not household cash)").checked(account_company).on_change(move |v, _, cx| d1.update(cx, |d, cx| { d.account_company = *v; cx.notify(); }))))
                            .child(if account_company {
                                Field::new().label("Company").child(Select::new(&f.5))
                            } else {
                                Field::new().label("Economic owner").child(Select::new(&f.3))
                            })
                            .child(Field::new().label_indent(false).child(Checkbox::new("entry-account-joint").label("Joint 50/50 with a second person").checked(account_joint && !account_company).on_change(move |v, _, cx| d2.update(cx, |d, cx| { d.account_joint = *v; cx.notify(); }))))
                            .child(Field::new().label("Second owner").child(Select::new(&f.4)))
                            .child(Field::new().label("Settled balance today").required(true).child(Input::new(&f.6).id("entry-account-balance")))
                            .child(Field::new().label("Bank minimum balance").child(Input::new(&f.7)))
                            .child(Field::new().label("Liquidity").child(Select::new(&f.8)))
                            .child(Field::new().label_indent(false).child(Checkbox::new("entry-account-include").label("Include in household calculations").checked(account_include).on_change(move |v, _, cx| d3.update(cx, |d, cx| { d.account_include = *v; cx.notify(); }))))
                            .child(Field::new().label("Visibility to other people").child(Select::new(&f.9)))
                            .child(Field::new().label("Use in calculations").child(Select::new(&f.10)))
                            .into_any_element()
                    }),
                    "entry-save-account",
                )
            }
            Entry::Series => {
                let f = (
                    forms.series_name.clone(),
                    forms.series_amount.clone(),
                    forms.series_low.clone(),
                    forms.series_high.clone(),
                    forms.series_recurrence.clone(),
                    forms.series_day.clone(),
                    forms.series_from.clone(),
                    forms.series_until.clone(),
                    forms.series_account.clone(),
                    forms.series_transfer_to.clone(),
                    forms.series_entity.clone(),
                    forms.series_certainty.clone(),
                    forms.series_category.clone(),
                );
                f.0.update(cx, |s, cx| s.set_value("", window, cx));
                f.1.update(cx, |s, cx| s.set_value("", window, cx));
                let as_of = self.household.as_of;
                f.6.update(cx, |s, cx| s.set_date(as_of, window, cx));
                f.7.update(cx, |s, cx| s.set_date(gpui_kit::base::Date::Single(None), window, cx));
                let draft = draft.clone();
                (
                    "Add planned movement".into(),
                    Box::new(move |cx: &App| {
                        let EntryDraft { series_direction, series_ranged, .. } = draft.read(cx).clone();
                        let d1 = draft.clone();
                        let d2 = draft.clone();
                        Form::vertical()
                            .columns(2)
                            .child(Field::new().label("Name").required(true).child(Input::new(&f.0).id("entry-series-name")))
                            .child(
                                Field::new().label("Direction").child(
                                    RadioGroup::horizontal("entry-series-direction")
                                        .children(["Income", "Expense", "Transfer"])
                                        .selected_index(Some(series_direction))
                                        .on_change(move |index, _, cx| d1.update(cx, |d, cx| { d.series_direction = *index; cx.notify(); })),
                                ),
                            )
                            .child(Field::new().label("Expected amount per occurrence").required(true).child(Input::new(&f.1).id("entry-series-amount")))
                            .child(Field::new().label_indent(false).child(Checkbox::new("entry-series-ranged").label("Give a range").checked(series_ranged).on_change(move |v, _, cx| d2.update(cx, |d, cx| { d.series_ranged = *v; cx.notify(); }))))
                            .child(Field::new().label("Lowest").child(Input::new(&f.2)))
                            .child(Field::new().label("Highest").child(Input::new(&f.3)))
                            .child(Field::new().label("Recurrence").child(Select::new(&f.4)))
                            .child(Field::new().label("Day of month / every N weeks").child(Input::new(&f.5)))
                            .child(Field::new().label("First occurrence on / from").child(DatePicker::new(&f.6)))
                            .child(Field::new().label("Until (optional)").child(DatePicker::new(&f.7)))
                            .child(Field::new().label("Account it posts to").child(Select::new(&f.8)))
                            .child(Field::new().label("Transfer to (transfers only)").child(Select::new(&f.9)))
                            .child(Field::new().label("Whose movement is it").child(Select::new(&f.10)))
                            .child(Field::new().label("Certainty").child(Select::new(&f.11)))
                            .child(Field::new().label("Category (tax rules match on it)").child(Input::new(&f.12)))
                            .into_any_element()
                    }),
                    "entry-save-series",
                )
            }
            Entry::Assumption => {
                let f = (forms.assumption_text.clone(), forms.assumption_certainty.clone(), forms.assumption_series.clone(), forms.assumption_expires.clone());
                f.0.update(cx, |s, cx| s.set_value("", window, cx));
                (
                    "New assumption".into(),
                    Box::new(move |_| {
                        Form::vertical()
                            .child(Field::new().label("Assumption").required(true).child(Input::new(&f.0).id("entry-assumption-text")))
                            .child(Field::new().label("Certainty").child(Select::new(&f.1)))
                            .child(Field::new().label("Applies to").child(Select::new(&f.2)))
                            .child(Field::new().label("Expires (optional)").child(DatePicker::new(&f.3)))
                            .into_any_element()
                    }),
                    "entry-save-assumption",
                )
            }
            Entry::Scenario => {
                let f = (forms.scenario_name.clone(), forms.scenario_description.clone());
                f.0.update(cx, |s, cx| s.set_value("", window, cx));
                let draft = draft.clone();
                (
                    "New scenario".into(),
                    Box::new(move |cx: &App| {
                        let private = draft.read(cx).scenario_private;
                        let d = draft.clone();
                        Form::vertical()
                            .child(Field::new().label("Name").required(true).child(Input::new(&f.0).id("entry-scenario-name")))
                            .child(Field::new().label("What changes").child(Input::new(&f.1)))
                            .child(Field::new().label_indent(false).child(Checkbox::new("entry-scenario-private").label("Private to me").checked(private).on_change(move |v, _, cx| d.update(cx, |x, cx| { x.scenario_private = *v; cx.notify(); }))))
                            .into_any_element()
                    }),
                    "entry-save-scenario",
                )
            }
            Entry::Actual => {
                let f = (forms.actual_date.clone(), forms.actual_account.clone(), forms.actual_amount.clone(), forms.actual_description.clone(), forms.actual_link_series.clone(), forms.actual_link_due.clone());
                let as_of = self.household.as_of;
                f.0.update(cx, |s, cx| s.set_date(as_of, window, cx));
                f.2.update(cx, |s, cx| s.set_value("", window, cx));
                (
                    "Record transaction".into(),
                    Box::new(move |cx: &App| {
                        let muted = cx.theme().muted_foreground;
                        v_flex()
                            .gap_3()
                            .child(div().text_xs().text_color(muted).child("Recording a transaction does not update the statement balance."))
                            .child(
                                Form::vertical()
                                    .child(Field::new().label("Date").child(DatePicker::new(&f.0)))
                                    .child(Field::new().label("Account").child(Select::new(&f.1)))
                                    .child(Field::new().label("Signed amount (+ in, − out)").required(true).child(Input::new(&f.2).id("entry-actual-amount")))
                                    .child(Field::new().label("Description").child(Input::new(&f.3).id("entry-actual-description")))
                                    .child(Field::new().label("Match to a planned occurrence").child(Select::new(&f.4)))
                                    .child(Field::new().label("Original due date of that occurrence").child(DatePicker::new(&f.5))),
                            )
                            .into_any_element()
                    }),
                    "entry-save-actual",
                )
            }
            Entry::Reconcile(account) => {
                let f = (forms.reconcile_balance.clone(), forms.reconcile_date.clone());
                let as_of = self.household.as_of;
                let current = self.household.account(account).map(|a| a.settled_balance.format()).unwrap_or_default();
                f.0.update(cx, |s, cx| s.set_value(current, window, cx));
                f.1.update(cx, |s, cx| s.set_date(as_of, window, cx));
                let name = self.household.account(account).map(|a| a.name.clone()).unwrap_or_default();
                (
                    format!("Reconcile “{name}”"),
                    Box::new(move |_| {
                        Form::vertical()
                            .child(Field::new().label("Settled balance on the statement").required(true).child(Input::new(&f.0).id("entry-reconcile-balance")))
                            .child(Field::new().label("As of").child(DatePicker::new(&f.1)))
                            .into_any_element()
                    }),
                    "entry-save-reconcile",
                )
            }
        };
        let body = std::rc::Rc::new(body);
        let wide = matches!(entry, Entry::Account | Entry::Series);
        let commit_label: &'static str = match entry {
            Entry::Person => "Add person",
            Entry::Company => "Add company",
            Entry::Account => "Add account",
            Entry::Series => "Add movement",
            Entry::Assumption => "Add assumption",
            Entry::Scenario => "Add scenario",
            Entry::Actual => "Record transaction",
            Entry::Reconcile(_) => "Reconcile",
        };
        window.open_dialog(cx, move |dialog, window, cx| {
            let this = this.clone();
            let body = body.clone();
            // `Dialog::w` takes pixels; 56 rem keeps the two-column forms readable.
            let wide_width = window.rem_size() * 56.;
            dialog
                .title(title.clone())
                .map(|d| if wide { d.w(wide_width) } else { d.w_96() })
                .child(v_flex().child(body(cx)))
                .footer(
                    DialogFooter::new()
                        .child(Button::new("entry-cancel").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                        .child(Button::new(confirm_id).primary().label(commit_label).on_click({
                            let this = this.clone();
                            move |_, window, cx| {
                                Self::confirm_entry(&this, entry, window, cx);
                            }
                        })),
                )
                .on_ok(move |_, window, cx| Self::confirm_entry(&this, entry, window, cx))
        });
    }

    fn confirm_entry(this: &WeakEntity<Self>, entry: Entry, window: &mut Window, cx: &mut App) -> bool {
        match this.update(cx, |app, cx| app.submit_entry(entry, window, cx)) {
            Ok(Ok(summary)) => {
                let _ = this.update(cx, |app, cx| {
                    app.note_result(summary.clone());
                    cx.notify();
                });
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

    /// Validates a form through the engine and applies it.
    pub fn submit_entry(&mut self, entry: Entry, window: &mut Window, cx: &mut Context<Self>) -> Result<String, String> {
        let currency = self.household.base_currency;
        let draft = self.entry_forms.draft.read(cx).clone();
        let summary = match entry {
            Entry::Person => {
                let name = self.entry_forms.person_name.read(cx).value().trim().to_string();
                if name.is_empty() {
                    return Err("Give the person a name.".into());
                }
                let role = ROLES.get(draft.person_role).copied().unwrap_or(HouseholdRole::Member);
                let id = self.household.add_person(Person { id: self.household.next_person_id(), name: name.clone(), role });
                format!("{name} added as {} ({id})", role.label().to_lowercase())
            }
            Entry::Company => {
                let name = self.entry_forms.company_name.read(cx).value().trim().to_string();
                if name.is_empty() {
                    return Err("Give the company a name.".into());
                }
                let owner = *self.entry_forms.people.get(selected_row(&self.entry_forms.company_owner, cx)).ok_or("Add a person first; a company needs an owner.")?;
                let jurisdiction = self.entry_forms.company_jurisdiction.read(cx).value().trim().to_string();
                self.household
                    .add_company(Company {
                        id: self.household.next_company_id(),
                        name: name.clone(),
                        jurisdiction: if jurisdiction.is_empty() { "not stated".into() } else { jurisdiction },
                        owners: vec![OwnershipShare { person: owner, basis_points: 10_000 }],
                        roles: vec![EntityRoleAssignment { person: owner, role: EntityRole::OwnerDirector }],
                        employees: Vec::new(),
                        constraints: Vec::new(),
                    })
                    .map_err(|e| e.to_string())?;
                format!("Company {name} added; its cash stays out of household cash")
            }
            Entry::Account => {
                let f = &self.entry_forms;
                let name = f.account_name.read(cx).value().trim().to_string();
                if name.is_empty() {
                    return Err("Give the account a name.".into());
                }
                let kind = ACCOUNT_KINDS.get(selected_row(&f.account_kind, cx)).copied().unwrap_or(AccountKind::Checking);
                let holder = if draft.account_company {
                    let company = *f.companies.get(selected_row(&f.account_company, cx)).ok_or("Add a company first.")?;
                    Holder::Company(company)
                } else {
                    let first = *f.people.get(selected_row(&f.account_holder, cx)).ok_or("Add a person first.")?;
                    if draft.account_joint {
                        let second = *f.people.get(selected_row(&f.account_second_holder, cx)).ok_or("Pick the second owner.")?;
                        if second == first {
                            return Err("A joint account needs two different people.".into());
                        }
                        Holder::Persons(vec![OwnershipShare { person: first, basis_points: 5_000 }, OwnershipShare { person: second, basis_points: 5_000 }])
                    } else {
                        Holder::Persons(vec![OwnershipShare { person: first, basis_points: 10_000 }])
                    }
                };
                let settled = read_money(&f.account_balance, currency, cx, "Settled balance")?.ok_or("Enter the settled balance (0 is fine).")?;
                let minimum = read_money(&f.account_minimum, currency, cx, "Bank minimum")?;
                let liquidity = match selected_row(&f.account_liquidity, cx) {
                    1 => Liquidity::Delayed { business_days: 2 },
                    2 => Liquidity::LockedUntil(self.household.as_of.checked_add_months(chrono::Months::new(6)).unwrap_or(self.household.as_of)),
                    _ => Liquidity::Immediate,
                };
                let visibility = VISIBILITY.get(selected_row(&f.account_visibility, cx)).copied().unwrap_or(VisibilityPreset::FullyShared);
                let access = ACCESS.get(selected_row(&f.account_access, cx)).copied().unwrap_or(CalculationAccess::Full);
                let institution = f.account_institution.read(cx).value().trim().to_string();
                let is_company = holder.is_company();
                let account = Account {
                    id: self.household.next_account_id(),
                    name: name.clone(),
                    institution: if institution.is_empty() { "—".into() } else { institution },
                    kind,
                    holder,
                    currency,
                    liquidity,
                    minimum_balance: minimum,
                    transfer_delay_days: 0,
                    fees: Vec::new(),
                    tax_treatment: String::new(),
                    source_of_truth: SourceOfTruth::Manual,
                    last_reconciled: Some(self.household.as_of),
                    withdrawals_permitted: !kind.is_liability(),
                    funds_categories: Vec::new(),
                    include_in_household: draft.account_include && !is_company,
                    settled_balance: settled,
                    pending_balance: Money::zero(currency),
                };
                self.household.add_account(account, visibility, access).map_err(|e| e.to_string())?;
                format!("Account {name} added with {} settled", settled.format())
            }
            Entry::Series => {
                let f = &self.entry_forms;
                let name = f.series_name.read(cx).value().trim().to_string();
                if name.is_empty() {
                    return Err("Give the series a name.".into());
                }
                let expected = read_money(&f.series_amount, currency, cx, "Amount")?.ok_or("Enter the expected amount.")?;
                if expected.is_negative() {
                    return Err("Amounts are positive; the direction says which way the money moves.".into());
                }
                let amount = if draft.series_ranged {
                    let low = read_money(&f.series_low, currency, cx, "Lowest")?.unwrap_or(expected);
                    let high = read_money(&f.series_high, currency, cx, "Highest")?.unwrap_or(expected);
                    if low.minor() > expected.minor() || high.minor() < expected.minor() {
                        return Err("The range must contain the expected amount.".into());
                    }
                    AmountSpec::Range { low, expected, high }
                } else {
                    AmountSpec::Exact(expected)
                };
                let from = f.series_from.read(cx).date().start().ok_or("Pick the first occurrence date.")?;
                let until = match f.series_until.read(cx).date().start() {
                    Some(end) if end < from => return Err("The end date is before the start.".into()),
                    Some(end) => Until::Date(end),
                    None => Until::Indefinite,
                };
                let n: u32 = f.series_day.read(cx).value().trim().parse().unwrap_or(0);
                let recurrence = match selected_row(&f.series_recurrence, cx) {
                    1 => Recurrence::OneTime { on: DateSpec::Exact(from) },
                    0 => {
                        let day = if (1..=31).contains(&n) { n as u8 } else { from.format("%d").to_string().parse().unwrap_or(1) };
                        Recurrence::Monthly { every_n_months: 1, day, from, until, invalid_day: InvalidDayPolicy::ClampToMonthEnd }
                    }
                    2 => Recurrence::LastDayOfMonth { every_n_months: 1, from, until },
                    3 => Recurrence::Weekly { every_n_weeks: 1, from, until },
                    4 => Recurrence::Weekly { every_n_weeks: n.max(1), from, until },
                    _ => Recurrence::Yearly { month: from.format("%m").to_string().parse().unwrap_or(1), day: from.format("%d").to_string().parse().unwrap_or(1), from, until },
                };
                let account = *f.accounts.get(selected_row(&f.series_account, cx)).ok_or("Add an account first.")?;
                let direction = match draft.series_direction {
                    0 => Direction::Income,
                    1 => Direction::Expense,
                    _ => {
                        let to = *f.accounts.get(selected_row(&f.series_transfer_to, cx)).ok_or("Pick the account the transfer goes to.")?;
                        if to == account {
                            return Err("A transfer needs two different accounts.".into());
                        }
                        Direction::Transfer { to }
                    }
                };
                let entity = f.entities.get(selected_row(&f.series_entity, cx)).copied().unwrap_or(EntityRef::Household);
                let certainty = Certainty::ALL.get(selected_row(&f.series_certainty, cx)).copied().unwrap_or(Certainty::UserEstimated);
                let category = f.series_category.read(cx).value().trim().to_string();
                let series = EventSeries {
                    id: self.household.next_series_id(),
                    name: name.clone(),
                    direction,
                    amount,
                    amount_changes: Vec::new(),
                    exceptions: Vec::new(),
                    recurrence,
                    settlement_lag_days: 0,
                    availability_lag_days: 0,
                    intraday_order: if direction == Direction::Income { 20 } else { 10 },
                    account,
                    linked_account: None,
                    entity,
                    certainty,
                    category: if category.is_empty() { "Uncategorised".into() } else { category },
                    tax_treatment: String::new(),
                    scenario: None,
                    notes: String::new(),
                };
                self.household.add_series(series).map_err(|e| e.to_string())?;
                format!("Series {name} added; forecasts recomputed")
            }
            Entry::Assumption => {
                let f = &self.entry_forms;
                let text = f.assumption_text.read(cx).value().trim().to_string();
                if text.is_empty() {
                    return Err("Write the assumption.".into());
                }
                let certainty = Certainty::ALL.get(selected_row(&f.assumption_certainty, cx)).copied().unwrap_or(Certainty::UserEstimated);
                let row = selected_row(&f.assumption_series, cx);
                let applies_to = if row == 0 { Vec::new() } else { f.series_ids.get(row - 1).map(|s| vec![*s]).unwrap_or_default() };
                let expires_on = f.assumption_expires.read(cx).date().start();
                self.household.add_assumption(Assumption {
                    id: self.household.next_assumption_id(),
                    text: text.clone(),
                    certainty,
                    source: AssumptionSource::UserEntered,
                    accepted_on: Some(self.household.as_of),
                    expires_on,
                    applies_to,
                    private_to: None,
                });
                format!("Assumption recorded and accepted today: {text}")
            }
            Entry::Scenario => {
                let f = &self.entry_forms;
                let name = f.scenario_name.read(cx).value().trim().to_string();
                if name.is_empty() {
                    return Err("Name the scenario.".into());
                }
                let description = f.scenario_description.read(cx).value().trim().to_string();
                let owner = self.viewer.person;
                self.household.add_scenario(
                    Scenario { id: self.household.next_scenario_id(), name: name.clone(), description, private_to: if draft.scenario_private { Some(owner) } else { None }, changes: Vec::new(), composed_of: Vec::new() },
                    owner,
                );
                format!("Scenario {name} added{}", if draft.scenario_private { " (private to you)" } else { "" })
            }
            Entry::Actual => {
                let f = &self.entry_forms;
                let date = f.actual_date.read(cx).date().start().ok_or("Pick the transaction date.")?;
                let account = *f.accounts.get(selected_row(&f.actual_account, cx)).ok_or("Add an account first.")?;
                let amount = read_money(&f.actual_amount, currency, cx, "Amount")?.ok_or("Enter the signed amount.")?;
                let description = f.actual_description.read(cx).value().trim().to_string();
                let id = self.household.next_transaction_id();
                self.household
                    .add_actual(ActualTransaction { id, date, account, amount, description: if description.is_empty() { "manual entry".into() } else { description.clone() } })
                    .map_err(|e| e.to_string())?;
                let link_row = selected_row(&f.actual_link_series, cx);
                let mut summary = format!("Actual of {} recorded on {}", amount.format(), date.format("%d %b %Y"));
                if link_row > 0 {
                    let series = *f.series_ids.get(link_row - 1).ok_or("Unknown series.")?;
                    let due = f.actual_link_due.read(cx).date().start().unwrap_or(date);
                    self.household
                        .link_actual(ReconciliationLink { series, original_due: due, transaction: id, amount: amount.abs() })
                        .map_err(|e| e.to_string())?;
                    summary.push_str(&format!("; reconciled to the occurrence due {}", due.format("%d %b %Y")));
                }
                summary
            }
            Entry::Reconcile(account) => {
                let f = &self.entry_forms;
                let settled = read_money(&f.reconcile_balance, currency, cx, "Settled balance")?.ok_or("Enter the settled balance.")?;
                let on = f.reconcile_date.read(cx).date().start().unwrap_or(self.household.as_of);
                self.household.reconcile_account(account, settled, on).map_err(|e| e.to_string())?;
                format!("Reconciled to {} as of {}", settled.format(), on.format("%d %b %Y"))
            }
        };
        log::info!("entry {:?}: {summary}", entry);
        self.mark_dirty();
        if self.household.person(self.viewer.person).is_none()
            && let Some(first) = self.household.people.first()
        {
            self.viewer = atlas_core::authz::Viewer::person(first.id);
        }
        self.rebuild_forms(window, cx);
        self.refresh_derived();
        cx.notify();
        Ok(summary)
    }

    /// Deletes an object through the engine's referential checks.
    pub fn delete_object(&mut self, object: ObjectRef, window: &mut Window, cx: &mut Context<Self>) {
        let result = match object {
            ObjectRef::Account(id) => self.household.remove_account(id),
            ObjectRef::Series(id) => self.household.remove_series(id),
            ObjectRef::Reservation(id) => self.household.remove_reservation(id),
            ObjectRef::Person(id) => self.household.remove_person(id),
            other => Err(atlas_core::EngineError::Insufficient(format!("{other} cannot be deleted from here"))),
        };
        match result {
            Ok(()) => {
                log::info!("deleted {object}");
                self.mark_dirty();
                self.rebuild_forms(window, cx);
                self.refresh_derived();
                window.push_notification(format!("Deleted {object}"), cx);
            }
            Err(err) => {
                alerting::report(Level::Warning, format!("delete refused for {object}: {err}"));
                window.push_notification(format!("Not deleted — {err}"), cx);
            }
        }
        cx.notify();
    }
}
