//! The layout tree of one window.
//!
//! A window shows a tree whose inner nodes are **splits** (children laid out
//! side by side along an axis, each taking a weighted share) and whose leaves
//! are **stacks** (panes shown as tabs, one of them active). A single-pane
//! stack looks like one pane; there is no separate "leaf pane" node, which
//! keeps every operation uniform: docking a pane onto the centre of anything
//! makes it a tab, docking it beside anything makes a new stack.
//!
//! The tree is pure data. Everything that changes it lives in [`crate::ops`],
//! which also keeps it normalized (see [`crate::ops::normalize`]): no empty
//! stack, no one-child split, no split nested in a same-axis split, weights
//! summing to one. Weights are relative shares of the parent along its axis,
//! so a split's children always fill it exactly.
//!
//! Serialized form (internally tagged, camelCase fields):
//!
//! ```json
//! { "type": "split", "id": "node_1", "axis": "horizontal", "weights": [0.66, 0.34],
//!   "children": [
//!     { "type": "stack", "id": "node_2", "panes": ["pane_1"], "activePaneId": "pane_1" },
//!     { "type": "stack", "id": "node_3", "panes": ["pane_2", "pane_3"], "activePaneId": "pane_3" }
//!   ] }
//! ```

use crate::ids::{NodeId, PaneId};
use serde::{Deserialize, Serialize};

/// The direction a split lays its children out along.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    /// Children sit left to right.
    Horizontal,
    /// Children sit top to bottom.
    Vertical,
}

impl Axis {
    /// The other axis.
    pub fn perpendicular(self) -> Axis {
        match self {
            Axis::Horizontal => Axis::Vertical,
            Axis::Vertical => Axis::Horizontal,
        }
    }
}

/// One side of a node — where a dropped pane lands relative to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    /// The axis a split has to run along to place something on this side.
    pub fn axis(self) -> Axis {
        match self {
            Side::Left | Side::Right => Axis::Horizontal,
            Side::Top | Side::Bottom => Axis::Vertical,
        }
    }

    /// True when this side comes first along its axis (left of, above).
    pub fn is_before(self) -> bool {
        matches!(self, Side::Left | Side::Top)
    }

    /// The side facing this one across a boundary (left ↔ right, top ↔ bottom).
    pub fn opposite(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
            Side::Top => Side::Bottom,
            Side::Bottom => Side::Top,
        }
    }

    /// Every side, in a stable order.
    pub fn all() -> [Side; 4] {
        [Side::Left, Side::Right, Side::Top, Side::Bottom]
    }
}

/// A rectangle in the unit square: `x`, `y`, `width`, `height` are fractions
/// of the window (0..=1). Produced by [`LayoutNode::rects`] from the weights;
/// the UI multiplies by the window size, the focus rules and the grid renderer
/// use it directly.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    /// The whole unit square.
    pub const UNIT: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
    };

    /// Right edge.
    pub fn right(&self) -> f64 {
        self.x + self.width
    }

    /// Bottom edge.
    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }

    /// Centre point.
    pub fn centre(&self) -> (f64, f64) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// The shorter side, i.e. the figure the minimum-size rule looks at.
    pub fn min_side(&self) -> f64 {
        self.width.min(self.height)
    }

    /// True when the point lies inside (left/top edges inclusive, right/bottom exclusive).
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    /// Length of the overlap of the two rects along `axis`.
    pub fn overlap_along(&self, other: &Rect, axis: Axis) -> f64 {
        let (a0, a1, b0, b1) = match axis {
            Axis::Horizontal => (self.x, self.right(), other.x, other.right()),
            Axis::Vertical => (self.y, self.bottom(), other.y, other.bottom()),
        };
        (a1.min(b1) - a0.max(b0)).max(0.0)
    }
}

/// One node of a window's layout tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum LayoutNode {
    /// Children laid out along `axis`; `weights[i]` is child `i`'s share of the
    /// split along that axis. Normalized trees have at least two children,
    /// weights summing to one and no child split of the same axis.
    Split {
        #[serde(default)]
        id: NodeId,
        axis: Axis,
        children: Vec<LayoutNode>,
        #[serde(default)]
        weights: Vec<f64>,
    },
    /// Panes shown as tabs. Normalized trees have at least one pane and an
    /// active pane that is a member.
    Stack {
        #[serde(default)]
        id: NodeId,
        panes: Vec<PaneId>,
        #[serde(default)]
        active_pane_id: Option<PaneId>,
    },
}

