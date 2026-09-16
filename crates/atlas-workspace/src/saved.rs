//! Saved layouts: a directory of named arrangements the user can come back to.
//!
//! Two kinds are kept side by side. A **layout** is a full
//! [`WorkspaceLayout`] — panes with their resources — and is bound to the
//! household it was saved from when it has one (an account pane from
//! household A means nothing in household B). A **template** is a
//! [`LayoutTemplate`], a shape with named slots and no resources, and is
//! portable everywhere.
//!
//! Each entry is one JSON file `<id>.json` in the directory, written with
//! [`crate::persist::save_atomic_json`]. Names must be non-empty and unique
//! within their kind.

use crate::persist::{self, PersistError};
use crate::presets::LayoutTemplate;
use crate::validate::Violation;
use crate::workspace::{PaneDefinition, Scope, WorkspaceLayout};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// What a saved entry holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SavedKind {
    /// A complete workspace, resources included; household-bound when scoped.
    Layout,
    /// A shape with slots; portable.
    Template,
}

impl SavedKind {
    /// The file-name prefix of ids of this kind.
    fn prefix(self) -> &'static str {
        match self {
            SavedKind::Layout => "layout",
            SavedKind::Template => "template",
        }
    }
}

/// One saved entry. Exactly one of `layout` and `template` is set, matching `kind`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedLayout {
    pub id: String,
    pub name: String,
    pub kind: SavedKind,
    #[serde(default)]
    pub scope: Scope,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub layout: Option<WorkspaceLayout>,
    #[serde(default)]
    pub template: Option<LayoutTemplate>,
}

/// Why a saved-layout operation refused.
#[derive(Error, Debug)]
pub enum SavedError {
    #[error("a saved {kind:?} named '{name}' already exists")]
    NameTaken { kind: SavedKind, name: String },
    #[error("a saved layout needs a name")]
    EmptyName,
    #[error("no saved layout with id '{0}'")]
    UnknownId(String),
    #[error("saved layout '{name}' belongs to household {saved:?} and cannot be opened in household {wanted:?}")]
    WrongHousehold { name: String, saved: Option<String>, wanted: Option<String> },
    #[error("saved entry '{0}' is a {1:?} without its content")]
    MissingContent(String, SavedKind),
    #[error("saved layout is not valid: {}", .0.iter().map(ToString::to_string).collect::<Vec<_>>().join("; "))]
    Invalid(Vec<Violation>),
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Persist(#[from] PersistError),
}

/// The saved layouts of one directory.
#[derive(Debug)]
pub struct SavedLayouts {
    dir: PathBuf,
    entries: BTreeMap<String, SavedLayout>,
}

impl SavedLayouts {
    /// Opens (creating if needed) the directory and reads every `*.json`
    /// entry in it. Files that do not parse are skipped with a warning, never
    /// deleted.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self, SavedError> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        let mut entries = BTreeMap::new();
        for item in fs::read_dir(&dir)? {
            let path = item?.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            match read_entry(&path) {
                Ok(entry) => {
                    entries.insert(entry.id.clone(), entry);
                }
                Err(error) => log::warn!("saved layouts: skipping {}: {error}", path.display()),
            }
        }
        log::info!("saved layouts: opened {} with {} entries", dir.display(), entries.len());
        Ok(SavedLayouts { dir, entries })
    }

