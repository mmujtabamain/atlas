//! Reading and writing layout files.
//!
//! A layout file is the JSON of a [`WorkspaceLayout`] with a `schemaVersion`.
//! Loading goes through [`migrate`], which brings older versions forward one
//! step at a time (the [`MIGRATORS`] chain — empty while there is only one
//! version, but the mechanism is in place and tested), normalizes every
//! window and validates the result. Anything that fails — unreadable JSON, a
//! version newer than this build, a tree that breaks the invariants — is
//! handled by [`load`] the same way: the original file is copied to
//! `<path>.corrupt-<timestamp>.json` (never deleted), the previous good copy
//! (`<path>.bak`) is tried, and failing that a fresh default is returned,
//! together with the reason so the UI can tell the user.
//!
//! Writing is atomic ([`save_atomic`]): the JSON goes to `<path>.tmp`, the
//! existing file is copied to `<path>.bak`, then the temp file is renamed
//! over the target, so a crash at any moment leaves either the old or the new
//! file complete.
//!
//! [`Autosave`] is the debounce policy the UI polls, and
//! [`scrub_view_state`] is the filter that keeps secrets and bulky documents
//! out of a layout file: a pane's view state is for scroll positions and
//! filters, not for tokens or the data itself.

use crate::validate::Violation;
use crate::workspace::WorkspaceLayout;
use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use thiserror::Error;

/// The layout file format this build writes.
pub const SCHEMA_VERSION: u32 = 1;

/// Largest view-state value (string, or serialized object/array) kept in a file.
pub const MAX_VIEW_STATE_VALUE_BYTES: usize = 16 * 1024;

/// Key fragments that mark a view-state entry as a secret. Compared against
/// the lowercased key, so `authToken`, `AUTHORIZATION` and `signedUrl` all match.
pub const SENSITIVE_KEY_FRAGMENTS: &[&str] = &["token", "secret", "password", "passwd", "authorization", "signedurl", "apikey", "api_key", "credential", "cookie"];

/// One migration step: takes a layout document at version `n` and returns it
/// at version `n + 1`, including the updated `schemaVersion` field.
pub type Migrator = fn(Value) -> Result<Value, PersistError>;

/// The migration chain: `MIGRATORS[i]` takes version `i + 1` to `i + 2`.
/// Empty while version 1 is the only version; a new format version adds a
/// function here and bumps [`SCHEMA_VERSION`].
pub const MIGRATORS: &[Migrator] = &[];

/// Why a document could not become a workspace.
#[derive(Error, Debug)]
pub enum PersistError {
    #[error("layout file is schema version {found}, but this build reads up to version {supported}; it was written by a newer Atlas")]
    Newer { found: u32, supported: u32 },
    #[error("layout file has no migration from version {from} to {to}")]
    NoMigrator { from: u32, to: u32 },
    #[error("layout migration from version {from} failed: {message}")]
    Migration { from: u32, message: String },
    #[error("layout file is not a workspace document: {0}")]
    Shape(String),
    #[error("layout file breaks {} invariant(s): {}", .0.len(), describe(.0))]
    Invalid(Vec<Violation>),
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("layout file is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
}

fn describe(violations: &[Violation]) -> String {
    violations.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")
}

/// The result of [`load`].
#[derive(Debug)]
pub enum LoadOutcome {
    /// The file was read and is valid.
    Loaded(WorkspaceLayout),
    /// There is no file yet; start from a default.
    Fresh,
    /// The file could not be used. `layout` is what to start from (the
    /// previous good copy when there is one, else a fresh default), the
    /// original is preserved at `diagnostics`, and `reason` says what was wrong.
    Recovered { layout: WorkspaceLayout, diagnostics: PathBuf, reason: String },
}

impl LoadOutcome {
    /// The layout to use, whatever happened (a fresh default for `Fresh`).
    pub fn into_layout(self) -> WorkspaceLayout {
        match self {
            LoadOutcome::Loaded(layout) | LoadOutcome::Recovered { layout, .. } => layout,
            LoadOutcome::Fresh => WorkspaceLayout::default(),
        }
    }
}

/// Turns a layout document into a workspace: version check, migration chain,
/// deserialization, normalization, validation.
pub fn migrate(value: Value) -> Result<WorkspaceLayout, PersistError> {
    migrate_with(value, MIGRATORS, SCHEMA_VERSION)
}

/// [`migrate`] with an explicit chain and target version. The chain's
/// `migrators[i]` takes version `i + 1` to `i + 2`. Exposed so the chain can
/// be tested with a synthetic future step.
pub fn migrate_with(value: Value, migrators: &[Migrator], target: u32) -> Result<WorkspaceLayout, PersistError> {
    let Value::Object(_) = &value else {
        return Err(PersistError::Shape(format!("expected a JSON object at the top level, found {}", kind_of(&value))));
    };
    let mut version = schema_version_of(&value);
    if version > target {
        return Err(PersistError::Newer { found: version, supported: target });
    }
    let mut document = value;
    while version < target {
        let step = migrators.get((version - 1) as usize).ok_or(PersistError::NoMigrator { from: version, to: version + 1 })?;
        log::info!("layout: migrating document from schema version {version} to {}", version + 1);
        document = step(document).map_err(|error| PersistError::Migration {
            from: version,
            message: error.to_string(),
        })?;
        let after = schema_version_of(&document);
        if after != version + 1 {
            return Err(PersistError::Migration {
                from: version,
                message: format!("the migrator left schemaVersion at {after}, expected {}", version + 1),
            });
        }
        version = after;
    }
    let mut layout: WorkspaceLayout = serde_json::from_value(document).map_err(|error| PersistError::Shape(error.to_string()))?;
    layout.schema_version = target;
    if layout.normalize() {
        log::info!("layout: normalized the loaded document (it had redundant or unweighted structure)");
    }
    let violations = layout.validate();
    if !violations.is_empty() {
        return Err(PersistError::Invalid(violations));
    }
    Ok(layout)
}

/// The document's `schemaVersion`; missing or zero means the first version.
pub fn schema_version_of(value: &Value) -> u32 {
    match value.get("schemaVersion").and_then(Value::as_u64) {
        Some(0) | None => 1,
        Some(version) => u32::try_from(version).unwrap_or(u32::MAX),
    }
}

fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

/// Reads the layout file at `path`. Never fails: see [`LoadOutcome`].
pub fn load(path: &Path) -> LoadOutcome {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            log::info!("layout: no file at {}, starting fresh", path.display());
            return LoadOutcome::Fresh;
        }
        Err(error) => return recover(path, format!("could not read {}: {error}", path.display())),
    };
    match parse(&bytes) {
        Ok(layout) => {
            log::info!("layout: loaded {} ({} panes, {} windows)", path.display(), layout.panes.len(), layout.windows.len());
            LoadOutcome::Loaded(layout)
        }
        Err(error) => recover(path, error.to_string()),
    }
}

