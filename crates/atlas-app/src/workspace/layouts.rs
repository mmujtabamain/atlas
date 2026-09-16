//! Saved layouts and templates: arrangements the person keeps on purpose.
//!
//! A **session** is what is open right now and follows the person around
//! without being asked ([`super::session`]). A **saved layout** is an
//! arrangement given a name — "Monthly review", "Tax planning" — kept in the
//! app's data directory (`layouts/`, one file per entry, see
//! [`atlas_workspace::saved`]) and loaded back on request. It belongs to the
//! household it was saved in, because its panes name that household's
//! records. A **template** is the same arrangement without the records: the
//! shape and the kinds of screens, usable in any household. The built-in
//! presets ([`atlas_workspace::Preset`]) are templates the app ships with.
//!
//! What the Layout menu offers, and what each does to the workspace:
//!
//! | command | effect |
//! |---|---|
//! | Save layout | writes the current arrangement under its current name (or asks for one) |
//! | Save layout as… | asks for a name and writes a new entry |
//! | Save as template… | the shape only, under a name; usable in any household |
//! | a saved layout | replaces the workspace with it, as one undoable step (the previous arrangement is one Undo away) |
//! | Open in new window | a saved layout's main window opens as a floating window beside the current workspace |
//! | a preset or a template | re-arranges the panes already open into that shape; a slot named after a screen that is not open gets a new pane of it |
//! | Manage layouts… | rename, duplicate, delete |
//!
//! Loading never silently drops a pane: a pane whose record is gone shows the
//! unavailable placeholder, a pane of a kind this build does not know shows
//! the unsupported one, and both offer Replace or Close (see [`super::pane`]).

use std::path::{Path, PathBuf};

use atlas_workspace::saved::{SavedError, SavedKind, SavedLayout, SavedLayouts};
use atlas_workspace::{LayoutTemplate, Preset, Scope, WindowId, WorkspaceLayout};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::form::{Field, Form};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::view::WorkspaceView;
use crate::nav::Route;

/// The directory under the data directory that holds saved layouts.
pub const LAYOUTS_DIR: &str = "layouts";

/// The store of saved layouts, opened on first use; `None` without a data
/// directory (nothing is kept) or when the directory cannot be used.
pub struct LayoutStore {
    dir: Option<PathBuf>,
    store: Option<SavedLayouts>,
    /// The saved entry the current arrangement came from or was saved as,
    /// so `Save layout` knows where to write.
    pub current: Option<(String, String)>,
}

impl LayoutStore {
    pub fn new(data_dir: Option<&Path>) -> Self {
        LayoutStore { dir: data_dir.map(|dir| dir.join(LAYOUTS_DIR)), store: None, current: None }
    }

    /// The store, opened if it was not yet.
    pub fn open(&mut self) -> Option<&mut SavedLayouts> {
        if self.store.is_none() {
            let dir = self.dir.clone()?;
            match SavedLayouts::open(&dir) {
                Ok(store) => self.store = Some(store),
                Err(err) => {
                    log::warn!("layouts: {} could not be opened: {err}", dir.display());
                    return None;
                }
            }
        }
        self.store.as_mut()
    }

    /// Whether layouts can be kept at all.
    pub fn persists(&self) -> bool {
        self.dir.is_some()
    }

    /// The saved entries, layouts before templates, each by name.
    pub fn entries(&mut self) -> Vec<SavedLayout> {
        let Some(store) = self.open() else {
            return Vec::new();
        };
        let mut entries: Vec<SavedLayout> = store.list().into_iter().cloned().collect();
        entries.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        entries
    }
}

/// Whether a template slot names a screen this build can open (so a slot
/// nothing fills becomes a new pane of it).
pub fn slot_is_screen(slot: &str) -> bool {
    Route::from_slug(slot).is_some_and(|route| route != Route::Welcome)
}

