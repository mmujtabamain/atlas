//! Household lifecycle and identity: Welcome, new / open / save / save-as /
//! sample, the unsaved-changes guard, the lock, the file-state machine and
//! the "Who is looking?" chooser. Native file dialogs are used when the
//! platform has them; a path field always works (the Linux box has no portal).

use std::path::PathBuf;
use std::rc::Rc;

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
    radio::RadioGroup,
    select::{Select, SelectState},
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::alerting::{self, Level};
use crate::app::AtlasApp;
use crate::launch::{Launch, Start};
use crate::nav::Route;

/// Retained state of the lifecycle dialogs.
pub struct LifecycleForm {
    pub path: Entity<InputState>,
    pub name: Entity<InputState>,
    pub first_person: Entity<InputState>,
    pub currency: Entity<SelectState<Vec<SharedString>>>,
    pub as_of: Entity<DatePickerState>,
    /// The person picked in the chooser before `Continue`.
    pub viewer_choice: Entity<ViewerChoice>,
}

/// The chooser's selected row.
#[derive(Default, Debug, Clone)]
pub struct ViewerChoice {
    pub index: usize,
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
            viewer_choice: cx.new(|_| ViewerChoice::default()),
        }
    }
}

/// How the app resolved its starting household.
pub struct Resolved {
    pub household: Household,
    pub file: Option<HouseholdFile>,
    pub notices: Vec<String>,
    /// A startup failure shown on Welcome (the requested file did not open).
    pub failure: Option<String>,
    /// Another editor holds the file: (owner, since).
    pub lock_holder: Option<(String, String)>,
}

/// A step to run once the unsaved-changes guard is passed.
pub type Continuation = Rc<dyn Fn(&mut AtlasApp, &mut Window, &mut Context<AtlasApp>)>;

impl AtlasApp {
    /// Resolves `--household / --new / --sample` into a household and file.
    /// With none of them (the default) nothing opens: Welcome shows.
    pub fn resolve_start(launch: &Launch, owner: &str) -> Resolved {
        let mut notices = Vec::new();
        let placeholder = || Household::empty("", Currency::USD, launch.as_of.unwrap_or_else(|| chrono::Local::now().date_naive()));
        match &launch.start {
            Start::Welcome => Resolved { household: placeholder(), file: None, notices, failure: None, lock_holder: None },
            Start::Sample => Resolved { household: fixtures::plan_household(), file: None, notices, failure: None, lock_holder: None },
            Start::Empty => Resolved {
                household: Household::empty("New household", Currency::USD, launch.as_of.unwrap_or_else(|| chrono::Local::now().date_naive())),
                file: None,
                notices,
                failure: None,
                lock_holder: None,
            },
            Start::File(path) => {
                let file = HouseholdFile::new(path);
                if file.exists() {
                    let mut lock_holder = None;
                    let writable = match file.acquire(owner, launch.take_over) {
                        Ok(_) => true,
                        Err(StoreError::Locked { owner: other, since, .. }) => {
                            notices.push(format!("{} is open by {other} since {since}. Viewing only; use Save as… to keep a copy.", path.display()));
                            lock_holder = Some((other, since));
                            false
                        }
                        Err(err) => {
                            notices.push(format!("Could not lock {}: {err}", path.display()));
                            false
                        }
                    };
                    let loaded = if writable { file.load() } else { file.load_read_only() };
                    match loaded {
                        Ok(household) => Resolved { household, file: Some(file), notices, failure: None, lock_holder },
                        Err(err) => {
                            if writable {
                                let _ = file.release(owner);
                            }
                            alerting::report(Level::Error, format!("failed to load household path_id={}: {err}", file.identity()));
                            Resolved { household: placeholder(), file: None, notices, failure: Some(format!("Could not open {}: {err}", path.display())), lock_holder: None }
                        }
                    }
                } else {
                    let household = Household::empty("New household", Currency::USD, launch.as_of.unwrap_or_else(|| chrono::Local::now().date_naive()));
                    let created = file.acquire(owner, false).and_then(|_| file.save_owned(&household, owner));
                    match created {
                        Ok(()) => notices.push(format!("Created {}.", path.display())),
                        Err(err) => {
                            let _ = file.release(owner);
                            alerting::report(Level::Error, format!("failed to create household path_id={}: {err}", file.identity()));
                            notices.push(format!("Could not create {}: {err}", path.display()));
                        }
                    }
                    Resolved { household, file: Some(file), notices, failure: None, lock_holder: None }
                }
            }
        }
    }

