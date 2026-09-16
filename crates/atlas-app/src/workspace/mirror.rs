//! Reading the dock engine's layout back into the model.
//!
//! gpui-kit's [`DockArea`](gpui_kit::component::dock::DockArea) reports every
//! edit — a divider dragged, a tab chosen, a tab closed, later a pane dragged
//! into another place — as one `LayoutChanged` event, and `dump` describes the
//! result as a [`PanelState`] tree with the **measured** pixel size of every
//! split slot. This module turns that tree into the model's [`LayoutNode`]:
//!
//! | engine (`PanelInfo`) | model |
//! |---|---|
//! | `Stack { sizes, axis }` — a split | `LayoutNode::Split`, weights = sizes ÷ their sum |
//! | `Tabs { active_index }` — a tab group | `LayoutNode::Stack`, active pane = the tab at that index |
//! | `Panel(json)` with `paneId` — one pane's own state | one member of the stack |
//!
//! Slots the engine measured at nothing (a split it has not laid out yet) come
//! back as equal weights, exactly as the model does for missing weights. The
//! engine's tree may carry the redundant structure the model forbids — the
//! centre root is always a split, even around one tab group — so the caller
//! normalizes the result (`WindowLayout::replace_root` does).
//!
//! Node ids are the model's, not the engine's: a stack keeps the id of the
//! model stack that already held its first pane and a split keeps the id of
//! the model split with the same axis over the same panes, so an edit that
//! changed one place leaves the ids everywhere else alone. Anything new is
//! minted from the model's [`IdSource`].

use std::collections::HashSet;
use std::fmt;

use atlas_workspace::{Axis, IdSource, LayoutNode, NodeId, PaneId};
use gpui_kit::component::dock::{PanelInfo, PanelState};

use super::pane::PANEL_NAME;

/// Why a dumped layout could not be read back.
#[derive(Debug, Clone, PartialEq)]
pub enum MirrorError {
    /// A leaf that is not one of this app's panes.
    UnknownPanel(String),
    /// A pane leaf without the `paneId` its `dump` writes.
    PaneWithoutId,
    /// A free-floating tiles canvas, which this workspace never creates.
    TilesUnsupported,
}

impl fmt::Display for MirrorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MirrorError::UnknownPanel(name) => write!(f, "the dock holds a panel of kind {name:?} that is not a pane of this workspace"),
            MirrorError::PaneWithoutId => f.write_str("a pane in the dock carries no pane id"),
            MirrorError::TilesUnsupported => f.write_str("the dock holds a tiles canvas, which the workspace does not use"),
        }
    }
}

impl std::error::Error for MirrorError {}

/// The model tree the engine's centre region describes; `None` for an empty
/// centre. `previous` is the model's current tree, whose node ids are kept
/// where the structure still matches; new ids come from `ids`.
pub fn layout_node_from_state(state: &PanelState, previous: Option<&LayoutNode>, ids: &mut IdSource) -> Result<Option<LayoutNode>, MirrorError> {
    let mut used = HashSet::new();
    convert(state, previous, ids, &mut used)
}

fn convert(state: &PanelState, previous: Option<&LayoutNode>, ids: &mut IdSource, used: &mut HashSet<NodeId>) -> Result<Option<LayoutNode>, MirrorError> {
    match &state.info {
        PanelInfo::Stack { sizes, .. } => {
            let axis = match state.info.axis() {
                Some(gpui_kit::Axis::Vertical) => Axis::Vertical,
                _ => Axis::Horizontal,
            };
            let mut children = Vec::new();
            let mut weights = Vec::new();
            for (index, child) in state.children.iter().enumerate() {
                if let Some(node) = convert(child, previous, ids, used)? {
                    children.push(node);
                    weights.push(sizes.get(index).map(|size| f64::from(f32::from(*size))).unwrap_or(0.0));
                }
            }
            match children.len() {
                0 => Ok(None),
                1 => Ok(children.pop()),
                _ => {
                    let panes: HashSet<PaneId> = children.iter().flat_map(LayoutNode::panes).collect();
                    let id = split_id_for(previous, axis, &panes, used).unwrap_or_else(|| ids.mint_node());
                    used.insert(id.clone());
                    Ok(Some(LayoutNode::Split { id, axis, children, weights: shares(&weights) }))
                }
            }
        }
        PanelInfo::Tabs { active_index } => {
            let mut panes = Vec::new();
            for child in &state.children {
                panes.push(pane_id_of(child)?);
            }
            if panes.is_empty() {
                return Ok(None);
            }
            let active_pane_id = panes.get(*active_index).or(panes.first()).cloned();
            let id = stack_id_for(previous, &panes, used).unwrap_or_else(|| ids.mint_node());
            used.insert(id.clone());
            Ok(Some(LayoutNode::Stack { id, panes, active_pane_id }))
        }
        // A pane standing in for a whole region: a stack of one.
        PanelInfo::Panel(_) => {
            let pane = pane_id_of(state)?;
            let id = stack_id_for(previous, std::slice::from_ref(&pane), used).unwrap_or_else(|| ids.mint_node());
            used.insert(id.clone());
            Ok(Some(LayoutNode::single(id, pane)))
        }
        PanelInfo::Tiles { .. } => Err(MirrorError::TilesUnsupported),
    }
}

