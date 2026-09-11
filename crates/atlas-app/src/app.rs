//! `AtlasApp`: the window's root view. It owns the household, the viewer, the
//! active section and the derived screen models; screens are pure rendering
//! over those models.

use atlas_core::authz::Viewer;
use atlas_core::fixtures;
use atlas_core::ids::EntityRef;
use atlas_core::model::Household;
use atlas_core::EngineError;
use chrono::NaiveDate;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Root, Sizable as _, Theme, ThemeMode, TitleBar,
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
use crate::screens::{self, Section, household::HouseholdOverview};

pub struct AtlasApp {
    household: Household,
    viewer: Viewer,
    section: Section,
    horizon: NaiveDate,
    sidebar_collapsed: bool,
    /// Derived once per state change; screens only read it.
    overview: Result<HouseholdOverview, EngineError>,
}

impl AtlasApp {
    pub fn new(launch: &Launch, _window: &mut Window, _cx: &mut Context<Self>) -> Self {
        let household = fixtures::plan_household();
        let viewer = Viewer::person(if launch.viewer == 'b' { fixtures::ids::PERSON_B } else { fixtures::ids::PERSON_A });
        let horizon = fixtures::default_horizon();
        let overview = Self::compute_overview(&household, viewer, horizon);
        log::info!("Atlas Financer window: section={} viewer={}", launch.section.slug(), viewer.person);
        AtlasApp {
            household,
            viewer,
            section: launch.section,
            horizon,
            sidebar_collapsed: false,
            overview,
        }
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
                    .child(Tag::secondary().xsmall().outline().child("M0 foundation")),
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
            Section::Settings => screens::settings::render(&self.household, &self.viewer_name(), cx).into_any_element(),
            other => screens::placeholder::render(other, cx).into_any_element(),
        }
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
