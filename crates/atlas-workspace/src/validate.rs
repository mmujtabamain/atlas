//! The invariants of a workspace, checked in one place.
//!
//! [`WorkspaceLayout::validate`] returns every violation it finds rather than
//! the first, so a corrupt file can be reported in full. The operations in
//! [`crate::ops`] and [`crate::workspace`] keep a live workspace free of
//! violations (and assert so in debug builds); files are checked on load and
//! rejected — kept aside, never deleted — when they fail.
//!
//! The rules:
//!
//! - every pane id is unique and every open pane is in exactly one stack of
//!   exactly one window;
//! - every pane in a tree has a definition and every definition is in a tree;
//! - every stack has at least one pane and an active pane that is a member;
//! - every split has at least two children, one weight per child, every
//!   weight positive, weights summing to one (within `1e-6`);
//! - no split is nested in a split of the same axis (a normalized tree has
//!   no redundant structure);
//! - node ids are unique across the whole workspace;
//! - window ids are unique, exactly one window is the main window, floating
//!   windows are never empty, a window's active pane is in that window, and
//!   the active window exists.

use crate::ids::{NodeId, PaneId, WindowId};
use crate::layout::{Axis, LayoutNode};
use crate::workspace::{WindowRole, WorkspaceLayout};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

/// Tolerance for the weight sum of a split.
pub const WEIGHT_TOLERANCE: f64 = 1e-6;

/// One broken invariant.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum Violation {
    #[error("pane {pane} appears in more than one stack (windows {windows:?})")]
    DuplicatePane { pane: PaneId, windows: Vec<WindowId> },
    #[error("pane {pane} is shown in window {window} but has no definition")]
    UndefinedPane { pane: PaneId, window: WindowId },
    #[error("pane {pane} has a definition but is not shown in any window")]
    OrphanDefinition { pane: PaneId },
    #[error("stack {node} in window {window} has no panes")]
    EmptyStack { node: NodeId, window: WindowId },
    #[error("stack {node} in window {window} has active pane {active:?}, which is not one of its panes")]
    ActiveNotMember { node: NodeId, window: WindowId, active: Option<PaneId> },
    #[error("split {node} in window {window} has {children} child(ren); a split needs at least two")]
    RedundantSplit { node: NodeId, window: WindowId, children: usize },
    #[error("split {node} in window {window} has {weights} weights for {children} children")]
    WeightCount { node: NodeId, window: WindowId, weights: usize, children: usize },
    #[error("split {node} in window {window} has a weight that is not a positive finite number: {weight}")]
    WeightNotPositive { node: NodeId, window: WindowId, weight: f64 },
    #[error("split {node} in window {window} has weights summing to {sum:.6}, not 1")]
    WeightSum { node: NodeId, window: WindowId, sum: f64 },
    #[error("split {node} in window {window} runs along the same axis ({axis:?}) as its parent {parent}; it should have been flattened")]
    SameAxisNesting { node: NodeId, parent: NodeId, window: WindowId, axis: Axis },
    #[error("layout node id {node} is used more than once")]
    DuplicateNode { node: NodeId },
    #[error("layout node in window {window} has a blank id")]
    BlankNodeId { window: WindowId },
    #[error("window id {window} is used more than once")]
    DuplicateWindow { window: WindowId },
    #[error("the workspace has {count} main windows; it needs exactly one")]
    MainWindowCount { count: usize },
    #[error("floating window {window} is empty; floating windows close with their last pane")]
    EmptyFloatingWindow { window: WindowId },
    #[error("window {window} has active pane {pane}, which is not in that window")]
    ActivePaneNotInWindow { window: WindowId, pane: PaneId },
    #[error("window {window} shows panes but has no active pane")]
    NoActivePane { window: WindowId },
    #[error("the active window {window} does not exist")]
    UnknownActiveWindow { window: WindowId },
    #[error("window {window} has a frame that is not a positive finite rectangle")]
    InvalidFrame { window: WindowId },
}

impl WorkspaceLayout {
    /// Every broken invariant, or an empty list for a valid workspace.
    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        let mut pane_windows: HashMap<PaneId, Vec<WindowId>> = HashMap::new();
        let mut node_ids: HashSet<NodeId> = HashSet::new();
        let mut window_ids: HashSet<WindowId> = HashSet::new();

        for window in &self.windows {
            if !window_ids.insert(window.id.clone()) {
                out.push(Violation::DuplicateWindow { window: window.id.clone() });
            }
            if let Some(tree) = &window.root {
                check_node(tree, None, &window.id, &mut node_ids, &mut pane_windows, &mut out);
            } else if window.role == WindowRole::Floating {
                out.push(Violation::EmptyFloatingWindow { window: window.id.clone() });
            }
            match (&window.active_pane, &window.root) {
                (Some(pane), _) if !window.contains_pane(pane) => out.push(Violation::ActivePaneNotInWindow {
                    window: window.id.clone(),
                    pane: pane.clone(),
                }),
                (None, Some(_)) => out.push(Violation::NoActivePane { window: window.id.clone() }),
                _ => {}
            }
            if let Some(frame) = &window.frame
                && !frame.is_sane()
            {
                out.push(Violation::InvalidFrame { window: window.id.clone() });
            }
        }

        let main_count = self.windows.iter().filter(|window| window.role == WindowRole::Main).count();
        if main_count != 1 {
            out.push(Violation::MainWindowCount { count: main_count });
        }

