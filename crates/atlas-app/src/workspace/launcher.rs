//! The pane launcher: the strip of screens under the title bar.
//!
//! The launcher is **not** navigation. In a workspace of panes there is no
//! "current page" to replace: each item creates or focuses a pane, and the
//! rest of the workspace stays exactly as it was. What an item does:
//!
//! | gesture | effect |
//! |---|---|
//! | click | focus the pane already showing that screen, else open one in the active stack |
//! | Shift-click | open another instance, even if one is open |
//! | right-click | a menu: open, open new instance, open right, open below, open in a new window, unpin |
//! | drag | a new pane in hand — drop it anywhere a pane can be dropped (a pane's centre or edge, the docking bands, a window edge) |
//! | drag onto another item | reorder the launcher |
//!
//! The strip is the person's to arrange: items can be reordered, unpinned
//! into the `…` overflow menu and pinned back, and restored to the default.
//! The arrangement is kept in `launcher.json` in the app's data directory
//! ([`LauncherConfig`]), independent of any household. The `+` button lists
//! every screen and opens the chosen one in the active stack.
//!
//! The launcher is its own cached view: it re-renders when the active pane's
//! destination changes (its highlight) or when its arrangement changes, not on
//! every notification of the app (see [`crate::shell`]).
//!
//! An item's right-click menu is a gpui-kit [`PopupMenu`] the launcher owns
//! itself ([`OpenMenu`]): built on the right-click, anchored at the pointer,
//! dropped when the menu dismisses. gpui-kit's `context_menu` wrapper keeps
//! its menu entity in an `Rc` cycle after dismissal, which the test harness's
//! leak check refuses; owning the state here avoids the cycle.

use std::path::PathBuf;

use atlas_workspace::persist;
use atlas_workspace::resolver::Intent;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dock::AnyDrag;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Icon, Selectable as _, Sizable as _, h_flex};
use gpui_kit::*;
use serde::{Deserialize, Serialize};

use super::view::WorkspaceView;
use crate::app::AtlasApp;
use crate::nav::{Destination, Route};

/// The height of the strip.
pub const LAUNCHER_HEIGHT: Pixels = px(36.);

/// The file the arrangement is kept in, inside the app's data directory.
pub const CONFIG_FILE: &str = "launcher.json";

/// What a drag from the launcher carries: the screen to open. Wrapped in the
/// dock engine's [`AnyDrag`] so its tab groups accept the drop and report it
/// through `DockEvent::DragDrop`; the workspace's own docking bands read the
/// same payload.
#[derive(Clone, Debug, PartialEq)]
pub struct LaunchDrag {
    pub route: Route,
    /// The launcher item the drag started from, for reordering the strip.
    pub destination: Destination,
}

/// The person's arrangement of the launcher: which screens are on the strip,
/// in what order, and which sit in the overflow menu.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LauncherConfig {
    /// Screens on the strip, by their home route's slug, in order.
    pub pinned: Vec<String>,
    /// Screens moved off the strip into the overflow menu.
    pub hidden: Vec<String>,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        LauncherConfig { pinned: Self::default_order().iter().map(|destination| destination.home().slug().to_string()).collect(), hidden: Vec::new() }
    }
}

impl LauncherConfig {
    /// The destinations in the order the app groups them; Settings lives in
    /// the title bar, so it is not a launcher item.
    pub fn default_order() -> Vec<Destination> {
        Destination::GROUPS.iter().flat_map(|group| group.iter().copied()).collect()
    }

    /// Reads the arrangement from `dir/launcher.json`, or the default when
    /// there is none or it cannot be read (which is logged, never fatal).
    /// Screens this build has that the file does not mention are appended to
    /// the strip, so a new screen shows up; slugs it does not know are dropped.
    pub fn load(dir: Option<&PathBuf>) -> Self {
        let mut config = match dir {
            Some(dir) => match persist::load_json::<LauncherConfig>(&dir.join(CONFIG_FILE)) {
                Ok(Some(config)) => config,
                Ok(None) => LauncherConfig::default(),
                Err(err) => {
                    log::warn!("launcher: {} could not be read ({err}); using the default arrangement", dir.join(CONFIG_FILE).display());
                    LauncherConfig::default()
                }
            },
            None => LauncherConfig::default(),
        };
        config.repair();
        config
    }