/// Parses and migrates raw file bytes.
pub fn parse(bytes: &[u8]) -> Result<WorkspaceLayout, PersistError> {
    let value: Value = serde_json::from_slice(bytes)?;
    migrate(value)
}

/// Puts the unusable file aside and works out what to start from instead.
fn recover(path: &Path, reason: String) -> LoadOutcome {
    log::warn!("layout: {reason}; keeping the file aside and recovering");
    let diagnostics = corrupt_copy_path(path);
    if let Err(error) = fs::copy(path, &diagnostics) {
        log::warn!("layout: could not copy {} to {}: {error}", path.display(), diagnostics.display());
    }
    let backup = backup_path(path);
    let layout = match fs::read(&backup).ok().map(|bytes| parse(&bytes)) {
        Some(Ok(previous)) => {
            log::info!("layout: recovered the previous copy from {}", backup.display());
            previous
        }
        Some(Err(error)) => {
            log::warn!("layout: the previous copy at {} is unusable too ({error}); starting from a default", backup.display());
            WorkspaceLayout::default()
        }
        None => WorkspaceLayout::default(),
    };
    LoadOutcome::Recovered { layout, diagnostics, reason }
}

/// Writes `layout` to `path` atomically, keeping the previous file as `<path>.bak`.
pub fn save_atomic(path: &Path, layout: &WorkspaceLayout) -> io::Result<()> {
    save_atomic_json(path, layout)
}

/// [`save_atomic`] for any serializable document (saved layouts use it too).
pub fn save_atomic_json<T: Serialize>(path: &Path, document: &T) -> io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(document).map_err(io::Error::other)?;
    let temp = temp_path(path);
    {
        let mut file = fs::File::create(&temp)?;
        io::Write::write_all(&mut file, &bytes)?;
        file.sync_all()?;
    }
    if path.exists() {
        // A copy rather than a rename keeps `path` in place until the final
        // rename, so there is no moment without a complete file.
        fs::copy(path, backup_path(path))?;
    }
    fs::rename(&temp, path)?;
    log::info!("layout: saved {} ({} bytes)", path.display(), bytes.len());
    Ok(())
}

