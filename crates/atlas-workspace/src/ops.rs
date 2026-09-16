//! The tree algebra: every way a window's layout tree changes.
//!
//! All operations here are **transactional**: they compute the result on a
//! clone of the tree (and of the id source), and only when the whole
//! operation succeeded do they swap the result in. A failed operation leaves
//! the tree byte-for-byte as it was, which is what lets the UI show an error
//! and carry on without repairing anything.
//!
//! Every mutating operation ends by running [`normalize`], the one routine
//! that restores the invariants ([`crate::validate`] checks them): no empty
//! stacks, no one-child splits, no same-axis nesting, weights summing to one,
//! a valid active pane in every stack. Because normalization runs after every
//! step, no operation ever has to reason about a half-repaired tree.
//!
//! Where a dropped pane lands is a [`DockTarget`]:
//!
//! - **centre** of a stack → the pane becomes a tab ([`DockTarget::Stack`]);
//! - **beside** any node — a stack, a split, i.e. a whole group — → a new
//!   single-pane stack on that side ([`DockTarget::Beside`]). Docking beside a
//!   split is "ancestor docking": the pane spans the whole group;
//! - **beside the window** → beside the root ([`DockTarget::WindowEdge`]);
//! - **beside a run of siblings** in a split ([`DockTarget::BesideRange`]):
//!   a same-axis split is always flattened, so `1|2|3` has no "2–3 group" to
//!   dock beneath; the range target wraps children `from..=to` in a group of
//!   their own and docks beside that group, so 4 can span 2 and 3 alone
//!   (`123/123/144`). The levels a drag can offer for one pane and side are
//!   listed by [`ancestor_targets`].
//!
//! The weight rule: the new pane takes `share` of the target (default
//! [`DEFAULT_SHARE`], clamped to `0.15..=0.85`). When the target already sits
//! in a split of the right axis the new stack becomes a sibling and takes its
//! share **from the target's weight only** — docking A right of C in `[B|C]`
//! gives `wB, wC·(1−s), wC·s` and B does not move. Otherwise the target is
//! wrapped in a new split of that axis with weights `[1−s, s]` (or `[s, 1−s]`
//! for left/top); when the target is itself a split of that axis the wrap is
//! flattened by normalization, so the group's children scale by `1−s` and
//! keep their ratios — that is what "spanning the group" means.
//!
//! Moving a pane ([`move_pane`]) is a removal followed by an insert, with
//! one rule about the space the pane leaves behind. When the pane was alone
//! in its stack, its slot in the parent split is freed; if the pane is moved
//! **beside** a node inside one of its former siblings — the neighbour it is
//! dropped next to, or something inside that neighbour's group — that
//! sibling's slot takes the freed weight before the insert divides it, so
//! the other siblings keep exactly the weights they had: moving 3 to the
//! right of 1 in `[1|2|3]` gives `[1|3|2]` with 2 as wide as before, and
//! moving 3 below 1 gives `[[1/3]|2]` with 2 untouched. A pane moved anywhere
//! else (into a stack as a tab, beside the split itself, to a window edge,
//! into another group) leaves its weight to the remaining siblings in
//! proportion, so they keep their ratio.
//!
//! A minimum-size rule ([`SplitLimits`]) rejects a split that would leave any
//! pane's rectangle narrower or shorter than `min_share` of the window, or
//! nest splits deeper than `max_depth` ([`check_limits`]). Centre (tab)
//! docking is never rejected, so a pane can always be placed somewhere.

use crate::ids::{IdSource, NodeId, PaneId, WindowId};
use crate::layout::{Axis, LayoutNode, Rect, Side, effective_weights};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// The share a new pane takes of its target when the caller does not say.
pub const DEFAULT_SHARE: f64 = 0.35;
/// The smallest share a caller may ask for.
pub const MIN_SHARE_ARG: f64 = 0.15;
/// The largest share a caller may ask for.
pub const MAX_SHARE_ARG: f64 = 0.85;
/// Weights whose sum is within this distance of one are left alone.
pub const WEIGHT_SUM_TOLERANCE: f64 = 1e-9;
/// Upper bound on normalization passes; a well-formed tree settles in two.
const MAX_NORMALIZE_PASSES: usize = 32;

/// Where a pane lands.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum DockTarget {
    /// Into a stack as a tab, at `index` (clamped) or at the end.
    Stack { node: NodeId, index: Option<usize> },
    /// A new single-pane stack on `side` of `node` — a stack or a split. The
    /// pane takes `share` of the node (see the module docs for the default).
    Beside { node: NodeId, side: Side, share: Option<f64> },
    /// A new single-pane stack on `side` of the whole window (beside the root).
    /// On an empty window this makes the first stack.
    WindowEdge { side: Side, share: Option<f64> },
    /// A new single-pane stack on `side` of the run of children `from..=to`
    /// of the split `split`, which is perpendicular to `side` (a split of
    /// columns takes a range target above or below; a split of rows, left or
    /// right). The run is wrapped in a group of its own first, so the pane
    /// spans exactly those siblings. A range of one child is `Beside` that
    /// child; the whole range is `Beside` the split.
    BesideRange { split: NodeId, from: usize, to: usize, side: Side, share: Option<f64> },
}

impl DockTarget {
    /// Tab-dock at the end of a stack.
    pub fn tab(node: NodeId) -> DockTarget {
        DockTarget::Stack { node, index: None }
    }

    /// Dock beside a node with the default share.
    pub fn beside(node: NodeId, side: Side) -> DockTarget {
        DockTarget::Beside { node, side, share: None }
    }

    /// Dock at a window edge with the default share.
    pub fn edge(side: Side) -> DockTarget {
        DockTarget::WindowEdge { side, share: None }
    }

    /// The node the target names, if any.
    pub fn node(&self) -> Option<&NodeId> {
        match self {
            DockTarget::Stack { node, .. } | DockTarget::Beside { node, .. } => Some(node),
            DockTarget::BesideRange { split, .. } => Some(split),
            DockTarget::WindowEdge { .. } => None,
        }
    }

    /// The side a beside-target docks on; `None` for tab docking.
    pub fn side(&self) -> Option<Side> {
        match self {
            DockTarget::Stack { .. } => None,
            DockTarget::Beside { side, .. } | DockTarget::WindowEdge { side, .. } | DockTarget::BesideRange { side, .. } => Some(*side),
        }
    }

    /// True when the target creates a new split (anything but tab docking).
    pub fn splits(&self) -> bool {
        !matches!(self, DockTarget::Stack { .. })
    }

    /// The same target aimed at another node (used after normalization replaced the original).
    fn with_node(&self, new_node: NodeId) -> DockTarget {
        match self {
            DockTarget::Stack { index, .. } => DockTarget::Stack { node: new_node, index: *index },
            DockTarget::Beside { side, share, .. } => DockTarget::Beside {
                node: new_node,
                side: *side,
                share: *share,
            },
            DockTarget::WindowEdge { .. } => self.clone(),
            DockTarget::BesideRange { from, to, side, share, .. } => DockTarget::BesideRange { split: new_node, from: *from, to: *to, side: *side, share: *share },
        }
    }

    /// A range target reduced to what it means: `Beside` the one child when
    /// the range has one member, `Beside` the split when it covers every
    /// child, itself otherwise. `Err` when the range or the axis is wrong.
    pub fn simplified(&self, tree: &LayoutNode) -> Result<DockTarget, OpError> {
        let DockTarget::BesideRange { split, from, to, side, share } = self else {
            return Ok(self.clone());
        };
        let node = tree.find(split).ok_or_else(|| OpError::UnknownNode(split.clone()))?;
        let children = node.children();
        if children.is_empty() {
            return Err(OpError::NotASplit(split.clone()));
        }
        if node.axis() == Some(side.axis()) {
            return Err(OpError::InvalidRange { split: split.clone(), reason: format!("the split runs along the same axis as the {side:?} side; a range can only be docked across it") });
        }
        if from > to || *to >= children.len() {
            return Err(OpError::InvalidRange { split: split.clone(), reason: format!("children {from}..={to} do not exist (the split has {} children)", children.len()) });
        }
        if *from == 0 && *to + 1 == children.len() {
            return Ok(DockTarget::Beside { node: split.clone(), side: *side, share: *share });
        }
        if from == to {
            return Ok(DockTarget::Beside { node: children[*from].id().clone(), side: *side, share: *share });
        }
        Ok(self.clone())
    }
}