    /// Writes the arrangement to `dir/launcher.json`.
    pub fn save(&self, dir: &PathBuf) -> std::io::Result<()> {
        persist::save_atomic_json(&dir.join(CONFIG_FILE), self)
    }

    /// Drops slugs that are not screens and adds screens the lists miss.
    fn repair(&mut self) {
        let known = |slug: &String| destination_of_slug(slug).is_some();
        self.pinned.retain(known);
        self.hidden.retain(known);
        self.pinned.dedup();
        for destination in Self::default_order() {
            let slug = destination.home().slug().to_string();
            if !self.pinned.contains(&slug) && !self.hidden.contains(&slug) {
                self.pinned.push(slug);
            }
        }
    }

    /// The screens on the strip, in order.
    pub fn pinned_destinations(&self) -> Vec<Destination> {
        self.pinned.iter().filter_map(|slug| destination_of_slug(slug)).collect()
    }

    /// The screens in the overflow menu, in the app's order.
    pub fn hidden_destinations(&self) -> Vec<Destination> {
        Self::default_order().into_iter().filter(|destination| self.hidden.iter().any(|slug| slug == destination.home().slug())).collect()
    }

    /// Moves a screen off the strip into the overflow menu.
    pub fn unpin(&mut self, destination: Destination) {
        let slug = destination.home().slug().to_string();
        self.pinned.retain(|pinned| *pinned != slug);
        if !self.hidden.contains(&slug) {
            self.hidden.push(slug);
        }
    }

    /// Puts a screen back on the strip, at the end.
    pub fn pin(&mut self, destination: Destination) {
        let slug = destination.home().slug().to_string();
        self.hidden.retain(|hidden| *hidden != slug);
        if !self.pinned.contains(&slug) {
            self.pinned.push(slug);
        }
    }

    /// Moves `destination` on the strip to just before `before` (or to the
    /// end). Pins it first if it was hidden.
    pub fn move_before(&mut self, destination: Destination, before: Option<Destination>) {
        if destination == before.unwrap_or(destination) && before.is_some() {
            return;
        }
        let slug = destination.home().slug().to_string();
        self.hidden.retain(|hidden| *hidden != slug);
        self.pinned.retain(|pinned| *pinned != slug);
        let at = before.and_then(|before| self.pinned.iter().position(|pinned| pinned == before.home().slug())).unwrap_or(self.pinned.len());
        self.pinned.insert(at, slug);
    }

    /// Back to the app's own order, everything on the strip.
    pub fn reset(&mut self) {
        *self = LauncherConfig::default();
    }
}

/// The destination whose home route has this slug.
fn destination_of_slug(slug: &str) -> Option<Destination> {
    let route = Route::from_slug(slug)?;
    let destination = route.destination()?;
    (destination.home() == route && destination != Destination::Settings).then_some(destination)
}

/// What the launcher shows; it re-renders only when this changes.
#[derive(Clone, Debug, PartialEq)]
pub struct LauncherSnapshot {
    /// The active pane's destination, highlighted on the strip.
    pub destination: Option<Destination>,
    pub config: LauncherConfig,
}

/// An item's right-click menu while it is open.
struct OpenMenu {
    menu: Entity<PopupMenu>,
    position: Point<Pixels>,
    _dismiss: Subscription,
}

/// The launcher strip, as its own cached view.
pub struct LauncherView {
    workspace: Entity<WorkspaceView>,
    /// The right-click menu on show, if any.
    open_menu: Option<OpenMenu>,
    config: LauncherConfig,
    /// Where the arrangement is saved; `None` keeps it for this run only.
    data_dir: Option<PathBuf>,
    snapshot: LauncherSnapshot,
    /// Renders since creation — how tests see that a frame reused the cache.
    renders: u64,
    _observe_app: Subscription,
}

