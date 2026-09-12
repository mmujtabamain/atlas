//! The window shell: the root view around the content.
//!
//! gpui rebuilds a view's whole element tree whenever the view is notified —
//! and a hover, a tooltip, a scroll tick or a keystroke in a dialog all notify
//! the view that rendered the element under the pointer. With one view for the
//! whole window (how the app started) every such event re-laid-out and
//! re-painted the title bar, the sidebar, the entire screen and the status bar:
//! 1,800 taffy nodes for a hover over a sidebar item.
//!
//! This module splits the window into views that gpui can cache independently
//! ([`Entity::cached`] reuses a view's layout and paint while the view is not
//! dirty and its bounds are unchanged):
//!
//! | view | notified by | cost when something else changes |
//! |---|---|---|
//! | [`Shell`] (root) | always renders | title bar + status bar, ~40 nodes |
//! | [`SidebarView`] | hover/click on the sidebar; the app, when what the sidebar shows changed | cached |
//! | [`AtlasApp`] (content) | hover/scroll/click inside the screen; every state change | cached |
//!
//! The sidebar does not simply observe the content view — a scroll tick
//! notifies the content view too, and re-rendering the sidebar on every tick
//! would be pointless. Instead it keeps a [`SidebarSnapshot`] of what it shows
//! and re-renders only when that snapshot changes.
//!
//! The title bar and the status bar stay inline in the shell: together they are
//! ~40 nodes, the status bar's frame counter changes every frame, and caching
//! them would buy less than it costs.

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Root, Sizable as _, Theme, ThemeMode, TitleBar,
    button::{Button, ButtonVariants as _},
    h_flex,
    separator::Separator,
    sidebar::{Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
    status_bar::StatusBar,
    tag::Tag,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::alerting;
use crate::app::AtlasApp;
use crate::launch::Launch;
use crate::screens::Section;

/// Width of the sidebar column: gpui-kit's `w_64` expanded, its icon width collapsed.
pub fn sidebar_width(collapsed: bool) -> Pixels {
    if collapsed { px(48.) } else { px(256.) }
}

/// The window's root view.
pub struct Shell {
    app: Entity<AtlasApp>,
    sidebar: Entity<SidebarView>,
}

impl Shell {
    /// Creates the content view and the shell around it.
    pub fn new(launch: &Launch, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let app = cx.new(|cx| AtlasApp::new(launch, window, cx));
        let sidebar = cx.new(|cx| SidebarView::new(app.clone(), cx));
        Shell { app, sidebar }
    }

    /// The content view: the household, the derived models, every screen.
    pub fn app(&self) -> &Entity<AtlasApp> {
        &self.app
    }

    /// The sidebar view (tests check that it is reused between frames).
    pub fn sidebar(&self) -> &Entity<SidebarView> {
        &self.sidebar
    }

    fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next = if cx.theme().is_dark() { ThemeMode::Light } else { ThemeMode::Dark };
        // `Theme::change` refreshes the window, which bypasses every view cache.
        Theme::change(next, Some(window), cx);
        cx.notify();
    }

    fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = cx.theme().is_dark();
        let app = self.app.read(cx);
        let reconciled = format!("reconciled {}", app.household().as_of.format("%d %b %Y"));
        let viewing_as = format!("Viewing as {}", app.viewer_display_name());
        let menu = app.render_household_menu(self.app.downgrade());
        let picker = self.app.clone();
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
                    .child(div().text_xs().text_color(cx.theme().muted_foreground).child(reconciled))
                    .child(menu)
                    .child(
                        Button::new("viewer")
                            .small()
                            .ghost()
                            .compact()
                            .icon(IconName::Eye)
                            .label(viewing_as)
                            .tooltip("Change who is looking")
                            .on_click(move |_, window, cx| picker.update(cx, |app, cx| app.open_viewer_picker(window, cx))),
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

    fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let mono = cx.theme().mono_font_family.clone();
        let app = self.app.read(cx);
        let household = app.household();
        StatusBar::new()
            .left(h_flex().items_center().gap_1().child(Icon::new(IconName::Check).xsmall()).child(household.name.clone()))
            .left(Separator::vertical().h_3())
            .left(div().text_color(muted).child(format!(
                "{} accounts · {} series · {} reservations · {} policies",
                household.accounts.len(),
                household.series.len(),
                household.reservations.len(),
                household.policies.len()
            )))
            .right(div().text_color(muted).child(match (app.file_path(), app.is_dirty()) {
                (Some(path), true) => format!("{} • unsaved", path.display()),
                (Some(path), false) => format!("{}", path.display()),
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
                    .font_family(mono)
                    .text_color(muted)
                    .child(app.perf().status_text()),
            )
            .right(Separator::vertical().h_3())
            .right(div().text_color(muted).child(format!("atlas-core {}", env!("CARGO_PKG_VERSION"))))
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Perf: the frame meter lives in the content view (tests read it there)
        // but it is driven from here, the one view that renders every frame.
        // Close the previous frame (its paint probe has fired by now), open this
        // one, and write the once-a-second summary when due.
        let is_dark = cx.theme().is_dark();
        let collapsed = self.app.update(cx, |app, _| {
            app.perf.begin_frame(app.section().slug());
            app.perf.log_window_info(window, is_dark);
            app.perf.log_summary_if_due(window);
            // The summary (histogram snapshot + file write) costs a few ms in a
            // debug build; keep it out of this frame's `build` figure.
            app.perf.restart_build_clock();
            app.sidebar_collapsed()
        });
        let sidebar_width = sidebar_width(collapsed);
        // The content column's width is set in pixels rather than `flex_1()` on
        // purpose: with an auto width taffy sizes the whole screen from its
        // content on every pass of every ancestor (docs/perf.md §2).
        let content_width = window.viewport_size().width - sidebar_width;
        let tree = v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            // Input counters only (no notify): they say in the log whether the
            // frames that happened were driven by the mouse or by something else.
            .on_mouse_move(cx.listener(|this, _, _, cx| this.app.read(cx).perf().count_mouse_move()))
            .on_scroll_wheel(cx.listener(|this, _, _, cx| this.app.read(cx).perf().count_wheel()))
            .child(self.render_title_bar(cx))
            .child(
                h_flex()
                    .items_stretch()
                    .flex_1()
                    .min_h_0()
                    // Cached views are laid out from the style given here (their
                    // contents are not measured), so both get a definite size.
                    .child(self.sidebar.clone().cached(StyleRefinement::default().w(sidebar_width).h_full().flex_none()))
                    .child(self.app.clone().cached(StyleRefinement::default().w(content_width).h_full().flex_none())),
            )
            .child(self.render_status_bar(cx))
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx));
        // The probe times gpui's layout/prepaint/paint of the whole tree and
        // closes the `draw≈` measurement when its paint ends.
        let probed = self.app.read(cx).perf().phase_probe(tree.into_any_element());
        self.app.update(cx, |app, _| app.perf.end_build());
        probed
    }
}

