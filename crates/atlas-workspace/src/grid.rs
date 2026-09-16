//! A character-grid picture of a layout tree, for tests and logs.
//!
//! The unit square is sampled at the centre of each cell of a `cols × rows`
//! grid; the cell shows the label of the **active pane** of the stack whose
//! rectangle contains that point. With three columns and three rows, the
//! tree `[1 | 2 | [3 / 4]]` with the right column split two thirds over one
//! third renders as:
//!
//! ```text
//! 123
//! 123
//! 124
//! ```
//!
//! A sample point that falls exactly on a boundary belongs to the earlier
//! (left or top) pane, so a 50/50 split rendered three cells wide reads
//! `112`, as the pictures in the workspace plan do. Cells covered by nothing
//! (impossible for a normalized tree, possible for a hand-built one) show `.`.

use crate::ids::PaneId;
use crate::layout::LayoutNode;

/// Renders the tree with `label` naming each pane.
pub fn render(root: &LayoutNode, cols: usize, rows: usize, label: impl Fn(&PaneId) -> char) -> String {
    let stacks: Vec<(Option<PaneId>, crate::layout::Rect)> = root
        .stack_rects()
        .into_iter()
        .map(|(id, rect)| (root.find(&id).and_then(LayoutNode::active_pane).cloned(), rect))
        .collect();
    let mut lines = Vec::with_capacity(rows);
    for row in 0..rows {
        let mut line = String::with_capacity(cols);
        for col in 0..cols {
            let x = (col as f64 + 0.5) / cols as f64;
            let y = (row as f64 + 0.5) / rows as f64;
            let cell = stacks.iter().find(|(_, rect)| covers(rect, x, y)).and_then(|(active, _)| active.as_ref()).map(&label).unwrap_or('.');
            line.push(cell);
        }
        lines.push(line);
    }
    lines.join("\n")
}

/// Inclusive containment with a little tolerance: stacks are checked in
/// pre-order, so a point on a shared edge goes to the left/top one.
fn covers(rect: &crate::layout::Rect, x: f64, y: f64) -> bool {
    const EPSILON: f64 = 1e-9;
    x >= rect.x - EPSILON && x <= rect.right() + EPSILON && y >= rect.y - EPSILON && y <= rect.bottom() + EPSILON
}

/// Renders an optional root; an empty window is all dots.
pub fn render_window(root: Option<&LayoutNode>, cols: usize, rows: usize, label: impl Fn(&PaneId) -> char) -> String {
    match root {
        Some(tree) => render(tree, cols, rows, label),
        None => (0..rows).map(|_| ".".repeat(cols)).collect::<Vec<_>>().join("\n"),
    }
}

/// Renders with panes labelled `1`–`9` then `a`–`z` in pre-order (`?` past that).
pub fn render_numbered(root: &LayoutNode, cols: usize, rows: usize) -> String {
    let order = root.panes();
    render(root, cols, rows, |pane| order.iter().position(|candidate| candidate == pane).map(ordinal_label).unwrap_or('?'))
}

/// `0 → '1'`, …, `8 → '9'`, `9 → 'a'`, …, `34 → 'z'`, then `'?'`.
pub fn ordinal_label(index: usize) -> char {
    match index {
        0..=8 => char::from(b'1' + index as u8),
        9..=34 => char::from(b'a' + (index - 9) as u8),
        _ => '?',
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::NodeId;
    use crate::layout::Axis;

    #[test]
    fn renders_a_hand_built_tree() {
        let tree = LayoutNode::split(
            NodeId::new("root"),
            Axis::Horizontal,
            vec![
                LayoutNode::single(NodeId::new("a"), PaneId::new("p1")),
                LayoutNode::single(NodeId::new("b"), PaneId::new("p2")),
                LayoutNode::split(
                    NodeId::new("c"),
                    Axis::Vertical,
                    vec![LayoutNode::single(NodeId::new("d"), PaneId::new("p3")), LayoutNode::single(NodeId::new("e"), PaneId::new("p4"))],
                    vec![2.0 / 3.0, 1.0 / 3.0],
                ),
            ],
            vec![1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0],
        );
        assert_eq!(render_numbered(&tree, 3, 3), "123\n123\n124");
        assert_eq!(render(&tree, 3, 1, |pane| pane.as_str().chars().last().unwrap_or('?')), "123");
        assert_eq!(render_window(None, 2, 2, |_| 'x'), "..\n..");
        assert_eq!(ordinal_label(9), 'a');
        assert_eq!(ordinal_label(99), '?');
    }
}
