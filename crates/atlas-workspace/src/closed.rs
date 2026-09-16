//! Recently closed panes, and putting them back.
//!
//! Closing a pane by mistake is common, so the last [`CLOSED_PANES_KEPT`]
//! closed panes are remembered with enough of their surroundings to reopen
//! them where they were: the stack and tab index, and the pane that was next
//! to them. [`WorkspaceLayout::reopen`] tries the old stack first, then next
//! to the old neighbour, then wherever the caller says (normally the active
//! stack).

use crate::ids::{NodeId, PaneId, WindowId};
use crate::layout::Side;
use crate::ops::{DockTarget, OpError};
use crate::workspace::{PaneDefinition, WorkspaceLayout};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// How many closed panes are remembered.
pub const CLOSED_PANES_KEPT: usize = 20;

/// One closed pane and where it was.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosedPane {
    /// What the pane showed.
    pub definition: PaneDefinition,
    /// The window it was in.
    pub window: WindowId,
    /// The stack it was in (which may since have vanished).
    pub stack: Option<NodeId>,
    /// Its tab position in that stack.
    pub index: usize,
    /// The pane next to it: a tab-mate, or the active pane of the adjacent
    /// stack when the closed pane was alone.
    pub neighbour: Option<PaneId>,
    /// When the neighbour was in an adjacent stack, the side of the neighbour
    /// the closed pane sat on; `None` for a tab-mate.
    #[serde(default)]
    pub neighbour_side: Option<Side>,
    /// When it was closed.
    pub closed_at: DateTime<Utc>,
}

/// The most recently closed panes, newest last.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosedPanes {
    cap: usize,
    entries: VecDeque<ClosedPane>,
}

impl Default for ClosedPanes {
    fn default() -> Self {
        ClosedPanes::new(CLOSED_PANES_KEPT)
    }
}

impl ClosedPanes {
    /// A list remembering at most `cap` panes.
    pub fn new(cap: usize) -> Self {
        ClosedPanes {
            cap: cap.max(1),
            entries: VecDeque::new(),
        }
    }

    /// Remembers a closed pane, forgetting the oldest past the cap.
    pub fn record(&mut self, closed: ClosedPane) {
        log::info!("closed panes: remembered {} ({} kept)", closed.definition.kind, (self.entries.len() + 1).min(self.cap));
        self.entries.push_back(closed);
        while self.entries.len() > self.cap {
            self.entries.pop_front();
        }
    }

    /// Takes the most recently closed pane.
    pub fn pop(&mut self) -> Option<ClosedPane> {
        self.entries.pop_back()
    }

    /// Takes the entry at `index` (0 = oldest), for a "reopen this one" menu.
    pub fn take(&mut self, index: usize) -> Option<ClosedPane> {
        self.entries.remove(index)
    }

    /// Every remembered pane, oldest first.
    pub fn iter(&self) -> impl Iterator<Item = &ClosedPane> {
        self.entries.iter()
    }

    /// Number of remembered panes.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when nothing is remembered.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The cap.
    pub fn cap(&self) -> usize {
        self.cap
    }

    /// Forgets everything.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl WorkspaceLayout {
    /// Reopens a closed pane: in its old stack at its old tab index when the
    /// stack still exists; else next to its old neighbour (as a tab when they
    /// shared a stack, beside the neighbour's stack on the remembered side
    /// otherwise); else at `fallback_target` in the active window — and if
    /// even that node is gone, at the right edge of the active window.
    pub fn reopen(&mut self, closed: ClosedPane, fallback_target: DockTarget) -> Result<PaneId, OpError> {
        let ClosedPane {
            definition,
            window,
            stack,
            index,
            neighbour,
            neighbour_side,
            ..
        } = closed;

        if let Some(stack) = stack
            && self.window(&window).is_some_and(|w| w.has_node(&stack))
        {
            log::info!("reopen: {} back into its stack {stack} in {window}", definition.kind);
            return self.open_pane(&window, definition, DockTarget::Stack { node: stack, index: Some(index) });
        }

        if let Some(neighbour) = neighbour
            && let Some(neighbour_window) = self.window_id_of(&neighbour)
            && let Some(neighbour_stack) = self.stack_of(&neighbour)
        {
            let target = match neighbour_side {
                Some(side) => DockTarget::Beside {
                    node: neighbour_stack.clone(),
                    side,
                    share: None,
                },
                None => DockTarget::Stack {
                    node: neighbour_stack.clone(),
                    index: Some(index),
                },
            };
            log::info!("reopen: {} next to its old neighbour {neighbour} in {neighbour_window}", definition.kind);
            match self.open_pane(&neighbour_window, definition.clone(), target) {
                Ok(pane) => return Ok(pane),
                Err(OpError::TooSmall { .. }) | Err(OpError::TooDeep { .. }) => {
                    // There is no room beside the neighbour any more; join
                    // its stack instead — tab docking is always allowed.
                    return self.open_pane(&neighbour_window, definition, DockTarget::tab(neighbour_stack));
                }
                Err(other) => return Err(other),
            }
        }

        let active_window = self.active_window().map(|w| w.id.clone()).ok_or_else(|| OpError::UnknownWindow(WindowId::main()))?;
        let fallback_fits = fallback_target.node().is_none_or(|node| self.window(&active_window).is_some_and(|w| w.has_node(node)));
        let target = if fallback_fits { fallback_target } else { DockTarget::edge(Side::Right) };
        log::info!("reopen: {} at the fallback target {target:?} in {active_window}", definition.kind);
        self.open_pane(&active_window, definition, target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn closed(kind: &str) -> ClosedPane {
        ClosedPane {
            definition: PaneDefinition::new(kind),
            window: WindowId::main(),
            stack: None,
            index: 0,
            neighbour: None,
            neighbour_side: None,
            closed_at: Utc::now(),
        }
    }

    #[test]
    fn the_list_is_capped_and_pops_newest_first() {
        let mut list = ClosedPanes::new(2);
        list.record(closed("a"));
        list.record(closed("b"));
        list.record(closed("c"));
        assert_eq!(list.len(), 2);
        assert_eq!(list.iter().map(|c| c.definition.kind.as_str()).collect::<Vec<_>>(), ["b", "c"]);
        assert_eq!(list.pop().unwrap().definition.kind, "c");
        assert_eq!(list.take(0).unwrap().definition.kind, "b");
        assert!(list.is_empty());
    }
}