/// The pane a leaf stands for.
fn pane_id_of(state: &PanelState) -> Result<PaneId, MirrorError> {
    if state.panel_name != PANEL_NAME {
        return Err(MirrorError::UnknownPanel(state.panel_name.clone()));
    }
    match &state.info {
        PanelInfo::Panel(info) => info.get("paneId").and_then(|value| value.as_str()).filter(|id| !id.is_empty()).map(PaneId::new).ok_or(MirrorError::PaneWithoutId),
        _ => Err(MirrorError::PaneWithoutId),
    }
}

/// Measured sizes as shares of their sum; equal shares when nothing was measured.
fn shares(sizes: &[f64]) -> Vec<f64> {
    let total: f64 = sizes.iter().filter(|size| size.is_finite() && **size > 0.0).sum();
    if total <= 0.0 || sizes.iter().any(|size| !size.is_finite() || *size <= 0.0) {
        return vec![1.0 / sizes.len() as f64; sizes.len()];
    }
    sizes.iter().map(|size| size / total).collect()
}

/// The id of the model stack that held the first of these panes, if unused so far.
fn stack_id_for(previous: Option<&LayoutNode>, panes: &[PaneId], used: &HashSet<NodeId>) -> Option<NodeId> {
    let first = panes.first()?;
    let id = previous?.find_stack_of(first)?;
    (!used.contains(&id)).then_some(id)
}

/// The id of the model split with this axis over exactly these panes, if unused so far.
fn split_id_for(previous: Option<&LayoutNode>, axis: Axis, panes: &HashSet<PaneId>, used: &HashSet<NodeId>) -> Option<NodeId> {
    let mut found = None;
    previous?.walk(&mut |node| {
        if found.is_some() || node.axis() != Some(axis) || used.contains(node.id()) {
            return;
        }
        let covered: HashSet<PaneId> = node.panes().into_iter().collect();
        if covered == *panes {
            found = Some(node.id().clone());
        }
    });
    found
}

/// True when the two trees show the same panes in the same arrangement with
/// the same active tabs, and every weight agrees within `tolerance`. Node ids
/// do not count: the engine's edit may have been mirrored with fresh ids.
pub fn trees_match(a: Option<&LayoutNode>, b: Option<&LayoutNode>, tolerance: f64) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => nodes_match(a, b, tolerance),
        _ => false,
    }
}

fn nodes_match(a: &LayoutNode, b: &LayoutNode, tolerance: f64) -> bool {
    match (a, b) {
        (LayoutNode::Stack { panes: pa, active_pane_id: aa, .. }, LayoutNode::Stack { panes: pb, active_pane_id: ab, .. }) => pa == pb && aa == ab,
        (LayoutNode::Split { axis: xa, children: ca, weights: wa, .. }, LayoutNode::Split { axis: xb, children: cb, weights: wb, .. }) => {
            xa == xb
                && ca.len() == cb.len()
                && wa.len() == wb.len()
                && wa.iter().zip(wb).all(|(a, b)| (a - b).abs() <= tolerance)
                && ca.iter().zip(cb).all(|(a, b)| nodes_match(a, b, tolerance))
        }
        _ => false,
    }
}

/// True when the two trees differ in anything but weights (and ids).
pub fn structure_differs(a: Option<&LayoutNode>, b: Option<&LayoutNode>) -> bool {
    !trees_match(a, b, f64::INFINITY)
}

/// The splits whose weights differ between two trees of the same structure,
/// by the id they carry in `b`, pre-order. Empty when the structures differ:
/// then the question has no answer.
pub fn resized_splits(a: Option<&LayoutNode>, b: Option<&LayoutNode>, tolerance: f64) -> Vec<NodeId> {
    let mut out = Vec::new();
    if let (Some(a), Some(b)) = (a, b)
        && !structure_differs(Some(a), Some(b))
    {
        collect_resized(a, b, tolerance, &mut out);
    }
    out
}

fn collect_resized(a: &LayoutNode, b: &LayoutNode, tolerance: f64, out: &mut Vec<NodeId>) {
    if let (LayoutNode::Split { weights: wa, children: ca, .. }, LayoutNode::Split { id, weights: wb, children: cb, .. }) = (a, b) {
        if wa.len() != wb.len() || wa.iter().zip(wb).any(|(a, b)| (a - b).abs() > tolerance) {
            out.push(id.clone());
        }
        for (child_a, child_b) in ca.iter().zip(cb) {
            collect_resized(child_a, child_b, tolerance, out);
        }
    }
}