/// The minimum-size rule: a split is refused when it would make any pane's
/// rectangle smaller than `min_share` of the window along either axis, or
/// nest splits deeper than `max_depth`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SplitLimits {
    /// Smallest allowed side of a pane rectangle, as a fraction of the window.
    pub min_share: f64,
    /// Deepest allowed nesting of splits (a lone stack has depth 0).
    pub max_depth: usize,
}

impl Default for SplitLimits {
    fn default() -> Self {
        SplitLimits { min_share: 0.08, max_depth: 12 }
    }
}

impl SplitLimits {
    /// No limits at all — for tests and for trees the user built deliberately.
    pub fn unlimited() -> Self {
        SplitLimits {
            min_share: 0.0,
            max_depth: usize::MAX,
        }
    }
}

/// Every way an operation can refuse. Operations never panic and never leave
/// a half-applied tree behind.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum OpError {
    #[error("pane {0} is not open in this window")]
    UnknownPane(PaneId),
    #[error("layout node {0} does not exist in this window")]
    UnknownNode(NodeId),
    #[error("window {0} does not exist")]
    UnknownWindow(WindowId),
    #[error("pane {0} is already placed in the layout")]
    PaneAlreadyPlaced(PaneId),
    #[error("layout node {0} is a split, not a stack; panes can only be tab-docked into stacks")]
    NotAStack(NodeId),
    #[error("layout node {0} is a stack, not a split; only splits have weights")]
    NotASplit(NodeId),
    #[error("invalid weights for split {node}: {reason}")]
    InvalidWeights { node: NodeId, reason: String },
    #[error("the operation would change nothing")]
    NoOp,
    #[error("the split would leave a pane only {smallest:.3} of the window wide or tall (minimum {min_share:.3}); dock it as a tab instead")]
    TooSmall { min_share: f64, smallest: f64 },
    #[error("the split would nest {depth} levels deep (maximum {max_depth}); dock it as a tab instead")]
    TooDeep { max_depth: usize, depth: usize },
    #[error("window {0} has no panes")]
    EmptyWindow(WindowId),
    #[error("pane {0} is alone in its stack; there is nothing to unstack it from")]
    NotStacked(PaneId),
    #[error("cannot detach pane {0}: it is already the only pane of its floating window")]
    AlreadyDetached(PaneId),
    #[error("invalid sibling range in split {split}: {reason}")]
    InvalidRange { split: NodeId, reason: String },
}

/// What [`normalize`] did. `replacements` records every node id that
/// vanished together with the id of the node now standing where it stood
/// (a collapsed split → its surviving child; a flattened split → the parent
/// it merged into), so a target that named the vanished node can be aimed at
/// its successor. `dropped` lists ids that vanished without a successor
/// (empty stacks and splits that lost every child).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NormalizeReport {
    /// True when the tree differs from the one handed in.
    pub changed: bool,
    /// `(removed node, node that replaced it)`, in the order they happened.
    pub replacements: Vec<(NodeId, NodeId)>,
    /// Nodes that vanished without a replacement.
    pub dropped: Vec<NodeId>,
}

impl NormalizeReport {
    /// Follows the replacement chain from `node` to the id that now stands
    /// in its place. A node that was not replaced resolves to itself; a
    /// dropped node resolves to itself too — the caller finds out it is gone
    /// when the lookup fails.
    pub fn resolve(&self, node: &NodeId) -> NodeId {
        let mut current = node.clone();
        // The chain is finite; the bound guards against a pathological cycle.
        for _ in 0..=self.replacements.len() {
            match self.replacements.iter().find(|(removed, _)| *removed == current) {
                Some((_, survivor)) if *survivor != current => current = survivor.clone(),
                _ => break,
            }
        }
        current
    }
}

/// What [`remove`] took out: where the pane was, so a closed pane can be
/// reopened in the same place (or next to the same neighbour).
#[derive(Clone, Debug, PartialEq)]
pub struct Removed {
    /// The stack the pane was in (it may have vanished — see `stack_survived`).
    pub stack: NodeId,
    /// The pane's tab position in that stack.
    pub index: usize,
    /// The pane the user would call its neighbour: the next tab of the same
    /// stack, or, when the pane was alone, the active pane of the adjacent
    /// sibling in the parent split.
    pub neighbour: Option<PaneId>,
    /// The side of the neighbour the pane sat on when the neighbour came
    /// from an adjacent stack; `None` when the neighbour was a tab-mate.
    pub neighbour_side: Option<Side>,
    /// True when the stack still exists after the removal.
    pub stack_survived: bool,
    /// What normalization did afterwards.
    pub report: NormalizeReport,
}

/// Clamps a requested share into the allowed range; `None` or NaN gives the default.
pub fn clamp_share(share: Option<f64>) -> f64 {
    match share {
        Some(value) if value.is_finite() => value.clamp(MIN_SHARE_ARG, MAX_SHARE_ARG),
        _ => DEFAULT_SHARE,
    }
}

/// Places a pane that is not yet in the tree. Transactional.
pub fn insert(root: &mut Option<LayoutNode>, ids: &mut IdSource, pane: &PaneId, target: &DockTarget, limits: &SplitLimits) -> Result<(), OpError> {
    let mut work = root.clone();
    let mut work_ids = ids.clone();
    insert_into(&mut work, &mut work_ids, pane, target)?;
    if target.splits() {
        check_limits(root.as_ref(), work.as_ref(), limits)?;
    }
    *root = work;
    *ids = work_ids;
    Ok(())
}

/// Takes a pane out of the tree. The emptied stack disappears, a split left
/// with one child is replaced by that child, weights renormalise; unrelated
/// subtrees keep their ids and weights. Transactional.
pub fn remove(root: &mut Option<LayoutNode>, pane: &PaneId) -> Result<Removed, OpError> {
    let mut work = root.clone();
    let removed = remove_from(&mut work, pane)?;
    *root = work;
    Ok(removed)
}

/// Moves a pane to a new target in the same tree: remove, then insert, on a
/// clone. Dropping a pane back where it already is — its own stack's centre,
/// or a side it already occupies — is [`OpError::NoOp`] and changes nothing,
/// so the caller records no history entry for it. If the removal collapsed
/// the target node away (the target was the split `[A|B]` and A is the pane
/// being moved) the target is re-aimed at the node that replaced it.
///
/// A pane that was alone in its stack and lands beside a node inside one of
/// its former siblings hands its freed slot to that sibling first (see the
/// module docs), so the siblings it did not touch keep their weights.
pub fn move_pane(root: &mut Option<LayoutNode>, ids: &mut IdSource, pane: &PaneId, target: &DockTarget, limits: &SplitLimits) -> Result<(), OpError> {
    let tree = root.as_ref().ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
    let source_stack = tree.find_stack_of(pane).ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
    if let Some(node) = target.node()
        && tree.find(node).is_none()
    {
        return Err(OpError::UnknownNode(node.clone()));
    }
    if is_noop(tree, pane, &source_stack, target) {
        return Err(OpError::NoOp);
    }
    if let DockTarget::Stack { node, index: Some(index) } = target
        && *node == source_stack
    {
        // A tab reorder inside one stack: the stack must not go through
        // remove (a single-pane stack would vanish), so it is done in place.
        return reorder_in_stack(root, pane, *index);
    }

    let handover = slot_handover(tree, &source_stack, target);
    let mut work = root.clone();
    let mut work_ids = ids.clone();
    let removed = remove_from(&mut work, pane)?;
    if let Some(handover) = &handover {
        hand_over_slot(&mut work, handover);
    }
    let aimed = match target.node() {
        Some(node) => range_after_removal(target, tree, &source_stack)?.with_node(removed.report.resolve(node)),
        None => target.clone(),
    };
    insert_into(&mut work, &mut work_ids, pane, &aimed)?;
    if aimed.splits() {
        check_limits(root.as_ref(), work.as_ref(), limits)?;
    }
    if work == *root {
        return Err(OpError::NoOp);
    }
    log::info!("layout: moved pane {pane} to {aimed:?}");
    *root = work;
    *ids = work_ids;
    Ok(())
}