/// What the person reads when a saved-layout command is refused.
pub fn describe_error(err: &SavedError) -> String {
    match err {
        SavedError::NameTaken { name, .. } => format!("There is already a layout called “{name}”. Choose another name."),
        SavedError::WrongHousehold { name, .. } => format!("“{name}” was saved in another household; it names that household's accounts. Save it as a template to use its shape here."),
        other => other.to_string(),
    }
}

/// The name a template gets from a layout's name.
pub fn template_name(name: &str) -> String {
    format!("{name} (template)")
}

impl WorkspaceView {
    /// The saved entries to list in the Layout menu.
    pub fn saved_layouts(&mut self) -> Vec<SavedLayout> {
        self.layouts.entries()
    }

    /// The name the current arrangement was saved under or loaded from.
    pub fn current_layout_name(&self) -> Option<&str> {
        self.layouts.current.as_ref().map(|(_, name)| name.as_str())
    }

    /// Whether saved layouts can be kept at all (there is a data directory).
    pub fn can_save_layouts(&self) -> bool {
        self.layouts.persists()
    }

    /// Writes an arbitrary layout as a saved entry — how tests plant a stale
    /// or foreign layout to load.
    pub fn store_layout_as(&mut self, name: &str, layout: &WorkspaceLayout) -> Result<String, SavedError> {
        let store = self.layouts.open().ok_or_else(|| SavedError::Io(std::io::Error::other("layouts are not kept without a data directory")))?;
        store.save_as(name, layout)
    }

