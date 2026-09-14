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
//! What the shell shows follows the design, and **each fact is stated once**:
//!
//! | region | states |
//! |---|---|
//! | title bar, leading | the household — its name and file menu, the sample marker |
//! | title bar, trailing | the file state with `Save` beside it, the viewer, the theme |
//! | sidebar | the app's own name, the eight destinations, Figure meanings and Settings |
//! | status bar | currency, the two dates, the last result, the file's path, gpui's frame reading |
//!
//! The household is named in the title bar and nowhere else; the app is named
//! in the sidebar, and in the title bar only while there is no sidebar (before
//! a household is open, and while the viewer gate is up). Saving is the most
//! used command in the app, so it sits in the chrome next to the state it acts
//! on, and the footer names the file that state is about.

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Collapsible, Disableable as _, Icon, Root, Sizable as _, Theme, ThemeMode, TitleBar,
    button::{Button, ButtonVariants as _},
    h_flex,
    separator::Separator,
    sidebar::{Sidebar, SidebarFooter, SidebarHeader, SidebarItem, SidebarMenu, SidebarMenuItem},
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

/// The file facts the chrome states, read once per frame (the shell renders on
/// every one of them, so `file_state_text` is formatted once and split, not
/// called from both bars).
struct FileChrome {
    /// The state machine's wording without the path: `Saved`, `Unsaved
    /// changes`, `Saving`, `Viewing only · <holder> opened <when>`.
    state: String,
    /// The file the state is about, when there is one.
    path: Option<String>,
    /// `Save`, or `Save…` when Save must first ask for a file.
    save_label: &'static str,
    saving: bool,
}

impl FileChrome {
    fn of(app: &AtlasApp) -> Self {
        let text = app.file_state_text();
        // With a file, `file_state_text` ends in ` · <path>`. The title bar
        // states the state and the status bar names the file, so the split
        // keeps the pair from saying the same thing twice.
        let split = app.file_path().is_some().then(|| text.rsplit_once(" · ")).flatten().map(|(state, path)| (state.to_string(), path.to_string()));
        let (state, path) = match split {
            Some((state, path)) => (state, Some(path)),
            None => (text, None),
        };
        FileChrome { state, path, save_label: app.save_command_label(), saving: app.is_saving() }
    }
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