impl LauncherView {
    pub fn new(app: Entity<AtlasApp>, workspace: Entity<WorkspaceView>, data_dir: Option<PathBuf>, cx: &mut Context<Self>) -> Self {
        let config = LauncherConfig::load(data_dir.as_ref());
        let snapshot = LauncherSnapshot { destination: app.read(cx).destination(), config: config.clone() };
        // Every notify of the app lands here (hover, scroll, edits); only a
        // changed destination is worth a re-render.
        let _observe_app = cx.observe(&app, |this, app, cx| {
            let destination = app.read(cx).destination();
            if destination != this.snapshot.destination {
                this.snapshot.destination = destination;
                cx.notify();
            }
        });
        LauncherView { workspace, open_menu: None, config, data_dir, snapshot, renders: 0, _observe_app }
    }

    /// Opens `destination`'s right-click menu at `position`.
    fn open_item_menu(&mut self, destination: Destination, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        let workspace = self.workspace.clone();
        let pinned = self.config.pinned_destinations().contains(&destination);
        let menu = PopupMenu::build(window, cx, move |menu, _, _| Self::item_menu(this, workspace, destination, pinned, menu));
        let dismiss = cx.subscribe(&menu, |this, _, _: &DismissEvent, cx| {
            this.open_menu = None;
            cx.notify();
        });
        log::info!("launcher: menu for {} opened", destination.label());
        self.open_menu = Some(OpenMenu { menu, position, _dismiss: dismiss });
        cx.notify();
    }

    /// How many times the launcher has been rendered.
    pub fn renders(&self) -> u64 {
        self.renders
    }

    pub fn snapshot(&self) -> &LauncherSnapshot {
        &self.snapshot
    }

    /// The arrangement on show.
    pub fn config(&self) -> &LauncherConfig {
        &self.config
    }

    /// Applies a change to the arrangement, saves it and redraws.
    pub fn rearrange(&mut self, change: impl FnOnce(&mut LauncherConfig), cx: &mut Context<Self>) {
        change(&mut self.config);
        self.config.repair();
        if let Some(dir) = &self.data_dir {
            match self.config.save(dir) {
                Ok(()) => log::info!("launcher: arrangement saved to {}", dir.join(CONFIG_FILE).display()),
                Err(err) => log::warn!("launcher: arrangement could not be saved to {}: {err}", dir.join(CONFIG_FILE).display()),
            }
        }
        self.snapshot.config = self.config.clone();
        cx.notify();
    }

    /// Opens `destination`'s home screen the way `intent` asks, through the workspace.
    fn open(workspace: &Entity<WorkspaceView>, destination: Destination, intent: Intent, window: &mut Window, cx: &mut App) {
        log::info!("launcher: {} ({intent:?})", destination.label());
        workspace.update(cx, |workspace, cx| {
            let _ = workspace.open(destination.home(), intent, window, cx);
        });
    }