        let mut shown: Vec<(&PaneId, &Vec<WindowId>)> = pane_windows.iter().collect();
        shown.sort_by(|a, b| a.0.cmp(b.0));
        for (pane, windows) in shown {
            if windows.len() > 1 {
                out.push(Violation::DuplicatePane {
                    pane: pane.clone(),
                    windows: windows.clone(),
                });
            }
            if !self.panes.contains_key(pane) {
                out.push(Violation::UndefinedPane {
                    pane: pane.clone(),
                    window: windows[0].clone(),
                });
            }
        }
        for pane in self.panes.keys() {
            if !pane_windows.contains_key(pane) {
                out.push(Violation::OrphanDefinition { pane: pane.clone() });
            }
        }

        if let Some(active) = &self.active_window
            && !window_ids.contains(active)
        {
            out.push(Violation::UnknownActiveWindow { window: active.clone() });
        }
        out
    }
}

/// Checks one node and its subtree, recording pane and node ids as it goes.
fn check_node(node: &LayoutNode, parent: Option<(&NodeId, Axis)>, window: &WindowId, node_ids: &mut HashSet<NodeId>, pane_windows: &mut HashMap<PaneId, Vec<WindowId>>, out: &mut Vec<Violation>) {
    if node.id().is_blank() {
        out.push(Violation::BlankNodeId { window: window.clone() });
    } else if !node_ids.insert(node.id().clone()) {
        out.push(Violation::DuplicateNode { node: node.id().clone() });
    }
    match node {
        LayoutNode::Stack { id, panes, active_pane_id } => {
            if panes.is_empty() {
                out.push(Violation::EmptyStack {
                    node: id.clone(),
                    window: window.clone(),
                });
            } else if !active_pane_id.as_ref().is_some_and(|active| panes.contains(active)) {
                out.push(Violation::ActiveNotMember {
                    node: id.clone(),
                    window: window.clone(),
                    active: active_pane_id.clone(),
                });
            }
            for pane in panes {
                pane_windows.entry(pane.clone()).or_default().push(window.clone());
            }
        }
        LayoutNode::Split { id, axis, children, weights } => {
            if children.len() < 2 {
                out.push(Violation::RedundantSplit {
                    node: id.clone(),
                    window: window.clone(),
                    children: children.len(),
                });
            }
            if weights.len() != children.len() {
                out.push(Violation::WeightCount {
                    node: id.clone(),
                    window: window.clone(),
                    weights: weights.len(),
                    children: children.len(),
                });
            } else {
                for weight in weights {
                    if !weight.is_finite() || *weight <= 0.0 {
                        out.push(Violation::WeightNotPositive {
                            node: id.clone(),
                            window: window.clone(),
                            weight: *weight,
                        });
                    }
                }
                let sum: f64 = weights.iter().sum();
                if (sum - 1.0).abs() > WEIGHT_TOLERANCE {
                    out.push(Violation::WeightSum {
                        node: id.clone(),
                        window: window.clone(),
                        sum,
                    });
                }
            }
            if let Some((parent_id, parent_axis)) = parent
                && parent_axis == *axis
            {
                out.push(Violation::SameAxisNesting {
                    node: id.clone(),
                    parent: parent_id.clone(),
                    window: window.clone(),
                    axis: *axis,
                });
            }
            for child in children {
                check_node(child, Some((id, *axis)), window, node_ids, pane_windows, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PaneId;
    use crate::workspace::{PaneDefinition, WindowLayout};

    #[test]
    fn a_hand_built_broken_workspace_reports_every_problem() {
        let mut layout = WorkspaceLayout::new("broken");
        let pane = PaneId::new("pane_1");
        let ghost = PaneId::new("pane_ghost");
        layout.panes.insert(ghost.clone(), PaneDefinition::new("today"));
        layout.windows[0].root = Some(LayoutNode::Split {
            id: NodeId::new("n1"),
            axis: Axis::Horizontal,
            children: vec![
                LayoutNode::Stack {
                    id: NodeId::new("n2"),
                    panes: vec![pane.clone()],
                    active_pane_id: None,
                },
                LayoutNode::Split {
                    id: NodeId::new("n2"),
                    axis: Axis::Horizontal,
                    children: vec![LayoutNode::Stack {
                        id: NodeId::new("n3"),
                        panes: vec![],
                        active_pane_id: None,
                    }],
                    weights: vec![0.5, 0.5],
                },
            ],
            weights: vec![0.9, 0.9],
        });
        layout.windows[0].active_pane = Some(ghost.clone());
        layout.windows.push(WindowLayout::new(WindowId::new("window_1"), WindowRole::Floating));
        layout.active_window = Some(WindowId::new("window_9"));

        let violations = layout.validate();
        let has = |predicate: fn(&Violation) -> bool| violations.iter().any(predicate);
        assert!(has(|v| matches!(v, Violation::ActiveNotMember { .. })));
        assert!(has(|v| matches!(v, Violation::DuplicateNode { .. })));
        assert!(has(|v| matches!(v, Violation::RedundantSplit { .. })));
        assert!(has(|v| matches!(v, Violation::WeightCount { .. })));
        assert!(has(|v| matches!(v, Violation::WeightSum { .. })));
        assert!(has(|v| matches!(v, Violation::SameAxisNesting { .. })));
        assert!(has(|v| matches!(v, Violation::EmptyStack { .. })));
        assert!(has(|v| matches!(v, Violation::UndefinedPane { .. })));
        assert!(has(|v| matches!(v, Violation::OrphanDefinition { .. })));
        assert!(has(|v| matches!(v, Violation::EmptyFloatingWindow { .. })));
        assert!(has(|v| matches!(v, Violation::ActivePaneNotInWindow { .. })));
        assert!(has(|v| matches!(v, Violation::UnknownActiveWindow { .. })));
        for violation in &violations {
            assert!(!violation.to_string().is_empty());
        }
    }
}