/// The active pane of every stack in the tree.
pub fn displayed_panes(root: Option<&LayoutNode>) -> HashSet<PaneId> {
    let mut out = HashSet::new();
    if let Some(root) = root {
        root.walk(&mut |node| {
            if let Some(active) = node.active_pane() {
                out.insert(active.clone());
            }
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::component::dock::DockAreaState;
    use gpui_kit::px;
    use serde_json::json;

    fn pane(id: &str) -> PanelState {
        let mut state = PanelState::new(PANEL_NAME);
        state.info = PanelInfo::panel(json!({ "paneId": id }));
        state
    }

    fn tabs(active: usize, panes: &[&str]) -> PanelState {
        let mut state = PanelState::new("TabPanel");
        state.info = PanelInfo::tabs(active);
        for pane_id in panes {
            state.add_child(pane(pane_id));
        }
        state
    }

    fn split(axis: gpui_kit::Axis, children: Vec<(PanelState, f32)>) -> PanelState {
        let mut state = PanelState::new("StackPanel");
        let sizes = children.iter().map(|(_, size)| px(*size)).collect();
        state.info = PanelInfo::stack(sizes, axis);
        for (child, _) in children {
            state.add_child(child);
        }
        state
    }

    #[test]
    fn a_dumped_area_becomes_a_normalizable_tree_with_weights_from_sizes() {
        // [ 1 | [ 2,3 / 4 ] ] with the right column split 600 over 200 and
        // the columns 650 over 350: what `DockArea::dump` writes.
        let state = DockAreaState {
            version: None,
            center: split(
                gpui_kit::Axis::Horizontal,
                vec![(tabs(0, &["pane_1"]), 650.), (split(gpui_kit::Axis::Vertical, vec![(tabs(1, &["pane_2", "pane_3"]), 600.), (tabs(0, &["pane_4"]), 200.)]), 350.)],
            ),
            left_dock: None,
            right_dock: None,
            bottom_dock: None,
        };
        let mut ids = IdSource::new();
        let root = layout_node_from_state(&state.center, None, &mut ids).unwrap().expect("a tree");
        assert_eq!(root.axis(), Some(Axis::Horizontal));
        assert!((root.weights()[0] - 0.65).abs() < 1e-6 && (root.weights()[1] - 0.35).abs() < 1e-6, "{:?}", root.weights());
        let right = &root.children()[1];
        assert_eq!(right.axis(), Some(Axis::Vertical));
        assert!((right.weights()[0] - 0.75).abs() < 1e-6, "{:?}", right.weights());
        assert_eq!(right.children()[0].active_pane(), Some(&PaneId::new("pane_3")), "the active tab is the second one");
        assert_eq!(root.panes().iter().map(PaneId::as_str).collect::<Vec<_>>(), ["pane_1", "pane_2", "pane_3", "pane_4"]);
        assert_eq!(atlas_workspace::grid::render_numbered(&root, 4, 4), "1113\n1113\n1113\n1114");
        // Every node got a minted id and they are all different.
        let node_ids = root.node_ids();
        let unique: HashSet<&NodeId> = node_ids.iter().collect();
        assert_eq!(unique.len(), node_ids.len());
    }

    #[test]
    fn an_empty_centre_and_a_lone_group_collapse() {
        let mut ids = IdSource::new();
        let empty = split(gpui_kit::Axis::Horizontal, Vec::new());
        assert_eq!(layout_node_from_state(&empty, None, &mut ids).unwrap(), None);
        let lone = split(gpui_kit::Axis::Horizontal, vec![(tabs(0, &["pane_1"]), 0.)]);
        let root = layout_node_from_state(&lone, None, &mut ids).unwrap().expect("a stack");
        assert!(root.is_stack());
        assert_eq!(root.stack_panes(), [PaneId::new("pane_1")]);
    }

    #[test]
    fn unmeasured_sizes_fall_back_to_equal_shares() {
        let mut ids = IdSource::new();
        let state = split(gpui_kit::Axis::Vertical, vec![(tabs(0, &["pane_1"]), 0.), (tabs(0, &["pane_2"]), 0.)]);
        let root = layout_node_from_state(&state, None, &mut ids).unwrap().expect("a split");
        assert_eq!(root.weights(), [0.5, 0.5]);
    }

    #[test]
    fn model_node_ids_survive_where_the_structure_did_not_change() {
        let previous = LayoutNode::split(
            NodeId::new("node_1"),
            Axis::Horizontal,
            vec![LayoutNode::single(NodeId::new("node_2"), PaneId::new("pane_1")), LayoutNode::single(NodeId::new("node_3"), PaneId::new("pane_2"))],
            vec![0.5, 0.5],
        );
        let mut ids = IdSource::new();
        ids.observe_node(&NodeId::new("node_3"));
        // The same two panes, resized: same ids.
        let resized = split(gpui_kit::Axis::Horizontal, vec![(tabs(0, &["pane_1"]), 700.), (tabs(0, &["pane_2"]), 300.)]);
        let root = layout_node_from_state(&resized, Some(&previous), &mut ids).unwrap().unwrap();
        assert_eq!(root.node_ids(), previous.node_ids());
        assert!(!structure_differs(Some(&root), Some(&previous)));
        assert!(!trees_match(Some(&root), Some(&previous), 0.01), "the weights moved");
        // A third pane beside them: the old ids stay, one new id is minted.
        let grown = split(gpui_kit::Axis::Horizontal, vec![(tabs(0, &["pane_1"]), 500.), (tabs(0, &["pane_2"]), 300.), (tabs(0, &["pane_3"]), 200.)]);
        let root = layout_node_from_state(&grown, Some(&previous), &mut ids).unwrap().unwrap();
        assert_eq!(root.children()[0].id(), &NodeId::new("node_2"));
        assert_eq!(root.children()[1].id(), &NodeId::new("node_3"));
        assert_eq!(root.children()[2].id(), &NodeId::new("node_4"), "minted past the ids in use");
        assert_ne!(root.id(), &NodeId::new("node_1"), "the split now covers other panes, so it is a different split");
        assert!(structure_differs(Some(&root), Some(&previous)));
    }

    #[test]
    fn foreign_panels_and_tiles_are_refused() {
        let mut ids = IdSource::new();
        let mut foreign = PanelState::new("SomethingElse");
        foreign.info = PanelInfo::panel(json!({}));
        let state = split(gpui_kit::Axis::Horizontal, vec![(tabs(0, &[]), 100.)]);
        let mut with_foreign = state.clone();
        with_foreign.children[0].add_child(foreign);
        assert_eq!(layout_node_from_state(&with_foreign, None, &mut ids), Err(MirrorError::UnknownPanel("SomethingElse".into())));
        let mut nameless = pane("");
        nameless.info = PanelInfo::panel(json!({}));
        let mut with_nameless = state.clone();
        with_nameless.children[0].add_child(nameless);
        assert_eq!(layout_node_from_state(&with_nameless, None, &mut ids), Err(MirrorError::PaneWithoutId));
        let mut tiles = PanelState::new("Tiles");
        tiles.info = PanelInfo::tiles(Vec::new());
        assert_eq!(layout_node_from_state(&tiles, None, &mut ids), Err(MirrorError::TilesUnsupported));
    }

    #[test]
    fn resized_splits_names_the_split_whose_weights_moved() {
        let before = LayoutNode::split(
            NodeId::new("row"),
            Axis::Horizontal,
            vec![
                LayoutNode::single(NodeId::new("s1"), PaneId::new("a")),
                LayoutNode::split(NodeId::new("column"), Axis::Vertical, vec![LayoutNode::single(NodeId::new("s2"), PaneId::new("b")), LayoutNode::single(NodeId::new("s3"), PaneId::new("c"))], vec![0.5, 0.5]),
            ],
            vec![0.5, 0.5],
        );
        let mut inner_moved = before.clone();
        if let LayoutNode::Split { children, .. } = &mut inner_moved
            && let LayoutNode::Split { weights, .. } = &mut children[1]
        {
            *weights = vec![0.3, 0.7];
        }
        assert_eq!(resized_splits(Some(&before), Some(&inner_moved), 0.005), [NodeId::new("column")]);
        assert!(resized_splits(Some(&before), Some(&before), 0.005).is_empty());
        // A different structure is not a resize at all.
        let stacked = LayoutNode::stack(NodeId::new("s"), vec![PaneId::new("a"), PaneId::new("b"), PaneId::new("c")]);
        assert!(resized_splits(Some(&before), Some(&stacked), 0.005).is_empty());
        assert!(resized_splits(None, Some(&before), 0.005).is_empty());
    }

    #[test]
    fn displayed_panes_are_the_active_tabs() {
        let tree = LayoutNode::split(
            NodeId::new("n1"),
            Axis::Horizontal,
            vec![LayoutNode::stack(NodeId::new("n2"), vec![PaneId::new("a"), PaneId::new("b")]), LayoutNode::single(NodeId::new("n3"), PaneId::new("c"))],
            vec![0.5, 0.5],
        );
        let displayed = displayed_panes(Some(&tree));
        assert_eq!(displayed, HashSet::from([PaneId::new("a"), PaneId::new("c")]));
        assert!(displayed_panes(None).is_empty());
    }
}