    /// The directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Every entry, sorted by kind then name.
    pub fn list(&self) -> Vec<&SavedLayout> {
        let mut out: Vec<&SavedLayout> = self.entries.values().collect();
        out.sort_by(|a, b| {
            a.kind
                .prefix()
                .cmp(b.kind.prefix())
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.id.cmp(&b.id))
        });
        out
    }

    /// One entry.
    pub fn get(&self, id: &str) -> Option<&SavedLayout> {
        self.entries.get(id)
    }

    /// The entry with this name and kind.
    pub fn find_by_name(&self, kind: SavedKind, name: &str) -> Option<&SavedLayout> {
        self.entries.values().find(|entry| entry.kind == kind && entry.name == name)
    }

    /// Saves a layout under `name`: creates the entry, or updates the
    /// existing layout of that name in place (same id, new content).
    pub fn save(&mut self, name: &str, layout: &WorkspaceLayout) -> Result<String, SavedError> {
        let name = clean_name(name)?;
        match self.find_by_name(SavedKind::Layout, &name).map(|entry| entry.id.clone()) {
            Some(id) => {
                self.update(&id, layout)?;
                Ok(id)
            }
            None => self.save_as(&name, layout),
        }
    }

    /// Saves a layout as a new entry; refuses a name that is already taken.
    pub fn save_as(&mut self, name: &str, layout: &WorkspaceLayout) -> Result<String, SavedError> {
        let name = clean_name(name)?;
        self.ensure_free(SavedKind::Layout, &name, None)?;
        let violations = layout.validate();
        if !violations.is_empty() {
            return Err(SavedError::Invalid(violations));
        }
        let now = Utc::now();
        let id = self.next_id(SavedKind::Layout);
        let mut stored = layout.clone();
        persist::scrub_workspace(&mut stored);
        let entry = SavedLayout {
            id: id.clone(),
            name,
            kind: SavedKind::Layout,
            scope: layout.scope.clone(),
            created_at: now,
            updated_at: now,
            layout: Some(stored),
            template: None,
        };
        self.write(entry)?;
        Ok(id)
    }

    /// Replaces the content of an existing layout entry.
    pub fn update(&mut self, id: &str, layout: &WorkspaceLayout) -> Result<(), SavedError> {
        let violations = layout.validate();
        if !violations.is_empty() {
            return Err(SavedError::Invalid(violations));
        }
        let mut entry = self.entries.get(id).cloned().ok_or_else(|| SavedError::UnknownId(id.to_owned()))?;
        let mut stored = layout.clone();
        persist::scrub_workspace(&mut stored);
        entry.kind = SavedKind::Layout;
        entry.scope = layout.scope.clone();
        entry.layout = Some(stored);
        entry.template = None;
        entry.updated_at = Utc::now();
        self.write(entry)
    }

    /// Saves a template as a new entry; refuses a name that is already taken.
    pub fn save_template(&mut self, name: &str, template: &LayoutTemplate) -> Result<String, SavedError> {
        let name = clean_name(name)?;
        self.ensure_free(SavedKind::Template, &name, None)?;
        let now = Utc::now();
        let id = self.next_id(SavedKind::Template);
        let entry = SavedLayout {
            id: id.clone(),
            name,
            kind: SavedKind::Template,
            scope: Scope::global(),
            created_at: now,
            updated_at: now,
            layout: None,
            template: Some(template.clone()),
        };
        self.write(entry)?;
        Ok(id)
    }

    /// Renames an entry (the name must be free within its kind).
    pub fn rename(&mut self, id: &str, name: &str) -> Result<(), SavedError> {
        let name = clean_name(name)?;
        let mut entry = self.entries.get(id).cloned().ok_or_else(|| SavedError::UnknownId(id.to_owned()))?;
        self.ensure_free(entry.kind, &name, Some(id))?;
        entry.name = name;
        entry.updated_at = Utc::now();
        self.write(entry)
    }

    /// Copies an entry under a new id and a "(copy)" name; returns the new id.
    pub fn duplicate(&mut self, id: &str) -> Result<String, SavedError> {
        let source = self.entries.get(id).cloned().ok_or_else(|| SavedError::UnknownId(id.to_owned()))?;
        let name = self.copy_name(source.kind, &source.name);
        let now = Utc::now();
        let new_id = self.next_id(source.kind);
        let entry = SavedLayout {
            id: new_id.clone(),
            name,
            created_at: now,
            updated_at: now,
            ..source
        };
        self.write(entry)?;
        Ok(new_id)
    }

    /// Deletes an entry and its file.
    pub fn delete(&mut self, id: &str) -> Result<SavedLayout, SavedError> {
        let entry = self.entries.remove(id).ok_or_else(|| SavedError::UnknownId(id.to_owned()))?;
        let path = self.path_of(id);
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        // The backup the atomic writer left behind would otherwise linger.
        let _ = fs::remove_file(persist::backup_path(&path));
        log::info!("saved layouts: deleted '{}' ({id})", entry.name);
        Ok(entry)
    }

    /// The workspace to install for an entry, for use under `scope`. A
    /// household-bound layout is refused for another household; a global
    /// layout and any template fit anywhere and take on the requested scope.
    /// A template is filled with placeholder panes named after its slots
    /// (kind = slot name, no resource); the caller replaces them.
    pub fn load_into(&self, id: &str, scope: &Scope) -> Result<WorkspaceLayout, SavedError> {
        let entry = self.entries.get(id).ok_or_else(|| SavedError::UnknownId(id.to_owned()))?;
        match entry.kind {
            SavedKind::Layout => {
                let saved = entry.layout.as_ref().ok_or_else(|| SavedError::MissingContent(id.to_owned(), entry.kind))?;
                if !entry.scope.accepts(scope) {
                    return Err(SavedError::WrongHousehold {
                        name: entry.name.clone(),
                        saved: entry.scope.household_id.clone(),
                        wanted: scope.household_id.clone(),
                    });
                }
                let mut layout = saved.clone();
                layout.scope = scope.clone();
                layout.normalize();
                let violations = layout.validate();
                if !violations.is_empty() {
                    return Err(SavedError::Invalid(violations));
                }
                log::info!("saved layouts: loaded layout '{}' ({id})", entry.name);
                Ok(layout)
            }
            SavedKind::Template => {
                let template = entry.template.as_ref().ok_or_else(|| SavedError::MissingContent(id.to_owned(), entry.kind))?;
                let placeholders: Vec<PaneDefinition> = template.slots.iter().map(|slot| PaneDefinition::new(slot.clone())).collect();
                let layout = WorkspaceLayout::from_template(entry.name.clone(), template, placeholders).with_scope(scope.clone());
                log::info!("saved layouts: instantiated template '{}' ({id}) with {} slot(s)", entry.name, template.slots.len());
                Ok(layout)
            }
        }
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the directory holds no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    // -- internals ----------------------------------------------------------

    fn path_of(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    fn write(&mut self, entry: SavedLayout) -> Result<(), SavedError> {
        persist::save_atomic_json(&self.path_of(&entry.id), &entry)?;
        log::info!("saved layouts: wrote {:?} '{}' ({})", entry.kind, entry.name, entry.id);
        self.entries.insert(entry.id.clone(), entry);
        Ok(())
    }

    fn ensure_free(&self, kind: SavedKind, name: &str, except: Option<&str>) -> Result<(), SavedError> {
        let taken = self.entries.values().any(|entry| entry.kind == kind && entry.name == name && Some(entry.id.as_str()) != except);
        if taken {
            return Err(SavedError::NameTaken { kind, name: name.to_owned() });
        }
        Ok(())
    }

    /// `layout-0001`, `layout-0002`, …: one past the highest existing number of that kind.
    fn next_id(&self, kind: SavedKind) -> String {
        let prefix = format!("{}-", kind.prefix());
        let highest = self
            .entries
            .keys()
            .filter_map(|id| id.strip_prefix(&prefix))
            .filter_map(|rest| rest.parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        format!("{prefix}{:04}", highest + 1)
    }

    /// `<name> (copy)`, then `<name> (copy 2)`, … whichever is free.
    fn copy_name(&self, kind: SavedKind, name: &str) -> String {
        let first = format!("{name} (copy)");
        if self.find_by_name(kind, &first).is_none() {
            return first;
        }
        (2..)
            .map(|n| format!("{name} (copy {n})"))
            .find(|candidate| self.find_by_name(kind, candidate).is_none())
            .unwrap_or(first)
    }
}

fn clean_name(name: &str) -> Result<String, SavedError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(SavedError::EmptyName);
    }
    Ok(trimmed.to_owned())
}

fn read_entry(path: &Path) -> Result<SavedLayout, SavedError> {
    let bytes = fs::read(path)?;
    let entry: SavedLayout = serde_json::from_slice(&bytes)?;
    match entry.kind {
        SavedKind::Layout if entry.layout.is_none() => Err(SavedError::MissingContent(entry.id, entry.kind)),
        SavedKind::Template if entry.template.is_none() => Err(SavedError::MissingContent(entry.id, entry.kind)),
        _ => Ok(entry),
    }
}
