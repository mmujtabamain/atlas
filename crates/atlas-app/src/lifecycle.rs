//! Household lifecycle (M12.2) and identity (M12.3): new / open / save /
//! save-as / load-sample, the dirty state, the lock, and the "who is looking?"
//! picker. Native file dialogs are used when the platform has them; a path
//! field always works (the Linux box has no portal).

use std::path::PathBuf;

use atlas_core::authz::Viewer;
use atlas_core::fixtures;
use atlas_core::ids::{EntityRef, PersonId};
use atlas_core::model::{Household, HouseholdRole, Person};
use atlas_core::Currency;
use atlas_store::{HouseholdFile, StoreError};
use chrono::NaiveDate;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, IndexPath, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _, DropdownButton},
    date_picker::{DatePicker, DatePickerState},
    dialog::DialogFooter,
    form::{Field, Form},
    h_flex,
    input::{Input, InputState},
    menu::PopupMenuItem,
    select::{Select, SelectState},
    v_flex,
};
use gpui_kit::*;

use crate::alerting::{self, Level};
use crate::app::AtlasApp;
use crate::launch::{Launch, Start};

/// Retained state of the lifecycle dialogs.
pub struct LifecycleForm {
    pub path: Entity<InputState>,
    pub name: Entity<InputState>,
    pub first_person: Entity<InputState>,
    pub currency: Entity<SelectState<Vec<SharedString>>>,
    pub as_of: Entity<DatePickerState>,
}

impl LifecycleForm {
    pub fn new(window: &mut Window, cx: &mut Context<AtlasApp>) -> Self {
        let codes: Vec<SharedString> = Currency::KNOWN.iter().map(|(code, _)| SharedString::from(*code)).collect();
        LifecycleForm {
            path: cx.new(|cx| InputState::new(window, cx).placeholder("/path/to/household.atlas.sqlite")),
            name: cx.new(|cx| InputState::new(window, cx).placeholder("e.g. Our household")),
            first_person: cx.new(|cx| InputState::new(window, cx).placeholder("your name")),
            currency: cx.new(|cx| SelectState::new(codes, Some(IndexPath::default()), window, cx)),
            as_of: cx.new(|cx| DatePickerState::new(window, cx).date_format("%d %b %Y")),
        }
    }
}

/// How the app resolved its starting household.
pub struct Resolved {
    pub household: Household,
    pub file: Option<HouseholdFile>,
    pub notices: Vec<String>,
}

impl AtlasApp {
    /// Resolves `--household / --new / --sample` into a household and file.
    pub fn resolve_start(launch: &Launch, owner: &str) -> Resolved {
        let mut notices = Vec::new();
        match &launch.start {
            Start::Sample => Resolved { household: fixtures::plan_household(), file: None, notices },
            Start::Empty => Resolved {
                household: Household::empty("New household", Currency::USD, launch.as_of.unwrap_or_else(|| chrono::Local::now().date_naive())),
                file: None,
                notices,
            },
            Start::File(path) => {
                let file = HouseholdFile::new(path);
                if file.exists() {
                    match file.acquire(owner, launch.take_over) {
                        Ok(_) => {}
                        Err(StoreError::Locked { owner: other, since, .. }) => {
                            notices.push(format!("{} is open by {other} since {since}; opened read-only until you take it over from the Household menu.", path.display()));
                        }
                        Err(err) => notices.push(format!("Could not lock {}: {err}", path.display())),
                    }
                    match file.load() {
                        Ok(household) => Resolved { household, file: Some(file), notices },
                        Err(err) => {
                            alerting::report(Level::Error, format!("failed to load household file {}: {err}", path.display()));
                            notices.push(format!("Could not load {}: {err}. Showing the sample instead.", path.display()));
                            Resolved { household: fixtures::plan_household(), file: None, notices }
                        }
                    }
                } else {
                    let household = Household::empty("New household", Currency::USD, launch.as_of.unwrap_or_else(|| chrono::Local::now().date_naive()));
                    match file.save(&household).and_then(|_| file.acquire(owner, true).map(|_| ())) {
                        Ok(()) => notices.push(format!("Created {}.", path.display())),
                        Err(err) => {
                            alerting::report(Level::Error, format!("failed to create household file {}: {err}", path.display()));
                            notices.push(format!("Could not create {}: {err}", path.display()));
                        }
                    }
                    Resolved { household, file: Some(file), notices }
                }
            }
        }
    }