    /// The startup failure Welcome shows, if any.
    pub fn startup_notice(&self) -> Option<&str> {
        self.startup_notice.as_deref()
    }

    /// Replaces the household (after new / open / sample), rebuilds every form
    /// that snapshots household data, recomputes, and asks who is looking.
    pub fn replace_household(&mut self, household: Household, file: Option<HouseholdFile>, is_sample: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(old) = self.file.take() {
            let _ = old.release(&self.owner);
        }
        log::info!(
            "household replaced records={} sample={is_sample} path_id={}",
            household.people.len(),
            file.as_ref().map(HouseholdFile::identity).unwrap_or_else(|| "none".into())
        );
        self.household = household;
        self.file = file;
        self.dirty = false;
        self.opened = true;
        self.is_sample = is_sample;
        self.lock_holder = None;
        self.startup_notice = None;
        self.viewer = Viewer::person(self.household.people.first().map(|p| p.id).unwrap_or(PersonId::new(0)));
        self.viewer_pending = self.household.people.len() > 1;
        self.horizon = if is_sample { fixtures::default_horizon() } else { self.household.as_of.checked_add_months(chrono::Months::new(12)).unwrap_or(self.household.as_of) };
        self.selected_account = None;
        self.selected_company = None;
        self.selected_person = None;
        self.history.clear();
        self.last_result = None;
        self.route = Route::Today;
        self.reset_decision_state();
        self.rebuild_forms(window, cx);
        self.refresh_derived();
        cx.notify();
        if self.viewer_pending {
            cx.defer_in(window, |this, window, cx| this.open_viewer_picker(window, cx));
        }
    }

    /// Closes the household and returns to Welcome (after the guard).
    pub fn close_household(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.guard_unsaved(window, cx, Rc::new(|app, _, cx| app.close_household_now(cx)));
    }

    fn close_household_now(&mut self, cx: &mut Context<Self>) {
        if let Some(old) = self.file.take() {
            let _ = old.release(&self.owner);
        }
        log::info!("household closed");
        self.household = Household::empty("", Currency::USD, chrono::Local::now().date_naive());
        self.opened = false;
        self.is_sample = false;
        self.viewer_pending = false;
        self.dirty = false;
        self.lock_holder = None;
        self.route = Route::Welcome;
        self.history.clear();
        self.last_result = None;
        self.refresh_derived();
        cx.notify();
    }

