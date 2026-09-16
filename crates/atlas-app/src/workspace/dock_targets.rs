//! The drag-target overlay: docking a pane beside a whole group, beside a
//! run of sibling panes, or at the window's edge.
//!
//! The dock engine's own drop zones are relative to **one pane**: its centre
//! (a tab) and its four edges (a split of that pane's slot). Layouts where a
//! pane spans several columns or rows — `123/123/144`, the plan's example —
//! need broader targets, and those must be explicit: the workspace never
//! rearranges unrelated panes on its own. While a pane is being dragged, this
//! module lays **bands** just inside the edges of the pane under the pointer,
//! one per broader target the layout offers on that side
//! ([`atlas_workspace::ops::ancestor_targets`]): the pane's parent group, a
//! run of its siblings, a larger group, the window. The band nearest the
//! edge is the broadest target; the pane's own edge zone stays the engine's,
//! further inside.
//!
//! | band | what dropping there does |
//! |---|---|
//! | `Beside` a split | a new stack beside the whole group, spanning it |
//! | `BesideRange` | the run of siblings is grouped first, then the pane goes beside that group |
//! | `WindowEdge` | a new stack along the whole window edge |
//!
//! At most [`MAX_VISIBLE_LEVELS`] bands show per side — the window's edge is
//! always the outermost — and `Space` while dragging cycles through the
//! inner levels when more exist than fit. Hovering a band shows the rectangle the dragged
//! pane would occupy and a label saying what will happen; a band whose move
//! the model refuses (the minimum-size rule) is drawn muted and says so.
//!
//! Everything here is geometry over the model and the panes' recorded
//! rectangles ([`super::pane::PaneBounds`]); the workspace view draws the
//! result and handles the drops.

use std::collections::HashMap;

use atlas_workspace::{DockTarget, LayoutNode, OpError, PaneId, Rect, Side, WindowId, WorkspaceLayout, ops};
use gpui_kit::{Bounds, Pixels, Point, SharedString, point, px, size};

use super::mirror;

/// How thick one band is.
pub const BAND_THICKNESS: Pixels = px(14.);
/// How many broader targets are shown on one side at once.
pub const MAX_VISIBLE_LEVELS: usize = 3;

/// A pane drag as the overlay follows it: which pane, where the pointer is,
/// and how far `Space` has cycled the visible levels.
#[derive(Clone, Debug, PartialEq)]
pub struct DragInFlight {
    pub pane: PaneId,
    pub pointer: Point<Pixels>,
    pub level_offset: usize,
}

/// What dropping on a band would do, decided by the model on a copy.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// The move is allowed; `preview` is where the pane would sit, as a share
    /// of the window.
    Allowed { preview: Rect },
    /// The pane already sits there: dropping changes nothing.
    Unchanged,
    /// The model refuses the move (not enough room, too deep); the reason.
    Refused(String),
}

/// One target band of the overlay.
#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    pub target: DockTarget,
    pub side: Side,
    /// 0 is the band nearest the edge, i.e. the broadest target shown.
    pub depth: usize,
    /// Where the band is drawn, in window coordinates.
    pub bounds: Bounds<Pixels>,
    /// What dropping here does, in plain words.
    pub label: String,
    pub outcome: Outcome,
    /// Whether the pointer is over this band.
    pub hovered: bool,
}

impl Band {
    /// The element id tests find the band by: `dock-band-<side>-<depth>`.
    pub fn element_id(&self) -> SharedString {
        SharedString::from(format!("dock-band-{}-{}", side_slug(self.side), self.depth))
    }

    /// Whether a drop here is accepted at all.
    pub fn accepts_drops(&self) -> bool {
        !matches!(self.outcome, Outcome::Refused(_))
    }
}

/// The displayed pane whose drawn rectangle contains `pointer`. Hidden tabs
/// are not drawn, so their stale rectangles are ignored.
pub fn hovered_pane(root: Option<&LayoutNode>, drawn: &HashMap<PaneId, Bounds<Pixels>>, pointer: Point<Pixels>) -> Option<PaneId> {
    let displayed = mirror::displayed_panes(root);
    let mut hit: Vec<&PaneId> = displayed.iter().filter(|pane| drawn.get(*pane).is_some_and(|bounds| bounds.contains(&pointer))).collect();
    hit.sort();
    hit.first().map(|pane| (*pane).clone())
}