/// A range target after the moved pane's stack left the tree: when that
/// stack was a direct child of the range's split, the children after it
/// shift down by one, so the range does too. A range that named only the
/// moved pane's own stack is refused — there is nothing left to dock beside.
fn range_after_removal(target: &DockTarget, tree: &LayoutNode, source_stack: &NodeId) -> Result<DockTarget, OpError> {
    let DockTarget::BesideRange { split, from, to, side, share } = target else {
        return Ok(target.clone());
    };
    let Some((parent, index)) = tree.position_of(source_stack) else {
        return Ok(target.clone());
    };
    let alone = tree.find(source_stack).map(LayoutNode::stack_panes).is_some_and(|panes| panes.len() == 1);
    if parent != *split || !alone {
        return Ok(target.clone());
    }
    let (from, to) = if index < *from {
        (from - 1, to - 1)
    } else if index <= *to {
        if from == to {
            return Err(OpError::InvalidRange { split: split.clone(), reason: "the range names only the pane being moved".to_string() });
        }
        (*from, to - 1)
    } else {
        (*from, *to)
    };
    Ok(DockTarget::BesideRange { split: split.clone(), from, to, side: *side, share: *share })
}

/// The dock targets a drag can offer for `pane` on `side`, from the narrowest
/// to the broadest: beside the pane's own stack; then, when the stack sits in
/// a split that runs across `side`, beside the runs of siblings that include
/// the stack — growing towards the first child, then towards the last — as
/// [`DockTarget::BesideRange`]; then beside each further ancestor split; and
/// finally the window's edge. Levels that would produce the same tree as an
/// earlier one (a full range is the split itself; the root split is the
/// window) are left out, so every entry is a different result.
pub fn ancestor_targets(root: &LayoutNode, pane: &PaneId, side: Side) -> Vec<DockTarget> {
    let Some(stack) = root.find_stack_of(pane) else {
        return Vec::new();
    };
    let mut levels: Vec<DockTarget> = vec![DockTarget::beside(stack.clone(), side)];
    let mut node = stack;
    while let Some((parent_id, index)) = root.position_of(&node) {
        let Some(parent) = root.find(&parent_id) else {
            break;
        };
        let count = parent.children().len();
        if parent.axis() != Some(side.axis()) && count > 1 {
            // Runs ending at this child, growing towards the first sibling…
            for from in (0..index).rev() {
                if from == 0 && index + 1 == count {
                    break;
                }
                levels.push(DockTarget::BesideRange { split: parent_id.clone(), from, to: index, side, share: None });
            }
            // …then runs starting at this child, growing towards the last.
            for to in index + 1..count {
                if index == 0 && to + 1 == count {
                    break;
                }
                levels.push(DockTarget::BesideRange { split: parent_id.clone(), from: index, to, side, share: None });
            }
        }
        if parent_id != *root.id() {
            levels.push(DockTarget::beside(parent_id.clone(), side));
        }
        node = parent_id;
    }
    levels.push(DockTarget::edge(side));
    levels
}

/// The slot a moved pane frees, and the former sibling that takes it: the
/// parent split, its children's weights before the move, and the sibling.
#[derive(Clone, Debug, PartialEq)]
struct SlotHandover {
    /// The split the pane's stack was a direct child of.
    parent: NodeId,
    /// Every child of that split with its share, in order, before the move.
    shares: Vec<(NodeId, f64)>,
    /// The child the freed share goes to.
    to: NodeId,
    /// The share the pane's stack held.
    freed: f64,
}

/// Decides whether moving `pane` (alone in `source_stack`) to `target` hands
/// its slot to a former sibling: only for a *beside* target that names a node
/// inside another child of the same parent split. `None` means the freed
/// weight is shared in proportion, which is what normalization does anyway.
fn slot_handover(tree: &LayoutNode, source_stack: &NodeId, target: &DockTarget) -> Option<SlotHandover> {
    let DockTarget::Beside { node: target_node, .. } = target else {
        return None;
    };
    let alone = tree.find(source_stack).map(LayoutNode::stack_panes).is_some_and(|panes| panes.len() == 1);
    if !alone {
        return None;
    }
    let (parent_id, index) = tree.position_of(source_stack)?;
    let parent = tree.find(&parent_id)?;
    let children = parent.children();
    let shares_list = effective_weights(parent.weights(), children.len());
    let sibling = children.iter().enumerate().find(|(position, child)| *position != index && child.find(target_node).is_some())?;
    Some(SlotHandover {
        parent: parent_id,
        shares: children.iter().zip(&shares_list).map(|(child, share)| (child.id().clone(), *share)).collect(),
        to: sibling.1.id().clone(),
        freed: shares_list[index],
    })
}

/// Applies a [`SlotHandover`] to the tree the pane was just removed from:
/// the parent's surviving children get their old shares back and the chosen
/// sibling gets the freed share on top. A parent that collapsed into that
/// sibling (it had two children) already handed everything over.
fn hand_over_slot(root: &mut Option<LayoutNode>, handover: &SlotHandover) {
    let Some(LayoutNode::Split { children, weights, .. }) = root.as_mut().and_then(|tree| tree.find_mut(&handover.parent)) else {
        return;
    };
    let mut restored = Vec::with_capacity(children.len());
    for child in children.iter() {
        let Some((_, share)) = handover.shares.iter().find(|(id, _)| id == child.id()) else {
            // A child this move did not create is unknown here only if the
            // tree changed shape in a way the rule does not describe; the
            // proportional weights normalization left are the honest answer.
            return;
        };
        let bonus = if child.id() == &handover.to { handover.freed } else { 0.0 };
        restored.push(share + bonus);
    }
    *weights = effective_weights(&restored, children.len());
}

/// Merges `pane` into the stack that holds `onto`, as its last tab.
pub fn stack(root: &mut Option<LayoutNode>, ids: &mut IdSource, pane: &PaneId, onto: &PaneId, limits: &SplitLimits) -> Result<(), OpError> {
    let tree = root.as_ref().ok_or_else(|| OpError::UnknownPane(onto.clone()))?;
    let target_stack = tree.find_stack_of(onto).ok_or_else(|| OpError::UnknownPane(onto.clone()))?;
    move_pane(root, ids, pane, &DockTarget::tab(target_stack), limits)
}

/// Pulls `pane` out of a multi-pane stack into a new stack on `side` of that stack.
pub fn unstack(root: &mut Option<LayoutNode>, ids: &mut IdSource, pane: &PaneId, side: Side, limits: &SplitLimits) -> Result<(), OpError> {
    let tree = root.as_ref().ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
    let source_stack = tree.find_stack_of(pane).ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
    let tab_count = tree.find(&source_stack).map(|node| node.stack_panes().len()).unwrap_or(0);
    if tab_count < 2 {
        return Err(OpError::NotStacked(pane.clone()));
    }
    move_pane(root, ids, pane, &DockTarget::beside(source_stack, side), limits)
}

/// Replaces a split's weights. They must match the child count and all be
/// finite and positive; they are normalised to sum to one. Identical weights
/// are [`OpError::NoOp`].
pub fn resize(root: &mut Option<LayoutNode>, split: &NodeId, weights: &[f64]) -> Result<(), OpError> {
    let tree = root.as_mut().ok_or_else(|| OpError::UnknownNode(split.clone()))?;
    let node = tree.find_mut(split).ok_or_else(|| OpError::UnknownNode(split.clone()))?;
    let LayoutNode::Split { children, weights: current, .. } = node else {
        return Err(OpError::NotASplit(split.clone()));
    };
    if weights.len() != children.len() {
        return Err(OpError::InvalidWeights {
            node: split.clone(),
            reason: format!("{} weights for {} children", weights.len(), children.len()),
        });
    }
    if let Some(bad) = weights.iter().find(|w| !w.is_finite() || **w <= 0.0) {
        return Err(OpError::InvalidWeights {
            node: split.clone(),
            reason: format!("weight {bad} is not a positive finite number"),
        });
    }
    let normalised = effective_weights(weights, children.len());
    let unchanged = current.len() == normalised.len() && current.iter().zip(&normalised).all(|(a, b)| (a - b).abs() <= WEIGHT_SUM_TOLERANCE);
    if unchanged {
        return Err(OpError::NoOp);
    }
    *current = normalised;
    Ok(())
}

/// Makes `pane` the active tab of its stack.
pub fn set_active(root: &mut Option<LayoutNode>, pane: &PaneId) -> Result<(), OpError> {
    let tree = root.as_mut().ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
    let stack_id = tree.find_stack_of(pane).ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
    if let Some(LayoutNode::Stack { active_pane_id, .. }) = tree.find_mut(&stack_id) {
        *active_pane_id = Some(pane.clone());
    }
    Ok(())
}