impl LayoutNode {
    /// A stack holding the given panes, the first one active.
    pub fn stack(id: NodeId, panes: Vec<PaneId>) -> LayoutNode {
        let active_pane_id = panes.first().cloned();
        LayoutNode::Stack { id, panes, active_pane_id }
    }

    /// A single-pane stack.
    pub fn single(id: NodeId, pane: PaneId) -> LayoutNode {
        LayoutNode::Stack {
            id,
            panes: vec![pane.clone()],
            active_pane_id: Some(pane),
        }
    }

    /// A split of the given children with the given weights (not normalized).
    pub fn split(id: NodeId, axis: Axis, children: Vec<LayoutNode>, weights: Vec<f64>) -> LayoutNode {
        LayoutNode::Split { id, axis, children, weights }
    }

    /// This node's id.
    pub fn id(&self) -> &NodeId {
        match self {
            LayoutNode::Split { id, .. } | LayoutNode::Stack { id, .. } => id,
        }
    }

    /// Replaces this node's id.
    pub fn set_id(&mut self, new_id: NodeId) {
        match self {
            LayoutNode::Split { id, .. } | LayoutNode::Stack { id, .. } => *id = new_id,
        }
    }

    /// True for a stack.
    pub fn is_stack(&self) -> bool {
        matches!(self, LayoutNode::Stack { .. })
    }

    /// True for a split.
    pub fn is_split(&self) -> bool {
        matches!(self, LayoutNode::Split { .. })
    }

    /// A split's axis; `None` for a stack.
    pub fn axis(&self) -> Option<Axis> {
        match self {
            LayoutNode::Split { axis, .. } => Some(*axis),
            LayoutNode::Stack { .. } => None,
        }
    }

    /// A split's children; empty for a stack.
    pub fn children(&self) -> &[LayoutNode] {
        match self {
            LayoutNode::Split { children, .. } => children,
            LayoutNode::Stack { .. } => &[],
        }
    }

    /// A split's weights; empty for a stack.
    pub fn weights(&self) -> &[f64] {
        match self {
            LayoutNode::Split { weights, .. } => weights,
            LayoutNode::Stack { .. } => &[],
        }
    }

    /// A stack's panes; empty for a split.
    pub fn stack_panes(&self) -> &[PaneId] {
        match self {
            LayoutNode::Stack { panes, .. } => panes,
            LayoutNode::Split { .. } => &[],
        }
    }

    /// A stack's active pane; `None` for a split.
    pub fn active_pane(&self) -> Option<&PaneId> {
        match self {
            LayoutNode::Stack { active_pane_id, .. } => active_pane_id.as_ref(),
            LayoutNode::Split { .. } => None,
        }
    }

    /// The node with the given id, anywhere in this subtree.
    pub fn find(&self, id: &NodeId) -> Option<&LayoutNode> {
        if self.id() == id {
            return Some(self);
        }
        self.children().iter().find_map(|child| child.find(id))
    }

    /// Mutable variant of [`LayoutNode::find`].
    pub fn find_mut(&mut self, id: &NodeId) -> Option<&mut LayoutNode> {
        if self.id() == id {
            return Some(self);
        }
        match self {
            LayoutNode::Split { children, .. } => children.iter_mut().find_map(|child| child.find_mut(id)),
            LayoutNode::Stack { .. } => None,
        }
    }

    /// The id of the stack that holds `pane`.
    pub fn find_stack_of(&self, pane: &PaneId) -> Option<NodeId> {
        match self {
            LayoutNode::Stack { id, panes, .. } => panes.contains(pane).then(|| id.clone()),
            LayoutNode::Split { children, .. } => children.iter().find_map(|child| child.find_stack_of(pane)),
        }
    }

    /// True when `pane` is somewhere in this subtree.
    pub fn contains_pane(&self, pane: &PaneId) -> bool {
        self.find_stack_of(pane).is_some()
    }

    /// The id of the split directly above `node`; `None` for the root or an unknown id.
    pub fn parent_of(&self, node: &NodeId) -> Option<NodeId> {
        match self {
            LayoutNode::Stack { .. } => None,
            LayoutNode::Split { id, children, .. } => {
                if children.iter().any(|child| child.id() == node) {
                    Some(id.clone())
                } else {
                    children.iter().find_map(|child| child.parent_of(node))
                }
            }
        }
    }

