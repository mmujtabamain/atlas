//! Sessions: the workspace as it was, back when the household opens again.
//!
//! A session is "what I currently have open" — the panes, their tabs and
//! splits, the active pane, the windows and where they are — written as the
//! model's JSON ([`atlas_workspace::persist`]) to one file per household in
//! the app's data directory (`sessions/<household>.json`), and read back the
//! next time that household is opened. It is not a saved layout: it is never
//! named, it changes with every move, and it belongs to one household.
//!
//! Saving is **debounced** (a burst of divider drags is one write, a little
//! after the last), **atomic** (a temporary file renamed over the old one,
//! which is kept as `.bak`) and **only ever of committed state** — the model
//! holds nothing of a drag in flight. Reading never fails the start of the
//! app: a corrupt or unreadable file is copied aside for diagnosis and the
//! workspace starts fresh, and the person is told.
//!
//! A `Launch` built in code (the tests) has no data directory, so it keeps
//! nothing; a command-line launch always has one. A launch that names a
//! screen (`--screen`, `--open`) asked for that screen, not for last time's
//! workspace, and skips the session.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use atlas_workspace::persist::{self, Autosave, LoadOutcome};
use atlas_workspace::{Scope, WorkspaceLayout};

/// The directory under the data directory that holds session files.
pub const SESSIONS_DIR: &str = "sessions";

/// How long the workspace waits after the last change before writing.
pub const DEBOUNCE: Duration = Duration::from_millis(750);

/// The session file for a household, from its identity.
pub fn session_path(data_dir: &Path, identity: &str) -> PathBuf {
    data_dir.join(SESSIONS_DIR).join(format!("{}.json", crate::lifecycle::slug(identity)))
}

/// One household's session file and the autosave policy over it.
pub struct SessionStore {
    /// `None` keeps the session for this run only.
    path: Option<PathBuf>,
    scope: Scope,
    autosave: Autosave,
    /// A write is scheduled for when the debounce lapses.
    pub flush_scheduled: bool,
    writes: u64,
}

impl SessionStore {
    /// The store for a household; without a data directory nothing is kept.
    pub fn for_household(data_dir: Option<&Path>, identity: &str) -> Self {
        SessionStore { path: data_dir.map(|dir| session_path(dir, identity)), scope: Scope::household(identity), autosave: Autosave::new(DEBOUNCE), flush_scheduled: false, writes: 0 }
    }

    /// Where the session is kept, if anywhere.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Whether anything is kept at all.
    pub fn persists(&self) -> bool {
        self.path.is_some()
    }

    /// The household this store belongs to.
    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    /// Reads the session. `Fresh` when nothing is kept or there is no file; a
    /// layout from the file is accepted only when it belongs to this household.
    pub fn load(&self) -> LoadOutcome {
        let Some(path) = &self.path else {
            return LoadOutcome::Fresh;
        };
        match persist::load(path) {
            LoadOutcome::Loaded(layout) if !layout.scope.accepts(&self.scope) => {
                log::warn!("session: {} belongs to another household ({:?}); starting fresh", path.display(), layout.scope);
                LoadOutcome::Fresh
            }
            outcome => outcome,
        }
    }

    /// Notes a change; the write happens once the debounce has lapsed.
    pub fn mark_dirty(&mut self, reason: impl Into<String>) {
        self.autosave.mark_dirty(reason);
    }

    pub fn is_dirty(&self) -> bool {
        self.autosave.is_dirty()
    }

    /// Whether the debounce has lapsed since the first unsaved change.
    pub fn due(&self, now: Instant) -> bool {
        self.autosave.due(now)
    }

    /// How many times the session has been written.
    pub fn writes(&self) -> u64 {
        self.writes
    }

    /// Writes `layout` now (scrubbed of anything that must not reach a file)
    /// and clears the dirty state. A store that keeps nothing only clears.
    pub fn write(&mut self, layout: &WorkspaceLayout) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            self.autosave.flushed();
            return Ok(());
        };
        let mut copy = layout.clone();
        let scrubbed = persist::scrub_workspace(&mut copy);
        if scrubbed > 0 {
            log::warn!("session: {scrubbed} view-state value(s) were left out of the file as sensitive or too large");
        }
        persist::save_atomic(path, &copy)?;
        self.writes += 1;
        let reason = self.autosave.last_reason.clone().unwrap_or_default();
        self.autosave.flushed();
        log::info!("session: written to {} ({} panes, {} windows) after {reason}", path.display(), copy.panes.len(), copy.windows.len());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_workspace::{DockTarget, PaneDefinition, Side, WindowId};

    #[core::prelude::v1::test]
    fn a_session_round_trips_and_a_foreign_one_is_refused() {
        let dir = std::env::temp_dir().join(format!("atlas-session-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut store = SessionStore::for_household(Some(&dir), "sample");
        assert!(matches!(store.load(), LoadOutcome::Fresh));
        let mut layout = WorkspaceLayout::new("Main").with_scope(Scope::household("sample"));
        layout.open_pane(&WindowId::main(), PaneDefinition::new("today"), DockTarget::edge(Side::Right)).unwrap();
        store.mark_dirty("open Today");
        assert!(store.is_dirty());
        assert!(!store.due(Instant::now()), "not before the debounce");
        store.write(&layout).unwrap();
        assert!(!store.is_dirty());
        assert_eq!(store.writes(), 1);
        assert!(session_path(&dir, "sample").exists());
        match store.load() {
            LoadOutcome::Loaded(read) => assert_eq!(read.panes.len(), 1),
            other => panic!("expected the session back, got {other:?}"),
        }
        // Another household's store does not take this file.
        let other = SessionStore::for_household(Some(&dir), "other");
        assert!(matches!(other.load(), LoadOutcome::Fresh));
        let mut foreign = SessionStore::for_household(Some(&dir), "sample");
        let mut wrong = WorkspaceLayout::new("Main").with_scope(Scope::household("someone-else"));
        wrong.open_pane(&WindowId::main(), PaneDefinition::new("today"), DockTarget::edge(Side::Right)).unwrap();
        foreign.write(&wrong).unwrap();
        assert!(matches!(store.load(), LoadOutcome::Fresh), "a file scoped to another household is ignored");
        // No data directory: nothing kept, writes succeed and clear.
        let mut none = SessionStore::for_household(None, "sample");
        none.mark_dirty("x");
        none.write(&layout).unwrap();
        assert!(!none.is_dirty() && !none.persists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