/// Restores the tree invariants and reports what changed. Runs to a fixpoint
/// and is idempotent: normalizing a normalized tree changes nothing and
/// reports `changed == false`.
///
/// Per pass, bottom-up: an empty stack is dropped; a stack's active pane is
/// repaired to a member (the first, by default); a split's children that are
/// same-axis splits are flattened into it (inner weights scaled by the slot
/// weight, so the picture is unchanged); a split with no children is dropped
/// and one with a single child is replaced by that child, which keeps its
/// own id; weights that are missing or invalid become equal, and weights
/// whose sum strays from one are renormalised.
pub fn normalize(root: &mut Option<LayoutNode>) -> NormalizeReport {
    let mut report = NormalizeReport::default();
    for _ in 0..MAX_NORMALIZE_PASSES {
        let before = root.clone();
        *root = root.take().and_then(|node| normalize_node(node, &mut report));
        if *root == before {
            break;
        }
        report.changed = true;
    }
    report
}

// ---------------------------------------------------------------------------
// Internals: the same steps without the transaction wrapper.
// ---------------------------------------------------------------------------

/// Inserts into the working tree and normalizes; the caller owns the transaction.
fn insert_into(root: &mut Option<LayoutNode>, ids: &mut IdSource, pane: &PaneId, target: &DockTarget) -> Result<(), OpError> {
    if let Some(tree) = root.as_ref()
        && tree.contains_pane(pane)
    {
        return Err(OpError::PaneAlreadyPlaced(pane.clone()));
    }
    match target {
        DockTarget::Stack { node, index } => {
            let tree = root.as_mut().ok_or_else(|| OpError::UnknownNode(node.clone()))?;
            let found = tree.find_mut(node).ok_or_else(|| OpError::UnknownNode(node.clone()))?;
            let LayoutNode::Stack { panes, active_pane_id, .. } = found else {
                return Err(OpError::NotAStack(node.clone()));
            };
            let at = index.map(|i| i.min(panes.len())).unwrap_or(panes.len());
            panes.insert(at, pane.clone());
            *active_pane_id = Some(pane.clone());
        }
        DockTarget::Beside { node, side, share } => {
            let tree = root.as_mut().ok_or_else(|| OpError::UnknownNode(node.clone()))?;
            let new_stack = LayoutNode::single(ids.mint_node(), pane.clone());
            place_beside(tree, ids, node, *side, clamp_share(*share), new_stack)?;
        }
        DockTarget::WindowEdge { side, share } => match root {
            None => *root = Some(LayoutNode::single(ids.mint_node(), pane.clone())),
            Some(tree) => {
                let root_id = tree.id().clone();
                let new_stack = LayoutNode::single(ids.mint_node(), pane.clone());
                place_beside(tree, ids, &root_id, *side, clamp_share(*share), new_stack)?;
            }
        },
        DockTarget::BesideRange { split, .. } => {
            let tree = root.as_mut().ok_or_else(|| OpError::UnknownNode(split.clone()))?;
            match target.simplified(tree)? {
                DockTarget::BesideRange { split, from, to, side, share } => {
                    let group = group_range(tree, ids, &split, from, to)?;
                    let new_stack = LayoutNode::single(ids.mint_node(), pane.clone());
                    place_beside(tree, ids, &group, side, clamp_share(share), new_stack)?;
                }
                // A range of one child or of every child is plain beside-docking.
                simpler => return insert_into(root, ids, pane, &simpler),
            }
        }
    }
    normalize(root);
    Ok(())
}

/// Wraps children `from..=to` of `split` in a new split of the same axis and
/// returns the new group's id. The group takes the sum of the children's
/// weights and the children keep their ratios inside it. The caller has
/// already checked the range.
fn group_range(tree: &mut LayoutNode, ids: &mut IdSource, split: &NodeId, from: usize, to: usize) -> Result<NodeId, OpError> {
    let Some(LayoutNode::Split { axis, children, weights, .. }) = tree.find_mut(split) else {
        return Err(OpError::NotASplit(split.clone()));
    };
    if from > to || to >= children.len() {
        return Err(OpError::InvalidRange { split: split.clone(), reason: format!("children {from}..={to} do not exist (the split has {} children)", children.len()) });
    }
    let shares = effective_weights(weights, children.len());
    let group_id = ids.mint_node();
    let grouped: Vec<LayoutNode> = children.drain(from..=to).collect();
    let group_shares: Vec<f64> = shares[from..=to].to_vec();
    let group_weight: f64 = group_shares.iter().sum();
    let group = LayoutNode::Split { id: group_id.clone(), axis: *axis, children: grouped, weights: effective_weights(&group_shares, to - from + 1) };
    children.insert(from, group);
    let mut new_weights: Vec<f64> = shares[..from].to_vec();
    new_weights.push(group_weight);
    new_weights.extend_from_slice(&shares[to + 1..]);
    *weights = new_weights;
    Ok(group_id)
}

/// Puts `new_node` on `side` of `target`: as a sibling when the parent split
/// already runs along that axis (taking `share` of the target's weight only),
/// otherwise by wrapping the target in a new split of that axis.
fn place_beside(tree: &mut LayoutNode, ids: &mut IdSource, target: &NodeId, side: Side, share: f64, new_node: LayoutNode) -> Result<(), OpError> {
    if tree.find(target).is_none() {
        return Err(OpError::UnknownNode(target.clone()));
    }
    let axis = side.axis();
    let sibling_slot = match tree.position_of(target) {
        Some((parent_id, index)) if tree.find(&parent_id).and_then(LayoutNode::axis) == Some(axis) => Some((parent_id, index)),
        _ => None,
    };
    match sibling_slot {
        Some((parent_id, index)) => {
            let Some(LayoutNode::Split { children, weights, .. }) = tree.find_mut(&parent_id) else {
                return Err(OpError::UnknownNode(parent_id));
            };
            // Weights may be unnormalised in a tree that came from a file;
            // work on their effective shares so the target really gives up
            // `share` of its own slice and nothing else moves.
            let mut shares = effective_weights(weights, children.len());
            let target_share = shares[index];
            shares[index] = target_share * (1.0 - share);
            let at = if side.is_before() { index } else { index + 1 };
            children.insert(at, new_node);
            shares.insert(at, target_share * share);
            *weights = shares;
        }
        None => {
            let slot = tree.find_mut(target).ok_or_else(|| OpError::UnknownNode(target.clone()))?;
            let placeholder = LayoutNode::Stack {
                id: NodeId::default(),
                panes: Vec::new(),
                active_pane_id: None,
            };
            let old = std::mem::replace(slot, placeholder);
            let (children, weights) = if side.is_before() {
                (vec![new_node, old], vec![share, 1.0 - share])
            } else {
                (vec![old, new_node], vec![1.0 - share, share])
            };
            *slot = LayoutNode::Split {
                id: ids.mint_node(),
                axis,
                children,
                weights,
            };
        }
    }
    Ok(())
}

/// Removes from the working tree and normalizes; the caller owns the transaction.
fn remove_from(root: &mut Option<LayoutNode>, pane: &PaneId) -> Result<Removed, OpError> {
    let tree = root.as_mut().ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
    let stack_id = tree.find_stack_of(pane).ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
    let (index, neighbour, neighbour_side, stack_survived) = describe_neighbour(tree, &stack_id, pane);
    if let Some(LayoutNode::Stack { panes, active_pane_id, .. }) = tree.find_mut(&stack_id) {
        panes.retain(|member| member != pane);
        if active_pane_id.as_ref() == Some(pane) {
            // The tab that slid into the closed one's position takes over,
            // or the last one when the closed tab was at the end.
            *active_pane_id = panes.get(index).or(panes.last()).cloned();
        }
    }
    let report = normalize(root);
    Ok(Removed {
        stack: stack_id,
        index,
        neighbour,
        neighbour_side,
        stack_survived,
        report,
    })
}