/// The bands for the pane under the pointer, on all four sides. Empty when
/// no pane is under the pointer or the layout offers nothing broader than
/// the pane itself.
pub fn bands_for(layout: &WorkspaceLayout, window: &WindowId, drawn: &HashMap<PaneId, Bounds<Pixels>>, drag: &DragInFlight) -> Vec<Band> {
    let Some(root) = layout.window(window).and_then(|window| window.root.as_ref()) else {
        return Vec::new();
    };
    let Some(hovered) = hovered_pane(Some(root), drawn, drag.pointer) else {
        return Vec::new();
    };
    let Some(pane_bounds) = drawn.get(&hovered).copied() else {
        return Vec::new();
    };
    let mut bands = Vec::new();
    for side in Side::all() {
        // The first level is the pane's own stack, which the engine's edge
        // zone already offers; the bands are for everything broader.
        let levels: Vec<DockTarget> = ops::ancestor_targets(root, &hovered, side).into_iter().skip(1).collect();
        if levels.is_empty() {
            continue;
        }
        // The window's edge is the target people reach for most, so it is
        // always the outermost band; `Space` cycles through the inner levels
        // (groups and runs) when more of them exist than fit.
        let (edges, inner): (Vec<DockTarget>, Vec<DockTarget>) = levels.into_iter().partition(|target| matches!(target, DockTarget::WindowEdge { .. }));
        let offset = if inner.is_empty() { 0 } else { drag.level_offset % inner.len() };
        let mut visible: Vec<DockTarget> = inner.iter().skip(offset).take(MAX_VISIBLE_LEVELS - 1).cloned().collect();
        visible.extend(edges);
        let shown = visible.len();
        for (index, target) in visible.iter().enumerate() {
            // The last of the visible levels is the broadest, and goes nearest the edge.
            let depth = shown - 1 - index;
            let bounds = band_bounds(pane_bounds, side, depth);
            let outcome = outcome_of(layout, window, &drag.pane, target);
            let label = describe(root, target, side);
            bands.push(Band { target: target.clone(), side, depth, bounds, label, outcome, hovered: bounds.contains(&drag.pointer) });
        }
    }
    bands
}

/// The band `depth` levels in from `side` of a pane drawn at `pane`.
/// Horizontal bands (top and bottom) span the pane's width less the corners
/// the vertical bands need, and the other way round, so no two bands overlap.
pub fn band_bounds(pane: Bounds<Pixels>, side: Side, depth: usize) -> Bounds<Pixels> {
    let inset = BAND_THICKNESS * depth as f32;
    let corner = BAND_THICKNESS * MAX_VISIBLE_LEVELS as f32;
    let right = pane.origin.x + pane.size.width;
    let bottom = pane.origin.y + pane.size.height;
    match side {
        Side::Top => Bounds::new(point(pane.origin.x + corner, pane.origin.y + inset), size((pane.size.width - corner * 2.).max(px(0.)), BAND_THICKNESS)),
        Side::Bottom => Bounds::new(point(pane.origin.x + corner, bottom - inset - BAND_THICKNESS), size((pane.size.width - corner * 2.).max(px(0.)), BAND_THICKNESS)),
        Side::Left => Bounds::new(point(pane.origin.x + inset, pane.origin.y + corner), size(BAND_THICKNESS, (pane.size.height - corner * 2.).max(px(0.)))),
        Side::Right => Bounds::new(point(right - inset - BAND_THICKNESS, pane.origin.y + corner), size(BAND_THICKNESS, (pane.size.height - corner * 2.).max(px(0.)))),
    }
}