    /// Runs `then` now, or after the person has decided what to do with
    /// unsaved changes (`Save and continue` / `Discard changes` / `Cancel`).
    pub fn guard_unsaved(&mut self, window: &mut Window, cx: &mut Context<Self>, then: Continuation) {
        if !self.opened || !self.dirty {
            then(self, window, cx);
            return;
        }
        let name = self.household.name.clone();
        let this = cx.entity().downgrade();
        let discard = then.clone();
        let save = then;
        window.open_dialog(cx, move |dialog, _, _| {
            let this_discard = this.clone();
            let this_save = this.clone();
            let discard = discard.clone();
            let save = save.clone();
            dialog
                .title(format!("Save changes to “{name}”?"))
                .w_96()
                .child(div().text_sm().child("Unsaved changes will be lost."))
                .footer(
                    DialogFooter::new()
                        .child(Button::new("guard-cancel").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                        .child(Button::new("guard-discard").danger().label("Discard changes").on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            let discard = discard.clone();
                            let _ = this_discard.update(cx, |app, cx| {
                                app.dirty = false;
                                discard(app, window, cx);
                            });
                        }))
                        .child(Button::new("guard-save").primary().label("Save and continue").on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            let save = save.clone();
                            let _ = this_save.update(cx, |app, cx| {
                                app.pending_after_save = Some(save);
                                app.save(window, cx);
                            });
                        })),
                )
        });
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

    /// The file state, exactly as the design's state machine words it.
    pub fn file_state_text(&self) -> String {
        if !self.opened {
            return "No household open".into();
        }
        if let Some((owner, since)) = &self.lock_holder {
            return format!("Viewing only · {owner} opened {since}");
        }
        match (self.file_path(), self.dirty, self.saving, self.edited_while_saving) {
            (Some(path), _, true, _) => format!("Saving · {}", path.display()),
            (Some(path), true, false, true) => format!("Saved earlier changes · newer changes unsaved · {}", path.display()),
            (Some(path), true, false, false) => format!("Unsaved changes · {}", path.display()),
            (Some(path), false, false, _) => format!("Saved · {}", path.display()),
            (None, true, _, _) => "Unsaved changes · no file".into(),
            (None, false, _, _) => "Not saved to a file".into(),
        }
    }

    /// `Save` when a file exists, `Save…` when Save must first ask for one.
    pub fn save_command_label(&self) -> &'static str {
        if self.file.is_some() && self.lock_holder.is_none() { "Save" } else { "Save…" }
    }

    /// The lock fact for Settings.
    pub fn lock_text(&self) -> String {
        match (&self.lock_holder, &self.file) {
            (Some((owner, since)), _) => format!("Held by {owner} since {since} — viewing only"),
            (None, Some(_)) => format!("Held by {} (this session)", self.owner),
            (None, None) => "No file, no lock".into(),
        }
    }

    /// Saves to the current file, or asks for one. A file held by someone
    /// else is never written: Save as… keeps a copy under another path.
    pub fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.file.clone() {
            Some(file) if self.lock_holder.is_none() => self.save_in_background(file, false, window, cx),
            _ => self.open_save_as(window, cx),
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
        log::info!("household save started path_id={}", file.identity());
        let started = std::time::Instant::now();
        let write = cx.background_spawn(async move {
            let result = if take_over {
                file.acquire(&owner, true).and_then(|_| file.save_owned(&household, &owner))
            } else {
                file.save_owned(&household, &owner)
            };
            if result.is_err() && take_over {
                let _ = file.release(&owner);
            }
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
                log::info!("perf: household save path_id={} took {:.1}ms off the UI thread", file.identity(), crate::perf::ms(took));
                if take_over {
                    if let Some(old) = self.file.take()
                        && old.path() != file.path()
                    {
                        let _ = old.release(&self.owner);
                    }
                    self.file = Some(file.clone());
                    self.lock_holder = None;
                }
                if self.edits == edits {
                    self.dirty = false;
                    self.edited_while_saving = false;
                } else {
                    log::info!("household edited while saving; it stays unsaved");
                    self.edited_while_saving = true;
                }
                self.note_result(format!("Saved to {}.", file.path().display()));
                window.push_notification(format!("Saved to {}", file.path().display()), cx);
                if let Some(then) = self.pending_after_save.take() {
                    then(self, window, cx);
                }
            }
            Err(err) => {
                alerting::report(Level::Error, format!("save failed path_id={}: {err}", file.identity()));
                self.pending_after_save = None;
                self.note_result(format!("Not saved · {err}"));
                window.push_notification(format!("Not saved: {err}"), cx);
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
        let copy = self.lock_holder.is_some();
        window.open_dialog(cx, move |dialog, _, _| {
            let this = this.clone();
            let browse = this.clone();
            dialog
                .title(if copy { "Save a copy as…" } else { "Save household as…" })
                .w_96()
                .child(
                    v_flex()
                        .gap_3()
                        .child(Form::vertical().child(Field::new().label("File (one household per file, one editor at a time)").child(Input::new(&path_state).id("save-as-path"))))
                        .child(
                            h_flex().gap_2().child(
                                Button::new("browse-save").small().outline().icon(IconName::FolderOpen).label("Browse…").on_click(move |_, window, cx| {
                                    let _ = browse.update(cx, |app, cx| app.browse_for_new_path(window, cx));
                                }),
                            ),
                        )
                        .when(copy, |this| this.child(div().text_xs().child("The original file is held by someone else; the copy must use a different path."))),
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
        let target = PathBuf::from(&text);
        if self.lock_holder.is_some() && self.file_path().as_deref() == Some(target.as_path()) {
            window.push_notification("That file is held by someone else; choose a different path for the copy.", cx);
            return;
        }
        let file = HouseholdFile::new(target);
        window.close_dialog(cx);
        self.save_in_background(file, true, window, cx);
    }

    /// Opens a household file (path field + native browse), after the guard.
    pub fn open_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.guard_unsaved(window, cx, Rc::new(|app, window, cx| app.open_open_dialog(window, cx)));
    }

    fn open_open_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                        ),
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
            let lock = file.acquire(&owner, false).map(|_| ());
            let loaded = match &lock {
                Ok(()) => file.load(),
                Err(StoreError::Locked { .. }) => file.load_read_only(),
                Err(_) => file.load_read_only(),
            };
            (file, loaded, lock)
        });
        cx.spawn(async move |this, cx| {
            let (file, loaded, lock) = read.await;
            let _ = this.update_in(cx, |app, window, cx| match loaded {
                Ok(household) => {
                    log::info!("perf: household load path_id={} took {:.1}ms off the UI thread", file.identity(), crate::perf::ms(started.elapsed()));
                    let mut lock_holder = None;
                    match lock {
                        Err(StoreError::Locked { owner, since, .. }) => {
                            window.push_notification(format!("Viewing only: {owner} has it open since {since}. Save as… keeps a copy."), cx);
                            lock_holder = Some((owner, since));
                        }
                        Err(err) => window.push_notification(format!("Opened without a lock: {err}"), cx),
                        _ => {}
                    }
                    let name = household.name.clone();
                    app.replace_household(household, Some(file), false, window, cx);
                    app.lock_holder = lock_holder;
                    app.note_result(format!("Opened “{name}”."));
                }
                Err(err) => {
                    if lock.as_ref().is_ok() {
                        let _ = file.release(&app.owner);
                    }
                    alerting::report(Level::Error, format!("open failed path_id={}: {err}", file.identity()));
                    window.push_notification(format!("Could not open: {err}"), cx);
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

    /// New empty household (name, currency, reconciliation date, first person),
    /// after the guard.
    pub fn open_new_household(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.guard_unsaved(window, cx, Rc::new(|app, window, cx| app.open_new_household_dialog(window, cx)));
    }

    fn open_new_household_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let form_name = self.lifecycle_form.name.clone();
        let form_person = self.lifecycle_form.first_person.clone();
        let currency = self.lifecycle_form.currency.clone();
        let as_of = self.lifecycle_form.as_of.clone();
        form_name.update(cx, |state, cx| state.set_value("", window, cx));
        form_person.update(cx, |state, cx| state.set_value("", window, cx));
        let today = chrono::Local::now().date_naive();
        as_of.update(cx, |state, cx| state.set_date(today, window, cx));
        let this = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let this = this.clone();
            let muted = cx.theme().muted_foreground;
            dialog
                .title("Create household")
                .w_96()
                .child(
                    v_flex()
                        .gap_3()
                        .child(
                            Form::vertical()
                                .child(Field::new().label("Household name").required(true).child(Input::new(&form_name).id("new-household-name")))
                                .child(Field::new().label("Currency").child(Select::new(&currency)))
                                .child(Field::new().label("Balances as of").child(DatePicker::new(&as_of)))
                                .child(Field::new().label("Your name").required(true).child(Input::new(&form_person).id("new-household-person"))),
                        )
                        .child(div().text_xs().text_color(muted).child("Currency cannot be changed later. One currency per household; no conversion.")),
                )
                .footer(
                    DialogFooter::new()
                        .child(Button::new("cancel-new-household").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                        .child(Button::new("confirm-new-household").primary().label("Create household").on_click(move |_, window, cx| {
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
        self.replace_household(household, None, false, window, cx);
        self.dirty = true;
        self.note_result(format!("“{name}” created for {person} in {currency}. Not saved to a file."));
    }

    /// Loads the fictitious sample household, after the guard.
    pub fn load_sample(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.guard_unsaved(
            window,
            cx,
            Rc::new(|app, window, cx| {
                app.replace_household(fixtures::plan_household(), None, true, window, cx);
                app.note_result("Sample household loaded — fictitious people, accounts and amounts.");
            }),
        );
    }

    /// "Who is looking?" — a radio per person, no secret by design. The
    /// initial chooser (after a multi-person load) offers `Back to Welcome`
    /// instead of a bypass into someone's data.
    pub fn open_viewer_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let people: Vec<(PersonId, String, String)> = self.household.people.iter().map(|p| (p.id, p.name.clone(), p.role.label().to_string())).collect();
        if people.is_empty() {
            window.push_notification("Add a person to choose a viewer.", cx);
            return;
        }
        let current = people.iter().position(|(id, _, _)| *id == self.viewer.person).unwrap_or(0);
        let choice = self.lifecycle_form.viewer_choice.clone();
        choice.update(cx, |c, _| c.index = current);
        let initial = self.viewer_pending;
        let draft_at_risk = self.decision.is_some() || self.decision_step > 0;
        let this = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let this = this.clone();
            let muted = cx.theme().muted_foreground;
            let selected = choice.read(cx).index;
            let choice_entity = choice.clone();
            let labels: Vec<String> = people.iter().map(|(_, name, role)| format!("{name} — {role}")).collect();
            let ids: Vec<PersonId> = people.iter().map(|(id, _, _)| *id).collect();
            let continue_this = this.clone();
            let back_this = this.clone();
            dialog
                .title("Who is looking?")
                .w_96()
                .child(
                    v_flex()
                        .gap_3()
                        .child(
                            RadioGroup::vertical("viewer-choice")
                                .children(labels)
                                .selected_index(Some(selected))
                                .on_change(move |index, _, cx| choice_entity.update(cx, |c, cx| { c.index = *index; cx.notify(); })),
                        )
                        .child(div().text_xs().text_color(muted).child("This changes what Atlas shows. It does not secure the household file."))
                        .when(draft_at_risk, |this| this.child(div().text_xs().text_color(muted).child("Switching to another person discards the purchase draft."))),
                )
                .footer(
                    DialogFooter::new()
                        .child(if initial {
                            Button::new("viewer-back").outline().label("Back to Welcome").on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                                let _ = back_this.update(cx, |app, cx| app.close_household_now(cx));
                            })
                        } else {
                            Button::new("viewer-cancel").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx))
                        })
                        .child(Button::new("viewer-continue").primary().label("Continue").on_click(move |_, window, cx| {
                            let id = ids.get(selected).copied();
                            window.close_dialog(cx);
                            if let Some(id) = id {
                                let _ = continue_this.update(cx, |app, cx| {
                                    app.set_viewer(Viewer::person(id), cx);
                                    app.viewer_pending = false;
                                    cx.notify();
                                });
                            }
                        })),
                )
        });
    }

    /// The Household ▸ menu in the title bar. `this` is the handle the menu
    /// items act through; the shell renders the menu, so it cannot come from
    /// a `Context<AtlasApp>` here.
    pub fn render_household_menu(&self, this: WeakEntity<Self>) -> impl IntoElement {
        let dirty = self.dirty;
        let label = if dirty { format!("{} •", self.household.name) } else { self.household.name.clone() };
        let save_label = self.save_command_label();
        DropdownButton::new("household-menu")
            .small()
            .button(Button::new("household-menu-button").small().ghost().icon(IconName::FolderOpen).label(label))
            .dropdown_menu(move |menu, _, _| {
                let new = this.clone();
                let open = this.clone();
                let sample = this.clone();
                let save = this.clone();
                let save_as = this.clone();
                let close = this.clone();
                menu.item(PopupMenuItem::new("New household…").icon(IconName::Plus).on_click(move |_, window, cx| {
                    let _ = new.update(cx, |app, cx| app.open_new_household(window, cx));
                }))
                .item(PopupMenuItem::new("Open household…").icon(IconName::FolderOpen).on_click(move |_, window, cx| {
                    let _ = open.update(cx, |app, cx| app.open_open(window, cx));
                }))
                .item(PopupMenuItem::new("Explore sample").on_click(move |_, window, cx| {
                    let _ = sample.update(cx, |app, cx| app.load_sample(window, cx));
                }))
                .item(PopupMenuItem::separator())
                .item(PopupMenuItem::new(save_label).icon(IconName::Save).on_click(move |_, window, cx| {
                    let _ = save.update(cx, |app, cx| app.save(window, cx));
                }))
                .item(PopupMenuItem::new("Save as…").on_click(move |_, window, cx| {
                    let _ = save_as.update(cx, |app, cx| app.open_save_as(window, cx));
                }))
                .item(PopupMenuItem::separator())
                .item(PopupMenuItem::new("Close household").on_click(move |_, window, cx| {
                    let _ = close.update(cx, |app, cx| app.close_household(window, cx));
                }))
            })
    }

    /// The viewer's display name.
    pub fn viewer_display_name(&self) -> String {
        self.household.entity_name(EntityRef::Person(self.viewer.person))
    }

    /// The viewer's name and household role (`Person A · Owner`).
    pub fn viewer_display_name_with_role(&self) -> String {
        match self.household.person(self.viewer.person) {
            Some(person) => format!("{} · {}", person.name, person.role.label()),
            None => "No one yet".into(),
        }
    }

    /// Whether gpui's frame-time overlay was requested at launch.
    pub fn perf_overlay(&self) -> bool {
        self.perf_overlay
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