/// What the sidebar shows. The sidebar re-renders only when this changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarSnapshot {
    pub section: Section,
    pub collapsed: bool,
    pub household_name: String,
    pub subtitle: String,
}

impl SidebarSnapshot {
    fn of(app: &AtlasApp) -> Self {
        let household = app.household();
        SidebarSnapshot {
            section: app.section(),
            collapsed: app.sidebar_collapsed(),
            household_name: household.name.clone(),
            subtitle: format!("{} · {}", household.base_currency, app.viewer_display_name()),
        }
    }
}

/// The navigation sidebar, as its own cached view.
pub struct SidebarView {
    app: Entity<AtlasApp>,
    snapshot: SidebarSnapshot,
    /// Renders since creation — how tests see that a frame reused the cache.
    renders: u64,
    _observe: Subscription,
}

impl SidebarView {
    fn new(app: Entity<AtlasApp>, cx: &mut Context<Self>) -> Self {
        let snapshot = SidebarSnapshot::of(app.read(cx));
        // Every notify of the content view lands here (hover, scroll, edits);
        // only a changed snapshot is worth a re-render.
        let _observe = cx.observe(&app, |this, app, cx| {
            let next = SidebarSnapshot::of(app.read(cx));
            if next != this.snapshot {
                log::debug!("perf: sidebar re-renders (section={} viewer={})", next.section.slug(), next.subtitle);
                this.snapshot = next;
                cx.notify();
            }
        });
        SidebarView { app, snapshot, renders: 0, _observe }
    }

    /// How many times the sidebar has been rendered.
    pub fn renders(&self) -> u64 {
        self.renders
    }

    pub fn snapshot(&self) -> &SidebarSnapshot {
        &self.snapshot
    }
}

impl Render for SidebarView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders += 1;
        let snapshot = &self.snapshot;
        let collapsed = snapshot.collapsed;
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
                            .child(snapshot.household_name.clone())
                            .child(div().text_xs().text_color(theme.muted_foreground).child(snapshot.subtitle.clone())),
                    )
                }),
        );
        for (group, sections) in Section::GROUPS {
            sidebar = sidebar.child(SidebarGroup::new(group).child(SidebarMenu::new().children(sections.iter().map(|section| {
                let section = *section;
                let item = SidebarMenuItem::new(section.label())
                    .icon(section.icon())
                    .active(section == snapshot.section)
                    .on_click(cx.listener(move |this, _, _, cx| this.app.update(cx, |app, cx| app.navigate(section, cx))));
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
}