/// What moving `pane` to `target` would do, tried on a copy of the model.
pub fn outcome_of(layout: &WorkspaceLayout, window: &WindowId, pane: &PaneId, target: &DockTarget) -> Outcome {
    let mut trial = layout.clone();
    match trial.move_pane(pane, window, target.clone()) {
        Ok(()) => {
            let preview = trial.window(window).and_then(|window| window.root.as_ref()).and_then(|root| root.pane_rects().into_iter().find(|(id, _)| id == pane)).map(|(_, rect)| rect).unwrap_or(Rect::UNIT);
            Outcome::Allowed { preview }
        }
        Err(OpError::NoOp) => Outcome::Unchanged,
        Err(err) => Outcome::Refused(err.to_string()),
    }
}

/// What dropping on `target` does, said the way the person reads it.
pub fn describe(root: &LayoutNode, target: &DockTarget, side: Side) -> String {
    let where_ = match side {
        Side::Top => "above",
        Side::Bottom => "below",
        Side::Left => "left of",
        Side::Right => "right of",
    };
    match target {
        DockTarget::WindowEdge { .. } => match side {
            Side::Top => "Dock along the top of the window".to_string(),
            Side::Bottom => "Dock along the bottom of the window".to_string(),
            Side::Left => "Dock along the left edge of the window".to_string(),
            Side::Right => "Dock along the right edge of the window".to_string(),
        },
        // Counts are of what the person sees: a stack of tabs is one pane on
        // screen, whatever it holds.
        DockTarget::BesideRange { split, from, to, .. } => {
            let panes: usize = root.find(split).map(|split| split.children().iter().skip(*from).take(to - from + 1).map(LayoutNode::leaf_count).sum()).unwrap_or(0);
            format!("Dock {where_} these {panes} panes")
        }
        DockTarget::Beside { node, .. } => match root.find(node) {
            Some(found) if found.is_stack() => format!("Dock {where_} this pane"),
            Some(found) => format!("Dock {where_} this group of {} panes", found.leaf_count()),
            None => format!("Dock {where_} this group"),
        },
        DockTarget::Stack { .. } => "Add as a tab".to_string(),
    }
}