    /// The position of `node` among its parent's children, with the parent's id.
    pub fn position_of(&self, node: &NodeId) -> Option<(NodeId, usize)> {
        match self {
            LayoutNode::Stack { .. } => None,
            LayoutNode::Split { id, children, .. } => {
                if let Some(index) = children.iter().position(|child| child.id() == node) {
                    Some((id.clone(), index))
                } else {
                    children.iter().find_map(|child| child.position_of(node))
                }
            }
        }
    }

    /// The ids of every split above `node`, nearest first. Empty for the root.
    pub fn ancestors_of(&self, node: &NodeId) -> Vec<NodeId> {
        let mut path = Vec::new();
        if self.collect_path(node, &mut path) {
            // The path is root-first and includes the node itself; the
            // ancestors are everything but the last entry, nearest first.
            path.pop();
            path.reverse();
            path
        } else {
            Vec::new()
        }
    }

    /// Pushes the ids from this node down to `target` onto `path`; true when found.
    fn collect_path(&self, target: &NodeId, path: &mut Vec<NodeId>) -> bool {
        path.push(self.id().clone());
        if self.id() == target {
            return true;
        }
        for child in self.children() {
            if child.collect_path(target, path) {
                return true;
            }
        }
        path.pop();
        false
    }

    /// Every pane in the subtree, pre-order (left/top first).
    pub fn panes(&self) -> Vec<PaneId> {
        let mut out = Vec::new();
        self.walk(&mut |node| {
            if let LayoutNode::Stack { panes, .. } = node {
                out.extend(panes.iter().cloned());
            }
        });
        out
    }

    /// Every node id in the subtree, pre-order.
    pub fn node_ids(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        self.walk(&mut |node| out.push(node.id().clone()));
        out
    }

    /// Calls `visit` on every node of the subtree, pre-order.
    pub fn walk(&self, visit: &mut dyn FnMut(&LayoutNode)) {
        visit(self);
        for child in self.children() {
            child.walk(visit);
        }
    }

    /// Number of stacks in the subtree.
    pub fn leaf_count(&self) -> usize {
        match self {
            LayoutNode::Stack { .. } => 1,
            LayoutNode::Split { children, .. } => children.iter().map(LayoutNode::leaf_count).sum(),
        }
    }

    /// Number of split levels above the deepest stack (a lone stack has depth 0).
    pub fn depth(&self) -> usize {
        match self {
            LayoutNode::Stack { .. } => 0,
            LayoutNode::Split { children, .. } => 1 + children.iter().map(LayoutNode::depth).max().unwrap_or(0),
        }
    }

    /// The rectangle of every node (splits and stacks) in the unit square,
    /// pre-order. The root fills the square; a split hands each child a slice
    /// along its axis proportional to the child's weight.
    pub fn rects(&self) -> Vec<(NodeId, Rect)> {
        let mut out = Vec::new();
        self.collect_rects(Rect::UNIT, &mut out);
        out
    }

    /// The rectangle of every stack, pre-order.
    pub fn stack_rects(&self) -> Vec<(NodeId, Rect)> {
        let stacks: std::collections::HashSet<NodeId> = self.stack_ids().into_iter().collect();
        self.rects().into_iter().filter(|(id, _)| stacks.contains(id)).collect()
    }

    /// The rectangle of every pane — a pane's rect is its stack's rect.
    pub fn pane_rects(&self) -> Vec<(PaneId, Rect)> {
        let mut out = Vec::new();
        for (id, rect) in self.rects() {
            if let Some(LayoutNode::Stack { panes, .. }) = self.find(&id) {
                out.extend(panes.iter().map(|pane| (pane.clone(), rect)));
            }
        }
        out
    }

    /// The rectangle of one node.
    pub fn rect_of(&self, node: &NodeId) -> Option<Rect> {
        self.rects().into_iter().find(|(id, _)| id == node).map(|(_, rect)| rect)
    }

    /// The ids of every stack, pre-order.
    pub fn stack_ids(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        self.walk(&mut |node| {
            if node.is_stack() {
                out.push(node.id().clone());
            }
        });
        out
    }