/// Works out the removed pane's tab index and neighbour before anything changes.
fn describe_neighbour(tree: &LayoutNode, stack_id: &NodeId, pane: &PaneId) -> (usize, Option<PaneId>, Option<Side>, bool) {
    let panes = tree.find(stack_id).map(LayoutNode::stack_panes).unwrap_or(&[]);
    let index = panes.iter().position(|member| member == pane).unwrap_or(0);
    if panes.len() > 1 {
        let tab_mate = panes.get(index + 1).or_else(|| index.checked_sub(1).and_then(|i| panes.get(i))).cloned();
        return (index, tab_mate, None, true);
    }
    // The pane is alone: its neighbour is the adjacent sibling in the parent
    // split (the one before it, else the one after), and the side records
    // where the pane sat relative to that sibling.
    let Some((parent_id, position)) = tree.position_of(stack_id) else {
        return (index, None, None, false);
    };
    let Some(parent) = tree.find(&parent_id) else {
        return (index, None, None, false);
    };
    let axis = parent.axis().unwrap_or(Axis::Horizontal);
    let (sibling, sibling_is_before) = match position.checked_sub(1).and_then(|i| parent.children().get(i)) {
        Some(before) => (Some(before), true),
        None => (parent.children().get(position + 1), false),
    };
    let Some(sibling) = sibling else {
        return (index, None, None, false);
    };
    let neighbour = first_active_pane(sibling);
    let side = match (axis, sibling_is_before) {
        (Axis::Horizontal, true) => Side::Right,
        (Axis::Horizontal, false) => Side::Left,
        (Axis::Vertical, true) => Side::Bottom,
        (Axis::Vertical, false) => Side::Top,
    };
    (index, neighbour, Some(side), false)
}

/// The active pane of the first stack (pre-order) in a subtree.
fn first_active_pane(node: &LayoutNode) -> Option<PaneId> {
    match node {
        LayoutNode::Stack { panes, active_pane_id, .. } => active_pane_id.clone().or_else(|| panes.first().cloned()),
        LayoutNode::Split { children, .. } => children.iter().find_map(first_active_pane),
    }
}

/// Reorders a tab inside its own stack; the stack keeps every pane.
fn reorder_in_stack(root: &mut Option<LayoutNode>, pane: &PaneId, index: usize) -> Result<(), OpError> {
    let tree = root.as_mut().ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
    let stack_id = tree.find_stack_of(pane).ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
    let Some(LayoutNode::Stack { panes, active_pane_id, .. }) = tree.find_mut(&stack_id) else {
        return Err(OpError::UnknownPane(pane.clone()));
    };
    let before = panes.clone();
    panes.retain(|member| member != pane);
    let at = index.min(panes.len());
    panes.insert(at, pane.clone());
    if *panes == before {
        return Err(OpError::NoOp);
    }
    *active_pane_id = Some(pane.clone());
    Ok(())
}

/// True when dropping `pane` on `target` would reproduce the tree it is in.
fn is_noop(tree: &LayoutNode, pane: &PaneId, source_stack: &NodeId, target: &DockTarget) -> bool {
    let panes = tree.find(source_stack).map(LayoutNode::stack_panes).unwrap_or(&[]);
    let alone = panes.len() == 1;
    match target {
        DockTarget::Stack { node, index } => {
            if node != source_stack {
                return false;
            }
            match index {
                None => true,
                Some(wanted) => {
                    let current = panes.iter().position(|member| member == pane).unwrap_or(0);
                    // After the pane is taken out there are len-1 slots; the
                    // insert index is clamped to that, so the final position
                    // is min(wanted, len-1).
                    (*wanted).min(panes.len().saturating_sub(1)) == current
                }
            }
        }
        DockTarget::Beside { node, side, .. } => {
            if !alone {
                return false;
            }
            if node == source_stack {
                return true;
            }
            already_beside(tree, source_stack, node, *side)
        }
        DockTarget::WindowEdge { side, .. } => {
            if !alone {
                return false;
            }
            if tree.id() == source_stack {
                return true;
            }
            at_edge_of(tree, tree.id(), source_stack, *side)
        }
        // A proper range wraps siblings that are not grouped today, so the
        // tree always changes; the one- and all-children forms are judged as
        // the beside-targets they simplify to.
        DockTarget::BesideRange { .. } => match target.simplified(tree) {
            Ok(DockTarget::BesideRange { .. }) | Err(_) => false,
            Ok(simpler) => is_noop(tree, pane, source_stack, &simpler),
        },
    }
}

/// True when `stack` already sits on `side` of `node`: as its immediate
/// sibling in a split of that axis, or as the first/last child when `node`
/// is the split directly above the stack.
fn already_beside(tree: &LayoutNode, stack: &NodeId, node: &NodeId, side: Side) -> bool {
    let axis = side.axis();
    if let (Some((parent_s, pos_s)), Some((parent_t, pos_t))) = (tree.position_of(stack), tree.position_of(node))
        && parent_s == parent_t
        && tree.find(&parent_s).and_then(LayoutNode::axis) == Some(axis)
    {
        return if side.is_before() { pos_s + 1 == pos_t } else { pos_t + 1 == pos_s };
    }
    at_edge_of(tree, node, stack, side)
}

/// True when `stack` is the first (left/top) or last (right/bottom) direct
/// child of the split `group`, and that split runs along the side's axis.
fn at_edge_of(tree: &LayoutNode, group: &NodeId, stack: &NodeId, side: Side) -> bool {
    let Some(group_node) = tree.find(group) else {
        return false;
    };
    if group_node.axis() != Some(side.axis()) {
        return false;
    }
    let children = group_node.children();
    let edge_child = if side.is_before() { children.first() } else { children.last() };
    edge_child.map(LayoutNode::id) == Some(stack)
}

/// Applies the minimum-size rule to `after`, the tree a split produced from
/// `before`: [`OpError::TooDeep`] when splits nest deeper than the limit,
/// [`OpError::TooSmall`] when a stack's rectangle is narrower or shorter than
/// `min_share` of the window. A stack that was already below the limit in
/// `before` and did not shrink further is tolerated, so a layout loaded from
/// a file with tiny panes can still be changed elsewhere; with no `before`
/// nothing is tolerated. The UI runs this on a tree the dock engine built on
/// its own before it accepts it.
pub fn check_limits(before: Option<&LayoutNode>, after: Option<&LayoutNode>, limits: &SplitLimits) -> Result<(), OpError> {
    let Some(after_tree) = after else {
        return Ok(());
    };
    let depth = after_tree.depth();
    if depth > limits.max_depth {
        return Err(OpError::TooDeep { max_depth: limits.max_depth, depth });
    }
    let before_sides: HashMap<NodeId, f64> = before
        .map(|tree| tree.stack_rects().into_iter().map(|(id, rect)| (id, rect.min_side())).collect())
        .unwrap_or_default();
    for (id, rect) in after_tree.stack_rects() {
        let side = rect.min_side();
        if side + WEIGHT_SUM_TOLERANCE >= limits.min_share {
            continue;
        }
        let tolerated = before_sides.get(&id).is_some_and(|previous| *previous <= side + WEIGHT_SUM_TOLERANCE);
        if !tolerated {
            return Err(OpError::TooSmall {
                min_share: limits.min_share,
                smallest: side,
            });
        }
    }
    Ok(())
}

/// One bottom-up normalization pass over a node; `None` when the node vanishes.
fn normalize_node(node: LayoutNode, report: &mut NormalizeReport) -> Option<LayoutNode> {
    match node {
        LayoutNode::Stack { id, panes, active_pane_id } => {
            if panes.is_empty() {
                report.dropped.push(id);
                return None;
            }
            let active = match active_pane_id {
                Some(active) if panes.contains(&active) => Some(active),
                _ => panes.first().cloned(),
            };
            Some(LayoutNode::Stack { id, panes, active_pane_id: active })
        }
        LayoutNode::Split { id, axis, children, weights } => {
            let count = children.len();
            let valid_weights = weights.len() == count && weights.iter().all(|w| w.is_finite() && *w > 0.0);
            let slot_weights: Vec<f64> = if valid_weights { weights } else { vec![1.0 / count.max(1) as f64; count] };
            let mut new_children: Vec<LayoutNode> = Vec::with_capacity(count);
            let mut new_weights: Vec<f64> = Vec::with_capacity(count);
            for (child, slot_weight) in children.into_iter().zip(slot_weights) {
                match normalize_node(child, report) {
                    None => {}
                    Some(LayoutNode::Split {
                        id: inner_id,
                        axis: inner_axis,
                        children: inner_children,
                        weights: inner_weights,
                    }) if inner_axis == axis => {
                        // Same-axis nesting: hoist the grandchildren into
                        // this split, scaled to the slot the inner split had.
                        let inner_shares = effective_weights(&inner_weights, inner_children.len());
                        for (grandchild, share) in inner_children.into_iter().zip(inner_shares) {
                            new_children.push(grandchild);
                            new_weights.push(slot_weight * share);
                        }
                        report.replacements.push((inner_id, id.clone()));
                    }
                    Some(child) => {
                        new_children.push(child);
                        new_weights.push(slot_weight);
                    }
                }
            }
            match new_children.len() {
                0 => {
                    report.dropped.push(id);
                    None
                }
                1 => {
                    let only = new_children.pop()?;
                    report.replacements.push((id, only.id().clone()));
                    Some(only)
                }
                _ => {
                    let sum: f64 = new_weights.iter().sum();
                    if (sum - 1.0).abs() > WEIGHT_SUM_TOLERANCE {
                        for weight in &mut new_weights {
                            *weight /= sum;
                        }
                    }
                    Some(LayoutNode::Split {
                        id,
                        axis,
                        children: new_children,
                        weights: new_weights,
                    })
                }
            }
        }
    }
}