    /// The right-click menu of an item: every way to open the screen, and
    /// taking the item off the strip.
    fn item_menu(this: WeakEntity<Self>, workspace: Entity<WorkspaceView>, destination: Destination, pinned: bool, menu: PopupMenu) -> PopupMenu {
        let open = |intent: Intent| {
            let workspace = workspace.clone();
            move |_: &ClickEvent, window: &mut Window, cx: &mut App| Self::open(&workspace, destination, intent, window, cx)
        };
        let toggle_pin = {
            let this = this.clone();
            move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                let _ = this.update(cx, |launcher, cx| launcher.rearrange(|config| if pinned { config.unpin(destination) } else { config.pin(destination) }, cx));
            }
        };
        menu.item(PopupMenuItem::new("Open").icon(IconName::ExternalLink).on_click(open(Intent::Open)))
            .item(PopupMenuItem::new("Open new instance").icon(IconName::CopyPlus).on_click(open(Intent::NewInstance)))
            .item(PopupMenuItem::new("Open right").icon(IconName::PanelRight).on_click(open(Intent::OpenRight)))
            .item(PopupMenuItem::new("Open below").icon(IconName::PanelBottom).on_click(open(Intent::OpenBelow)))
            .item(PopupMenuItem::new("Open in new window").icon(IconName::AppWindow).on_click(open(Intent::OpenNewWindow)))
            .separator()
            .item(PopupMenuItem::new(if pinned { "Unpin from launcher" } else { "Pin to launcher" }).icon(if pinned { IconName::PinOff } else { IconName::Pin }).on_click(toggle_pin))
    }

    /// One item of the strip: a button that opens the screen, draggable as a
    /// new pane, with the right-click menu, and a drop target for reordering.
    fn render_item(&self, destination: Destination, active: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let slug = destination.home().slug();
        let workspace = self.workspace.clone();
        let reorder = cx.weak_entity();
        let theme = cx.theme();
        let chip_bg = theme.popover;
        let chip_fg = theme.popover_foreground;
        let chip_border = theme.border;
        div()
            .id(SharedString::from(format!("launcher-item-{slug}")))
            .test_support()
            .on_drag(AnyDrag::new(LaunchDrag { route: destination.home(), destination }), move |_, _, _, cx| {
                cx.new(|_| LaunchDragPreview { label: destination.label(), icon: destination.icon(), bg: chip_bg, fg: chip_fg, border: chip_border })
            })
            .on_drop(move |dropped: &AnyDrag, _, cx| {
                if let Some(launch) = dropped.value().downcast_ref::<LaunchDrag>() {
                    let moved = launch.destination;
                    let _ = reorder.update(cx, |launcher, cx| launcher.rearrange(|config| config.move_before(moved, Some(destination)), cx));
                }
            })
            .child(
                Button::new(SharedString::from(format!("launcher-{slug}")))
                    .ghost()
                    .compact()
                    .small()
                    .icon(destination.icon())
                    .label(destination.label())
                    .selected(active)
                    .tooltip(format!("Open {} (Shift for another instance, drag to place it)", destination.label()))
                    .on_click(move |event: &ClickEvent, window, cx| {
                        let intent = if event.modifiers().shift { Intent::NewInstance } else { Intent::Open };
                        Self::open(&workspace, destination, intent, window, cx);
                    }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_item_menu(destination, event.position, window, cx);
                }),
            )
    }

    /// The open right-click menu, anchored at the pointer and above everything.
    fn render_open_menu(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let open = self.open_menu.as_ref()?;
        let menu = open.menu.clone();
        if !menu.focus_handle(cx).contains_focused(window, cx) {
            menu.focus_handle(cx).focus(window, cx);
        }
        Some(deferred(anchored().position(open.position).snap_to_window_with_margin(px(8.)).child(menu)).with_priority(gpui_kit::base::POPUP_PRIORITY).into_any_element())
    }

    /// The `+` button: every screen, opened in the active stack.
    fn render_add(&self) -> impl IntoElement {
        let workspace = self.workspace.clone();
        Button::new("launcher-add").ghost().compact().small().icon(IconName::Plus).tooltip("Add a pane").dropdown_menu(move |menu, _, _| {
            LauncherConfig::default_order().into_iter().chain(std::iter::once(Destination::Settings)).fold(menu, |menu, destination| {
                let workspace = workspace.clone();
                menu.item(PopupMenuItem::new(destination.label()).icon(destination.icon()).on_click(move |_, window, cx| Self::open(&workspace, destination, Intent::Open, window, cx)))
            })
        })
    }

    /// The `…` button: the screens taken off the strip, and the way back to the default.
    fn render_more(&self, hidden: Vec<Destination>, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.workspace.clone();
        let this = cx.weak_entity();
        Button::new("launcher-more").ghost().compact().small().icon(IconName::Ellipsis).tooltip("Screens not on the launcher, and the launcher's arrangement").dropdown_menu(move |menu, _, _| {
            let mut menu = hidden.iter().fold(menu, |menu, destination| {
                let destination = *destination;
                let workspace = workspace.clone();
                let this = this.clone();
                menu.item(PopupMenuItem::new(destination.label()).icon(destination.icon()).on_click(move |_, window, cx| Self::open(&workspace, destination, Intent::Open, window, cx))).item(
                    PopupMenuItem::new(format!("Pin {} to the launcher", destination.label())).icon(IconName::Pin).on_click(move |_, _, cx| {
                        let _ = this.update(cx, |launcher, cx| launcher.rearrange(|config| config.pin(destination), cx));
                    }),
                )
            });
            if !hidden.is_empty() {
                menu = menu.separator();
            }
            let this = this.clone();
            menu.item(PopupMenuItem::new("Restore the default launcher").icon(IconName::RotateCcw).on_click(move |_, _, cx| {
                let _ = this.update(cx, |launcher, cx| launcher.rearrange(LauncherConfig::reset, cx));
            }))
        })
    }
}

