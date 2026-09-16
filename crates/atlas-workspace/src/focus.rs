//! Keyboard navigation between panes.
//!
//! "Focus the pane to the right" is answered from geometry, not from the
//! tree: every stack has a rectangle in the unit square
//! ([`LayoutNode::stack_rects`]), and the neighbour in a direction is the
//! stack whose rectangle touches the source's edge on that side with the
//! largest overlap along the other axis (ties go to the one nearest the
//! source's centre). In `123 / 123 / 124`, going right from 2 reaches 3
//! (two thirds of the shared edge) rather than 4 (one third); going left
//! from 4 reaches 2, and going down from 3 reaches 4.
//!
//! [`move_direction_target`] uses the same neighbour to answer "move this
//! pane to the right": it lands beside the neighbour on the side facing the
//! source — or, when the pane is alone in its stack and already sits there,
//! on the neighbour's far side, so the pane jumps over it. With no neighbour
//! the pane goes to the window edge.

use crate::ids::{NodeId, PaneId};
use crate::layout::{Axis, LayoutNode, Rect, Side};
use crate::ops::DockTarget;
use serde::{Deserialize, Serialize};

/// Where the user wants to go.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    /// The side of a neighbour that faces a pane coming from this direction
    /// (moving right, you arrive at the neighbour's left side).
    pub fn arriving_side(self) -> Side {
        match self {
            Direction::Left => Side::Right,
            Direction::Right => Side::Left,
            Direction::Up => Side::Bottom,
            Direction::Down => Side::Top,
        }
    }

    /// The window edge in this direction.
    pub fn edge(self) -> Side {
        match self {
            Direction::Left => Side::Left,
            Direction::Right => Side::Right,
            Direction::Up => Side::Top,
            Direction::Down => Side::Bottom,
        }
    }

    /// The axis of travel.
    pub fn axis(self) -> Axis {
        match self {
            Direction::Left | Direction::Right => Axis::Horizontal,
            Direction::Up | Direction::Down => Axis::Vertical,
        }
    }
}

/// Tolerance for "touching" and "equal overlap" comparisons.
const EPSILON: f64 = 1e-6;

/// The stack next to `from`'s stack in `direction`, by the overlap rule.
pub fn neighbour_stack(root: &LayoutNode, from: &PaneId, direction: Direction) -> Option<NodeId> {
    let source_id = root.find_stack_of(from)?;
    let rects = root.stack_rects();
    let source = rects.iter().find(|(id, _)| *id == source_id).map(|(_, rect)| *rect)?;
    let (source_cx, source_cy) = source.centre();
    let perpendicular = direction.axis().perpendicular();

    let mut best: Option<(NodeId, f64, f64, f64)> = None; // (id, gap, overlap, centre distance)
    for (id, rect) in &rects {
        if *id == source_id {
            continue;
        }
        let gap = match direction {
            Direction::Right => rect.x - source.right(),
            Direction::Left => source.x - rect.right(),
            Direction::Down => rect.y - source.bottom(),
            Direction::Up => source.y - rect.bottom(),
        };
        if gap < -EPSILON {
            continue;
        }
        let overlap = rect.overlap_along(&source, perpendicular);
        if overlap <= EPSILON {
            continue;
        }
        let (cx, cy) = rect.centre();
        let distance = ((cx - source_cx).powi(2) + (cy - source_cy).powi(2)).sqrt();
        let better = match &best {
            None => true,
            Some((_, best_gap, best_overlap, best_distance)) => {
                if (gap - best_gap).abs() > EPSILON {
                    gap < *best_gap
                } else if (overlap - best_overlap).abs() > EPSILON {
                    overlap > *best_overlap
                } else {
                    distance < *best_distance
                }
            }
        };
        if better {
            best = Some((id.clone(), gap, overlap, distance));
        }
    }
    best.map(|(id, _, _, _)| id)
}

/// The pane that gets the focus when moving from `from` in `direction`: the
/// neighbour stack's active pane. `None` at the window edge.
pub fn neighbour(root: &LayoutNode, from: &PaneId, direction: Direction) -> Option<PaneId> {
    let stack = neighbour_stack(root, from, direction)?;
    root.find(&stack).and_then(LayoutNode::active_pane).cloned()
}

/// Where a keyboard "move pane" lands. See the module docs.
pub fn move_direction_target(root: &LayoutNode, from: &PaneId, direction: Direction) -> Option<DockTarget> {
    let source_stack = root.find_stack_of(from)?;
    let Some(target_stack) = neighbour_stack(root, from, direction) else {
        return Some(DockTarget::edge(direction.edge()));
    };
    let alone = root.find(&source_stack).map(|stack| stack.stack_panes().len() == 1).unwrap_or(false);
    let near_side = direction.arriving_side();
    let already_there = alone && are_adjacent_siblings(root, &source_stack, &target_stack, direction);
    let side = if already_there { near_side.opposite() } else { near_side };
    Some(DockTarget::beside(target_stack, side))
}

/// True when `source` sits immediately before/after `target` in a split of
/// the direction's axis, i.e. moving one step would reproduce the tree.
fn are_adjacent_siblings(root: &LayoutNode, source: &NodeId, target: &NodeId, direction: Direction) -> bool {
    let (Some((parent_s, pos_s)), Some((parent_t, pos_t))) = (root.position_of(source), root.position_of(target)) else {
        return false;
    };
    if parent_s != parent_t || root.find(&parent_s).and_then(LayoutNode::axis) != Some(direction.axis()) {
        return false;
    }
    match direction {
        Direction::Right | Direction::Down => pos_s + 1 == pos_t,
        Direction::Left | Direction::Up => pos_t + 1 == pos_s,
    }
}

/// The rectangle of the stack holding `pane`.
pub fn rect_of_pane(root: &LayoutNode, pane: &PaneId) -> Option<Rect> {
    let stack = root.find_stack_of(pane)?;
    root.rect_of(&stack)
}