    /// `Save layout`: writes the current arrangement under its current name,
    /// or asks for one.
    pub fn save_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.layouts.current.clone() {
            Some((id, name)) => {
                let layout = self.layout_for_saving(cx);
                let result = self.layouts.open().ok_or_else(|| SavedError::Io(std::io::Error::other("layouts are not kept without a data directory"))).and_then(|store| store.update(&id, &layout));
                match result {
                    Ok(()) => {
                        log::info!("layouts: saved '{name}' ({id})");
                        window.push_notification(format!("Layout “{name}” saved"), cx);
                    }
                    Err(err) => self.refuse_layout_command(&err, window, cx),
                }
            }
            None => self.prompt_save_layout_as(false, window, cx),
        }
    }

    /// `Save layout as…`: asks for a name and writes a new entry under it.
    pub fn prompt_save_layout_as(&mut self, as_template: bool, window: &mut Window, cx: &mut Context<Self>) {
        let suggested = self.layouts.current.as_ref().map(|(_, name)| if as_template { template_name(name) } else { name.clone() }).unwrap_or_default();
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(if as_template { "e.g. Review (template)" } else { "e.g. Monthly review" }));
        input.update(cx, |state, cx| state.set_value(suggested, window, cx));
        let this = cx.weak_entity();
        let value = input.clone();
        window.open_dialog(cx, move |dialog, _, cx| {
            let this = this.clone();
            let value = value.clone();
            let muted = cx.theme().muted_foreground;
            dialog
                .title(if as_template { "Save as template" } else { "Save layout as" })
                .w_96()
                .child(
                    v_flex()
                        .gap_3()
                        .child(Form::vertical().child(Field::new().label("Name").required(true).child(Input::new(&input).id("layout-name"))))
                        .child(div().text_xs().text_color(muted).child(if as_template {
                            "A template keeps the arrangement and the kinds of screens, not the accounts or people, so it works in any household."
                        } else {
                            "A saved layout keeps the panes, their records and their arrangement for this household."
                        })),
                )
                .footer(
                    DialogFooter::new()
                        .child(Button::new("layout-save-cancel").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                        .child(Button::new("layout-save-confirm").primary().label(if as_template { "Save template" } else { "Save layout" }).on_click(move |_, window, cx| {
                            let name = value.read(cx).value().trim().to_string();
                            if name.is_empty() {
                                window.push_notification("Name the layout first.", cx);
                                return;
                            }
                            window.close_dialog(cx);
                            let _ = this.update(cx, |workspace, cx| {
                                if as_template {
                                    workspace.save_template(&name, window, cx);
                                } else {
                                    workspace.save_layout_as(&name, window, cx);
                                }
                            });
                        })),
                )
        });
    }

    /// Writes the current arrangement as a new saved layout called `name`.
    pub fn save_layout_as(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let layout = self.layout_for_saving(cx);
        let result = self.layouts.open().ok_or_else(|| SavedError::Io(std::io::Error::other("layouts are not kept without a data directory"))).and_then(|store| store.save_as(name, &layout));
        match result {
            Ok(id) => {
                log::info!("layouts: saved '{name}' as {id}");
                self.layouts.current = Some((id, name.to_string()));
                window.push_notification(format!("Layout “{name}” saved"), cx);
                cx.notify();
            }
            Err(err) => self.refuse_layout_command(&err, window, cx),
        }
    }

    /// Writes the shape of the main window as a template called `name`.
    pub fn save_template(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let template = match self.layout.to_template_named(&WindowId::main(), name) {
            Ok(template) => template,
            Err(err) => {
                self.report(&Err::<(), _>(err), window, cx);
                return;
            }
        };
        let result = self.layouts.open().ok_or_else(|| SavedError::Io(std::io::Error::other("layouts are not kept without a data directory"))).and_then(|store| store.save_template(name, &template));
        match result {
            Ok(id) => {
                log::info!("layouts: template '{name}' saved as {id} ({} slots)", template.slots.len());
                window.push_notification(format!("Template “{name}” saved"), cx);
                cx.notify();
            }
            Err(err) => self.refuse_layout_command(&err, window, cx),
        }
    }

    /// Replaces the workspace with a saved layout (one Undo away), or
    /// re-arranges the open panes by a saved template.
    pub fn load_saved_layout(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let scope = self.layout.scope.clone();
        let entry = self.layouts.open().and_then(|store| store.get(id).cloned());
        let Some(entry) = entry else {
            window.push_notification("That layout no longer exists.", cx);
            return;
        };
        match entry.kind {
            SavedKind::Template => {
                if let Some(template) = entry.template.clone() {
                    self.apply_template(&template, window, cx);
                }
            }
            SavedKind::Layout => {
                let loaded = self.layouts.open().ok_or_else(|| SavedError::Io(std::io::Error::other("layouts are not kept without a data directory"))).and_then(|store| store.load_into(id, &scope));
                match loaded {
                    Ok(mut layout) => {
                        layout.set_limits(*self.layout.limits());
                        let name = entry.name.clone();
                        log::info!("layouts: loading '{name}' ({id}) in place of the current workspace");
                        self.load_layout(layout, format!("Load layout {name}"), window, cx);
                        self.layouts.current = Some((id.to_string(), name.clone()));
                        window.push_notification(format!("Layout “{name}” loaded — Undo brings the previous arrangement back"), cx);
                        cx.notify();
                    }
                    Err(err) => self.refuse_layout_command(&err, window, cx),
                }
            }
        }
    }

    /// Opens a saved layout's main window as a floating window beside the
    /// current workspace, with fresh pane ids so nothing collides.
    pub fn open_saved_layout_in_new_window(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let scope = self.layout.scope.clone();
        let entry = self.layouts.open().and_then(|store| store.get(id).cloned());
        let Some(entry) = entry else {
            window.push_notification("That layout no longer exists.", cx);
            return;
        };
        let loaded = self.layouts.open().ok_or_else(|| SavedError::Io(std::io::Error::other("layouts are not kept without a data directory"))).and_then(|store| store.load_into(id, &scope));
        let source = match loaded {
            Ok(source) => source,
            Err(err) => {
                self.refuse_layout_command(&err, window, cx);
                return;
            }
        };
        let before = self.layout.clone();
        let frame = self.floating_frame(None, window);
        match self.layout.import_window(&source, &WindowId::main(), frame) {
            Ok(new_window) => {
                self.sync_pane_entities(cx);
                self.record(format!("Open layout {} in a new window", entry.name), before, cx);
                log::info!("layouts: '{}' opened as floating window {new_window}", entry.name);
                self.open_floating_window(&new_window, frame, cx);
                self.rebuild_area(window, cx);
                self.sync_chrome(cx);
            }
            Err(err) => self.report(&Err::<(), _>(err), window, cx),
        }
    }

    /// Re-arranges the main window's panes into a preset's shape.
    pub fn apply_preset(&mut self, preset: Preset, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_template(&preset.template(), window, cx);
    }

    /// Re-arranges the main window's panes into `template`'s shape, as one
    /// undoable step. Slots nothing fills that name a screen get a new pane.
    pub fn apply_template(&mut self, template: &LayoutTemplate, window: &mut Window, cx: &mut Context<Self>) {
        let before = self.layout.clone();
        match self.layout.apply_template(&WindowId::main(), template, slot_is_screen) {
            Ok(created) => {
                self.sync_pane_entities(cx);
                self.record(format!("Arrange as {}", template.name), before, cx);
                log::info!("layouts: main window arranged as '{}' ({} panes created)", template.name, created.len());
                self.rebuild_area(window, cx);
                self.focus_active(window, cx);
                self.sync_chrome(cx);
            }
            Err(err) => self.report(&Err::<(), _>(err), window, cx),
        }
    }

    /// Renames a saved entry.
    pub fn rename_saved_layout(&mut self, id: &str, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.layouts.open().ok_or_else(|| SavedError::Io(std::io::Error::other("layouts are not kept without a data directory"))).and_then(|store| store.rename(id, name));
        match result {
            Ok(()) => {
                if let Some((current, current_name)) = self.layouts.current.as_mut()
                    && current == id
                {
                    *current_name = name.to_string();
                }
                log::info!("layouts: {id} renamed to '{name}'");
                cx.notify();
            }
            Err(err) => self.refuse_layout_command(&err, window, cx),
        }
    }

    /// Duplicates a saved entry.
    pub fn duplicate_saved_layout(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.layouts.open().ok_or_else(|| SavedError::Io(std::io::Error::other("layouts are not kept without a data directory"))).and_then(|store| store.duplicate(id));
        match result {
            Ok(copy) => {
                log::info!("layouts: {id} duplicated as {copy}");
                cx.notify();
            }
            Err(err) => self.refuse_layout_command(&err, window, cx),
        }
    }

    /// Deletes a saved entry.
    pub fn delete_saved_layout(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.layouts.open().ok_or_else(|| SavedError::Io(std::io::Error::other("layouts are not kept without a data directory"))).and_then(|store| store.delete(id));
        match result {
            Ok(entry) => {
                if self.layouts.current.as_ref().is_some_and(|(current, _)| current == id) {
                    self.layouts.current = None;
                }
                log::info!("layouts: '{}' ({id}) deleted", entry.name);
                window.push_notification(format!("Layout “{}” deleted", entry.name), cx);
                cx.notify();
            }
            Err(err) => self.refuse_layout_command(&err, window, cx),
        }
    }

    /// The manage dialog: every saved layout and template with rename,
    /// duplicate and delete, and the two ways to open a layout.
    pub fn open_manage_layouts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let entries = self.saved_layouts();
        let this = cx.weak_entity();
        let rename_input = cx.new(|cx| InputState::new(window, cx).placeholder("new name"));
        window.open_dialog(cx, move |dialog, _, cx| {
            let this = this.clone();
            let rename_input = rename_input.clone();
            let muted = cx.theme().muted_foreground;
            let mut list = v_flex().gap_2().w_full();
            if entries.is_empty() {
                list = list.child(div().text_sm().text_color(muted).child("No saved layouts yet. Use Save layout as… in the Layout menu."));
            }
            for entry in &entries {
                let id = entry.id.clone();
                let kind = match entry.kind {
                    SavedKind::Layout => "layout",
                    SavedKind::Template => "template",
                };
                let label = h_flex().flex_1().min_w_0().gap_2().items_center().child(div().text_sm().child(entry.name.clone())).child(div().text_xs().text_color(muted).child(kind));
                let load = {
                    let this = this.clone();
                    let id = id.clone();
                    move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                        window.close_dialog(cx);
                        let _ = this.update(cx, |workspace, cx| workspace.load_saved_layout(&id, window, cx));
                    }
                };
                let in_window = {
                    let this = this.clone();
                    let id = id.clone();
                    move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                        window.close_dialog(cx);
                        let _ = this.update(cx, |workspace, cx| workspace.open_saved_layout_in_new_window(&id, window, cx));
                    }
                };
                let rename = {
                    let this = this.clone();
                    let id = id.clone();
                    let input = rename_input.clone();
                    move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                        let name = input.read(cx).value().trim().to_string();
                        if name.is_empty() {
                            window.push_notification("Type the new name in the field below first.", cx);
                            return;
                        }
                        window.close_dialog(cx);
                        let _ = this.update(cx, |workspace, cx| workspace.rename_saved_layout(&id, &name, window, cx));
                    }
                };
                let duplicate = {
                    let this = this.clone();
                    let id = id.clone();
                    move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                        window.close_dialog(cx);
                        let _ = this.update(cx, |workspace, cx| workspace.duplicate_saved_layout(&id, window, cx));
                    }
                };
                let delete = {
                    let this = this.clone();
                    let id = id.clone();
                    move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                        window.close_dialog(cx);
                        let _ = this.update(cx, |workspace, cx| workspace.delete_saved_layout(&id, window, cx));
                    }
                };
                let is_layout = entry.kind == SavedKind::Layout;
                list = list.child(
                    h_flex()
                        .id(SharedString::from(format!("saved-layout-{id}")))
                        .w_full()
                        .gap_2()
                        .items_center()
                        .child(label)
                        .child(Button::new(SharedString::from(format!("layout-load-{id}"))).small().outline().label(if is_layout { "Load" } else { "Arrange" }).on_click(load))
                        .when(is_layout, |this| this.child(Button::new(SharedString::from(format!("layout-window-{id}"))).small().ghost().label("New window").on_click(in_window)))
                        .child(Button::new(SharedString::from(format!("layout-rename-{id}"))).small().ghost().label("Rename").on_click(rename))
                        .child(Button::new(SharedString::from(format!("layout-duplicate-{id}"))).small().ghost().label("Duplicate").on_click(duplicate))
                        .child(Button::new(SharedString::from(format!("layout-delete-{id}"))).small().ghost().label("Delete").on_click(delete)),
                );
            }
            dialog
                .title("Saved layouts")
                .w(px(640.))
                .child(
                    v_flex()
                        .gap_3()
                        .child(list)
                        .child(Form::vertical().child(Field::new().label("New name (for Rename)").child(Input::new(&rename_input).id("layout-rename-name"))))
                        .child(div().text_xs().text_color(muted).child("Load replaces the workspace (Undo brings the previous arrangement back). Arrange re-shapes the panes already open. New window opens the layout beside the workspace.")),
                )
                .footer(DialogFooter::new().child(Button::new("layout-manage-close").outline().label("Close").on_click(|_, window, cx| window.close_dialog(cx))))
        });
    }

    /// The layout as it is saved: scoped, without transient window frames.
    fn layout_for_saving(&self, cx: &App) -> WorkspaceLayout {
        let _ = cx;
        let mut copy = self.layout.clone();
        if copy.scope.is_global() {
            copy.scope = Scope::household(self.app.read(cx).household_identity());
        }
        copy
    }

    /// Tells the person why a saved-layout command was refused.
    fn refuse_layout_command(&self, err: &SavedError, window: &mut Window, cx: &mut Context<Self>) {
        log::warn!("layouts: refused: {err}");
        window.push_notification(gpui_kit::component::notification::Notification::warning(describe_error(err)), cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn slots_that_name_screens_are_recognised_and_errors_read_well() {
        assert!(slot_is_screen("today"));
        assert!(slot_is_screen("accounts"));
        assert!(!slot_is_screen("main"));
        assert!(!slot_is_screen("welcome"));
        assert_eq!(template_name("Review"), "Review (template)");
        assert!(describe_error(&SavedError::NameTaken { kind: SavedKind::Layout, name: "Review".into() }).contains("already"));
    }
}
