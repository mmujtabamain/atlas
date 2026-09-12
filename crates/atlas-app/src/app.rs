//! `AtlasApp`: the window's root view. It owns the household, the viewer, the
//! active section and the derived screen models; screens are pure rendering
//! over those models.

use atlas_core::authz::Viewer;
use atlas_core::fixtures;
use atlas_core::ids::EntityRef;
use atlas_core::model::Household;
use atlas_core::{EngineError, Money};
use chrono::NaiveDate;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Root, Sizable as _, Theme, ThemeMode, TitleBar, WindowExt as _,
    alert::Alert,
    button::{Button, ButtonVariants as _},
    h_flex,
    scroll::ScrollableElement as _,
    separator::Separator,
    sidebar::{Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
    status_bar::StatusBar,
    tag::Tag,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::alerting::{self, Level};
use crate::launch::{Launch, Start};
use crate::perf;
use crate::screens::{
    self, Section,
    entities::EntityModels,
    household::HouseholdOverview,
    liquidity::LiquidityModel,
    timeline::{TimelineFilter, TimelineModel},
    projections::ProjectionModel,
    assumptions::AssumptionsModel,
    taxes::{E05Schedule, TaxModel},
    rules::RulesModel,
    scenarios::ScenariosModel,
};
use atlas_core::ids::ScenarioId;
use crate::scenario_entry::ScenarioForms;
use crate::decision_entry::DecisionForm;
use atlas_core::decision::{Decision, PurchasePlan};
use crate::privacy_entry::PrivacyForms;
use crate::screens::privacy::PrivacyModel;
use atlas_core::ids::RuleId;
use atlas_core::rules::TieBreak;
use crate::rules_entry::RuleForm;
use atlas_core::model::{TaxKind, TaxRule, TaxTiming, ThresholdBasis};
use gpui_kit::component::input::InputEvent;
use atlas_core::assumptions::{Derivation, apply_derived, derive};
use atlas_core::ids::AssumptionId;
use atlas_core::forecast::Case;
use atlas_core::timeline::{AmountSpec, Exception, ExceptionKind, OccurrenceStatus};
use atlas_core::vocab::Certainty;
use gpui_kit::component::{date_picker::{DatePicker, DatePickerState}, select::SelectEvent};
use chrono::Months;
use atlas_core::ids::{AccountId, CompanyId, PersonId, ReservationId, SeriesId};
use atlas_core::liquidity::Boundary;
use atlas_core::model::{Coverage, Hardness, Reservation};
use atlas_core::ids::ObjectRef;
use atlas_core::Disclosure;
use gpui_kit::component::{
    IndexPath,
    dialog::DialogFooter,
    form::{Field, Form},
    input::{Input, InputState},
    radio::RadioGroup,
    select::{Select, SelectState},
};

pub struct AtlasApp {
    pub(crate) household: Household,
    pub(crate) viewer: Viewer,
    pub(crate) section: Section,
    pub(crate) horizon: NaiveDate,
    pub(crate) sidebar_collapsed: bool,
    /// Derived once per state change; screens only read it.
    pub(crate) overview: Result<HouseholdOverview, EngineError>,
    pub(crate) entities: Result<EntityModels, EngineError>,
    pub(crate) liquidity: Result<LiquidityModel, EngineError>,
    pub(crate) boundary: Boundary,
    pub(crate) selected_person: Option<PersonId>,
    pub(crate) selected_company: Option<CompanyId>,
    pub(crate) selected_account: Option<AccountId>,
    pub(crate) reservation_form: ReservationForm,
    pub(crate) timeline_filter: TimelineFilter,
    pub(crate) timeline: Result<TimelineModel, EngineError>,
    pub(crate) timeline_controls: TimelineControls,
    pub(crate) series_form: SeriesForm,
    pub(crate) projection_boundary: Boundary,
    pub(crate) projection_case: Case,
    pub(crate) projection_scenario: bool,
    pub(crate) projection: Result<ProjectionModel, EngineError>,
    pub(crate) derivation_series: Option<SeriesId>,
    pub(crate) derivation: Derivation,
    pub(crate) sensitivity_boundary: Boundary,
    pub(crate) sensitivity_scenario: bool,
    pub(crate) assumptions: Result<AssumptionsModel, EngineError>,
    pub(crate) tax_scenario: bool,
    e05_amount: Money,
    e05_split: bool,
    e05_schedule: E05Schedule,
    pub(crate) taxes: Result<TaxModel, EngineError>,
    pub(crate) tax_controls: TaxControls,
    pub(crate) tax_form: TaxRuleForm,
    pub(crate) tax_form_effective_from_override: Option<NaiveDate>,
    pub(crate) rules_scenario: bool,
    pub(crate) simulated_rule: Option<RuleId>,
    pub(crate) rules: Result<RulesModel, EngineError>,
    pub(crate) rule_form: RuleForm,
    pub(crate) scenario_selection: Vec<ScenarioId>,
    pub(crate) scenario_case: Case,
    pub(crate) scenarios: Result<ScenariosModel, EngineError>,
    pub(crate) scenario_forms: ScenarioForms,
    pub(crate) decision_step: usize,
    pub(crate) decision_plan: PurchasePlan,
    pub(crate) decision: Option<Result<Decision, EngineError>>,
    pub(crate) decision_form: DecisionForm,
    pub(crate) privacy: PrivacyModel,
    pub(crate) privacy_forms: PrivacyForms,
    /// Where the household is saved, once it has a file (M12).
    pub(crate) file: Option<atlas_store::HouseholdFile>,
    /// Unsaved changes since the last save/load.
    pub(crate) dirty: bool,
    /// Lock owner name for the household file.
    pub(crate) owner: String,
    pub(crate) lifecycle_form: crate::lifecycle::LifecycleForm,
    pub(crate) entry_forms: crate::entry::EntryForms,
    /// Frame timing behind the status-bar FPS counter and the `perf:` log lines.
    pub(crate) perf: crate::perf::FrameMeter,
    pub(crate) _subscriptions: Vec<Subscription>,
}

/// Retained controls of the Taxes screen.
pub struct TaxControls {
    pub e05_amount: Entity<InputState>,
}

/// Values behind the stateless controls of the new-rule dialog.
#[derive(Debug, Clone, Default)]
pub struct TaxRuleDraft {
    pub kind: usize,
    pub timing: usize,
}

/// Retained state of the "New tax rule" dialog (§12.2).
pub struct TaxRuleForm {
    name: Entity<InputState>,
    tax_type: Entity<InputState>,
    category: Entity<SelectState<Vec<SharedString>>>,
    rate: Entity<InputState>,
    threshold: Entity<InputState>,
    source: Entity<InputState>,
    effective_from: Entity<DatePickerState>,
    effective_to: Entity<DatePickerState>,
    draft: Entity<TaxRuleDraft>,
    categories: Vec<String>,
}

impl TaxRuleForm {
    fn new(household: &Household, window: &mut Window, cx: &mut Context<AtlasApp>) -> Self {
        let categories = household.categories();
        let names: Vec<SharedString> = categories.iter().map(|c| SharedString::from(c.clone())).collect();
        TaxRuleForm {
            name: cx.new(|cx| InputState::new(window, cx).placeholder("e.g. Municipal levy on card spending")),
            tax_type: cx.new(|cx| InputState::new(window, cx).placeholder("e.g. Card transaction tax")),
            category: cx.new(|cx| SelectState::new(names, Some(IndexPath::default()), window, cx)),
            rate: cx.new(|cx| InputState::new(window, cx).placeholder("rate in percent, e.g. 2.5")),
            threshold: cx.new(|cx| InputState::new(window, cx).placeholder("threshold amount (only for threshold rules)")),
            source: cx.new(|cx| InputState::new(window, cx).placeholder("official source or note — rule stays unverified")),
            effective_from: cx.new(|cx| DatePickerState::new(window, cx).date_format("%d %b %Y")),
            effective_to: cx.new(|cx| DatePickerState::new(window, cx).date_format("%d %b %Y")),
            draft: cx.new(|_| TaxRuleDraft::default()),
            categories,
        }
    }
}

/// Retained filter controls of the Timeline screen. Each `Select` is owned
/// here; a `SelectEvent::Confirm` subscription rebuilds the filter.
pub struct TimelineControls {
    pub entity: Entity<SelectState<Vec<SharedString>>>,
    pub account: Entity<SelectState<Vec<SharedString>>>,
    pub certainty: Entity<SelectState<Vec<SharedString>>>,
    pub status: Entity<SelectState<Vec<SharedString>>>,
    pub horizon: Entity<SelectState<Vec<SharedString>>>,
    entities: Vec<EntityRef>,
    accounts: Vec<AccountId>,
}

impl TimelineControls {
    fn new(household: &Household, viewer: Viewer, window: &mut Window, cx: &mut Context<AtlasApp>) -> Self {
        let mut entities = vec![EntityRef::Household];
        entities.extend(household.people.iter().map(|p| EntityRef::Person(p.id)));
        entities.extend(
            household
                .companies
                .iter()
                .filter(|c| matches!(household.disclosure_for(viewer, ObjectRef::Company(c.id)), Disclosure::Full | Disclosure::SelectedFields))
                .map(|c| EntityRef::Company(c.id)),
        );
        let mut entity_names: Vec<SharedString> = vec!["All entities".into()];
        entity_names.extend(entities.iter().map(|e| SharedString::from(household.entity_name(*e))));
        let accounts: Vec<AccountId> = household
            .accounts
            .iter()
            .filter(|a| !matches!(household.disclosure_for(viewer, ObjectRef::Account(a.id)), Disclosure::Hidden | Disclosure::Aggregate))
            .map(|a| a.id)
            .collect();
        let mut account_names: Vec<SharedString> = vec!["All accounts".into()];
        account_names.extend(accounts.iter().filter_map(|id| household.account(*id)).map(|a| SharedString::from(a.name.clone())));
        let mut certainty_names: Vec<SharedString> = vec!["All certainties".into()];
        certainty_names.extend(Certainty::ALL.iter().map(|c| SharedString::from(c.label())));
        let mut status_names: Vec<SharedString> = vec!["All statuses".into()];
        status_names.extend(STATUSES.iter().map(|s| SharedString::from(s.label())));
        let horizon_names: Vec<SharedString> = HORIZONS.iter().map(|(label, _)| SharedString::from(*label)).collect();
        let first = Some(IndexPath::default());
        TimelineControls {
            entity: cx.new(|cx| SelectState::new(entity_names, first, window, cx)),
            account: cx.new(|cx| SelectState::new(account_names, first, window, cx)),
            certainty: cx.new(|cx| SelectState::new(certainty_names, first, window, cx)),
            status: cx.new(|cx| SelectState::new(status_names, first, window, cx)),
            horizon: cx.new(|cx| SelectState::new(horizon_names, first, window, cx)),
            entities,
            accounts,
        }
    }

    fn all(&self) -> [Entity<SelectState<Vec<SharedString>>>; 5] {
        [self.entity.clone(), self.account.clone(), self.certainty.clone(), self.status.clone(), self.horizon.clone()]
    }
}

/// Status filter options, in display order.
const STATUSES: [OccurrenceStatus; 7] = [
    OccurrenceStatus::Planned,
    OccurrenceStatus::Due,
    OccurrenceStatus::Overdue,
    OccurrenceStatus::PartiallyFulfilled,
    OccurrenceStatus::Fulfilled,
    OccurrenceStatus::Skipped,
    OccurrenceStatus::Cancelled,
];

/// Horizon options: label and months after `as_of` (`None` = the fixture default).
const HORIZONS: [(&str, Option<u32>); 4] = [
    ("Through 31 Jan 2027 (default)", None),
    ("Next 3 months", Some(3)),
    ("Next 6 months", Some(6)),
    ("Next 12 months", Some(12)),
];

/// Retained state of the series editor dialog (§9.2–9.4).
pub struct SeriesForm {
    amount: Entity<InputState>,
    change_from: Entity<DatePickerState>,
    change_amount: Entity<InputState>,
    skip_on: Entity<DatePickerState>,
    end_after: Entity<DatePickerState>,
    editing: Option<SeriesId>,
}

impl SeriesForm {
    fn new(window: &mut Window, cx: &mut Context<AtlasApp>) -> Self {
        SeriesForm {
            amount: cx.new(|cx| InputState::new(window, cx).placeholder("expected amount for the whole series")),
            change_from: cx.new(|cx| DatePickerState::new(window, cx).date_format("%d %b %Y")),
            change_amount: cx.new(|cx| InputState::new(window, cx).placeholder("new amount from that date on")),
            skip_on: cx.new(|cx| DatePickerState::new(window, cx).date_format("%d %b %Y")),
            end_after: cx.new(|cx| DatePickerState::new(window, cx).date_format("%d %b %Y")),
            editing: None,
        }
    }
}

/// Values behind the stateless controls of the reservation dialog. They live
/// in their own entity because the dialog builder runs while `AtlasApp` is
/// being rendered and must not read it.
#[derive(Debug, Clone, Default)]
pub struct ReservationDraft {
    pub coverage: usize,
    pub hardness: usize,
}

/// Retained state of the "New reservation" form (§17).
pub struct ReservationForm {
    name: Entity<InputState>,
    amount: Entity<InputState>,
    purpose: Entity<InputState>,
    account: Entity<SelectState<Vec<SharedString>>>,
    nested_in: Entity<SelectState<Vec<SharedString>>>,
    draft: Entity<ReservationDraft>,
    account_ids: Vec<AccountId>,
    nested_ids: Vec<ReservationId>,
}

impl ReservationForm {
    fn new(household: &Household, viewer: Viewer, window: &mut Window, cx: &mut Context<AtlasApp>) -> Self {
        let account_ids = screens::liquidity::editable_accounts(household, viewer);
        let account_names: Vec<SharedString> = account_ids
            .iter()
            .filter_map(|id| household.account(*id))
            .map(|a| SharedString::from(a.name.clone()))
            .collect();
        let nested_ids: Vec<ReservationId> = household.reservations.iter().filter(|r| r.is_active()).map(|r| r.id).collect();
        let nested_names: Vec<SharedString> = nested_ids
            .iter()
            .filter_map(|id| household.reservation(*id))
            .map(|r| SharedString::from(format!("{} ({})", r.name, household.account(r.account).map(|a| a.name.as_str()).unwrap_or("?"))))
            .collect();
        ReservationForm {
            name: cx.new(|cx| InputState::new(window, cx).placeholder("e.g. Car reserve")),
            amount: cx.new(|cx| InputState::new(window, cx).placeholder("e.g. 250,000")),
            purpose: cx.new(|cx| InputState::new(window, cx).placeholder("What the money is held for")),
            account: cx.new(|cx| SelectState::new(account_names, Some(IndexPath::default()), window, cx)),
            nested_in: cx.new(|cx| SelectState::new(nested_names, None, window, cx)),
            draft: cx.new(|_| ReservationDraft::default()),
            account_ids,
            nested_ids,
        }
    }
}

impl AtlasApp {
    pub fn new(launch: &Launch, _window: &mut Window, _cx: &mut Context<Self>) -> Self {
        let _window: &mut Window = _window;
        let _cx: &mut Context<Self> = _cx;
        let owner = launch.owner.clone();
        let resolved = Self::resolve_start(launch, &owner);
        let household = resolved.household;
        let viewer = Viewer::person(match launch.viewer_id {
            Some(id) => PersonId::new(id),
            None if launch.viewer == 'b' => household.people.get(1).map(|p| p.id).unwrap_or(fixtures::ids::PERSON_B),
            None => household.people.first().map(|p| p.id).unwrap_or(fixtures::ids::PERSON_A),
        });
        let horizon = household.as_of.checked_add_months(Months::new(12)).unwrap_or(fixtures::default_horizon()).max(fixtures::default_horizon().min(household.as_of.checked_add_months(Months::new(12)).unwrap_or(household.as_of)));
        let horizon = if launch.start == Start::Sample { fixtures::default_horizon() } else { horizon };
        for notice in &resolved.notices {
            log::warn!("{notice}");
        }
        let notices = resolved.notices.clone();
        _cx.defer_in(_window, move |_, window, cx| {
            for notice in notices {
                window.push_notification(notice, cx);
            }
        });
        let overview = Self::compute_overview(&household, viewer, horizon);
        let entities = Self::compute_entities(&household, viewer);
        let boundary = Boundary::Household;
        let liquidity = Self::compute_liquidity(&household, viewer, boundary, horizon);
        let reservation_form = ReservationForm::new(&household, viewer, _window, _cx);
        let timeline_filter = TimelineFilter { entity: None, account: None, certainty: None, status: None, scenario: None, through: horizon };
        let timeline = Self::compute_timeline(&household, viewer, timeline_filter.clone());
        let timeline_controls = TimelineControls::new(&household, viewer, _window, _cx);
        let series_form = SeriesForm::new(_window, _cx);
        let projection = Self::compute_projection(&household, viewer, Boundary::Household, Case::Expected, None, horizon);
        let assumptions = Self::compute_assumptions(&household, viewer, None, Derivation::ALL[0], Boundary::Household, false, horizon);
        let e05_amount = Money::from_major(100_000, household.base_currency);
        let taxes = Self::compute_taxes(&household, viewer, horizon, false, e05_amount, true, E05Schedule::PlanExample);
        let tax_controls = TaxControls { e05_amount: _cx.new(|cx| InputState::new(_window, cx).default_value("100,000")) };
        let tax_form = TaxRuleForm::new(&household, _window, _cx);
        let rules = Self::compute_rules(&household, viewer, horizon, false, None);
        let rule_form = RuleForm::new(&household, _window, _cx);
        let scenario_selection: Vec<ScenarioId> = household.scenarios.first().map(|s| vec![s.id]).unwrap_or_default();
        let scenarios = Self::compute_scenarios(&household, viewer, horizon, Case::Expected, &scenario_selection);
        let scenario_forms = ScenarioForms::new(&household, _window, _cx);
        let decision_plan = atlas_core::decision::default_plan_for(&household, household.as_of, viewer);
        let decision_form = DecisionForm::new(&household, &decision_plan, viewer, _window, _cx);
        let privacy = PrivacyModel::compute(&household, viewer);
        let privacy_forms = PrivacyForms::new(&household, viewer.person, _window, _cx);
        let lifecycle_form = crate::lifecycle::LifecycleForm::new(_window, _cx);
        let entry_forms = crate::entry::EntryForms::new(&household, _window, _cx);
        let file = resolved.file;
        let mut subscriptions: Vec<Subscription> = timeline_controls
            .all()
            .iter()
            .map(|state| {
                _cx.subscribe_in(state, _window, |this, _, event: &SelectEvent<Vec<SharedString>>, _, cx| {
                    let SelectEvent::Confirm(_) = event;
                    this.apply_timeline_filters(cx);
                })
            })
            .collect();
        subscriptions.push(_cx.subscribe_in(&tax_controls.e05_amount, _window, |this, state, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                let text = state.read(cx).value().to_string();
                this.set_e05_amount_text(&text, cx);
            }
        }));
        log::info!("Atlas Financer window: section={} viewer={}", launch.section.slug(), viewer.person);
        AtlasApp {
            household,
            viewer,
            section: launch.section,
            horizon,
            sidebar_collapsed: false,
            overview,
            entities,
            liquidity,
            boundary,
            selected_person: None,
            selected_company: None,
            selected_account: None,
            reservation_form,
            timeline_filter,
            timeline,
            timeline_controls,
            series_form,
            projection_boundary: Boundary::Household,
            projection_case: Case::Expected,
            projection_scenario: false,
            projection,
            derivation_series: None,
            derivation: Derivation::ALL[0],
            sensitivity_boundary: Boundary::Household,
            sensitivity_scenario: false,
            assumptions,
            tax_scenario: false,
            e05_amount,
            e05_split: true,
            e05_schedule: E05Schedule::PlanExample,
            taxes,
            tax_controls,
            tax_form,
            tax_form_effective_from_override: None,
            rules_scenario: false,
            simulated_rule: None,
            rules,
            rule_form,
            scenario_selection,
            scenario_case: Case::Expected,
            scenarios,
            scenario_forms,
            decision_step: 0,
            decision_plan,
            decision: None,
            decision_form,
            privacy,
            privacy_forms,
            file,
            dirty: false,
            owner,
            lifecycle_form,
            entry_forms,
            perf: crate::perf::FrameMeter::new(),
            _subscriptions: subscriptions,
        }
    }

    /// Rebuilds every form whose option lists snapshot household data.
    pub fn rebuild_forms(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reservation_form = ReservationForm::new(&self.household, self.viewer, window, cx);
        self.timeline_controls = TimelineControls::new(&self.household, self.viewer, window, cx);
        self.tax_form = TaxRuleForm::new(&self.household, window, cx);
        self.rule_form = RuleForm::new(&self.household, window, cx);
        self.scenario_forms = ScenarioForms::new(&self.household, window, cx);
        self.decision_form = DecisionForm::new(&self.household, &self.decision_plan, self.viewer, window, cx);
        self.privacy_forms = PrivacyForms::new(&self.household, self.viewer.person, window, cx);
        self.entry_forms = crate::entry::EntryForms::new(&self.household, window, cx);
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
        self._subscriptions = subscriptions;
    }

    fn compute_taxes(household: &Household, viewer: Viewer, through: NaiveDate, scenario: bool, e05_amount: Money, e05_split: bool, e05_schedule: E05Schedule) -> Result<TaxModel, EngineError> {
        let result = perf::timed(&format!("compute taxes (viewer={} through={through} scenario={scenario})", viewer.person), || TaxModel::compute(household, viewer, through, scenario, e05_amount, e05_split, e05_schedule));
        if let Err(err) = &result {
            alerting::report(Level::Error, format!("tax model failed: {err}"));
        }
        result
    }

    fn refresh_taxes(&mut self) {
        self.taxes = Self::compute_taxes(&self.household, self.viewer, self.horizon, self.tax_scenario, self.e05_amount, self.e05_split, self.e05_schedule);
    }

    /// The derived tax model, if the engine could compute it.
    pub fn taxes(&self) -> Option<&TaxModel> {
        self.taxes.as_ref().ok()
    }

    pub fn set_tax_scenario(&mut self, on: bool, cx: &mut Context<Self>) {
        self.tax_scenario = on;
        self.refresh_taxes();
        cx.notify();
    }

    pub fn set_e05_split(&mut self, split: bool, cx: &mut Context<Self>) {
        self.e05_split = split;
        self.refresh_taxes();
        cx.notify();
    }

    pub fn set_e05_schedule(&mut self, schedule: E05Schedule, cx: &mut Context<Self>) {
        self.e05_schedule = schedule;
        self.refresh_taxes();
        cx.notify();
    }

    /// Parses the E05 amount as typed; unparsable text leaves the last valid amount.
    pub fn set_e05_amount_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Ok(amount) = Money::parse(text, self.household.base_currency)
            && amount.is_positive()
            && amount != self.e05_amount
        {
            self.e05_amount = amount;
            self.refresh_taxes();
            cx.notify();
        }
    }

    // ----- user tax rules (§12.2) ----------------------------------------------------

    pub fn open_new_tax_rule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let form = &self.tax_form;
        let (name, tax_type, category, rate, threshold, source, from, to, draft) = (
            form.name.clone(),
            form.tax_type.clone(),
            form.category.clone(),
            form.rate.clone(),
            form.threshold.clone(),
            form.source.clone(),
            form.effective_from.clone(),
            form.effective_to.clone(),
            form.draft.clone(),
        );
        let this = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let TaxRuleDraft { kind, timing } = draft.read(cx).clone();
            let draft_entity = draft.clone();
            let this = this.clone();
            dialog
                .title("New tax rule (user-authored, unverified)")
                .w_96()
                .child(
                    Form::vertical()
                        .child(Field::new().label("Name").required(true).child(Input::new(&name).id("tax-rule-name")))
                        .child(Field::new().label("Tax type").child(Input::new(&tax_type).id("tax-rule-type")))
                        .child(Field::new().label("Applies to series category").child(Select::new(&category).placeholder("Pick a category")))
                        .child(
                            Field::new().label("Kind (§14.3: full amount vs excess must be explicit)").child(
                                RadioGroup::vertical("tax-rule-kind")
                                    .children(["Flat rate on every matching transaction", "Rate on the full amount once a transaction exceeds the threshold", "Rate on the excess above the threshold"])
                                    .selected_index(Some(kind))
                                    .on_change({
                                        let draft = draft_entity.clone();
                                        move |index, _, cx| draft.update(cx, |d, cx| { d.kind = *index; cx.notify(); })
                                    }),
                            ),
                        )
                        .child(Field::new().label("Rate (%)").required(true).child(Input::new(&rate).id("tax-rule-rate")))
                        .child(Field::new().label("Threshold (per transaction)").child(Input::new(&threshold).id("tax-rule-threshold")))
                        .child(
                            Field::new().label("Timing (§12.3)").child(
                                RadioGroup::vertical("tax-rule-timing")
                                    .children(["Paid immediately", "Withheld at source, creditable", "Withheld at source, final"])
                                    .selected_index(Some(timing))
                                    .on_change({
                                        let draft = draft_entity.clone();
                                        move |index, _, cx| draft.update(cx, |d, cx| { d.timing = *index; cx.notify(); })
                                    }),
                            ),
                        )
                        .child(Field::new().label("Effective from").required(true).child(DatePicker::new(&from)))
                        .child(Field::new().label("Effective to (optional)").child(DatePicker::new(&to)))
                        .child(Field::new().label("Source").child(Input::new(&source).id("tax-rule-source"))),
                )
                .footer(
                    DialogFooter::new()
                        .child(Button::new("cancel-tax-rule").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                        .child(Button::new("save-tax-rule").primary().label("Add rule").on_click({
                            let this = this.clone();
                            move |_, window, cx| {
                                Self::confirm_tax_rule(&this, window, cx);
                            }
                        })),
                )
                .on_ok(move |_, window, cx| Self::confirm_tax_rule(&this, window, cx))
        });
    }

    fn confirm_tax_rule(this: &WeakEntity<Self>, window: &mut Window, cx: &mut App) -> bool {
        match this.update(cx, |app, cx| app.submit_tax_rule(cx)) {
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

    /// Sets the new-rule form's effective-from date (tests and scripted runs
    /// cannot drive the calendar popup).
    pub fn set_tax_form_effective_from(&mut self, date: NaiveDate, cx: &mut Context<Self>) {
        self.tax_form_effective_from_override = Some(date);
        cx.notify();
    }

    pub fn submit_tax_rule(&mut self, cx: &mut Context<Self>) -> Result<String, String> {
        let form = &self.tax_form;
        let currency = self.household.base_currency;
        let name = form.name.read(cx).value().trim().to_string();
        if name.is_empty() {
            return Err("Give the rule a name.".to_string());
        }
        let rate_text = form.rate.read(cx).value().trim().replace('%', "");
        let rate: f64 = rate_text.parse().map_err(|_| format!("Rate {rate_text:?} is not a percentage."))?;
        if !(0.0..=100.0).contains(&rate) {
            return Err("The rate must be between 0 and 100 percent.".to_string());
        }
        let rate_basis_points = (rate * 100.0).round() as u32;
        let category_index = form.category.read(cx).selected_index(cx).map(|p| p.row).ok_or("Pick the series category the rule applies to.")?;
        let category = form.categories.get(category_index).cloned().ok_or("Pick the series category the rule applies to.")?;
        let TaxRuleDraft { kind, timing } = form.draft.read(cx).clone();
        let kind = match kind {
            0 => TaxKind::FlatRate { rate_basis_points },
            on_excess => {
                let threshold_text = form.threshold.read(cx).value().trim().to_string();
                let threshold = Money::parse(&threshold_text, currency).map_err(|e| format!("Threshold: {e}"))?;
                TaxKind::FlatAboveThreshold { rate_basis_points, threshold, basis: ThresholdBasis::PerTransaction, on_excess_only: on_excess == 2 }
            }
        };
        let timing = match timing {
            1 => TaxTiming::WithheldAtSource { creditable: true },
            2 => TaxTiming::WithheldAtSource { creditable: false },
            _ => TaxTiming::Immediate,
        };
        let effective_from = form
            .effective_from
            .read(cx)
            .date()
            .start()
            .or(self.tax_form_effective_from_override)
            .ok_or("Pick the date the rule takes effect.")?;
        let effective_to = form.effective_to.read(cx).date().start();
        if let Some(end) = effective_to
            && end < effective_from
        {
            return Err("The end date must not be before the start date.".to_string());
        }
        let tax_type = form.tax_type.read(cx).value().trim().to_string();
        let source = form.source.read(cx).value().trim().to_string();
        let rule = TaxRule {
            id: self.household.next_tax_rule_id(),
            name: name.clone(),
            tax_type: if tax_type.is_empty() { "User-defined tax".into() } else { tax_type },
            categories: vec![category.clone()],
            scope: format!("Series in category “{category}”"),
            kind,
            timing,
            effective_from,
            effective_to,
            source: if source.is_empty() { "user-authored, no source attached — unverified (§12.7)".into() } else { format!("{source} — unverified until reviewed") },
            explanation: "User-authored rule; simulated alongside the DEMO pack, never labelled legally compliant (§12.7, M54).".into(),
        };
        let pack = self.household.add_user_tax_rule(rule);
        log::info!("user tax rule “{name}” added to {pack}");
        self.mark_dirty();
        self.refresh_derived();
        cx.notify();
        Ok(format!("Rule “{name}” added to {pack} (unverified); forecasts recomputed."))
    }

    fn compute_assumptions(
        household: &Household,
        viewer: Viewer,
        derivation_series: Option<SeriesId>,
        derivation: Derivation,
        boundary: Boundary,
        scenario: bool,
        horizon: NaiveDate,
    ) -> Result<AssumptionsModel, EngineError> {
        let result = perf::timed(&format!("compute assumptions (viewer={} boundary={boundary:?} scenario={scenario})", viewer.person), || AssumptionsModel::compute(household, viewer, derivation_series, derivation, boundary, scenario, horizon));
        if let Err(err) = &result {
            alerting::report(Level::Error, format!("assumptions model failed: {err}"));
        }
        result
    }

    fn refresh_assumptions(&mut self) {
        self.assumptions = Self::compute_assumptions(
            &self.household,
            self.viewer,
            self.derivation_series,
            self.derivation,
            self.sensitivity_boundary,
            self.sensitivity_scenario,
            self.horizon,
        );
    }

    /// The derived assumptions model, if the engine could compute it.
    pub fn assumptions(&self) -> Option<&AssumptionsModel> {
        self.assumptions.as_ref().ok()
    }

    pub fn select_derivation_series(&mut self, series: SeriesId, cx: &mut Context<Self>) {
        self.derivation_series = Some(series);
        self.refresh_assumptions();
        cx.notify();
    }

    pub fn select_derivation(&mut self, derivation: Derivation, cx: &mut Context<Self>) {
        self.derivation = derivation;
        self.refresh_assumptions();
        cx.notify();
    }

    pub fn select_sensitivity_boundary(&mut self, boundary: Boundary, cx: &mut Context<Self>) {
        self.sensitivity_boundary = boundary;
        self.refresh_assumptions();
        cx.notify();
    }

    pub fn set_sensitivity_scenario(&mut self, on: bool, cx: &mut Context<Self>) {
        self.sensitivity_scenario = on;
        self.refresh_assumptions();
        cx.notify();
    }

    /// §2.5 — records acceptance of an assumption today.
    pub fn accept_assumption(&mut self, id: AssumptionId, window: &mut Window, cx: &mut Context<Self>) {
        match self.household.accept_assumption(id, self.household.as_of) {
            Ok(()) => {
                log::info!("assumption {id} accepted on {}", self.household.as_of);
                self.mark_dirty();
        self.refresh_derived();
                window.push_notification(format!("Assumption #{} accepted on {}", id.raw(), self.household.as_of.format("%d %b %Y")), cx);
            }
            Err(err) => {
                alerting::report(Level::Warning, format!("accept_assumption failed: {err}"));
                window.push_notification(err.to_string(), cx);
            }
        }
        cx.notify();
    }

    /// §10.7 — applies the current derivation to an assumption; it then needs acceptance.
    pub fn apply_derivation(&mut self, assumption: AssumptionId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(series) = self.derivation_series.or_else(|| self.assumptions.as_ref().ok().map(|m| m.derivation_series)) else { return };
        match derive(&self.household, series, self.derivation).and_then(|d| apply_derived(&mut self.household, &d, assumption).map(|_| d)) {
            Ok(derived) => {
                log::info!("derivation applied to {assumption}: {}", derived.statement);
                self.mark_dirty();
        self.refresh_derived();
                window.push_notification(format!("Assumption #{} now reads: {} — accept it to use it.", assumption.raw(), derived.amount.describe()), cx);
            }
            Err(err) => {
                alerting::report(Level::Warning, format!("apply_derivation failed: {err}"));
                window.push_notification(err.to_string(), cx);
            }
        }
        cx.notify();
    }

    fn compute_projection(household: &Household, viewer: Viewer, boundary: Boundary, case: Case, scenario: Option<atlas_core::ids::ScenarioId>, through: NaiveDate) -> Result<ProjectionModel, EngineError> {
        let result = perf::timed(&format!("compute projection (viewer={} boundary={boundary:?} case={case:?} scenario={scenario:?} through={through})", viewer.person), || ProjectionModel::compute(household, viewer, boundary, case, scenario, through));
        if let Err(err) = &result {
            alerting::report(Level::Error, format!("projection failed for {boundary:?} {case:?}: {err}"));
        }
        result
    }

    fn refresh_projection(&mut self) {
        let scenario = if self.projection_scenario { Some(fixtures::ids::BUY_CAR) } else { None };
        self.projection = Self::compute_projection(&self.household, self.viewer, self.projection_boundary, self.projection_case, scenario, self.horizon);
    }

    /// The derived projection, if the engine could compute it.
    pub fn projection(&self) -> Option<&ProjectionModel> {
        self.projection.as_ref().ok()
    }

    pub fn select_projection_boundary(&mut self, boundary: Boundary, cx: &mut Context<Self>) {
        self.projection_boundary = boundary;
        self.refresh_projection();
        cx.notify();
    }

    pub fn select_projection_case(&mut self, case: Case, cx: &mut Context<Self>) {
        log::info!("projection case: {case:?}");
        self.projection_case = case;
        self.refresh_projection();
        cx.notify();
    }

    pub fn set_projection_scenario(&mut self, on: bool, cx: &mut Context<Self>) {
        self.projection_scenario = on;
        self.refresh_projection();
        cx.notify();
    }

    fn compute_timeline(household: &Household, viewer: Viewer, filter: TimelineFilter) -> Result<TimelineModel, EngineError> {
        let result = perf::timed(&format!("compute timeline (viewer={} through={})", viewer.person, filter.through), || TimelineModel::compute(household, viewer, filter));
        if let Err(err) = &result {
            alerting::report(Level::Error, format!("timeline model failed: {err}"));
        }
        result
    }

    /// The derived timeline, if the engine could compute it.
    pub fn timeline(&self) -> Option<&TimelineModel> {
        self.timeline.as_ref().ok()
    }

    pub fn timeline_filter(&self) -> &TimelineFilter {
        &self.timeline_filter
    }

    /// Rebuilds the timeline filter from the Select states (§9 filters).
    pub fn apply_timeline_filters(&mut self, cx: &mut Context<Self>) {
        let row = |state: &Entity<SelectState<Vec<SharedString>>>, cx: &App| state.read(cx).selected_index(cx).map(|p| p.row).unwrap_or(0);
        let controls = &self.timeline_controls;
        let entity_row = row(&controls.entity, cx);
        let account_row = row(&controls.account, cx);
        let certainty_row = row(&controls.certainty, cx);
        let status_row = row(&controls.status, cx);
        let horizon_row = row(&controls.horizon, cx);
        self.timeline_filter.entity = if entity_row == 0 { None } else { controls.entities.get(entity_row - 1).copied() };
        self.timeline_filter.account = if account_row == 0 { None } else { controls.accounts.get(account_row - 1).copied() };
        self.timeline_filter.certainty = if certainty_row == 0 { None } else { Certainty::ALL.get(certainty_row - 1).copied() };
        self.timeline_filter.status = if status_row == 0 { None } else { STATUSES.get(status_row - 1).copied() };
        self.timeline_filter.through = match HORIZONS.get(horizon_row).and_then(|(_, months)| *months) {
            Some(months) => self.household.as_of.checked_add_months(Months::new(months)).unwrap_or(self.horizon),
            None => self.horizon,
        };
        log::info!("timeline filters: {:?}", self.timeline_filter);
        self.timeline = Self::compute_timeline(&self.household, self.viewer, self.timeline_filter.clone());
        cx.notify();
    }

    /// Toggles the “Buy car” scenario overlay on the timeline (§18).
    pub fn set_timeline_scenario(&mut self, on: bool, cx: &mut Context<Self>) {
        self.timeline_filter.scenario = if on { Some(fixtures::ids::BUY_CAR) } else { None };
        self.timeline = Self::compute_timeline(&self.household, self.viewer, self.timeline_filter.clone());
        cx.notify();
    }

    // ----- series editor (§9.2–9.4) --------------------------------------------------

    pub fn open_series_editor(&mut self, id: SeriesId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(series) = self.household.series_by_id(id).cloned() else { return };
        self.series_form.editing = Some(id);
        let expected = series.amount.expected().format();
        self.series_form.amount.update(cx, |state, cx| state.set_value(expected, window, cx));
        self.series_form.change_amount.update(cx, |state, cx| state.set_value("", window, cx));
        for picker in [&self.series_form.change_from, &self.series_form.skip_on, &self.series_form.end_after] {
            picker.update(cx, |state, cx| state.set_date(gpui_kit::base::Date::Single(None), window, cx));
        }
        let amount = self.series_form.amount.clone();
        let change_from = self.series_form.change_from.clone();
        let change_amount = self.series_form.change_amount.clone();
        let skip_on = self.series_form.skip_on.clone();
        let end_after = self.series_form.end_after.clone();
        let this = cx.entity().downgrade();
        let title = format!("Edit series “{}”", series.name);
        let summary = format!("{} · {} · {}", series.direction.label(), series.recurrence.describe(), series.certainty.label());
        window.open_dialog(cx, move |dialog, _, _| {
            let this = this.clone();
            dialog
                .title(title.clone())
                .w_96()
                .child(
                    v_flex()
                        .gap_3()
                        .child(div().text_xs().child(summary.clone()))
                        .child(
                            Form::vertical()
                                .child(Field::new().label("Expected amount — whole series (§9.2 “edit entire series”)").child(Input::new(&amount).id("series-amount")))
                                .child(Field::new().label("New amount from this date on (§9.3 effective-dated change)").child(DatePicker::new(&change_from)))
                                .child(Field::new().label("New amount").child(Input::new(&change_amount).id("series-change-amount")))
                                .child(Field::new().label("Skip the occurrence due on (§9.2 “edit only this occurrence”)").child(DatePicker::new(&skip_on)))
                                .child(Field::new().label("End the series after (§9.4 last salary date)").child(DatePicker::new(&end_after))),
                        ),
                )
                .footer(
                    DialogFooter::new()
                        .child(Button::new("cancel-series").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                        .child(Button::new("save-series").primary().label("Apply changes").on_click({
                            let this = this.clone();
                            move |_, window, cx| {
                                Self::confirm_series_edit(&this, window, cx);
                            }
                        })),
                )
                .on_ok(move |_, window, cx| Self::confirm_series_edit(&this, window, cx))
        });
    }

    fn confirm_series_edit(this: &WeakEntity<Self>, window: &mut Window, cx: &mut App) -> bool {
        match this.update(cx, |app, cx| app.submit_series_edit(cx)) {
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

    /// Applies whichever parts of the series form were filled in.
    pub fn submit_series_edit(&mut self, cx: &mut Context<Self>) -> Result<String, String> {
        let id = self.series_form.editing.ok_or("No series is being edited.")?;
        let currency = self.household.series_by_id(id).map(|s| s.amount.currency()).ok_or("Unknown series.")?;
        let amount_text = self.series_form.amount.read(cx).value().trim().to_string();
        let change_from = self.series_form.change_from.read(cx).date().start();
        let change_amount_text = self.series_form.change_amount.read(cx).value().trim().to_string();
        let skip_on = self.series_form.skip_on.read(cx).date().start();
        let end_after = self.series_form.end_after.read(cx).date().start();

        let new_amount = if amount_text.is_empty() { None } else { Some(Money::parse(&amount_text, currency).map_err(|e| format!("Expected amount: {e}"))?) };
        let change = match (change_from, change_amount_text.is_empty()) {
            (Some(from), false) => Some((from, Money::parse(&change_amount_text, currency).map_err(|e| format!("New amount: {e}"))?)),
            (Some(_), true) => return Err("Enter the new amount that applies from the chosen date.".to_string()),
            (None, false) => return Err("Pick the date the new amount applies from.".to_string()),
            (None, true) => None,
        };

        let series = self.household.series.iter_mut().find(|s| s.id == id).ok_or("Unknown series.")?;
        let mut applied = Vec::new();
        if let Some(amount) = new_amount
            && amount != series.amount.expected()
        {
            series.amount = match series.amount {
                AmountSpec::Exact(_) => AmountSpec::Exact(amount),
                AmountSpec::Range { low, high, .. } => AmountSpec::Range { low: low.min(amount).unwrap_or(low), expected: amount, high: high.max(amount).unwrap_or(high) },
            };
            applied.push(format!("expected amount now {}", amount.format()));
        }
        if let Some((from, amount)) = change {
            series.change_amount_from(from, AmountSpec::Exact(amount));
            applied.push(format!("{} from {}", amount.format(), from.format("%d %b %Y")));
        }
        if let Some(day) = skip_on {
            series.set_exception(Exception { original_due: day, kind: ExceptionKind::Skip });
            applied.push(format!("skip {}", day.format("%d %b %Y")));
        }
        if let Some(last) = end_after {
            series.end_after(last);
            applied.push(format!("ends after {}", last.format("%d %b %Y")));
        }
        if applied.is_empty() {
            return Err("Nothing to apply — change an amount, add an exception or set an end date.".to_string());
        }
        let name = series.name.clone();
        log::info!("series {id} edited: {}", applied.join("; "));
        self.mark_dirty();
        self.refresh_derived();
        cx.notify();
        Ok(format!("“{name}”: {}", applied.join("; ")))
    }

    fn compute_liquidity(household: &Household, viewer: Viewer, boundary: Boundary, horizon: NaiveDate) -> Result<LiquidityModel, EngineError> {
        let result = perf::timed(&format!("compute liquidity (viewer={} boundary={boundary:?})", viewer.person), || LiquidityModel::compute(household, viewer, boundary, horizon));
        if let Err(err) = &result {
            alerting::report(Level::Error, format!("liquidity model failed for {boundary:?}: {err}"));
        }
        result
    }

    fn compute_entities(household: &Household, viewer: Viewer) -> Result<EntityModels, EngineError> {
        let result = perf::timed(&format!("compute entities (viewer={})", viewer.person), || EntityModels::compute(household, viewer));
        if let Err(err) = &result {
            alerting::report(Level::Error, format!("entity models failed for {}: {err}", viewer.person));
        }
        result
    }

    fn compute_overview(household: &Household, viewer: Viewer, horizon: NaiveDate) -> Result<HouseholdOverview, EngineError> {
        let result = perf::timed(&format!("compute household overview (viewer={} horizon={horizon})", viewer.person), || HouseholdOverview::compute(household, viewer, horizon));
        if let Err(err) = &result {
            alerting::report(Level::Error, format!("household overview failed for {}: {err}", viewer.person));
        }
        result
    }

    /// Recomputes every derived model after the household or viewer changed.
    pub(crate) fn refresh_derived(&mut self) {
        let started = std::time::Instant::now();
        self.refresh_derived_models();
        log::info!("perf: refresh_derived (every screen model) took {:.1}ms", perf::ms(started.elapsed()));
    }

    fn refresh_derived_models(&mut self) {
        self.overview = Self::compute_overview(&self.household, self.viewer, self.horizon);
        self.entities = Self::compute_entities(&self.household, self.viewer);
        self.liquidity = Self::compute_liquidity(&self.household, self.viewer, self.boundary, self.horizon);
        self.timeline = Self::compute_timeline(&self.household, self.viewer, self.timeline_filter.clone());
        self.refresh_projection();
        self.refresh_assumptions();
        self.refresh_taxes();
        self.refresh_rules();
        self.refresh_scenarios();
        if self.decision.is_some() {
            self.evaluate_decision();
        }
        self.privacy = perf::timed(&format!("compute privacy (viewer={})", self.viewer.person), || PrivacyModel::compute(&self.household, self.viewer));
    }

    /// The privacy model for the current viewer.
    pub fn privacy(&self) -> &PrivacyModel {
        &self.privacy
    }

    /// Frame timings (status-bar counter, perf log).
    pub fn perf(&self) -> &perf::FrameMeter {
        &self.perf
    }

    /// The evaluated decision, if the result step has been reached.
    pub fn decision(&self) -> Option<&Decision> {
        self.decision.as_ref().and_then(|d| d.as_ref().ok())
    }

    pub fn decision_plan(&self) -> &PurchasePlan {
        &self.decision_plan
    }

    pub fn decision_step(&self) -> usize {
        self.decision_step
    }

    // ----- scenarios (§18, M8) -----------------------------------------------------

    fn compute_scenarios(household: &Household, viewer: Viewer, through: NaiveDate, case: Case, selection: &[ScenarioId]) -> Result<ScenariosModel, EngineError> {
        let result = perf::timed(&format!("compute scenarios (viewer={} case={case:?} selected={})", viewer.person, selection.len()), || ScenariosModel::compute(household, viewer, through, case, selection));
        if let Err(err) = &result {
            alerting::report(Level::Error, format!("scenarios model failed: {err}"));
        }
        result
    }

    fn refresh_scenarios(&mut self) {
        self.scenarios = Self::compute_scenarios(&self.household, self.viewer, self.horizon, self.scenario_case, &self.scenario_selection);
    }

    /// The derived scenarios model, if the engine could compute it.
    pub fn scenarios(&self) -> Option<&ScenariosModel> {
        self.scenarios.as_ref().ok()
    }

    pub fn select_scenario(&mut self, id: ScenarioId, selected: bool, cx: &mut Context<Self>) {
        self.scenario_selection.retain(|s| *s != id);
        if selected {
            self.scenario_selection.push(id);
        }
        log::info!("scenario selection: {:?}", self.scenario_selection);
        self.refresh_scenarios();
        cx.notify();
    }

    pub fn set_scenario_case(&mut self, case: Case, cx: &mut Context<Self>) {
        if self.scenario_case != case {
            self.scenario_case = case;
            self.refresh_scenarios();
            cx.notify();
        }
    }

    // ----- rules (§14, M7) ---------------------------------------------------------

    fn compute_rules(household: &Household, viewer: Viewer, through: NaiveDate, scenario: bool, simulated: Option<RuleId>) -> Result<RulesModel, EngineError> {
        let result = perf::timed(&format!("compute rules (viewer={} scenario={scenario} simulated={simulated:?})", viewer.person), || RulesModel::compute(household, viewer, through, scenario, simulated));
        if let Err(err) = &result {
            alerting::report(Level::Error, format!("rules model failed: {err}"));
        }
        result
    }

    fn refresh_rules(&mut self) {
        self.rules = Self::compute_rules(&self.household, self.viewer, self.horizon, self.rules_scenario, self.simulated_rule);
    }

    /// The derived rules model, if the engine could compute it.
    pub fn rules(&self) -> Option<&RulesModel> {
        self.rules.as_ref().ok()
    }

    pub fn set_rules_scenario(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.rules_scenario != on {
            self.rules_scenario = on;
            self.refresh_rules();
            cx.notify();
        }
    }

    pub fn set_rules_tie_break(&mut self, tie_break: TieBreak, cx: &mut Context<Self>) {
        if self.household.rule_tie_break != tie_break {
            log::info!("rule tie-break policy: {}", tie_break.slug());
            self.household.rule_tie_break = tie_break;
            self.mark_dirty();
            self.refresh_derived();
            cx.notify();
        }
    }

    pub fn simulate_rule(&mut self, id: RuleId, cx: &mut Context<Self>) {
        self.simulated_rule = if self.simulated_rule == Some(id) { None } else { Some(id) };
        self.refresh_rules();
        cx.notify();
    }

    pub fn toggle_rule(&mut self, id: RuleId, window: &mut Window, cx: &mut Context<Self>) {
        let enabled = self.household.rule(id).map(|r| r.enabled).unwrap_or(false);
        match self.household.set_rule_enabled(id, !enabled) {
            Ok(()) => {
                log::info!("rule {id} {}", if enabled { "disabled" } else { "enabled" });
                self.mark_dirty();
                self.refresh_derived();
                window.push_notification(format!("Rule {id} {} (new version recorded); forecasts recomputed.", if enabled { "disabled" } else { "enabled" }), cx);
            }
            Err(err) => {
                alerting::report(Level::Warning, format!("toggling rule {id} failed: {err}"));
                window.push_notification(format!("Not changed — {err}"), cx);
            }
        }
        cx.notify();
    }

    pub fn bump_rule_priority(&mut self, id: RuleId, delta: i32, window: &mut Window, cx: &mut Context<Self>) {
        let Some(priority) = self.household.rule(id).map(|r| r.priority) else { return };
        match self.household.set_rule_priority(id, priority + delta) {
            Ok(()) => {
                log::info!("rule {id} priority {priority} → {}", priority + delta);
                self.mark_dirty();
                self.refresh_derived();
                window.push_notification(format!("Rule {id} priority {priority} → {} (new version recorded).", priority + delta), cx);
            }
            Err(err) => {
                alerting::report(Level::Warning, format!("re-prioritising rule {id} failed: {err}"));
                window.push_notification(format!("Not changed — {err}"), cx);
            }
        }
        cx.notify();
    }

    pub fn delete_rule(&mut self, id: RuleId, window: &mut Window, cx: &mut Context<Self>) {
        match self.household.remove_rule(id) {
            Ok(()) => {
                log::info!("rule {id} deleted");
                if self.simulated_rule == Some(id) {
                    self.simulated_rule = None;
                }
                self.mark_dirty();
                self.refresh_derived();
                window.push_notification(format!("Rule {id} deleted; forecasts recomputed."), cx);
            }
            Err(err) => {
                alerting::report(Level::Warning, format!("deleting rule {id} failed: {err}"));
                window.push_notification(format!("Not deleted — {err}"), cx);
            }
        }
        cx.notify();
    }

    /// The derived liquidity model, if the engine could compute it.
    pub fn liquidity(&self) -> Option<&LiquidityModel> {
        self.liquidity.as_ref().ok()
    }

    pub fn select_boundary(&mut self, boundary: Boundary, cx: &mut Context<Self>) {
        log::info!("liquidity boundary: {boundary:?}");
        self.boundary = boundary;
        self.liquidity = Self::compute_liquidity(&self.household, self.viewer, boundary, self.horizon);
        cx.notify();
    }

    // ----- reservations (§17, E01) ------------------------------------------------

    /// Opens the "New reservation" dialog. The builder only reads the form
    /// entities, never `self` (it runs while this view is rendering).
    pub fn open_new_reservation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let form_name = self.reservation_form.name.clone();
        let form_amount = self.reservation_form.amount.clone();
        let form_purpose = self.reservation_form.purpose.clone();
        let form_account = self.reservation_form.account.clone();
        let form_nested = self.reservation_form.nested_in.clone();
        let draft = self.reservation_form.draft.clone();
        let this = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let draft_entity = draft.clone();
            let ReservationDraft { coverage, hardness } = draft.read(cx).clone();
            let this = this.clone();
            dialog
                .title("New reservation")
                .w_96()
                .child(
                    Form::vertical()
                        .child(Field::new().label("Name").required(true).child(Input::new(&form_name).id("reservation-name")))
                        .child(Field::new().label("Account").child(Select::new(&form_account).placeholder("Pick an account")))
                        .child(Field::new().label("Amount").required(true).child(Input::new(&form_amount).id("reservation-amount")))
                        .child(
                            Field::new().label("Coverage (§17)").child(
                                RadioGroup::vertical("reservation-coverage")
                                    .children(["Disjoint — adds to every other earmark", "Includes the account's bank minimum", "Nested inside another earmark"])
                                    .selected_index(Some(coverage))
                                    .on_change({
                                        let draft = draft_entity.clone();
                                        move |index, _, cx| {
                                            draft.update(cx, |draft, cx| {
                                                draft.coverage = *index;
                                                cx.notify();
                                            });
                                        }
                                    }),
                            ),
                        )
                        .child(Field::new().label("Nested in").child(Select::new(&form_nested).placeholder("Only for a nested earmark")))
                        .child(
                            Field::new().label("Hardness").child(
                                RadioGroup::horizontal("reservation-hardness")
                                    .children(["Hard constraint", "User-relaxable preference"])
                                    .selected_index(Some(hardness))
                                    .on_change({
                                        let draft = draft_entity.clone();
                                        move |index, _, cx| {
                                            draft.update(cx, |draft, cx| {
                                                draft.hardness = *index;
                                                cx.notify();
                                            });
                                        }
                                    }),
                            ),
                        )
                        .child(Field::new().label("Purpose").child(Input::new(&form_purpose).id("reservation-purpose"))),
                )
                .footer(
                    DialogFooter::new()
                        .child(Button::new("cancel-reservation").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                        .child(Button::new("save-reservation").primary().label("Add reservation").on_click({
                            let this = this.clone();
                            move |_, window, cx| {
                                Self::confirm_new_reservation(&this, window, cx);
                            }
                        })),
                )
                // Enter inside the form confirms too.
                .on_ok(move |_, window, cx| Self::confirm_new_reservation(&this, window, cx))
        });
    }

    /// Runs the submission from a button or the Enter key; closes the dialog
    /// only on success and returns whether it closed.
    fn confirm_new_reservation(this: &WeakEntity<Self>, window: &mut Window, cx: &mut App) -> bool {
        match this.update(cx, |app, cx| app.submit_new_reservation(cx)) {
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

    /// Reads the form, validates it and adds the earmark. `Err` is the message
    /// to show the user; the dialog stays open.
    pub fn submit_new_reservation(&mut self, cx: &mut Context<Self>) -> Result<String, String> {
        let form = &self.reservation_form;
        let name = form.name.read(cx).value().trim().to_string();
        if name.is_empty() {
            return Err("Give the reservation a name.".to_string());
        }
        let account_index = form.account.read(cx).selected_index(cx).map(|p| p.row).ok_or("Pick an account.")?;
        let account_id = *form.account_ids.get(account_index).ok_or("Pick an account.")?;
        let currency = self.household.account(account_id).map(|a| a.currency).unwrap_or(self.household.base_currency);
        let amount = Money::parse(&form.amount.read(cx).value(), currency).map_err(|e| e.to_string())?;
        if !amount.is_positive() {
            return Err("The amount must be positive.".to_string());
        }
        let ReservationDraft { coverage, hardness } = form.draft.read(cx).clone();
        let coverage = match coverage {
            1 => Coverage::CoversAccountMinimum,
            2 => {
                let nested_index = form.nested_in.read(cx).selected_index(cx).map(|p| p.row).ok_or("Pick the earmark this one nests inside.")?;
                let outer = *form.nested_ids.get(nested_index).ok_or("Pick the earmark this one nests inside.")?;
                if self.household.reservation(outer).map(|r| r.account) != Some(account_id) {
                    return Err("A nested earmark must sit on the same account as the earmark it nests inside.".to_string());
                }
                Coverage::NestedIn(outer)
            }
            _ => Coverage::Disjoint,
        };
        let hardness = if hardness == 1 { Hardness::SoftUserRelaxable } else { Hardness::Hard };
        let purpose = form.purpose.read(cx).value().trim().to_string();
        let reservation = Reservation {
            id: self.household.next_reservation_id(),
            name: name.clone(),
            account: account_id,
            amount,
            coverage,
            hardness,
            purpose,
            released_on: None,
        };
        match self.household.add_reservation(reservation) {
            Ok(id) => {
                log::info!("reservation {id} added: {name} {} on {account_id}", amount.format());
                self.mark_dirty();
        self.refresh_derived();
                cx.notify();
                Ok(format!("Reserved {} for “{name}” — ledger cash unchanged, free cash reduced (§17)", amount.format()))
            }
            Err(err) => {
                alerting::report(Level::Warning, format!("add_reservation rejected: {err}"));
                Err(err.to_string())
            }
        }
    }

    /// Asks before recording the payment that releases an earmark (E01).
    pub fn open_release_reservation(&mut self, id: ReservationId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(reservation) = self.household.reservation(id).cloned() else { return };
        let account_name = self.household.account(reservation.account).map(|a| a.name.clone()).unwrap_or_default();
        let this = cx.entity().downgrade();
        let title = format!("Pay and release “{}”?", reservation.name);
        let body = format!(
            "Records the {} payment from {} and releases the earmark. Ledger cash falls by the same amount; free cash stays where it is because the same obligation is never deducted twice (E01).",
            reservation.amount.format(),
            account_name
        );
        window.open_dialog(cx, move |dialog, _, _| {
            let this = this.clone();
            dialog
                .title(title.clone())
                .w_96()
                .child(div().text_sm().child(body.clone()))
                .footer(
                    DialogFooter::new()
                        .child(Button::new("cancel-release").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                        .child(Button::new("confirm-release").primary().label("Pay and release").on_click({
                            let this = this.clone();
                            move |_, window, cx| {
                                Self::confirm_release(&this, id, window, cx);
                            }
                        })),
                )
                .on_ok(move |_, window, cx| Self::confirm_release(&this, id, window, cx))
        });
    }

    fn confirm_release(this: &WeakEntity<Self>, id: ReservationId, window: &mut Window, cx: &mut App) -> bool {
        match this.update(cx, |app, cx| app.pay_and_release(id, cx)) {
            Ok(Ok(summary)) => window.push_notification(summary, cx),
            Ok(Err(message)) => window.push_notification(message, cx),
            Err(_) => {}
        }
        window.close_dialog(cx);
        true
    }

    /// E01 in one step: pay the obligation, release the earmark, recompute.
    pub fn pay_and_release(&mut self, id: ReservationId, cx: &mut Context<Self>) -> Result<String, String> {
        let name = self.household.reservation(id).map(|r| r.name.clone()).unwrap_or_else(|| id.to_string());
        match self.household.pay_and_release(id, self.household.as_of) {
            Ok(balance) => {
                log::info!("reservation {id} paid and released; new settled balance {}", balance.format());
                self.mark_dirty();
        self.refresh_derived();
                cx.notify();
                Ok(format!("“{name}” paid and released; settled balance now {}", balance.format()))
            }
            Err(err) => {
                alerting::report(Level::Error, format!("pay_and_release failed for {id}: {err}"));
                Err(err.to_string())
            }
        }
    }

    pub fn select_person(&mut self, id: PersonId, cx: &mut Context<Self>) {
        self.selected_person = Some(id);
        cx.notify();
    }

    pub fn select_company(&mut self, id: CompanyId, cx: &mut Context<Self>) {
        self.selected_company = Some(id);
        cx.notify();
    }

    pub fn select_account(&mut self, id: AccountId, cx: &mut Context<Self>) {
        log::info!("account selected: {id}");
        self.selected_account = Some(id);
        cx.notify();
    }

    pub fn selected_account(&self) -> Option<AccountId> {
        self.selected_account
    }

    /// Switches the main area to `section`.
    pub fn navigate(&mut self, section: Section, cx: &mut Context<Self>) {
        if self.section != section {
            log::info!("navigate: {} → {}", self.section.slug(), section.slug());
            self.section = section;
            cx.notify();
        }
    }

    pub fn section(&self) -> Section {
        self.section
    }

    pub fn household(&self) -> &Household {
        &self.household
    }

    pub fn viewer(&self) -> Viewer {
        self.viewer
    }

    /// The derived overview, if the engine could compute it.
    pub fn overview(&self) -> Option<&HouseholdOverview> {
        self.overview.as_ref().ok()
    }

    /// The derived entity models, if the engine could compute them.
    pub fn entities(&self) -> Option<&EntityModels> {
        self.entities.as_ref().ok()
    }

    /// Changes who is looking; every screen re-projects (M10 adds the UI).
    pub fn set_viewer(&mut self, viewer: Viewer, cx: &mut Context<Self>) {
        if self.viewer != viewer {
            let from = self.viewer_name();
            self.viewer = viewer;
            let to = self.viewer_name();
            // §5.18: a viewer switch is authorization-sensitive and goes on the audit log.
            self.household.record_audit(viewer.person, None, atlas_core::authz::AuditKind::ViewerSwitched, format!("viewer switched from {from} to {to}; every screen re-filtered through their policies"), None, chrono::Local::now().naive_local());
            self.mark_dirty();
        }
        // The decision builder must only offer sources this viewer may see.
        self.decision_plan = atlas_core::decision::default_plan_for(&self.household, self.household.as_of, self.viewer);
        self.decision = None;
        self.decision_step = 0;
        self.refresh_derived();
        cx.notify();
    }

    fn viewer_name(&self) -> String {
        match self.household.person(self.viewer.person) {
            Some(person) => person.name.clone(),
            None => "no one yet".to_string(),
        }
    }

    fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next = if cx.theme().is_dark() { ThemeMode::Light } else { ThemeMode::Dark };
        Theme::change(next, Some(window), cx);
        cx.notify();
    }

    // ----- shell regions --------------------------------------------------------

    fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = cx.theme().is_dark();
        TitleBar::new()
            .child(
                h_flex()
                    .items_center()
                    .gap_3()
                    .child(Icon::new(IconName::Wallet).small())
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Atlas Financer"))
                    .child(Tag::secondary().xsmall().outline().child("M12 real data")),
            )
            .child(
                h_flex()
                    .items_center()
                    .justify_end()
                    .px_2()
                    .gap_3()
                    .child(div().text_xs().text_color(cx.theme().muted_foreground).child(format!(
                        "reconciled {}",
                        self.household.as_of.format("%d %b %Y")
                    )))
                    .child(self.render_household_menu(cx))
                    .child(
                        Button::new("viewer")
                            .small()
                            .ghost()
                            .compact()
                            .icon(IconName::Eye)
                            .label(format!("Viewing as {}", self.viewer_name()))
                            .tooltip("Change who is looking")
                            .on_click(cx.listener(|this, _, window, cx| this.open_viewer_picker(window, cx))),
                    )
                    .child(
                        Button::new("theme")
                            .small()
                            .ghost()
                            .compact()
                            .icon(if is_dark { IconName::Sun } else { IconName::Moon })
                            .tooltip(if is_dark { "Switch to light theme" } else { "Switch to dark theme" })
                            .on_click(cx.listener(|this, _, window, cx| this.toggle_theme(window, cx))),
                    ),
            )
    }

    /// The sidebar's layout width: `w_64` expanded, gpui-kit's icon width collapsed.
    fn sidebar_width(&self) -> Pixels {
        if self.sidebar_collapsed { px(48.) } else { px(256.) }
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let collapsed = self.sidebar_collapsed;
        let theme = cx.theme();
        let mut sidebar = Sidebar::new("main-sidebar").collapsed(collapsed).w_64().header(
            SidebarHeader::new()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size_8()
                        .flex_shrink_0()
                        .rounded(theme.radius)
                        .bg(theme.sidebar_primary)
                        .text_color(theme.sidebar_primary_foreground)
                        .child(Icon::new(IconName::Wallet)),
                )
                .when(!collapsed, |this| {
                    this.child(
                        v_flex()
                            .flex_1()
                            .overflow_hidden()
                            .text_sm()
                            .child(self.household.name.clone())
                            .child(div().text_xs().text_color(theme.muted_foreground).child(format!(
                                "{} · {}",
                                self.household.base_currency,
                                self.viewer_name()
                            ))),
                    )
                }),
        );
        for (group, sections) in Section::GROUPS {
            sidebar = sidebar.child(SidebarGroup::new(group).child(SidebarMenu::new().children(sections.iter().map(|section| {
                let section = *section;
                let item = SidebarMenuItem::new(section.label())
                    .icon(section.icon())
                    .active(section == self.section)
                    .on_click(cx.listener(move |this, _, _, cx| this.navigate(section, cx)));
                match section.pending_milestone() {
                    Some(m) => item.suffix(move |_, cx| {
                        div().text_xs().text_color(cx.theme().muted_foreground).child(format!("M{}", m.number)).into_any_element()
                    }),
                    None => item,
                }
            }))));
        }
        sidebar.footer(
            SidebarFooter::new().child(
                v_flex()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Deterministic · no AI")
                    .when(!collapsed, |this| this.child("Every figure opens its chain")),
            ),
        )
    }

    fn render_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let started = std::time::Instant::now();
        let element = self.render_section(cx);
        let clone = self.perf.content_clone_so_far();
        self.perf.record_content(clone, started.elapsed().saturating_sub(clone));
        element
    }

    /// Clones the screen model (measured — the clone is what the perf log
    /// calls `content clone`) so the screen function can borrow `cx` mutably.
    fn cloned<M: Clone>(&self, model: &M) -> M {
        let started = std::time::Instant::now();
        let cloned = model.clone();
        self.perf.add_content_clone(started.elapsed());
        cloned
    }

    fn render_section(&self, cx: &mut Context<Self>) -> AnyElement {
        match self.section {
            Section::Household => match &self.overview {
                Ok(overview) => screens::household::render(overview, &self.household, self.viewer, cx).into_any_element(),
                Err(err) => v_flex()
                    .id("screen-household")
                    .test_support()
                    .w_full()
                    .gap_4()
                    .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child("Household"))
                    .child(
                        Alert::error("overview-error", format!("The household overview could not be calculated: {err}"))
                            .title("Calculation failed"),
                    )
                    .child(div().text_sm().text_color(cx.theme().muted_foreground).child(
                        "The failure was logged and, when alerts are configured, posted to the team. Fix the fixture or the policy and reopen the screen.",
                    ))
                    .into_any_element(),
            },
            Section::People | Section::Companies | Section::Accounts => match &self.entities {
                Ok(models) => {
                    let models = self.cloned(models);
                    match self.section {
                        Section::People => screens::people::render(&models, &self.household, self.viewer, self.selected_person, cx).into_any_element(),
                        Section::Companies => screens::companies::render(&models, &self.household, self.viewer, self.selected_company, cx).into_any_element(),
                        _ => screens::accounts::render(&models, &self.household, self.viewer, self.selected_account, cx).into_any_element(),
                    }
                }
                Err(err) => self.render_engine_failure(self.section, err, cx),
            },
            Section::Liquidity => match &self.liquidity {
                Ok(model) => {
                    let model = self.cloned(model);
                    screens::liquidity::render(&model, &self.household, self.viewer, cx).into_any_element()
                }
                Err(err) => self.render_engine_failure(Section::Liquidity, err, cx),
            },
            Section::Timeline => match &self.timeline {
                Ok(model) => {
                    let model = self.cloned(model);
                    screens::timeline::render(&model, &self.timeline_controls, &self.household, self.viewer, cx).into_any_element()
                }
                Err(err) => self.render_engine_failure(Section::Timeline, err, cx),
            },
            Section::Projections => match &self.projection {
                Ok(model) => {
                    let model = self.cloned(model);
                    screens::projections::render(&model, &self.household, self.viewer, cx).into_any_element()
                }
                Err(err) => self.render_engine_failure(Section::Projections, err, cx),
            },
            Section::Assumptions => match &self.assumptions {
                Ok(model) => {
                    let model = self.cloned(model);
                    screens::assumptions::render(&model, &self.household, self.viewer, cx).into_any_element()
                }
                Err(err) => self.render_engine_failure(Section::Assumptions, err, cx),
            },
            Section::Taxes => match &self.taxes {
                Ok(model) => {
                    let model = self.cloned(model);
                    screens::taxes::render(&model, &self.tax_controls, &self.household, self.viewer, cx).into_any_element()
                }
                Err(err) => self.render_engine_failure(Section::Taxes, err, cx),
            },
            Section::Rules => match &self.rules {
                Ok(model) => {
                    let model = self.cloned(model);
                    screens::rules::render(&model, &self.household, cx).into_any_element()
                }
                Err(err) => self.render_engine_failure(Section::Rules, err, cx),
            },
            Section::Scenarios => match &self.scenarios {
                Ok(model) => {
                    let model = self.cloned(model);
                    screens::scenarios::render(&model, &self.household, cx).into_any_element()
                }
                Err(err) => self.render_engine_failure(Section::Scenarios, err, cx),
            },
            Section::Decisions => screens::decisions::render(self.decision_step, &self.decision_form, self.decision.as_ref(), &self.household, &self.viewer_name(), cx).into_any_element(),
            Section::Privacy => {
                let model = self.cloned(&self.privacy);
                screens::privacy::render(&model, &self.household, &self.viewer_name(), cx).into_any_element()
            }
            Section::Settings => screens::settings::render(&self.household, &self.viewer_name(), cx).into_any_element(),
        }
    }

    fn render_engine_failure(&self, section: Section, err: &EngineError, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .id(SharedString::from(format!("screen-{}", section.slug())))
            .test_support()
            .w_full()
            .gap_4()
            .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(section.label()))
            .child(Alert::error("engine-error", format!("This screen could not be calculated: {err}")).title("Calculation failed"))
            .child(div().text_sm().text_color(cx.theme().muted_foreground).child(
                "The failure was logged and, when alerts are configured, posted to the team.",
            ))
            .into_any_element()
    }

    fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        StatusBar::new()
            .left(h_flex().items_center().gap_1().child(Icon::new(IconName::Check).xsmall()).child(self.household.name.clone()))
            .left(Separator::vertical().h_3())
            .left(div().text_color(muted).child(format!(
                "{} accounts · {} series · {} reservations · {} policies",
                self.household.accounts.len(),
                self.household.series.len(),
                self.household.reservations.len(),
                self.household.policies.len()
            )))
            .right(div().text_color(muted).child(match (&self.file, self.dirty) {
                (Some(file), true) => format!("{} • unsaved", file.path().display()),
                (Some(file), false) => format!("{}", file.path().display()),
                (None, true) => "not saved yet • unsaved changes — Household ▸ Save as…".to_string(),
                (None, false) => "in memory — Household ▸ Save as… to keep it".to_string(),
            }))
            .right(Separator::vertical().h_3())
            .right(div().text_color(muted).child(alerting::status_label()))
            .right(Separator::vertical().h_3())
            .right(
                div()
                    .id("perf-counter")
                    .test_support()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(muted)
                    .child(self.perf.status_text()),
            )
            .right(Separator::vertical().h_3())
            .right(div().text_color(muted).child(format!("atlas-core {}", env!("CARGO_PKG_VERSION"))))
    }
}