    fn render_title_bar(&self, fullscreen: bool, file: &FileChrome, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = cx.theme().is_dark();
        let muted = cx.theme().muted_foreground;
        let app = self.app.read(cx);
        let opened = app.is_opened();
        let is_sample = app.is_sample();
        // The sidebar names the app whenever it is on screen; the title bar
        // then leads with the household instead of repeating the app's name.
        let named_in_sidebar = opened && !app.viewer_pending();
        let viewer_label = if opened { format!("Who is looking: {}", app.viewer_display_name()) } else { String::new() };
        let menu = app.render_household_menu(self.app.downgrade());
        let picker = self.app.clone();
        let save = self.app.clone();
        TitleBar::new()
            .when(fullscreen, |bar| bar.pl_0())
            .child(
                h_flex()
                    .items_center()
                    .gap_3()
                    .when(!named_in_sidebar, |this| {
                        this.child(Icon::new(IconName::Wallet).small()).child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Atlas Financer"))
                    })
                    .when(opened, |this| this.child(menu))
                    .when(opened && is_sample, |this| this.child(Tag::secondary().xsmall().outline().child("Fictitious sample"))),
            )
            .child(
                h_flex()
                    .items_center()
                    .justify_end()
                    .px_2()
                    .gap_3()
                    // Save and the state it acts on are one pair, set apart
                    // from the viewer and the theme by the wider gap.
                    .when(opened, |this| {
                        this.child(
                            h_flex()
                                .items_center()
                                .gap_2()
                                .child(div().id("file-state").test_support().text_xs().text_color(muted).child(file.state.clone()))
                                .child(
                                    Button::new("title-save")
                                        .small()
                                        .outline()
                                        .label(file.save_label)
                                        // A save already running must not be started twice.
                                        .disabled(file.saving)
                                        .tooltip("Write the household to its file")
                                        .on_click(move |_, window, cx| save.update(cx, |app, cx| app.save(window, cx))),
                                ),
                        )
                    })
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

    fn render_status_bar(&self, file: &FileChrome, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let mono = cx.theme().mono_font_family.clone();
        let app = self.app.read(cx);
        let household = app.household();
        let opened = app.is_opened();
        let last_result = app.last_result().map(str::to_string);
        let mut bar = StatusBar::new();
        if opened {
            bar = bar
                .left(div().child(household.base_currency.code().to_string()))
                .left(Separator::vertical().h_3())
                .left(div().text_color(muted).child(format!("Balances as of {}", household.as_of.format("%d %b %Y"))))
                .left(Separator::vertical().h_3())
                .left(div().text_color(muted).child(format!("Forecast through {}", app.horizon().format("%d %b %Y"))));
            if let Some(result) = last_result {
                bar = bar.left(Separator::vertical().h_3()).left(div().id("last-result").test_support().text_color(muted).child(result));
            }
        } else {
            bar = bar.left(div().text_color(muted).child("No household open"));
        }
        // The title bar states the file's state; the footer names the file. A
        // long path keeps its folder and its name and loses the middle.
        if let Some(path) = file.path.clone() {
            bar = bar
                .right(
                    div()
                        .id("file-path")
                        .test_support()
                        .max_w(px(360.))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis_middle()
                        .text_color(muted)
                        .child(path),
                )
                .right(Separator::vertical().h_3());
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
        // One read for both bars: the title bar states the file's state, the
        // status bar names the file.
        let file = FileChrome::of(self.app.read(cx));
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
            .child(self.render_title_bar(window.is_fullscreen(), &file, cx))
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
            .child(self.render_status_bar(&file, cx))
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
///
/// It is the destination and the collapse state and nothing else: the sidebar
/// states the app's own name, which never changes, so a new viewer, a renamed
/// household or a different currency no longer costs a sidebar frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarSnapshot {
    pub destination: Option<Destination>,
    pub collapsed: bool,
}

impl SidebarSnapshot {
    fn of(app: &AtlasApp) -> Self {
        SidebarSnapshot { destination: app.destination(), collapsed: app.sidebar_collapsed() }
    }
}

/// One group of destinations, and the hairline that divides it from the group
/// before it.
///
/// gpui-kit's `SidebarGroup` spends a 32 px label row on every group whether or
/// not the group is named — and none of these are, because naming only the
/// first group (as the design's illustrations do) names nothing. It is the
/// index of the sidebar's child that names the items inside it: `main-sidebar`
/// ▸ `1-0-0` is the second group's first destination, which is how
/// `crates/atlas-app/tests/ui.rs` clicks them. This renders the menu under
/// exactly the id the group would have given it, and spends the band on the
/// rule instead.
#[derive(Clone)]
struct NavGroup {
    /// Draw the dividing rule above this group.
    rule: bool,
    collapsed: bool,
    menu: SidebarMenu,
}

impl Collapsible for NavGroup {
    fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }
}

impl SidebarItem for NavGroup {
    fn render(self, id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let menu_id = SharedString::from(format!("{}-0", id.into()));
        let hairline = cx.theme().sidebar_border;
        v_flex()
            .when(self.rule, |this| this.child(div().w_full().my_2().h(px(1.)).bg(hairline)))
            .child(self.menu.collapsed(self.collapsed).render(menu_id, window, cx))
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
                log::debug!("perf: sidebar re-renders (destination={:?} collapsed={})", next.destination, next.collapsed);
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
        let collapsed = self.snapshot.collapsed;
        let active = self.snapshot.destination;
        let theme = cx.theme();
        // The header is the app's own mark and name. The household is named in
        // the title bar, which is where its menu and its file commands are.
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
                .when(!collapsed, |this| this.child(div().flex_1().overflow_hidden().text_sm().font_weight(FontWeight::SEMIBOLD).child("Atlas Financer"))),
        );
        for (index, group) in Destination::GROUPS.iter().enumerate() {
            let menu = SidebarMenu::new().children(group.iter().map(|destination| {
                let destination = *destination;
                SidebarMenuItem::new(destination.label())
                    .icon(destination.icon())
                    .active(active == Some(destination))
                    .on_click(cx.listener(move |this, _, _, cx| this.app.update(cx, |app, cx| app.navigate(destination.home(), cx))))
            }));
            sidebar = sidebar.child(NavGroup { rule: index > 0, collapsed, menu });
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
                            .when(active == Some(Destination::Settings), |b| b.primary())
                            .on_click(cx.listener(|this, _, _, cx| this.app.update(cx, |app, cx| app.navigate(Route::Settings, cx)))),
                    ),
            ),
        )
    }
}
