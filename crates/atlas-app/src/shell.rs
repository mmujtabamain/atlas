//! The window shell: the root view around the content.
//!
//! gpui rebuilds a view's whole element tree whenever the view is notified —
//! and a hover, a tooltip, a scroll tick or a keystroke in a dialog all notify
//! the view that rendered the element under the pointer. With one view for the
//! whole window every such event re-laid-out and re-painted the title bar,
//! the sidebar, the entire screen and the status bar.
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
//! What the shell shows follows the design: the title bar carries the app
//! name, the household menu, the sample marker, the viewer and the theme; the
//! sidebar the eight destinations and the footer (Figure meanings, Settings);
//! the status bar the currency, the two dates, the file state with Save, the
//! last result and gpui's frame reading.

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

use crate::app::AtlasApp;
use crate::launch::Launch;
use crate::nav::{Destination, Route};

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

    fn render_title_bar(&self, fullscreen: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = cx.theme().is_dark();
        let app = self.app.read(cx);
        let opened = app.is_opened();
        let is_sample = app.is_sample();
        let viewer_label = if opened { format!("Who is looking: {}", app.viewer_display_name()) } else { String::new() };
        let menu = app.render_household_menu(self.app.downgrade());
        let picker = self.app.clone();
        TitleBar::new()
            .when(fullscreen, |bar| bar.pl_0())
            .child(
                h_flex()
                    .items_center()
                    .gap_3()
                    .child(Icon::new(IconName::Wallet).small())
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Atlas Financer"))
                    .when(opened, |this| this.child(menu))
                    .when(opened && is_sample, |this| this.child(Tag::secondary().xsmall().outline().child("Fictitious sample"))),
            )
            .child(
                h_flex()
                    .items_center()
                    .justify_end()
                    .px_2()
                    .gap_3()
                    .when(opened, |this| {
                        this.child(
                            Button::new("viewer")
                                .small()
                                .ghost()
                                .compact()
                                .icon(IconName::Eye)
                                .label(viewer_label)
                                .tooltip("Change who is looking")
                                .on_click(move |_, window, cx| picker.update(cx, |app, cx| app.open_viewer_picker(window, cx))),
                        )
                    })
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
        let opened = app.is_opened();
        let save = self.app.clone();
        let file_state = app.file_state_text();
        let save_label = app.save_command_label();
        let last_result = app.last_result().map(str::to_string);
        let mut bar = StatusBar::new();
        if opened {
            bar = bar
                .left(div().child(household.base_currency.code().to_string()))
                .left(Separator::vertical().h_3())
                .left(div().text_color(muted).child(format!("Balances as of {}", household.as_of.format("%d %b %Y"))))
                .left(Separator::vertical().h_3())
                .left(div().text_color(muted).child(format!("Forecast through {}", app.horizon().format("%d %b %Y"))))
                .left(Separator::vertical().h_3())
                .left(div().id("file-state").test_support().text_color(muted).child(file_state))
                .left(
                    Button::new("status-save")
                        .xsmall()
                        .ghost()
                        .compact()
                        .label(save_label)
                        .on_click(move |_, window, cx| save.update(cx, |app, cx| app.save(window, cx))),
                );
            if let Some(result) = last_result {
                bar = bar.left(Separator::vertical().h_3()).left(div().id("last-result").test_support().text_color(muted).child(result));
            }
        } else {
            bar = bar.left(div().text_color(muted).child("No household open"));
        }
        bar.right(
            div()
                .id("perf-counter")
                .test_support()
                .font_family(mono)
                .text_color(muted)
                .child(app.perf().status_text()),
        )
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Perf: the frame meter lives in the content view (tests read it there)
        // but it is driven from here, the one view that renders every frame.
        // Close the previous frame (its paint probe has fired by now), open this
        // one, and write the once-a-second summary when due.
        let is_dark = cx.theme().is_dark();
        let (collapsed, show_sidebar) = self.app.update(cx, |app, _| {
            app.perf.begin_frame(app.route().slug());
            app.perf.log_window_info(window, is_dark);
            app.perf.log_summary_if_due(window);
            // The summary (histogram snapshot + file write) costs a few ms in a
            // debug build; keep it out of this frame's `build` figure.
            app.perf.restart_build_clock();
            (app.sidebar_collapsed(), app.is_opened() && !app.viewer_pending())
        });
        let sidebar_width = if show_sidebar { sidebar_width(collapsed) } else { px(0.) };
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
            .child(self.render_title_bar(window.is_fullscreen(), cx))
            .child(
                h_flex()
                    .items_stretch()
                    .flex_1()
                    .min_h_0()
                    // Cached views are laid out from the style given here (their
                    // contents are not measured), so both get a definite size.
                    .when(show_sidebar, |this| this.child(self.sidebar.clone().cached(StyleRefinement::default().w(sidebar_width).h_full().flex_none())))
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
    pub destination: Option<Destination>,
    pub collapsed: bool,
    pub household_name: String,
    pub subtitle: String,
}

impl SidebarSnapshot {
    fn of(app: &AtlasApp) -> Self {
        let household = app.household();
        SidebarSnapshot {
            destination: app.destination(),
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
                log::debug!("perf: sidebar re-renders (destination={:?} viewer={})", next.destination, next.subtitle);
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
        // Group breaks are visual separators, not named groups: the group label
        // stays empty so the sidebar draws only the items.
        for group in Destination::GROUPS {
            sidebar = sidebar.child(SidebarGroup::new("").child(SidebarMenu::new().children(group.iter().map(|destination| {
                let destination = *destination;
                SidebarMenuItem::new(destination.label())
                    .icon(destination.icon())
                    .active(snapshot.destination == Some(destination))
                    .on_click(cx.listener(move |this, _, _, cx| this.app.update(cx, |app, cx| app.navigate(destination.home(), cx))))
            }))));
        }
        let meanings = self.app.clone();
        sidebar.footer(
            SidebarFooter::new().child(
                v_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        Button::new("sidebar-figure-meanings")
                            .small()
                            .ghost()
                            .compact()
                            .icon(IconName::BookOpen)
                            .when(!collapsed, |b| b.label("Figure meanings…"))
                            .tooltip("What the tags on every figure mean")
                            .on_click(move |_, window, cx| meanings.update(cx, |app, cx| app.open_figure_meanings(None, window, cx))),
                    )
                    .child(
                        Button::new("sidebar-settings")
                            .small()
                            .ghost()
                            .compact()
                            .icon(IconName::Settings)
                            .when(!collapsed, |b| b.label("Settings"))
                            .when(snapshot.destination == Some(Destination::Settings), |b| b.primary())
                            .on_click(cx.listener(|this, _, _, cx| this.app.update(cx, |app, cx| app.navigate(Route::Settings, cx)))),
                    ),
            ),
        )
    }
}
