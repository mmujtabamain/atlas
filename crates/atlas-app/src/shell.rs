//! The window shell: the root view around the content.
//!
//! gpui rebuilds a view's whole element tree whenever the view is notified —
//! and a hover, a tooltip, a scroll tick or a keystroke in a dialog all notify
//! the view that rendered the element under the pointer. With one view for the
//! whole window every such event re-laid-out and re-painted the title bar,
//! the launcher, the entire screen and the status bar.
//!
//! This module splits the window into views that gpui can cache independently
//! ([`Entity::cached`] reuses a view's layout and paint while the view is not
//! dirty and its bounds are unchanged):
//!
//! | view | notified by | cost when something else changes |
//! |---|---|---|
//! | [`Shell`] (root) | always renders | title bar + status bar, ~40 nodes |
//! | [`LauncherView`] | hover/click on the strip; the app, when the active pane's destination changed; its own rearrangement | cached |
//! | [`WorkspaceView`] (content while a household is usable) | a pane inside it; every layout change | cached; each pane is a cached view of its own inside gpui-kit's dock |
//! | [`AtlasApp`] (content before that: Welcome, the viewer gate) | hover/scroll/click inside the screen; every state change | cached |
//!
//! The launcher does not simply observe the content view — a scroll tick
//! notifies the content view too, and re-rendering the strip on every tick
//! would be pointless. It keeps a snapshot of what it shows and re-renders
//! only when that changes (see [`crate::workspace::launcher`]).
//!
//! What the shell shows follows the design, and **each fact is stated once**:
//!
//! | region | states |
//! |---|---|
//! | title bar, leading | the app's name, the household — its name and file menu — the sample marker, the layout menu |
//! | title bar, trailing | the jobs indicator while there are jobs, the file state with `Save` beside it, the viewer, Figure meanings, the theme, Settings |
//! | launcher strip | the screens, as panes to open or focus; the `+` and `…` menus |
//! | status bar | currency, the two dates, the last result, the file's path, gpui's frame reading |
//!
//! The household is named in the title bar and nowhere else. Saving is the
//! most used command in the app, so it sits in the chrome next to the state
//! it acts on, and the footer names the file that state is about. The layout
//! menu sits beside the household because a layout is workspace-level state:
//! it belongs to the window, not to any pane.

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Root, Sizable as _, Theme, ThemeMode, TitleBar, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    separator::Separator,
    status_bar::StatusBar,
    tag::Tag,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::AtlasApp;
use crate::launch::Launch;
use crate::nav::Route;
use crate::workspace::jobs::{JobCenter, JobEvent};
use crate::workspace::launcher::{LAUNCHER_HEIGHT, LauncherView};
use crate::workspace::{WorkspaceView, commands};
use atlas_workspace::JobState;
use atlas_workspace::resolver::Intent;

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
    workspace: Entity<WorkspaceView>,
    launcher: Entity<LauncherView>,
    _job_events: Subscription,
}

impl Shell {
    /// Creates the content view, the workspace that shows its screens as
    /// panes, the launcher, and the shell around them.
    pub fn new(launch: &Launch, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let app = cx.new(|cx| AtlasApp::new(launch, window, cx));
        commands::bind_keys(cx);
        let workspace = cx.new(|cx| WorkspaceView::new(app.clone(), launch, window, cx));
        app.update(cx, |app, _| app.attach_workspace(workspace.downgrade()));
        let launcher = cx.new(|cx| LauncherView::new(app.clone(), workspace.clone(), launch.data_dir.clone(), cx));
        // A job that ends is announced whether or not the pane that started
        // it is still open: a toast, and the jobs list keeps the outcome.
        let jobs = app.read(cx).jobs().clone();
        let _job_events = cx.subscribe_in(&jobs, window, |this, jobs, event: &JobEvent, window, cx| {
            let JobEvent::Finished(id) = event;
            let Some(job) = jobs.read(cx).get(*id).cloned() else {
                return;
            };
            match &job.state {
                JobState::Completed { summary } => window.push_notification(summary.clone(), cx),
                JobState::Failed { error, .. } => window.push_notification(gpui_kit::component::notification::Notification::error(format!("{} failed: {error}", job.title)), cx),
                JobState::Cancelled => window.push_notification(format!("{} cancelled", job.title), cx),
                _ => {}
            }
            let _ = this;
        });
        Shell { app, workspace, launcher, _job_events }
    }