impl Render for AtlasApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Perf: close the previous frame (its paint probe has fired by now),
        // open this one, and write the once-a-second summary when due.
        self.perf.begin_frame(self.section.slug());
        self.perf.log_window_info(window, cx.theme().is_dark());
        self.perf.log_summary_if_due(window);
        // The summary (histogram snapshot + file write) costs a few ms in a
        // debug build; keep it out of this frame's `build` figure.
        self.perf.restart_build_clock();
        let content_width = window.viewport_size().width - self.sidebar_width();
        let tree = v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            // Input counters only (no notify): they say in the log whether the
            // frames that happened were driven by the mouse or by something else.
            .on_mouse_move(cx.listener(|this, _, _, _| this.perf.count_mouse_move()))
            .on_scroll_wheel(cx.listener(|this, _, _, _| this.perf.count_wheel()))
            .child(self.render_title_bar(cx))
            .child(
                h_flex()
                    .items_stretch()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_sidebar(cx))
                    .child(
                        // The main column owns the scroll region; its inset is inside it.
                        // Its width is set in pixels rather than `flex_1()` on purpose:
                        // with an auto width taffy sizes the whole screen from its
                        // content on every pass of every ancestor, which multiplied the
                        // per-frame text measurements ~3× (see perf.rs; measured on the
                        // Privacy screen: 8,300 → 2,600 measure callbacks per frame).
                        v_flex()
                            .id("main-column")
                            .w(content_width)
                            .flex_none()
                            .h_full()
                            .p_6()
                            .gap_6()
                            .child(self.render_content(cx))
                            .overflow_y_scrollbar(),
                    ),
            )
            .child(self.render_status_bar(cx))
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx));
        // The probe times gpui's layout/prepaint/paint of the whole tree and
        // closes the `draw≈` measurement when its paint ends.
        let probed = self.perf.phase_probe(tree.into_any_element());
        self.perf.end_build();
        probed
    }
}
