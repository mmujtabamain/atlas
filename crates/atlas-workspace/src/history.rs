//! Undo and redo for layout changes.
//!
//! The history keeps whole snapshots. A workspace is small (a few windows, a
//! few dozen panes) and a snapshot is the one representation that makes undo
//! trivially correct for every operation, including ones added later. The UI
//! takes a snapshot *before* an operation and pushes it with a label once the
//! operation succeeded — an operation that failed or reported
//! [`crate::ops::OpError::NoOp`] leaves no entry.
//!
//! `undo` hands back the snapshot to install and stores the current state on
//! the redo stack; a new `push` clears the redo stack, as every editor does.

use crate::workspace::WorkspaceLayout;

/// Default number of undo steps kept.
pub const DEFAULT_HISTORY_LIMIT: usize = 100;

/// One undoable step: what the workspace looked like before `label` happened.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryEntry {
    /// What the step did, for the Undo menu item ("Move pane", "Close Today").
    pub label: String,
    /// The workspace before the step.
    pub before: WorkspaceLayout,
}

/// The undo/redo stacks.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutHistory {
    /// How many undo steps are kept; the oldest is evicted past it.
    pub limit: usize,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
}

impl Default for LayoutHistory {
    fn default() -> Self {
        LayoutHistory::new(DEFAULT_HISTORY_LIMIT)
    }
}

impl LayoutHistory {
    /// A history keeping at most `limit` undo steps.
    pub fn new(limit: usize) -> Self {
        LayoutHistory {
            limit: limit.max(1),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Records that `label` is about to change (or has just changed) the
    /// workspace whose prior state is `before`. Clears the redo stack.
    pub fn push(&mut self, label: impl Into<String>, before: WorkspaceLayout) {
        let label = label.into();
        log::info!("history: recorded '{label}' ({} undo steps)", self.undo.len() + 1);
        self.undo.push(HistoryEntry { label, before });
        if self.undo.len() > self.limit {
            let excess = self.undo.len() - self.limit;
            self.undo.drain(..excess);
        }
        self.redo.clear();
    }

    /// Steps back: returns the workspace to install and the label of the
    /// step being undone, and remembers `current` for redo.
    pub fn undo(&mut self, current: &WorkspaceLayout) -> Option<(WorkspaceLayout, String)> {
        let entry = self.undo.pop()?;
        log::info!("history: undo '{}'", entry.label);
        self.redo.push(HistoryEntry {
            label: entry.label.clone(),
            before: current.clone(),
        });
        Some((entry.before, entry.label))
    }

    /// Steps forward again after an undo.
    pub fn redo(&mut self, current: &WorkspaceLayout) -> Option<(WorkspaceLayout, String)> {
        let entry = self.redo.pop()?;
        log::info!("history: redo '{}'", entry.label);
        self.undo.push(HistoryEntry {
            label: entry.label.clone(),
            before: current.clone(),
        });
        Some((entry.before, entry.label))
    }

    /// True when there is something to undo.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// True when there is something to redo.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The label of the step `undo` would take back.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|entry| entry.label.as_str())
    }

    /// The label of the step `redo` would replay.
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|entry| entry.label.as_str())
    }

    /// The labels of every undo step, oldest first.
    pub fn labels(&self) -> Vec<&str> {
        self.undo.iter().map(|entry| entry.label.as_str()).collect()
    }

    /// Number of undo steps.
    pub fn len(&self) -> usize {
        self.undo.len()
    }

    /// True when there is nothing to undo.
    pub fn is_empty(&self) -> bool {
        self.undo.is_empty()
    }

    /// Forgets everything.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_and_redo_swap_snapshots() {
        let mut history = LayoutHistory::new(3);
        let a = WorkspaceLayout::new("a");
        let b = WorkspaceLayout::new("b");
        history.push("rename", a.clone());
        assert!(history.can_undo());
        assert!(!history.can_redo());
        let (restored, label) = history.undo(&b).unwrap();
        assert_eq!(restored, a);
        assert_eq!(label, "rename");
        assert!(history.can_redo());
        let (again, _) = history.redo(&a).unwrap();
        assert_eq!(again, b);
        assert!(!history.can_redo());
        assert_eq!(history.labels(), vec!["rename"]);
    }

    #[test]
    fn the_cap_evicts_the_oldest_and_push_clears_redo() {
        let mut history = LayoutHistory::new(2);
        history.push("one", WorkspaceLayout::new("1"));
        history.push("two", WorkspaceLayout::new("2"));
        history.push("three", WorkspaceLayout::new("3"));
        assert_eq!(history.labels(), vec!["two", "three"]);
        history.undo(&WorkspaceLayout::new("x"));
        assert!(history.can_redo());
        history.push("four", WorkspaceLayout::new("4"));
        assert!(!history.can_redo());
        assert_eq!(history.labels(), vec!["two", "four"]);
    }
}