    /// The jobs indicator: how many jobs are running, and the list of jobs
    /// with the way to each job's screen, a retry for a failed job and a
    /// cancel for one that allows it. Absent while there are no jobs at all.
    fn render_jobs_indicator(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let jobs: Entity<JobCenter> = self.app.read(cx).jobs().clone();
        let (running, all) = {
            let centre = jobs.read(cx);
            if centre.jobs().is_empty() {
                return None;
            }
            (centre.running_count(), centre.jobs().to_vec())
        };
        let failed = all.iter().filter(|job| matches!(job.state, JobState::Failed { .. })).count();
        let label = if running > 0 {
            format!("{running} running")
        } else if failed > 0 {
            format!("{failed} failed")
        } else {
            "Done".to_string()
        };
        let icon = if running > 0 {
            IconName::LoaderCircle
        } else if failed > 0 {
            IconName::CircleX
        } else {
            IconName::Check
        };
        let workspace = self.workspace.clone();
        let button = Button::new("jobs").small().ghost().compact().icon(icon).label(label).tooltip("Background work: what is running, what finished, what failed").dropdown_menu(move |menu, _, cx| {
            let centre = jobs.read(cx);
            let mut menu = menu;
            for job in centre.jobs().iter().rev() {
                let line = centre.describe(job);
                let route = centre.associated_route(job.id);
                let workspace = workspace.clone();
                let open = move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                    if let Some(route) = route {
                        workspace.update(cx, |workspace, cx| {
                            let _ = workspace.open(route, Intent::Open, window, cx);
                        });
                    }
                };
                menu = menu.item(PopupMenuItem::new(line).icon(match job.state {
                    JobState::Queued | JobState::Running { .. } => IconName::LoaderCircle,
                    JobState::Completed { .. } => IconName::Check,
                    JobState::Failed { .. } => IconName::CircleX,
                    JobState::Cancelled => IconName::Ban,
                }).disabled(route.is_none()).on_click(open));
                if centre.can_retry(job.id) {
                    let id = job.id;
                    let jobs = jobs.clone();
                    menu = menu.item(PopupMenuItem::new(format!("Retry: {}", job.title)).icon(IconName::RotateCcw).on_click(move |_, window, cx| {
                        jobs.update(cx, |jobs, cx| {
                            if let Err(err) = jobs.retry(id, window, cx) {
                                log::warn!("jobs: retry of {id} refused: {err}");
                            }
                        });
                    }));
                }
                if job.cancellable && job.state.is_active() {
                    let id = job.id;
                    let jobs = jobs.clone();
                    menu = menu.item(PopupMenuItem::new(format!("Cancel: {}", job.title)).icon(IconName::Ban).on_click(move |_, _, cx| {
                        jobs.update(cx, |jobs, cx| {
                            if let Err(err) = jobs.cancel(id, cx) {
                                log::warn!("jobs: cancel of {id} refused: {err}");
                            }
                        });
                    }));
                }
            }
            let jobs = jobs.clone();
            menu.separator().item(PopupMenuItem::new("Clear finished jobs").icon(IconName::Trash).on_click(move |_, _, cx| jobs.update(cx, |jobs, cx| jobs.clear_finished(cx))))
        });
        Some(button)
    }

    /// The content view: the household, the derived models, every screen.
    pub fn app(&self) -> &Entity<AtlasApp> {
        &self.app
    }

    /// The pane workspace: the content column while a household is open and
    /// the viewer is chosen.
    pub fn workspace(&self) -> &Entity<WorkspaceView> {
        &self.workspace
    }

    /// The launcher strip (tests check that it is reused between frames).
    pub fn launcher(&self) -> &Entity<LauncherView> {
        &self.launcher
    }

    fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next = if cx.theme().is_dark() { ThemeMode::Light } else { ThemeMode::Dark };
        // `Theme::change` refreshes the window, which bypasses every view cache.
        Theme::change(next, Some(window), cx);
        cx.notify();
    }

    /// The layout menu: the workspace-level commands. Undo and redo name the
    /// step they would take back; saved layouts join this menu once they exist.
    fn render_layout_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.workspace.clone();
        let (undo, redo) = {
            let history = self.workspace.read(cx).history();
            (history.undo_label().map(str::to_owned), history.redo_label().map(str::to_owned))
        };
        Button::new("layout-menu").small().ghost().compact().icon(IconName::LayoutGrid).label("Layout").tooltip("Arrange the workspace: undo, redo, reset").dropdown_menu(move |menu, _, _| {
            let undo_workspace = workspace.clone();
            let redo_workspace = workspace.clone();
            let reset_workspace = workspace.clone();
            menu.item(PopupMenuItem::new(match &undo {
                Some(label) => format!("Undo: {label}"),
                None => "Nothing to undo".to_string(),
            })
            .icon(IconName::Undo2)
            .disabled(undo.is_none())
            .on_click(move |_, window, cx| {
                undo_workspace.update(cx, |workspace, cx| {
                    workspace.undo(window, cx);
                });
            }))
            .item(PopupMenuItem::new(match &redo {
                Some(label) => format!("Redo: {label}"),
                None => "Nothing to redo".to_string(),
            })
            .icon(IconName::Redo2)
            .disabled(redo.is_none())
            .on_click(move |_, window, cx| {
                redo_workspace.update(cx, |workspace, cx| {
                    workspace.redo(window, cx);
                });
            }))
            .separator()
            .item(PopupMenuItem::new("Reset layout").icon(IconName::RotateCcw).on_click(move |_, window, cx| {
                reset_workspace.update(cx, |workspace, cx| workspace.reset_layout(window, cx));
            }))
        })
    }

    fn render_title_bar(&self, fullscreen: bool, file: &FileChrome, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dark = cx.theme().is_dark();
        let muted = cx.theme().muted_foreground;
        let app = self.app.read(cx);
        let opened = app.is_opened();
        let usable = opened && !app.viewer_pending();
        let is_sample = app.is_sample();
        let viewer_label = if opened { format!("Who is looking: {}", app.viewer_display_name()) } else { String::new() };
        let menu = app.render_household_menu(self.app.downgrade());
        let picker = self.app.clone();
        let save = self.app.clone();
        let meanings = self.app.clone();
        let settings = self.workspace.clone();
        TitleBar::new()
            .when(fullscreen, |bar| bar.pl_0())
            .child(
                h_flex()
                    .items_center()
                    .gap_3()
                    .child(h_flex().items_center().gap_2().child(Icon::new(IconName::Wallet).small()).child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Atlas Financer")))
                    .when(opened, |this| this.child(menu))
                    .when(opened && is_sample, |this| this.child(Tag::secondary().xsmall().outline().child("Fictitious sample")))
                    .when(usable, |this| this.child(self.render_layout_menu(cx))),
            )
            .child(
                h_flex()
                    .items_center()
                    .justify_end()
                    .px_2()
                    .gap_3()
                    .children(self.render_jobs_indicator(cx))
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
                    .when(usable, |this| {
                        this.child(
                            Button::new("title-figure-meanings")
                                .small()
                                .ghost()
                                .compact()
                                .icon(IconName::BookOpen)
                                .tooltip("What the tags on every figure mean")
                                .on_click(move |_, window, cx| meanings.update(cx, |app, cx| app.open_figure_meanings(None, window, cx))),
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
                    )
                    .when(usable, |this| {
                        this.child(
                            Button::new("title-settings")
                                .small()
                                .ghost()
                                .compact()
                                .icon(IconName::Settings)
                                .tooltip("Open Settings in a pane")
                                .on_click(move |_, window, cx| {
                                    settings.update(cx, |workspace, cx| {
                                        let _ = workspace.open(Route::Settings, Intent::Open, window, cx);
                                    })
                                }),
                        )
                    }),
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
        let usable = self.app.update(cx, |app, _| {
            app.perf.begin_frame(app.route().slug());
            app.perf.log_window_info(window, is_dark);
            app.perf.log_summary_if_due(window);
            // The summary (histogram snapshot + file write) costs a few ms in a
            // debug build; keep it out of this frame's `build` figure.
            app.perf.restart_build_clock();
            app.is_opened() && !app.viewer_pending()
        });
        // One read for both bars: the title bar states the file's state, the
        // status bar names the file.
        let file = FileChrome::of(self.app.read(cx));
        // The content column's width is set in pixels rather than `flex_1()` on
        // purpose: with an auto width taffy sizes the whole screen from its
        // content on every pass of every ancestor (docs/perf.md §2).
        let content_width = window.viewport_size().width;
        let content_style = StyleRefinement::default().w(content_width).h_full().flex_none();
        // The workspace shows the screens as panes once there is a household
        // and someone looking; before that the content view shows Welcome or
        // the viewer gate itself, and there is no launcher.
        let content: AnyElement = if usable { self.workspace.clone().cached(content_style).into_any_element() } else { self.app.clone().cached(content_style).into_any_element() };
        let tree = v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            // Input counters only (no notify): they say in the log whether the
            // frames that happened were driven by the mouse or by something else.
            .on_mouse_move(cx.listener(|this, _, _, cx| this.app.read(cx).perf().count_mouse_move()))
            .on_scroll_wheel(cx.listener(|this, _, _, cx| this.app.read(cx).perf().count_wheel()))
            .child(self.render_title_bar(window.is_fullscreen(), &file, cx))
            // Cached views are laid out from the style given here (their
            // contents are not measured), so each gets a definite size.
            .when(usable, |this| this.child(self.launcher.clone().cached(StyleRefinement::default().w(content_width).h(LAUNCHER_HEIGHT).flex_none())))
            .child(h_flex().items_stretch().flex_1().min_h_0().child(content))
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