    fn collect_rects(&self, rect: Rect, out: &mut Vec<(NodeId, Rect)>) {
        out.push((self.id().clone(), rect));
        if let LayoutNode::Split { axis, children, weights, .. } = self {
            let shares = effective_weights(weights, children.len());
            let mut offset = 0.0;
            for (child, share) in children.iter().zip(shares) {
                let child_rect = match axis {
                    Axis::Horizontal => Rect {
                        x: rect.x + offset * rect.width,
                        y: rect.y,
                        width: share * rect.width,
                        height: rect.height,
                    },
                    Axis::Vertical => Rect {
                        x: rect.x,
                        y: rect.y + offset * rect.height,
                        width: rect.width,
                        height: share * rect.height,
                    },
                };
                child.collect_rects(child_rect, out);
                offset += share;
            }
        }
    }
}

/// The weights as shares summing to one. Missing or invalid weights (wrong
/// length, non-finite, non-positive) fall back to equal shares so a tree that
/// is not yet normalized still has a picture.
pub fn effective_weights(weights: &[f64], count: usize) -> Vec<f64> {
    if count == 0 {
        return Vec::new();
    }
    let valid = weights.len() == count && weights.iter().all(|w| w.is_finite() && *w > 0.0);
    if !valid {
        return vec![1.0 / count as f64; count];
    }
    let sum: f64 = weights.iter().sum();
    weights.iter().map(|w| w / sum).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> LayoutNode {
        // [ pane_1 | [ pane_2,pane_3 / pane_4 ] ] with 0.5 / 0.5 and 0.25 / 0.75.
        LayoutNode::split(
            NodeId::new("root"),
            Axis::Horizontal,
            vec![
                LayoutNode::single(NodeId::new("left"), PaneId::new("pane_1")),
                LayoutNode::split(
                    NodeId::new("right"),
                    Axis::Vertical,
                    vec![
                        LayoutNode::stack(NodeId::new("top"), vec![PaneId::new("pane_2"), PaneId::new("pane_3")]),
                        LayoutNode::single(NodeId::new("bottom"), PaneId::new("pane_4")),
                    ],
                    vec![0.25, 0.75],
                ),
            ],
            vec![0.5, 0.5],
        )
    }

    #[test]
    fn navigation_helpers_agree_on_the_sample_tree() {
        let tree = sample();
        assert_eq!(tree.find_stack_of(&PaneId::new("pane_3")), Some(NodeId::new("top")));
        assert_eq!(tree.parent_of(&NodeId::new("top")), Some(NodeId::new("right")));
        assert_eq!(tree.parent_of(&NodeId::new("root")), None);
        assert_eq!(tree.ancestors_of(&NodeId::new("bottom")), vec![NodeId::new("right"), NodeId::new("root")]);
        assert_eq!(tree.position_of(&NodeId::new("bottom")), Some((NodeId::new("right"), 1)));
        assert_eq!(tree.panes().iter().map(|p| p.as_str()).collect::<Vec<_>>(), ["pane_1", "pane_2", "pane_3", "pane_4"]);
        assert_eq!(tree.node_ids().len(), 5);
        assert_eq!(tree.leaf_count(), 3);
        assert_eq!(tree.depth(), 2);
    }

    #[test]
    fn rects_follow_the_weights() {
        let tree = sample();
        let bottom = tree.rect_of(&NodeId::new("bottom")).unwrap();
        assert!((bottom.x - 0.5).abs() < 1e-12);
        assert!((bottom.y - 0.25).abs() < 1e-12);
        assert!((bottom.width - 0.5).abs() < 1e-12);
        assert!((bottom.height - 0.75).abs() < 1e-12);
        assert_eq!(tree.stack_rects().len(), 3);
        assert_eq!(tree.pane_rects().len(), 4);
    }

    #[test]
    fn serialized_form_is_tagged_and_camel_cased() {
        let json = serde_json::to_value(sample()).unwrap();
        assert_eq!(json["type"], "split");
        assert_eq!(json["axis"], "horizontal");
        assert_eq!(json["children"][1]["children"][0]["type"], "stack");
        assert_eq!(json["children"][1]["children"][0]["activePaneId"], "pane_2");
        let back: LayoutNode = serde_json::from_value(json).unwrap();
        assert_eq!(back, sample());
    }

    #[test]
    fn invalid_weights_fall_back_to_equal_shares() {
        assert_eq!(effective_weights(&[], 2), vec![0.5, 0.5]);
        assert_eq!(effective_weights(&[1.0, -1.0], 2), vec![0.5, 0.5]);
        assert_eq!(effective_weights(&[3.0, 1.0], 2), vec![0.75, 0.25]);
    }
}