impl Render for LauncherView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders += 1;
        let open_menu = self.render_open_menu(window, cx);
        let active = self.snapshot.destination;
        let pinned = self.config.pinned_destinations();
        let hidden = self.config.hidden_destinations();
        let theme = cx.theme();
        let border = theme.border;
        let background = theme.title_bar;
        let items: Vec<AnyElement> = pinned.into_iter().map(|destination| self.render_item(destination, active == Some(destination), cx).into_any_element()).collect();
        h_flex()
            .id("launcher")
            .test_support()
            .w_full()
            .h(LAUNCHER_HEIGHT)
            .flex_none()
            .items_center()
            .px_2()
            .gap_1()
            .bg(background)
            .border_b_1()
            .border_color(border)
            .child(self.render_add())
            .children(items)
            .child(div().flex_1())
            .child(self.render_more(hidden, cx))
            .children(open_menu)
    }
}

/// The chip that follows the pointer while a screen is dragged off the launcher.
struct LaunchDragPreview {
    label: &'static str,
    icon: IconName,
    bg: Hsla,
    fg: Hsla,
    border: Hsla,
}

impl Render for LaunchDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        h_flex().gap_2().items_center().px_3().py_1().rounded(px(6.)).bg(self.bg).text_color(self.fg).border_1().border_color(self.border).text_sm().child(Icon::new(self.icon).small()).child(self.label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn the_default_arrangement_lists_every_destination_but_settings_in_order() {
        let config = LauncherConfig::default();
        assert_eq!(config.pinned_destinations(), LauncherConfig::default_order());
        assert!(!config.pinned_destinations().contains(&Destination::Settings));
        assert!(config.hidden.is_empty());
    }

    #[core::prelude::v1::test]
    fn pin_unpin_reorder_and_reset_keep_every_screen_exactly_once() {
        let mut config = LauncherConfig::default();
        config.unpin(Destination::Sharing);
        assert_eq!(config.hidden_destinations(), vec![Destination::Sharing]);
        assert!(!config.pinned_destinations().contains(&Destination::Sharing));
        config.move_before(Destination::Accounts, Some(Destination::Today));
        assert_eq!(config.pinned_destinations()[0], Destination::Accounts);
        assert_eq!(config.pinned_destinations()[1], Destination::Today);
        // Moving a hidden screen onto the strip pins it there.
        config.move_before(Destination::Sharing, None);
        assert_eq!(config.pinned_destinations().last(), Some(&Destination::Sharing));
        assert!(config.hidden.is_empty());
        config.pin(Destination::Sharing);
        assert_eq!(config.pinned.iter().filter(|slug| *slug == "policies").count(), 1, "pinning twice does not duplicate");
        config.reset();
        assert_eq!(config, LauncherConfig::default());
    }

    #[core::prelude::v1::test]
    fn a_file_with_unknown_and_missing_screens_is_repaired_on_load() {
        let dir = std::env::temp_dir().join(format!("atlas-launcher-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut stale = LauncherConfig { pinned: vec!["forecast".into(), "no-such-screen".into(), "today".into()], hidden: vec!["accounts".into(), "settings".into()] };
        stale.save(&dir).unwrap();
        let loaded = LauncherConfig::load(Some(&dir));
        assert_eq!(loaded.pinned_destinations()[..2], [Destination::Forecast, Destination::Today], "the order is kept and the unknown slug dropped");
        assert_eq!(loaded.hidden_destinations(), vec![Destination::Accounts], "settings is not a launcher screen");
        for destination in LauncherConfig::default_order() {
            let on_strip = loaded.pinned_destinations().contains(&destination);
            let in_menu = loaded.hidden_destinations().contains(&destination);
            assert!(on_strip ^ in_menu, "{destination:?} appears exactly once");
        }
        // No file, or no directory: the default.
        assert_eq!(LauncherConfig::load(Some(&dir.join("missing"))), LauncherConfig::default());
        assert_eq!(LauncherConfig::load(None), LauncherConfig::default());
        // A broken file: the default, not a crash.
        std::fs::write(dir.join(CONFIG_FILE), b"{ not json").unwrap();
        assert_eq!(LauncherConfig::load(Some(&dir)), LauncherConfig::default());
        stale.reset();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