/// `top`, `bottom`, `left`, `right`.
pub fn side_slug(side: Side) -> &'static str {
    match side {
        Side::Top => "top",
        Side::Bottom => "bottom",
        Side::Left => "left",
        Side::Right => "right",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_workspace::PaneDefinition;

    /// `1 | 2 | 3` in equal thirds, drawn across a 1200 × 600 window at the origin.
    fn three_columns() -> (WorkspaceLayout, HashMap<PaneId, Bounds<Pixels>>) {
        let mut layout = WorkspaceLayout::new("test");
        let window = WindowId::main();
        let one = layout.open_pane(&window, PaneDefinition::new("today"), DockTarget::edge(Side::Right)).unwrap();
        let two = layout.open_pane(&window, PaneDefinition::new("accounts"), DockTarget::WindowEdge { side: Side::Right, share: Some(0.5) }).unwrap();
        let three = layout.open_pane(&window, PaneDefinition::new("rules"), DockTarget::WindowEdge { side: Side::Right, share: Some(1.0 / 3.0) }).unwrap();
        let mut drawn = HashMap::new();
        drawn.insert(one, Bounds::new(point(px(0.), px(0.)), size(px(400.), px(600.))));
        drawn.insert(two, Bounds::new(point(px(400.), px(0.)), size(px(400.), px(600.))));
        drawn.insert(three, Bounds::new(point(px(800.), px(0.)), size(px(400.), px(600.))));
        (layout, drawn)
    }

    #[test]
    fn the_pane_under_the_pointer_is_found_and_hidden_tabs_are_ignored() {
        let (mut layout, mut drawn) = three_columns();
        let root = layout.main_window().unwrap().root.clone();
        let panes = layout.main_window().unwrap().panes();
        assert_eq!(hovered_pane(root.as_ref(), &drawn, point(px(900.), px(300.))), Some(panes[2].clone()));
        assert_eq!(hovered_pane(root.as_ref(), &drawn, point(px(1300.), px(300.))), None);
        // A fourth pane tabbed behind the third keeps a stale rectangle; the
        // displayed pane wins.
        let stack = layout.stack_of(&panes[2]).unwrap();
        let four = layout.open_pane(&WindowId::main(), PaneDefinition::new("forecast"), DockTarget::tab(stack)).unwrap();
        drawn.insert(four.clone(), drawn[&panes[2]]);
        layout.set_active_pane(&panes[2]).unwrap();
        let root = layout.main_window().unwrap().root.clone();
        assert_eq!(hovered_pane(root.as_ref(), &drawn, point(px(900.), px(300.))), Some(panes[2].clone()));
    }

    #[test]
    fn bands_below_the_third_column_offer_its_neighbour_run_and_the_window() {
        let (mut layout, mut drawn) = three_columns();
        let panes = layout.main_window().unwrap().panes();
        // The dragged pane is a fourth one, tabbed behind pane 3.
        let stack = layout.stack_of(&panes[2]).unwrap();
        let four = layout.open_pane(&WindowId::main(), PaneDefinition::new("forecast"), DockTarget::tab(stack)).unwrap();
        layout.set_active_pane(&panes[2]).unwrap();
        drawn.insert(four.clone(), drawn[&panes[2]]);
        let drag = DragInFlight { pane: four.clone(), pointer: point(px(1000.), px(590.)), level_offset: 0 };
        let bands = bands_for(&layout, &WindowId::main(), &drawn, &drag);
        let bottom: Vec<&Band> = bands.iter().filter(|band| band.side == Side::Bottom).collect();
        assert_eq!(bottom.len(), 2, "{bottom:#?}");
        // Nearest the edge: the window; inside it: the run of 2 and 3.
        let window_band = bottom.iter().find(|band| band.depth == 0).unwrap();
        assert!(matches!(window_band.target, DockTarget::WindowEdge { side: Side::Bottom, .. }));
        assert_eq!(window_band.label, "Dock along the bottom of the window");
        assert!(window_band.hovered, "the pointer at y=590 is in the outermost 14 px");
        let run_band = bottom.iter().find(|band| band.depth == 1).unwrap();
        assert!(matches!(run_band.target, DockTarget::BesideRange { from: 1, to: 2, side: Side::Bottom, .. }), "{:?}", run_band.target);
        assert_eq!(run_band.label, "Dock below these 2 panes");
        assert!(!run_band.hovered);
        match &run_band.outcome {
            Outcome::Allowed { preview } => {
                assert!((preview.x - 1.0 / 3.0).abs() < 1e-9 && (preview.width - 2.0 / 3.0).abs() < 1e-9, "spans columns 2 and 3: {preview:?}");
                assert!(preview.y > 0.5, "sits below them: {preview:?}");
            }
            other => panic!("expected an allowed move, got {other:?}"),
        }
        // Along the row there is no run and the root is the window: one band.
        let right: Vec<&Band> = bands.iter().filter(|band| band.side == Side::Right).collect();
        assert_eq!(right.len(), 1);
        assert!(matches!(right[0].target, DockTarget::WindowEdge { side: Side::Right, .. }));
        for band in &bands {
            assert!(band.bounds.size.width >= px(0.) && band.bounds.size.height >= px(0.));
            assert!(band.element_id().starts_with("dock-band-"));
        }
    }

    #[test]
    fn a_refused_move_and_an_unchanged_one_are_reported() {
        let (mut layout, drawn) = three_columns();
        let panes = layout.main_window().unwrap().panes();
        // Tight limits: no split may leave a pane under 40 % of the window.
        layout.set_limits(atlas_workspace::SplitLimits { min_share: 0.4, max_depth: 12 });
        let drag = DragInFlight { pane: panes[0].clone(), pointer: point(px(1000.), px(590.)), level_offset: 0 };
        let bands = bands_for(&layout, &WindowId::main(), &drawn, &drag);
        let window_band = bands.iter().find(|band| band.side == Side::Bottom && band.depth == 0).unwrap();
        assert!(matches!(window_band.outcome, Outcome::Refused(_)), "{:?}", window_band.outcome);
        assert!(!window_band.accepts_drops());
        // Pane 3 is already at the window's right edge.
        let drag = DragInFlight { pane: panes[2].clone(), pointer: point(px(1190.), px(300.)), level_offset: 0 };
        let bands = bands_for(&layout, &WindowId::main(), &drawn, &drag);
        let right = bands.iter().find(|band| band.side == Side::Right && band.depth == 0).unwrap();
        assert_eq!(right.outcome, Outcome::Unchanged);
        assert!(right.accepts_drops());
    }

    #[test]
    fn space_cycles_the_visible_levels() {
        let (mut layout, mut drawn) = three_columns();
        let panes = layout.main_window().unwrap().panes();
        // Five columns give the middle pane four runs plus the window below it.
        let four = layout.open_pane(&WindowId::main(), PaneDefinition::new("forecast"), DockTarget::WindowEdge { side: Side::Right, share: Some(0.25) }).unwrap();
        let five = layout.open_pane(&WindowId::main(), PaneDefinition::new("people"), DockTarget::WindowEdge { side: Side::Right, share: Some(0.2) }).unwrap();
        drawn.insert(four.clone(), Bounds::new(point(px(1200.), px(0.)), size(px(400.), px(600.))));
        drawn.insert(five.clone(), Bounds::new(point(px(1600.), px(0.)), size(px(400.), px(600.))));
        let mut drag = DragInFlight { pane: five, pointer: point(px(1000.), px(590.)), level_offset: 0 };
        let first = bands_for(&layout, &WindowId::main(), &drawn, &drag);
        let labels = |bands: &[Band]| {
            let mut bottom: Vec<&Band> = bands.iter().filter(|band| band.side == Side::Bottom).collect();
            bottom.sort_by_key(|band| band.depth);
            bottom.iter().map(|band| band.label.clone()).collect::<Vec<_>>()
        };
        // Pane 3 in the middle of five: runs 2–3, 1–3, 3–4, 3–5 and the window.
        assert_eq!(labels(&first), ["Dock along the bottom of the window", "Dock below these 3 panes", "Dock below these 2 panes"]);
        drag.level_offset = 1;
        let second = bands_for(&layout, &WindowId::main(), &drawn, &drag);
        assert_eq!(labels(&second), ["Dock along the bottom of the window", "Dock below these 2 panes", "Dock below these 3 panes"], "the window stays outermost; the inner levels moved on");
        drag.level_offset = 4;
        assert_eq!(labels(&bands_for(&layout, &WindowId::main(), &drawn, &drag)), labels(&first), "cycling wraps around");
        assert_eq!(panes.len(), 3);
    }

    #[test]
    fn band_rectangles_sit_inside_the_edge_and_leave_the_corners_free() {
        let pane = Bounds::new(point(px(100.), px(50.)), size(px(400.), px(300.)));
        let bottom0 = band_bounds(pane, Side::Bottom, 0);
        assert_eq!(bottom0.origin.y + bottom0.size.height, px(350.));
        assert_eq!(bottom0.size.height, BAND_THICKNESS);
        assert_eq!(bottom0.origin.x, px(100.) + BAND_THICKNESS * 3.);
        let bottom1 = band_bounds(pane, Side::Bottom, 1);
        assert_eq!(bottom1.origin.y + bottom1.size.height, px(350.) - BAND_THICKNESS);
        let right0 = band_bounds(pane, Side::Right, 0);
        assert_eq!(right0.origin.x + right0.size.width, px(500.));
        assert_eq!(right0.origin.y, px(50.) + BAND_THICKNESS * 3.);
        let top2 = band_bounds(pane, Side::Top, 2);
        assert_eq!(top2.origin.y, px(50.) + BAND_THICKNESS * 2.);
        let left1 = band_bounds(pane, Side::Left, 1);
        assert_eq!(left1.origin.x, px(100.) + BAND_THICKNESS);
        // A tiny pane never produces a negative size.
        let tiny = band_bounds(Bounds::new(point(px(0.), px(0.)), size(px(10.), px(10.))), Side::Top, 0);
        assert_eq!(tiny.size.width, px(0.));
    }
}