/// Where the app keeps its own files — workspace sessions, saved layouts,
/// the launcher's arrangement — for the person running it: an `override_env`
/// variable when set (tests and portable installs), else the platform's
/// per-user application directory with `app_dir` inside it:
/// `$XDG_CONFIG_HOME/<app_dir>` or `~/.config/<app_dir>` on Linux,
/// `~/Library/Application Support/<app_dir>` on macOS, `%APPDATA%\<app_dir>` on
/// Windows. Falls back to `.<app_dir>` in the current directory when no home
/// is known, so there is always an answer.
pub fn app_data_dir(app_dir: &str, override_env: &str) -> PathBuf {
    if let Some(dir) = std::env::var_os(override_env).filter(|value| !value.is_empty()) {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME").filter(|value| !value.is_empty()).map(PathBuf::from);
    if cfg!(target_os = "macos") {
        if let Some(home) = home {
            return home.join("Library").join("Application Support").join(app_dir);
        }
    } else if cfg!(target_os = "windows") {
        if let Some(appdata) = std::env::var_os("APPDATA").filter(|value| !value.is_empty()) {
            return PathBuf::from(appdata).join(app_dir);
        }
    } else {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
            return PathBuf::from(xdg).join(app_dir);
        }
        if let Some(home) = home {
            return home.join(".config").join(app_dir);
        }
    }
    PathBuf::from(format!(".{app_dir}"))
}

/// Reads a JSON document written by [`save_atomic_json`]; `None` when there
/// is no file, `Err` when it cannot be read or parsed.
pub fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, PersistError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(PersistError::Io(err)),
    };
    serde_json::from_slice(&bytes).map(Some).map_err(PersistError::Json)
}

/// `<path>.tmp`
pub fn temp_path(path: &Path) -> PathBuf {
    with_suffix(path, ".tmp")
}

/// `<path>.bak`
pub fn backup_path(path: &Path) -> PathBuf {
    with_suffix(path, ".bak")
}