    /// Replaces the household (after new / open / sample), rebuilds every form
    /// that snapshots household data, recomputes, and asks who is looking.
    pub fn replace_household(&mut self, household: Household, file: Option<HouseholdFile>, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(old) = self.file.take() {
            let _ = old.release(&self.owner);
        }
        self.household = household;
        self.file = file;
        self.dirty = false;
        self.viewer = Viewer::person(self.household.people.first().map(|p| p.id).unwrap_or(PersonId::new(0)));
        self.selected_account = None;
        self.selected_company = None;
        self.selected_person = None;
        self.rebuild_forms(window, cx);
        self.refresh_derived();
        cx.notify();
        if self.household.people.len() > 1 {
            cx.defer_in(window, |this, window, cx| this.open_viewer_picker(window, cx));
        }
    }

    /// Records an unsaved change (every mutation calls this). The edit count
    /// lets a save that finishes in the background tell whether the household
    /// changed while it was writing.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        self.edits += 1;
    }

    /// Whether a save is in flight (the status bar says so).
    pub fn is_saving(&self) -> bool {
        self.saving
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn file_path(&self) -> Option<PathBuf> {
        self.file.as_ref().map(|f| f.path().to_path_buf())
    }

    /// Saves to the current file, or asks for one.
    pub fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.file.clone() {
            Some(file) => self.save_in_background(file, false, window, cx),
            None => self.open_save_as(window, cx),
        }
    }

    /// Writes the household to `file` on a background thread — the SQLite
    /// rewrite (backup copy, sixteen tables, commit) is I/O the UI must not
    /// wait on — and finishes on the foreground: `dirty` clears only if
    /// nothing was edited while the file was being written, and a save-as
    /// (`take_over`) acquires the lock and becomes the current file.
    fn save_in_background(&mut self, file: HouseholdFile, take_over: bool, _window: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            log::info!("save requested while a save is in flight; ignored");
            return;
        }
        let household = self.household.clone();
        let owner = self.owner.clone();
        let edits = self.edits;
        self.saving = true;
        cx.notify();
        log::info!("saving {} to {} in the background", household.name, file.path().display());
        let started = std::time::Instant::now();
        let write = cx.background_spawn(async move {
            let result = file.save(&household).and_then(|_| if take_over { file.acquire(&owner, true).map(|_| ()) } else { Ok(()) });
            (file, result)
        });
        cx.spawn(async move |this, cx| {
            let (file, result) = write.await;
            let _ = this.update_in(cx, |app, window, cx| app.finish_save(file, result, take_over, edits, started.elapsed(), window, cx));
        })
        .detach();
    }

    fn finish_save(&mut self, file: HouseholdFile, result: Result<(), StoreError>, take_over: bool, edits: u64, took: std::time::Duration, window: &mut Window, cx: &mut Context<Self>) {
        self.saving = false;
        match result {
            Ok(()) => {
                log::info!("perf: save of {} took {:.1}ms off the UI thread", file.path().display(), crate::perf::ms(took));
                if take_over {
                    if let Some(old) = self.file.take()
                        && old.path() != file.path()
                    {
                        let _ = old.release(&self.owner);
                    }
                    self.file = Some(file.clone());
                }
                if self.edits == edits {
                    self.dirty = false;
                } else {
                    log::info!("household edited while saving; it stays unsaved");
                }
                window.push_notification(format!("Saved to {}", file.path().display()), cx);
            }
            Err(err) => {
                alerting::report(Level::Error, format!("save failed for {}: {err}", file.path().display()));
                window.push_notification(format!("Couldn’t save: {err}"), cx);
            }
        }
        cx.notify();
    }

    /// Saves under a new path (the field is prefilled with a sensible default).
    pub fn open_save_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let suggested = self
            .file_path()
            .unwrap_or_else(|| atlas_store::default_folder().join(format!("{}.atlas.sqlite", slug(&self.household.name))));
        let path_state = self.lifecycle_form.path.clone();
        path_state.update(cx, |state, cx| state.set_value(suggested.display().to_string(), window, cx));
        let this = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let this = this.clone();
            let browse = this.clone();
            dialog
                .title("Save household as…")
                .w_96()
                .child(
                    v_flex()
                        .gap_3()
                        .child(Form::vertical().child(Field::new().label("File (SQLite, one editor at a time)").child(Input::new(&path_state).id("save-as-path"))))
                        .child(
                            h_flex().gap_2().child(
                                Button::new("browse-save").small().outline().icon(IconName::FolderOpen).label("Browse…").on_click(move |_, window, cx| {
                                    let _ = browse.update(cx, |app, cx| app.browse_for_new_path(window, cx));
                                }),
                            ),
                        ),
                )
                .footer(
                    DialogFooter::new()
                        .child(Button::new("cancel-save-as").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                        .child(Button::new("confirm-save-as").primary().label("Save").on_click(move |_, window, cx| {
                            let _ = this.update(cx, |app, cx| app.save_as_from_form(window, cx));
                        })),
                )
        });
    }

    fn save_as_from_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.lifecycle_form.path.read(cx).value().trim().to_string();
        if text.is_empty() {
            window.push_notification("Enter a file path.", cx);
            return;
        }
        let file = HouseholdFile::new(PathBuf::from(text));
        window.close_dialog(cx);
        self.save_in_background(file, true, window, cx);
    }

    /// Opens a household file (path field + native browse).
    pub fn open_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path_state = self.lifecycle_form.path.clone();
        let default = atlas_store::default_folder().display().to_string();
        path_state.update(cx, |state, cx| state.set_value(default, window, cx));
        let this = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let this = this.clone();
            let browse = this.clone();
            dialog
                .title("Open household…")
                .w_96()
                .child(
                    v_flex()
                        .gap_3()
                        .child(Form::vertical().child(Field::new().label("File").child(Input::new(&path_state).id("open-path"))))
                        .child(
                            h_flex().gap_2().child(
                                Button::new("browse-open").small().outline().icon(IconName::FolderOpen).label("Browse…").on_click(move |_, window, cx| {
                                    let _ = browse.update(cx, |app, cx| app.browse_for_open(window, cx));
                                }),
                            ),
                        )
                        .child(div().text_xs().child("Unsaved changes in the current household are discarded when another file opens.")),
                )
                .footer(
                    DialogFooter::new()
                        .child(Button::new("cancel-open").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                        .child(Button::new("confirm-open").primary().label("Open").on_click(move |_, window, cx| {
                            let _ = this.update(cx, |app, cx| app.open_from_form(window, cx));
                        })),
                )
        });
    }

    /// Loads the file on a background thread (SQLite read + JSON decode of
    /// every table), then swaps the household in on the foreground.
    fn open_from_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.lifecycle_form.path.read(cx).value().trim().to_string();
        let file = HouseholdFile::new(PathBuf::from(&text));
        if !file.exists() {
            window.push_notification(format!("{text} does not exist."), cx);
            return;
        }
        window.close_dialog(cx);
        let owner = self.owner.clone();
        let started = std::time::Instant::now();
        let read = cx.background_spawn(async move {
            let loaded = file.load();
            let lock = match &loaded {
                Ok(_) => Some(file.acquire(&owner, false).map(|_| ())),
                Err(_) => None,
            };
            (file, loaded, lock)
        });
        cx.spawn(async move |this, cx| {
            let (file, loaded, lock) = read.await;
            let _ = this.update_in(cx, |app, window, cx| match loaded {
                Ok(household) => {
                    log::info!("perf: load of {} took {:.1}ms off the UI thread", file.path().display(), crate::perf::ms(started.elapsed()));
                    match lock {
                        Some(Err(StoreError::Locked { owner, since, .. })) => window.push_notification(format!("Opened; note it is also open by {owner} since {since}."), cx),
                        Some(Err(err)) => window.push_notification(format!("Opened without a lock: {err}"), cx),
                        _ => {}
                    }
                    let name = household.name.clone();
                    app.replace_household(household, Some(file), window, cx);
                    window.push_notification(format!("Opened “{name}”"), cx);
                }
                Err(err) => {
                    alerting::report(Level::Error, format!("open failed for {}: {err}", file.path().display()));
                    window.push_notification(format!("Couldn’t open: {err}"), cx);
                }
            });
        })
        .detach();
    }

    /// Native open dialog when the platform has one; otherwise a notification.
    fn browse_for_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some("Open".into()) });
        let path_state = self.lifecycle_form.path.clone();
        cx.spawn_in(window, async move |_, cx| {
            match receiver.await {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.first() {
                        let text = path.display().to_string();
                        let _ = cx.update(|window, cx| path_state.update(cx, |state, cx| state.set_value(text, window, cx)));
                    }
                }
                Ok(Ok(None)) => {}
                Ok(Err(err)) => log::warn!("native open dialog unavailable: {err}"),
                Err(_) => log::warn!("native open dialog cancelled by the platform"),
            }
        })
        .detach();
    }

    fn browse_for_new_path(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_new_path(&atlas_store::default_folder(), Some(&format!("{}.atlas.sqlite", slug(&self.household.name))));
        let path_state = self.lifecycle_form.path.clone();
        cx.spawn_in(window, async move |_, cx| {
            match receiver.await {
                Ok(Ok(Some(path))) => {
                    let text = path.display().to_string();
                    let _ = cx.update(|window, cx| path_state.update(cx, |state, cx| state.set_value(text, window, cx)));
                }
                Ok(Ok(None)) => {}
                Ok(Err(err)) => log::warn!("native save dialog unavailable: {err}"),
                Err(_) => log::warn!("native save dialog cancelled by the platform"),
            }
        })
        .detach();
    }

    /// New empty household (name, currency, reconciliation date, first person).
    pub fn open_new_household(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let form_name = self.lifecycle_form.name.clone();
        let form_person = self.lifecycle_form.first_person.clone();
        let currency = self.lifecycle_form.currency.clone();
        let as_of = self.lifecycle_form.as_of.clone();
        form_name.update(cx, |state, cx| state.set_value("", window, cx));
        form_person.update(cx, |state, cx| state.set_value("", window, cx));
        let today = chrono::Local::now().date_naive();
        as_of.update(cx, |state, cx| state.set_date(today, window, cx));
        let this = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let this = this.clone();
            dialog
                .title("New household")
                .w_96()
                .child(
                    Form::vertical()
                        .child(Field::new().label("Household name").required(true).child(Input::new(&form_name).id("new-household-name")))
                        .child(Field::new().label("Base currency (USD by default)").child(Select::new(&currency)))
                        .child(Field::new().label("Balances reconciled as of").child(DatePicker::new(&as_of)))
                        .child(Field::new().label("First person (you)").required(true).child(Input::new(&form_person).id("new-household-person"))),
                )
                .footer(
                    DialogFooter::new()
                        .child(Button::new("cancel-new-household").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                        .child(Button::new("confirm-new-household").primary().label("Create").on_click(move |_, window, cx| {
                            let _ = this.update(cx, |app, cx| app.create_household_from_form(window, cx));
                        })),
                )
        });
    }

    fn create_household_from_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.lifecycle_form.name.read(cx).value().trim().to_string();
        let person = self.lifecycle_form.first_person.read(cx).value().trim().to_string();
        if name.is_empty() || person.is_empty() {
            window.push_notification("Name the household and its first person.", cx);
            return;
        }
        let code_index = self.lifecycle_form.currency.read(cx).selected_index(cx).map(|p| p.row).unwrap_or(0);
        let currency = Currency::KNOWN.get(code_index).and_then(|(code, _)| Currency::from_code(code)).unwrap_or(Currency::USD);
        let as_of = self.lifecycle_form.as_of.read(cx).date().start().unwrap_or_else(|| chrono::Local::now().date_naive());
        let mut household = Household::empty(&name, currency, as_of);
        household.add_person(Person { id: PersonId::new(1), name: person.clone(), role: HouseholdRole::Owner });
        window.close_dialog(cx);
        self.replace_household(household, None, window, cx);
        self.dirty = true;
        window.push_notification(format!("“{name}” created for {person} in {currency}; add accounts, then save from the Household menu."), cx);
    }

    /// Loads the fictitious sample household.
    pub fn load_sample(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_household(fixtures::plan_household(), None, window, cx);
        window.push_notification("Sample household loaded (fictitious plan numbers).", cx);
    }

    /// "Who is looking?" — a button per person (M12.3, no secret by design).
    pub fn open_viewer_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let people: Vec<(PersonId, String, String)> = self.household.people.iter().map(|p| (p.id, p.name.clone(), p.role.label().to_string())).collect();
        if people.is_empty() {
            window.push_notification("Add a person first.", cx);
            return;
        }
        let this = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let this = this.clone();
            let muted = cx.theme().muted_foreground;
            dialog
                .title("Who is looking?")
                .w_96()
                .child(
                    v_flex()
                        .gap_2()
                        .child(div().text_xs().text_color(muted).child("Every screen is filtered by this person's access policies (§7). The demo identifies people by choice, without a secret."))
                        .children(people.iter().map(|(id, name, role)| {
                            let id = *id;
                            let this = this.clone();
                            Button::new(SharedString::from(format!("pick-viewer-{}", id.raw())))
                                .w_full()
                                .outline()
                                .label(format!("{name} — {role}"))
                                .on_click(move |_, window, cx| {
                                    let _ = this.update(cx, |app, cx| {
                                        app.set_viewer(Viewer::person(id), cx);
                                        window.close_dialog(cx);
                                    });
                                })
                        })),
                )
        });
    }

    /// The title-bar Household menu.
    /// The Household ▸ menu in the title bar. `this` is the handle the menu
    /// items act through; the shell renders the menu, so it cannot come from
    /// a `Context<AtlasApp>` here.
    pub fn render_household_menu(&self, this: WeakEntity<Self>) -> impl IntoElement {
        let dirty = self.dirty;
        let label = if dirty { format!("{} •", self.household.name) } else { self.household.name.clone() };
        DropdownButton::new("household-menu")
            .small()
            .button(Button::new("household-menu-button").small().ghost().icon(IconName::FolderOpen).label(label))
            .dropdown_menu(move |menu, _, _| {
                let save = this.clone();
                let save_as = this.clone();
                let open = this.clone();
                let new = this.clone();
                let sample = this.clone();
                let who = this.clone();
                menu.item(PopupMenuItem::new(if dirty { "Save •" } else { "Save" }).icon(IconName::Save).on_click(move |_, window, cx| {
                    let _ = save.update(cx, |app, cx| app.save(window, cx));
                }))
                .item(PopupMenuItem::new("Save as…").on_click(move |_, window, cx| {
                    let _ = save_as.update(cx, |app, cx| app.open_save_as(window, cx));
                }))
                .item(PopupMenuItem::new("Open…").icon(IconName::FolderOpen).on_click(move |_, window, cx| {
                    let _ = open.update(cx, |app, cx| app.open_open(window, cx));
                }))
                .item(PopupMenuItem::separator())
                .item(PopupMenuItem::new("New household…").icon(IconName::Plus).on_click(move |_, window, cx| {
                    let _ = new.update(cx, |app, cx| app.open_new_household(window, cx));
                }))
                .item(PopupMenuItem::new("Load sample household").on_click(move |_, window, cx| {
                    let _ = sample.update(cx, |app, cx| app.load_sample(window, cx));
                }))
                .item(PopupMenuItem::separator())
                .item(PopupMenuItem::new("Who is looking?…").icon(IconName::Eye).on_click(move |_, window, cx| {
                    let _ = who.update(cx, |app, cx| app.open_viewer_picker(window, cx));
                }))
            })
    }

    /// The viewer's display name.
    pub fn viewer_display_name(&self) -> String {
        self.household.entity_name(EntityRef::Person(self.viewer.person))
    }
}

/// A file-name-safe version of a household name.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() { "household".into() } else { trimmed }
}

impl Drop for AtlasApp {
    fn drop(&mut self) {
        if let Some(file) = &self.file {
            let _ = file.release(&self.owner);
        }
    }
}

/// Default reconciliation date for new households when none is given.
pub fn today_or(date: Option<NaiveDate>) -> NaiveDate {
    date.unwrap_or_else(|| chrono::Local::now().date_naive())
}