/// The unit-square rectangle of each stack, keyed by id (a convenience for callers of the focus rules).
pub fn stack_rect_map(root: &LayoutNode) -> HashMap<NodeId, Rect> {
    root.stack_rects().into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(n: u64) -> PaneId {
        PaneId::numbered(n)
    }

    /// Builds `[1 | 2 | [3 / 4]]` through the operations.
    fn three_columns() -> (Option<LayoutNode>, IdSource) {
        let mut root = None;
        let mut ids = IdSource::new();
        let limits = SplitLimits::default();
        insert(&mut root, &mut ids, &pane(1), &DockTarget::edge(Side::Right), &limits).unwrap();
        insert(&mut root, &mut ids, &pane(2), &DockTarget::WindowEdge { side: Side::Right, share: Some(0.5) }, &limits).unwrap();
        insert(
            &mut root,
            &mut ids,
            &pane(3),
            &DockTarget::WindowEdge {
                side: Side::Right,
                share: Some(1.0 / 3.0),
            },
            &limits,
        )
        .unwrap();
        let stack3 = root.as_ref().unwrap().find_stack_of(&pane(3)).unwrap();
        insert(
            &mut root,
            &mut ids,
            &pane(4),
            &DockTarget::Beside {
                node: stack3,
                side: Side::Bottom,
                share: Some(1.0 / 3.0),
            },
            &limits,
        )
        .unwrap();
        (root, ids)
    }

    #[test]
    fn window_edge_docking_flattens_into_equal_thirds() {
        let (root, _) = three_columns();
        let tree = root.unwrap();
        let weights = tree.weights();
        assert_eq!(weights.len(), 3);
        for w in weights {
            assert!((w - 1.0 / 3.0).abs() < 1e-12, "{weights:?}");
        }
        assert_eq!(tree.depth(), 2);
    }

    #[test]
    fn sibling_docking_takes_share_from_the_target_only() {
        let mut root = None;
        let mut ids = IdSource::new();
        let limits = SplitLimits::default();
        insert(&mut root, &mut ids, &pane(1), &DockTarget::edge(Side::Right), &limits).unwrap();
        insert(&mut root, &mut ids, &pane(2), &DockTarget::WindowEdge { side: Side::Right, share: Some(0.5) }, &limits).unwrap();
        let stack2 = root.as_ref().unwrap().find_stack_of(&pane(2)).unwrap();
        insert(
            &mut root,
            &mut ids,
            &pane(3),
            &DockTarget::Beside {
                node: stack2,
                side: Side::Right,
                share: Some(0.4),
            },
            &limits,
        )
        .unwrap();
        let weights = root.as_ref().unwrap().weights().to_vec();
        assert!((weights[0] - 0.5).abs() < 1e-12);
        assert!((weights[1] - 0.3).abs() < 1e-12);
        assert!((weights[2] - 0.2).abs() < 1e-12);
    }

    #[test]
    fn a_failed_insert_leaves_the_tree_untouched() {
        let (mut root, mut ids) = three_columns();
        let before = root.clone();
        let ids_before = ids.clone();
        let err = insert(&mut root, &mut ids, &pane(9), &DockTarget::tab(NodeId::new("nowhere")), &SplitLimits::default()).unwrap_err();
        assert_eq!(err, OpError::UnknownNode(NodeId::new("nowhere")));
        assert_eq!(root, before);
        assert_eq!(ids, ids_before);
        let err = insert(&mut root, &mut ids, &pane(1), &DockTarget::edge(Side::Left), &SplitLimits::default()).unwrap_err();
        assert_eq!(err, OpError::PaneAlreadyPlaced(pane(1)));
        assert_eq!(root, before);
    }

    #[test]
    fn removing_the_last_pane_empties_the_window() {
        let mut root = None;
        let mut ids = IdSource::new();
        insert(&mut root, &mut ids, &pane(1), &DockTarget::edge(Side::Right), &SplitLimits::default()).unwrap();
        let removed = remove(&mut root, &pane(1)).unwrap();
        assert!(root.is_none());
        assert!(!removed.stack_survived);
        assert_eq!(removed.neighbour, None);
    }

    #[test]
    fn removing_collapses_and_reports_the_survivor() {
        let (mut root, _) = three_columns();
        let tree = root.clone().unwrap();
        let split34 = tree.parent_of(&tree.find_stack_of(&pane(3)).unwrap()).unwrap();
        let stack4 = tree.find_stack_of(&pane(4)).unwrap();
        let removed = remove(&mut root, &pane(3)).unwrap();
        assert_eq!(removed.report.resolve(&split34), stack4);
        assert_eq!(removed.neighbour, Some(pane(4)));
        assert_eq!(removed.neighbour_side, Some(Side::Top));
        assert!(root.as_ref().unwrap().find(&split34).is_none());
    }

    #[test]
    fn resize_validates_and_normalises() {
        let (mut root, _) = three_columns();
        let root_id = root.as_ref().unwrap().id().clone();
        assert!(matches!(resize(&mut root, &root_id, &[1.0, 1.0]), Err(OpError::InvalidWeights { .. })));
        assert!(matches!(resize(&mut root, &root_id, &[1.0, -1.0, 1.0]), Err(OpError::InvalidWeights { .. })));
        resize(&mut root, &root_id, &[2.0, 1.0, 1.0]).unwrap();
        assert_eq!(root.as_ref().unwrap().weights(), &[0.5, 0.25, 0.25]);
        assert_eq!(resize(&mut root, &root_id, &[0.5, 0.25, 0.25]), Err(OpError::NoOp));
        let stack1 = root.as_ref().unwrap().find_stack_of(&pane(1)).unwrap();
        assert_eq!(resize(&mut root, &stack1, &[1.0]), Err(OpError::NotASplit(stack1)));
    }

    #[test]
    fn normalize_is_idempotent_on_a_messy_tree() {
        let messy = LayoutNode::split(
            NodeId::new("outer"),
            Axis::Horizontal,
            vec![
                LayoutNode::split(NodeId::new("lonely"), Axis::Vertical, vec![LayoutNode::single(NodeId::new("a"), pane(1))], vec![]),
                LayoutNode::split(
                    NodeId::new("inner"),
                    Axis::Horizontal,
                    vec![
                        LayoutNode::single(NodeId::new("b"), pane(2)),
                        LayoutNode::Stack {
                            id: NodeId::new("empty"),
                            panes: vec![],
                            active_pane_id: None,
                        },
                        LayoutNode::single(NodeId::new("c"), pane(3)),
                    ],
                    vec![3.0, 1.0, 1.0],
                ),
            ],
            vec![1.0, 1.0],
        );
        let mut root = Some(messy);
        let report = normalize(&mut root);
        assert!(report.changed);
        let once = root.clone();
        let again = normalize(&mut root);
        assert!(!again.changed);
        assert_eq!(root, once);
        let tree = root.unwrap();
        assert_eq!(tree.id(), &NodeId::new("outer"));
        assert_eq!(tree.children().len(), 3);
        assert_eq!(tree.children().iter().map(|c| c.id().as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        let weights = tree.weights();
        assert!((weights[0] - 0.5).abs() < 1e-12);
        assert!((weights[1] - 0.375).abs() < 1e-12);
        assert!((weights[2] - 0.125).abs() < 1e-12);
        assert_eq!(report.resolve(&NodeId::new("lonely")), NodeId::new("a"));
        assert_eq!(report.resolve(&NodeId::new("inner")), NodeId::new("outer"));
        assert!(report.dropped.contains(&NodeId::new("empty")));
    }

    /// `[1 | 2 | 3]` in equal thirds, through the operations.
    fn equal_thirds() -> (Option<LayoutNode>, IdSource) {
        let mut root = None;
        let mut ids = IdSource::new();
        let limits = SplitLimits::default();
        insert(&mut root, &mut ids, &pane(1), &DockTarget::edge(Side::Right), &limits).unwrap();
        insert(&mut root, &mut ids, &pane(2), &DockTarget::WindowEdge { side: Side::Right, share: Some(0.5) }, &limits).unwrap();
        insert(&mut root, &mut ids, &pane(3), &DockTarget::WindowEdge { side: Side::Right, share: Some(1.0 / 3.0) }, &limits).unwrap();
        (root, ids)
    }

    fn close_to(actual: &[f64], expected: &[f64]) -> bool {
        actual.len() == expected.len() && actual.iter().zip(expected).all(|(a, b)| (a - b).abs() < 1e-9)
    }

    #[test]
    fn a_pane_moved_beside_a_neighbour_hands_its_slot_to_that_neighbour() {
        // 3 to the right of 1: 1's slot grows by 3's and is divided; 2 keeps
        // its width and only changes place in the order.
        let (mut root, mut ids) = equal_thirds();
        let stack1 = root.as_ref().unwrap().find_stack_of(&pane(1)).unwrap();
        let stack2 = root.as_ref().unwrap().find_stack_of(&pane(2)).unwrap();
        move_pane(&mut root, &mut ids, &pane(3), &DockTarget::Beside { node: stack1.clone(), side: Side::Right, share: Some(0.5) }, &SplitLimits::default()).unwrap();
        let tree = root.as_ref().unwrap();
        assert_eq!(tree.panes(), [pane(1), pane(3), pane(2)]);
        assert!(close_to(tree.weights(), &[1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0]), "{:?}", tree.weights());
        assert_eq!(tree.children()[2].id(), &stack2, "2's stack is the same node");

        // 3 below 1: 1's slot grows by 3's and is divided vertically; 2 is untouched.
        let (mut root, mut ids) = equal_thirds();
        let stack1 = root.as_ref().unwrap().find_stack_of(&pane(1)).unwrap();
        move_pane(&mut root, &mut ids, &pane(3), &DockTarget::Beside { node: stack1, side: Side::Bottom, share: Some(0.5) }, &SplitLimits::default()).unwrap();
        let tree = root.as_ref().unwrap();
        assert_eq!(tree.axis(), Some(Axis::Horizontal));
        assert!(close_to(tree.weights(), &[2.0 / 3.0, 1.0 / 3.0]), "{:?}", tree.weights());
        let column = &tree.children()[0];
        assert_eq!(column.axis(), Some(Axis::Vertical));
        assert_eq!(column.panes(), [pane(1), pane(3)]);
        assert!(close_to(column.weights(), &[0.5, 0.5]));
        assert_eq!(tree.children()[1].stack_panes(), [pane(2)]);

        // 1 to the right of 3: the neighbour on the far side takes 1's slot;
        // 2, in between, keeps its width.
        let (mut root, mut ids) = equal_thirds();
        let stack3 = root.as_ref().unwrap().find_stack_of(&pane(3)).unwrap();
        move_pane(&mut root, &mut ids, &pane(1), &DockTarget::Beside { node: stack3, side: Side::Right, share: Some(0.5) }, &SplitLimits::default()).unwrap();
        let tree = root.as_ref().unwrap();
        assert_eq!(tree.panes(), [pane(2), pane(3), pane(1)]);
        assert!(close_to(tree.weights(), &[1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0]), "{:?}", tree.weights());
    }

    #[test]
    fn a_pane_moved_anywhere_else_leaves_its_slot_to_the_siblings_in_proportion() {
        // [1 | 2 | 3] with 1 twice as wide as 2: 3 into 1's stack as a tab
        // leaves 1 and 2 sharing 3's third at 2:1.
        let (mut root, mut ids) = equal_thirds();
        let root_id = root.as_ref().unwrap().id().clone();
        resize(&mut root, &root_id, &[0.5, 0.25, 0.25]).unwrap();
        let stack1 = root.as_ref().unwrap().find_stack_of(&pane(1)).unwrap();
        move_pane(&mut root, &mut ids, &pane(3), &DockTarget::tab(stack1), &SplitLimits::default()).unwrap();
        let tree = root.as_ref().unwrap();
        assert!(close_to(tree.weights(), &[2.0 / 3.0, 1.0 / 3.0]), "{:?}", tree.weights());

        // Beside the split itself (ancestor docking) is not "beside a
        // sibling": the survivors keep their ratio.
        let (mut root, mut ids) = equal_thirds();
        let root_id = root.as_ref().unwrap().id().clone();
        resize(&mut root, &root_id, &[0.5, 0.25, 0.25]).unwrap();
        move_pane(&mut root, &mut ids, &pane(3), &DockTarget::Beside { node: root_id, side: Side::Bottom, share: Some(0.25) }, &SplitLimits::default()).unwrap();
        let tree = root.as_ref().unwrap();
        assert_eq!(tree.axis(), Some(Axis::Vertical));
        let row = &tree.children()[0];
        assert!(close_to(row.weights(), &[2.0 / 3.0, 1.0 / 3.0]), "{:?}", row.weights());

        // A pane that is not alone frees nothing.
        let (mut root, mut ids) = equal_thirds();
        let stack1 = root.as_ref().unwrap().find_stack_of(&pane(1)).unwrap();
        let stack2 = root.as_ref().unwrap().find_stack_of(&pane(2)).unwrap();
        move_pane(&mut root, &mut ids, &pane(3), &DockTarget::tab(stack1.clone()), &SplitLimits::default()).unwrap();
        let before = root.as_ref().unwrap().weights().to_vec();
        move_pane(&mut root, &mut ids, &pane(3), &DockTarget::Beside { node: stack2, side: Side::Right, share: Some(0.5) }, &SplitLimits::default()).unwrap();
        let tree = root.as_ref().unwrap();
        assert!(close_to(&tree.weights()[..1], &before[..1]), "1's stack keeps its slot: {:?} vs {before:?}", tree.weights());
    }

    #[test]
    fn check_limits_refuses_slivers_and_depth_but_tolerates_what_was_already_small() {
        let (root, _) = equal_thirds();
        let limits = SplitLimits { min_share: 0.4, max_depth: 12 };
        let err = check_limits(None, root.as_ref(), &limits).unwrap_err();
        assert!(matches!(err, OpError::TooSmall { smallest, .. } if (smallest - 1.0 / 3.0).abs() < 1e-9), "{err:?}");
        // The same tree, judged against itself: nothing shrank, so nothing is refused.
        assert_eq!(check_limits(root.as_ref(), root.as_ref(), &limits), Ok(()));
        assert_eq!(check_limits(None, root.as_ref(), &SplitLimits::default()), Ok(()));
        let shallow = SplitLimits { min_share: 0.0, max_depth: 0 };
        assert!(matches!(check_limits(None, root.as_ref(), &shallow), Err(OpError::TooDeep { depth: 1, max_depth: 0 })));
        assert_eq!(check_limits(None, None, &shallow), Ok(()));
    }

    #[test]
    fn share_is_clamped() {
        assert_eq!(clamp_share(None), DEFAULT_SHARE);
        assert_eq!(clamp_share(Some(f64::NAN)), DEFAULT_SHARE);
        assert_eq!(clamp_share(Some(0.01)), MIN_SHARE_ARG);
        assert_eq!(clamp_share(Some(0.99)), MAX_SHARE_ARG);
        assert_eq!(clamp_share(Some(0.5)), 0.5);
    }

    /// The tree as a picture, every pane labelled by its own number.
    fn picture(root: &Option<LayoutNode>, cols: usize, rows: usize) -> String {
        crate::grid::render(root.as_ref().expect("a tree"), cols, rows, |pane| pane.minted_counter().and_then(|n| char::from_digit((n % 10) as u32, 10)).unwrap_or('?'))
    }

    #[test]
    fn a_range_target_spans_exactly_those_siblings() {
        // 4 below 2 and 3 only: 1 keeps its full height and its width.
        let (mut root, mut ids) = equal_thirds();
        let root_id = root.as_ref().unwrap().id().clone();
        let target = DockTarget::BesideRange { split: root_id.clone(), from: 1, to: 2, side: Side::Bottom, share: Some(1.0 / 3.0) };
        insert(&mut root, &mut ids, &pane(4), &target, &SplitLimits::default()).unwrap();
        assert_eq!(picture(&root, 3, 3), "123\n123\n144");
        let tree = root.as_ref().unwrap();
        assert_eq!(tree.axis(), Some(Axis::Horizontal));
        assert!(close_to(tree.weights(), &[1.0 / 3.0, 2.0 / 3.0]), "{:?}", tree.weights());
        let column = &tree.children()[1];
        assert_eq!(column.axis(), Some(Axis::Vertical));
        assert_eq!(column.panes(), [pane(2), pane(3), pane(4)]);
        assert!(close_to(column.weights(), &[2.0 / 3.0, 1.0 / 3.0]), "{:?}", column.weights());
        let row = &column.children()[0];
        assert_eq!(row.axis(), Some(Axis::Horizontal));
        assert!(close_to(row.weights(), &[0.5, 0.5]), "2 and 3 keep their ratio: {:?}", row.weights());

        // The whole range is the split itself: the same as the window's edge.
        let (mut root, mut ids) = equal_thirds();
        let root_id = root.as_ref().unwrap().id().clone();
        let target = DockTarget::BesideRange { split: root_id, from: 0, to: 2, side: Side::Bottom, share: Some(1.0 / 3.0) };
        insert(&mut root, &mut ids, &pane(4), &target, &SplitLimits::default()).unwrap();
        assert_eq!(picture(&root, 3, 3), "123\n123\n444");

        // A range of one child is that child.
        let (mut root, mut ids) = equal_thirds();
        let root_id = root.as_ref().unwrap().id().clone();
        let target = DockTarget::BesideRange { split: root_id, from: 0, to: 0, side: Side::Bottom, share: Some(1.0 / 3.0) };
        insert(&mut root, &mut ids, &pane(4), &target, &SplitLimits::default()).unwrap();
        assert_eq!(picture(&root, 3, 3), "123\n123\n423");
    }

    #[test]
    fn a_range_along_the_splits_own_axis_or_out_of_bounds_is_refused_unchanged() {
        let (mut root, mut ids) = equal_thirds();
        let root_id = root.as_ref().unwrap().id().clone();
        let before = root.clone();
        let along = DockTarget::BesideRange { split: root_id.clone(), from: 0, to: 1, side: Side::Right, share: None };
        assert!(matches!(insert(&mut root, &mut ids, &pane(4), &along, &SplitLimits::default()), Err(OpError::InvalidRange { .. })));
        let beyond = DockTarget::BesideRange { split: root_id.clone(), from: 1, to: 3, side: Side::Bottom, share: None };
        assert!(matches!(insert(&mut root, &mut ids, &pane(4), &beyond, &SplitLimits::default()), Err(OpError::InvalidRange { .. })));
        let backwards = DockTarget::BesideRange { split: root_id, from: 2, to: 1, side: Side::Bottom, share: None };
        assert!(matches!(insert(&mut root, &mut ids, &pane(4), &backwards, &SplitLimits::default()), Err(OpError::InvalidRange { .. })));
        let stack = root.as_ref().unwrap().find_stack_of(&pane(1)).unwrap();
        let not_a_split = DockTarget::BesideRange { split: stack, from: 0, to: 0, side: Side::Bottom, share: None };
        assert!(matches!(insert(&mut root, &mut ids, &pane(4), &not_a_split, &SplitLimits::default()), Err(OpError::NotASplit(_))));
        assert_eq!(root, before, "every refusal leaves the tree as it was");
    }

    #[test]
    fn moving_a_pane_to_a_range_accounts_for_the_slot_it_leaves() {
        // 1 moves below 2 and 3: once 1 is gone the range is the whole row,
        // so 1 spans the window.
        let (mut root, mut ids) = equal_thirds();
        let root_id = root.as_ref().unwrap().id().clone();
        let target = DockTarget::BesideRange { split: root_id, from: 1, to: 2, side: Side::Bottom, share: Some(1.0 / 3.0) };
        move_pane(&mut root, &mut ids, &pane(1), &target, &SplitLimits::default()).unwrap();
        assert_eq!(picture(&root, 2, 3), "23\n23\n11");

        // 2 moves below "2 and 3": without itself the range is just 3.
        let (mut root, mut ids) = equal_thirds();
        let root_id = root.as_ref().unwrap().id().clone();
        let target = DockTarget::BesideRange { split: root_id, from: 1, to: 2, side: Side::Bottom, share: Some(0.5) };
        move_pane(&mut root, &mut ids, &pane(2), &target, &SplitLimits::default()).unwrap();
        assert_eq!(picture(&root, 2, 2), "13\n12");

        // A range naming only the moved pane is docking the pane beside
        // itself: nothing would change.
        let (mut root, mut ids) = equal_thirds();
        let root_id = root.as_ref().unwrap().id().clone();
        let before = root.clone();
        let target = DockTarget::BesideRange { split: root_id, from: 1, to: 1, side: Side::Bottom, share: None };
        assert_eq!(move_pane(&mut root, &mut ids, &pane(2), &target, &SplitLimits::default()), Err(OpError::NoOp));
        assert_eq!(root, before);

        // 4, a tab of 3, moves below 2 and 3: the plan's cross-column case
        // reached from a drag.
        let (mut root, mut ids) = equal_thirds();
        let stack3 = root.as_ref().unwrap().find_stack_of(&pane(3)).unwrap();
        insert(&mut root, &mut ids, &pane(4), &DockTarget::tab(stack3), &SplitLimits::default()).unwrap();
        let root_id = root.as_ref().unwrap().id().clone();
        let target = DockTarget::BesideRange { split: root_id, from: 1, to: 2, side: Side::Bottom, share: Some(1.0 / 3.0) };
        move_pane(&mut root, &mut ids, &pane(4), &target, &SplitLimits::default()).unwrap();
        assert_eq!(picture(&root, 3, 3), "123\n123\n144");
    }

    #[test]
    fn ancestor_targets_list_every_distinct_level_from_narrow_to_broad() {
        let (root, _) = equal_thirds();
        let tree = root.as_ref().unwrap();
        let root_id = tree.id().clone();
        let stack3 = tree.find_stack_of(&pane(3)).unwrap();
        // Below 3: 3 itself, 3 with 2, then the window (3 with 1 and 2 is the window).
        assert_eq!(
            ancestor_targets(tree, &pane(3), Side::Bottom),
            vec![
                DockTarget::beside(stack3.clone(), Side::Bottom),
                DockTarget::BesideRange { split: root_id.clone(), from: 1, to: 2, side: Side::Bottom, share: None },
                DockTarget::edge(Side::Bottom),
            ]
        );
        // Below 2, in the middle: with 1, then with 3, then the window.
        let stack2 = tree.find_stack_of(&pane(2)).unwrap();
        assert_eq!(
            ancestor_targets(tree, &pane(2), Side::Bottom),
            vec![
                DockTarget::beside(stack2, Side::Bottom),
                DockTarget::BesideRange { split: root_id.clone(), from: 0, to: 1, side: Side::Bottom, share: None },
                DockTarget::BesideRange { split: root_id, from: 1, to: 2, side: Side::Bottom, share: None },
                DockTarget::edge(Side::Bottom),
            ]
        );
        // Right of 3: the row runs the same way, so there are no runs; the
        // root is the window.
        assert_eq!(ancestor_targets(tree, &pane(3), Side::Right), vec![DockTarget::beside(stack3, Side::Right), DockTarget::edge(Side::Right)]);
        assert!(ancestor_targets(tree, &pane(9), Side::Right).is_empty());

        // `1 | [2 / 3]`: right of 3 offers 3, the column, the window.
        let (mut root, mut ids) = equal_thirds();
        let root_id = root.as_ref().unwrap().id().clone();
        move_pane(&mut root, &mut ids, &pane(3), &DockTarget::BesideRange { split: root_id, from: 1, to: 1, side: Side::Bottom, share: Some(0.5) }, &SplitLimits::default()).unwrap();
        let tree = root.as_ref().unwrap();
        assert_eq!(picture(&root, 2, 2), "12\n13");
        let stack3 = tree.find_stack_of(&pane(3)).unwrap();
        let column = tree.position_of(&stack3).unwrap().0;
        assert_eq!(ancestor_targets(tree, &pane(3), Side::Right), vec![DockTarget::beside(stack3.clone(), Side::Right), DockTarget::beside(column.clone(), Side::Right), DockTarget::edge(Side::Right)]);
        // Below 3: the column runs the same way (no runs), then the row above
        // it where the column is the last of two children (no partial run),
        // then the window.
        assert_eq!(ancestor_targets(tree, &pane(3), Side::Bottom), vec![DockTarget::beside(stack3, Side::Bottom), DockTarget::beside(column, Side::Bottom), DockTarget::edge(Side::Bottom)]);
    }
}