/// `<path>.corrupt-<timestamp>.json`, made unique when the name is taken.
pub fn corrupt_copy_path(path: &Path) -> PathBuf {
    let stamp = Utc::now().format("%Y%m%dT%H%M%S%.3fZ").to_string();
    let first = with_suffix(path, &format!(".corrupt-{stamp}.json"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| with_suffix(path, &format!(".corrupt-{stamp}-{n}.json")))
        .find(|candidate| !candidate.exists())
        .unwrap_or(first)
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map(|name| name.to_os_string()).unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

/// The autosave policy: the UI marks the layout dirty on every change and
/// polls [`Autosave::due`]; a save is due once `debounce` has passed since
/// the *first* unsaved change, so a run of quick changes is written once and
/// a steady stream of them still reaches disk regularly.
#[derive(Clone, Debug, PartialEq)]
pub struct Autosave {
    /// How long to wait after the first unsaved change.
    pub debounce: Duration,
    /// When the first unsaved change happened.
    pub dirty_since: Option<Instant>,
    /// What the last change was, for the log line when it is written.
    pub last_reason: Option<String>,
    changes: u32,
}

impl Default for Autosave {
    fn default() -> Self {
        Autosave::new(Duration::from_millis(750))
    }
}

impl Autosave {
    /// A policy with the given debounce.
    pub fn new(debounce: Duration) -> Self {
        Autosave {
            debounce,
            dirty_since: None,
            last_reason: None,
            changes: 0,
        }
    }

    /// Records a change.
    pub fn mark_dirty(&mut self, reason: impl Into<String>) {
        if self.dirty_since.is_none() {
            self.dirty_since = Some(Instant::now());
        }
        self.changes += 1;
        self.last_reason = Some(reason.into());
    }

    /// Records a change at an explicit time (for tests and replay).
    pub fn mark_dirty_at(&mut self, reason: impl Into<String>, now: Instant) {
        if self.dirty_since.is_none() {
            self.dirty_since = Some(now);
        }
        self.changes += 1;
        self.last_reason = Some(reason.into());
    }

    /// True when there are unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty_since.is_some()
    }

    /// Number of changes since the last flush.
    pub fn pending_changes(&self) -> u32 {
        self.changes
    }

    /// True when there are unsaved changes and the debounce has elapsed at `now`.
    pub fn due(&self, now: Instant) -> bool {
        self.dirty_since.is_some_and(|since| now.saturating_duration_since(since) >= self.debounce)
    }

    /// The layout was written; forget the pending changes.
    pub fn flushed(&mut self) {
        if let Some(reason) = &self.last_reason {
            log::info!("layout: autosaved after {} change(s), last: {reason}", self.changes);
        }
        self.dirty_since = None;
        self.last_reason = None;
        self.changes = 0;
    }
}

/// True when a key names something that must not be written to disk.
pub fn is_sensitive_key(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    SENSITIVE_KEY_FRAGMENTS.iter().any(|fragment| lowered.contains(fragment))
}

/// True when `view_state` contains nothing [`scrub_view_state`] would remove.
pub fn is_persistable_view_state(view_state: &Value) -> bool {
    match view_state {
        Value::Object(map) => map.iter().all(|(key, value)| !is_sensitive_key(key) && !is_oversized(value) && is_persistable_view_state(value)),
        Value::Array(items) => items.iter().all(|value| !is_oversized(value) && is_persistable_view_state(value)),
        Value::String(text) => text.len() <= MAX_VIEW_STATE_VALUE_BYTES,
        _ => true,
    }
}

/// Removes every entry whose key looks like a secret and every value over
/// [`MAX_VIEW_STATE_VALUE_BYTES`] (so a raw document pasted into a pane's
/// state never ends up in the layout file), recursively. Returns how many
/// entries were removed.
pub fn scrub_view_state(view_state: &mut Value) -> usize {
    match view_state {
        Value::Object(map) => {
            let doomed: Vec<String> = map.iter().filter(|(key, value)| is_sensitive_key(key) || is_oversized(value)).map(|(key, _)| key.clone()).collect();
            let mut removed = doomed.len();
            for key in &doomed {
                log::warn!("layout: dropped view-state entry '{key}' (secret-like key or oversized value)");
                map.remove(key);
            }
            for value in map.values_mut() {
                removed += scrub_view_state(value);
            }
            removed
        }
        Value::Array(items) => {
            let before = items.len();
            items.retain(|value| !is_oversized(value));
            let mut removed = before - items.len();
            for value in items.iter_mut() {
                removed += scrub_view_state(value);
            }
            removed
        }
        _ => 0,
    }
}

/// True when a string is longer than the cap, or a container serializes longer than it.
fn is_oversized(value: &Value) -> bool {
    match value {
        Value::String(text) => text.len() > MAX_VIEW_STATE_VALUE_BYTES,
        Value::Array(_) | Value::Object(_) => serde_json::to_vec(value).map(|bytes| bytes.len() > MAX_VIEW_STATE_VALUE_BYTES).unwrap_or(true),
        _ => false,
    }
}

/// Scrubs the view state of every pane in a workspace; returns the number of entries removed.
pub fn scrub_workspace(layout: &mut WorkspaceLayout) -> usize {
    layout.panes.values_mut().map(|definition| scrub_view_state(&mut definition.view_state)).sum()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_data_dir_honours_the_override_and_falls_back_sensibly() {
        let dir = super::app_data_dir("atlas-financer", "ATLAS_TEST_DATA_DIR_OVERRIDE_UNSET");
        assert!(dir.to_string_lossy().contains("atlas-financer"), "{dir:?}");
        // SAFETY: tests in this module run single-threaded with respect to this variable.
        unsafe { std::env::set_var("ATLAS_TEST_DATA_DIR_OVERRIDE", "/tmp/atlas-override") };
        assert_eq!(super::app_data_dir("atlas-financer", "ATLAS_TEST_DATA_DIR_OVERRIDE"), std::path::PathBuf::from("/tmp/atlas-override"));
        unsafe { std::env::remove_var("ATLAS_TEST_DATA_DIR_OVERRIDE") };
    }

    use super::*;
    use serde_json::json;

    #[test]
    fn schema_version_defaults_to_one() {
        assert_eq!(schema_version_of(&json!({})), 1);
        assert_eq!(schema_version_of(&json!({ "schemaVersion": 0 })), 1);
        assert_eq!(schema_version_of(&json!({ "schemaVersion": 3 })), 3);
    }

    #[test]
    fn non_object_documents_are_rejected_by_shape() {
        assert!(matches!(migrate(json!([1, 2])), Err(PersistError::Shape(_))));
        assert!(matches!(migrate(json!({ "schemaVersion": 99 })), Err(PersistError::Newer { found: 99, .. })));
    }

    #[test]
    fn autosave_is_due_after_the_debounce() {
        let mut autosave = Autosave::new(Duration::from_secs(1));
        let t0 = Instant::now();
        assert!(!autosave.due(t0));
        autosave.mark_dirty_at("move", t0);
        autosave.mark_dirty_at("resize", t0 + Duration::from_millis(900));
        assert!(!autosave.due(t0 + Duration::from_millis(999)));
        assert!(autosave.due(t0 + Duration::from_secs(1)));
        assert_eq!(autosave.pending_changes(), 2);
        autosave.flushed();
        assert!(!autosave.is_dirty());
        assert!(!autosave.due(t0 + Duration::from_secs(5)));
    }

    #[test]
    fn scrub_drops_secrets_and_bulk() {
        let mut state = json!({
            "scroll": 120,
            "authToken": "abc",
            "nested": { "password": "x", "filter": "open", "SignedUrl": "https://…" },
            "blob": "x".repeat(MAX_VIEW_STATE_VALUE_BYTES + 1),
            "list": [ { "secret": 1 }, "fine" ]
        });
        assert!(!is_persistable_view_state(&state));
        let removed = scrub_view_state(&mut state);
        assert_eq!(removed, 5);
        assert_eq!(state, json!({ "scroll": 120, "nested": { "filter": "open" }, "list": [ {}, "fine" ] }));
        assert!(is_persistable_view_state(&state));
    }
}
