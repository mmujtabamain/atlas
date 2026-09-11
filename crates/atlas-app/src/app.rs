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
use crate::launch::Launch;
use crate::screens::{self, Section, entities::EntityModels, household::HouseholdOverview, liquidity::LiquidityModel};
use atlas_core::ids::{AccountId, CompanyId, PersonId, ReservationId};
use atlas_core::liquidity::Boundary;
use atlas_core::model::{Coverage, Hardness, Reservation};
use gpui_kit::component::{
    IndexPath,
    dialog::DialogFooter,
    form::{Field, Form},
    input::{Input, InputState},
    radio::RadioGroup,
    select::{Select, SelectState},
};

pub struct AtlasApp {
    household: Household,
    viewer: Viewer,
    section: Section,
    horizon: NaiveDate,
    sidebar_collapsed: bool,
    /// Derived once per state change; screens only read it.
    overview: Result<HouseholdOverview, EngineError>,
    entities: Result<EntityModels, EngineError>,
    liquidity: Result<LiquidityModel, EngineError>,
    boundary: Boundary,
    selected_person: Option<PersonId>,
    selected_company: Option<CompanyId>,
    selected_account: Option<AccountId>,
    reservation_form: ReservationForm,
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
        let household = fixtures::plan_household();
        let viewer = Viewer::person(if launch.viewer == 'b' { fixtures::ids::PERSON_B } else { fixtures::ids::PERSON_A });
        let horizon = fixtures::default_horizon();
        let overview = Self::compute_overview(&household, viewer, horizon);
        let entities = Self::compute_entities(&household, viewer);
        let boundary = Boundary::Household;
        let liquidity = Self::compute_liquidity(&household, viewer, boundary, horizon);
        let reservation_form = ReservationForm::new(&household, viewer, _window, _cx);
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
        }
    }

    fn compute_liquidity(household: &Household, viewer: Viewer, boundary: Boundary, horizon: NaiveDate) -> Result<LiquidityModel, EngineError> {
        let result = LiquidityModel::compute(household, viewer, boundary, horizon);
        if let Err(err) = &result {
            alerting::report(Level::Error, format!("liquidity model failed for {boundary:?}: {err}"));
        }
        result
    }

    fn compute_entities(household: &Household, viewer: Viewer) -> Result<EntityModels, EngineError> {
        let result = EntityModels::compute(household, viewer);
        if let Err(err) = &result {
            alerting::report(Level::Error, format!("entity models failed for {}: {err}", viewer.person));
        }
        result
    }

    fn compute_overview(household: &Household, viewer: Viewer, horizon: NaiveDate) -> Result<HouseholdOverview, EngineError> {
        let result = HouseholdOverview::compute(household, viewer, horizon);
        if let Err(err) = &result {
            alerting::report(Level::Error, format!("household overview failed for {}: {err}", viewer.person));
        }
        result
    }

    /// Recomputes every derived model after the household or viewer changed.
    fn refresh_derived(&mut self) {
        self.overview = Self::compute_overview(&self.household, self.viewer, self.horizon);
        self.entities = Self::compute_entities(&self.household, self.viewer);
        self.liquidity = Self::compute_liquidity(&self.household, self.viewer, self.boundary, self.horizon);
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
        self.viewer = viewer;
        self.refresh_derived();
        cx.notify();
    }

    fn viewer_name(&self) -> String {
        self.household.entity_name(EntityRef::Person(self.viewer.person))
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
                    .child(Tag::secondary().xsmall().outline().child("M2 liquidity")),
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
                    .child(
                        h_flex()
                            .id("viewer")
                            .test_support()
                            .gap_1()
                            .items_center()
                            .text_xs()
                            .child(Icon::new(IconName::Eye).xsmall())
                            .child(format!("Viewing as {}", self.viewer_name())),
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
        match self.section {
            Section::Household => match &self.overview {
                Ok(overview) => screens::household::render(overview, &self.household, self.viewer, cx).into_any_element(),
                Err(err) => v_flex()
                    .id("screen-household")
                    .test_support()
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
                    let models = models.clone();
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
                    let model = model.clone();
                    screens::liquidity::render(&model, &self.household, self.viewer, cx).into_any_element()
                }
                Err(err) => self.render_engine_failure(Section::Liquidity, err, cx),
            },
            Section::Settings => screens::settings::render(&self.household, &self.viewer_name(), cx).into_any_element(),
            other => screens::placeholder::render(other, cx).into_any_element(),
        }
    }

    fn render_engine_failure(&self, section: Section, err: &EngineError, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .id(SharedString::from(format!("screen-{}", section.slug())))
            .test_support()
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
            .right(div().text_color(muted).child(alerting::status_label()))
            .right(Separator::vertical().h_3())
            .right(div().text_color(muted).child(format!("atlas-core {}", env!("CARGO_PKG_VERSION"))))
    }
}

impl Render for AtlasApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_title_bar(cx))
            .child(
                h_flex()
                    .items_stretch()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_sidebar(cx))
                    .child(
                        // The main column owns the scroll region; its inset is inside it.
                        v_flex()
                            .id("main-column")
                            .flex_1()
                            .min_w_0()
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
            .children(Root::render_notification_layer(window, cx))
    }
}
