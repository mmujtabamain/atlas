//! The drag-target overlay: dropping a pane *between* two panes, or along
//! the window's edge.
//!
//! The dock engine's own drop zones are relative to **one pane**: its centre
//! (a tab) and its four halves (a split of that pane's slot). Everything
//! broader is this overlay's, and it is laid where the eye expects it while
//! a tab is held, when the skin widens the gaps between the cards and pulls
//! them in from the window's edges ([`super::skin`]):
//!
//! | zone | where | what dropping there does |
//! |---|---|---|
//! | [`ZoneKind::Gap`] | the gap between two children of a split | the pane lands between them, a new column or row of that split |
//! | [`ZoneKind::Edge`] | the strip along a window edge, in the room the cards freed | a new stack along the whole edge — or, with `Space`, beside the group or the pane that meets the edge there |
//!
//! A gap is one split and one divider, so a layout offers exactly as many
//! gap zones as it has dividers, nested groups included; there is nothing to
//! stack or cycle. An edge strip starts as the window's edge; `Space` walks
//! the levels [`atlas_workspace::ops::ancestor_targets`] lists for the pane
//! that meets the edge under the pointer, and the strip's highlight shrinks
//! to the span of the group it would dock beside.
//!
//! Hovering a zone shows the rectangle the dragged pane would occupy and a
//! label saying what will happen; a zone whose move the model refuses (the
//! minimum-size rule) is drawn muted and says so.
//!
//! Everything here is geometry over the model's rectangles and the area's
//! bounds; the workspace view draws the result and handles the drops.

use std::collections::HashMap;

use atlas_workspace::{DockTarget, LayoutNode, NodeId, OpError, PaneId, Rect, Side, WindowId, WorkspaceLayout, ops};
use gpui_kit::{Bounds, Pixels, Point, SharedString, point, px, size};

use super::kinds;
use super::mirror;
use crate::nav::Route;

/// How much wider than the gap its drop zone is, so the gap is easy to hit.
const GAP_SLOP: Pixels = px(6.);
/// The room an edge strip leaves to the corners and to the cards.
const STRIP_MARGIN: Pixels = px(3.);

/// What is being dragged: a pane already in the layout, or a screen from
/// the launcher that becomes a pane where it is dropped.
#[derive(Clone, Debug, PartialEq)]
pub enum Dragged {
    Pane(PaneId),
    New(Route),
}

/// A drag as the overlay follows it: what is dragged, where the pointer is
/// (in the window the drag started in), how far `Space` has cycled an edge
/// strip's levels, and — when the pointer has left that window for another
/// window of the workspace — which window, and the displayed pane under it.
#[derive(Clone, Debug, PartialEq)]
pub struct DragInFlight {
    pub dragged: Dragged,
    pub pointer: Point<Pixels>,
    pub level_offset: usize,
    pub elsewhere: Option<(WindowId, Option<PaneId>)>,
}

/// What dropping on a zone would do, decided by the model on a copy.
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

/// Where a zone is.
#[derive(Clone, Debug, PartialEq)]
pub enum ZoneKind {
    /// The gap before child `index` of `split` (between `index - 1` and `index`).
    Gap { split: NodeId, index: usize },
    /// The strip along `side` of the window, showing level `level` of its
    /// targets (0 is the window's edge itself).
    Edge { side: Side, level: usize },
}

/// One drop zone of the overlay.
#[derive(Clone, Debug, PartialEq)]
pub struct Zone {
    pub kind: ZoneKind,
    pub target: DockTarget,
    /// Where the zone takes a drop, in window coordinates.
    pub bounds: Bounds<Pixels>,
    /// Where the zone is drawn: the strip's whole length, or the span of the
    /// group a cycled level docks beside.
    pub drawn: Bounds<Pixels>,
    /// What dropping here does, in plain words.
    pub label: String,
    pub outcome: Outcome,
    /// Whether the pointer is over this zone.
    pub hovered: bool,
}

impl Zone {
    /// The element id tests find the zone by: `dock-band-<side>-<level>` for
    /// an edge strip, `dock-gap-<split>-<index>` for a gap.
    pub fn element_id(&self) -> SharedString {
        match &self.kind {
            ZoneKind::Edge { side, level } => SharedString::from(format!("dock-band-{}-{level}", side_slug(*side))),
            ZoneKind::Gap { split, index } => SharedString::from(format!("dock-gap-{split}-{index}")),
        }
    }

    /// Whether a drop here is accepted at all.
    pub fn accepts_drops(&self) -> bool {
        !matches!(self.outcome, Outcome::Refused(_))
    }

    /// The side a zone docks on.
    pub fn side(&self) -> Side {
        match &self.kind {
            ZoneKind::Edge { side, .. } => *side,
            ZoneKind::Gap { .. } => self.target.side().unwrap_or(Side::Right),
        }
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

/// The geometry the zones are laid on: the dock area as drawn, the inset the
/// cards pulled in by, and the gap between them.
#[derive(Clone, Copy, Debug)]
pub struct Field {
    /// The area's bounds, padding included, in window coordinates.
    pub area: Bounds<Pixels>,
    /// The padding between the area's edge and the cards.
    pub inset: Pixels,
    /// The gap between two cards.
    pub gap: Pixels,
}

impl Field {
    /// The rectangle the cards share: the area less its inset.
    pub fn content(&self) -> Bounds<Pixels> {
        Bounds::new(self.area.origin + point(self.inset, self.inset), size((self.area.size.width - self.inset * 2.).max(px(0.)), (self.area.size.height - self.inset * 2.).max(px(0.))))
    }

    /// A model rectangle (a share of the window) as pixels within the content.
    pub fn place(&self, rect: Rect) -> Bounds<Pixels> {
        let content = self.content();
        Bounds::new(
            point(content.origin.x + content.size.width * rect.x as f32, content.origin.y + content.size.height * rect.y as f32),
            size(content.size.width * rect.width as f32, content.size.height * rect.height as f32),
        )
    }
}

/// Every zone of the window for the drag in flight: one per divider of every
/// split, and one strip per window edge.
pub fn zones_for(layout: &WorkspaceLayout, window: &WindowId, field: Field, drag: &DragInFlight) -> Vec<Zone> {
    let Some(root) = layout.window(window).and_then(|window| window.root.as_ref()) else {
        return Vec::new();
    };
    let rects: HashMap<NodeId, Rect> = root.rects().into_iter().collect();
    let mut zones = Vec::new();
    gap_zones(layout, window, root, &rects, field, drag, &mut zones);
    for side in Side::all() {
        if let Some(zone) = edge_zone(layout, window, root, &rects, field, drag, side) {
            zones.push(zone);
        }
    }
    // One zone takes the pointer: the first hit in tree order (an outer
    // split's gap before an inner one's), so a T-junction is not two.
    let mut taken = false;
    for zone in &mut zones {
        zone.hovered = !taken && zone.bounds.contains(&drag.pointer);
        taken |= zone.hovered;
    }
    zones
}

/// The gap zones: the divider before every child but the first, of every
/// split, walked pre-order.
fn gap_zones(layout: &WorkspaceLayout, window: &WindowId, root: &LayoutNode, rects: &HashMap<NodeId, Rect>, field: Field, drag: &DragInFlight, out: &mut Vec<Zone>) {
    let Some(axis) = root.axis() else {
        return;
    };
    let children = root.children();
    let Some(split_rect) = rects.get(root.id()).copied() else {
        return;
    };
    let split_bounds = field.place(split_rect);
    let thickness = field.gap + GAP_SLOP * 2.;
    for index in 1..children.len() {
        let (Some(before), Some(after)) = (rects.get(children[index - 1].id()), rects.get(children[index].id())) else {
            continue;
        };
        let after_bounds = field.place(*after);
        // The divider sits where the next child starts; the zone is centred on it.
        let bounds = match axis {
            atlas_workspace::Axis::Horizontal => Bounds::new(point(after_bounds.origin.x - thickness / 2., split_bounds.origin.y + STRIP_MARGIN), size(thickness, (split_bounds.size.height - STRIP_MARGIN * 2.).max(px(0.)))),
            atlas_workspace::Axis::Vertical => Bounds::new(point(split_bounds.origin.x + STRIP_MARGIN, after_bounds.origin.y - thickness / 2.), size((split_bounds.size.width - STRIP_MARGIN * 2.).max(px(0.)), thickness)),
        };
        let _ = before;
        let side = match axis {
            atlas_workspace::Axis::Horizontal => Side::Right,
            atlas_workspace::Axis::Vertical => Side::Bottom,
        };
        let target = DockTarget::beside(children[index - 1].id().clone(), side);
        let outcome = outcome_of(layout, window, &drag.dragged, &target);
        out.push(Zone { kind: ZoneKind::Gap { split: root.id().clone(), index }, target, bounds, drawn: bounds, label: describe_gap(axis), outcome, hovered: false });
    }
    for child in children {
        gap_zones(layout, window, child, rects, field, drag, out);
    }
}

/// The strip along `side`, showing the level `Space` has cycled to for the
/// pane that meets the edge under the pointer.
fn edge_zone(layout: &WorkspaceLayout, window: &WindowId, root: &LayoutNode, rects: &HashMap<NodeId, Rect>, field: Field, drag: &DragInFlight, side: Side) -> Option<Zone> {
    let area = field.area;
    let inset = field.inset;
    let thickness = (inset - STRIP_MARGIN * 2.).max(px(0.));
    let strip = match side {
        Side::Top => Bounds::new(point(area.origin.x + inset, area.origin.y + STRIP_MARGIN), size((area.size.width - inset * 2.).max(px(0.)), thickness)),
        Side::Bottom => Bounds::new(point(area.origin.x + inset, area.origin.y + area.size.height - STRIP_MARGIN - thickness), size((area.size.width - inset * 2.).max(px(0.)), thickness)),
        Side::Left => Bounds::new(point(area.origin.x + STRIP_MARGIN, area.origin.y + inset), size(thickness, (area.size.height - inset * 2.).max(px(0.)))),
        Side::Right => Bounds::new(point(area.origin.x + area.size.width - STRIP_MARGIN - thickness, area.origin.y + inset), size(thickness, (area.size.height - inset * 2.).max(px(0.)))),
    };
    // The levels: the window's edge first, then the groups and the pane that
    // meet the edge where the pointer is, broadest first.
    let mut levels = vec![DockTarget::edge(side)];
    if let Some(pane) = pane_at_edge(root, rects, field, side, drag.pointer) {
        let mut deeper: Vec<DockTarget> = ops::ancestor_targets(root, &pane, side).into_iter().filter(|target| !matches!(target, DockTarget::WindowEdge { .. })).collect();
        deeper.reverse();
        levels.extend(deeper);
    }
    let level = drag.level_offset % levels.len();
    let target = levels[level].clone();
    // The highlight spans what the level docks beside; the window's edge spans it all.
    let drawn = match target.node().and_then(|node| span_of(root, rects, &target, node)) {
        Some(span) => {
            let span = field.place(span);
            match side {
                Side::Top | Side::Bottom => Bounds::new(point(span.origin.x, strip.origin.y), size(span.size.width, strip.size.height)),
                Side::Left | Side::Right => Bounds::new(point(strip.origin.x, span.origin.y), size(strip.size.width, span.size.height)),
            }
        }
        None => strip,
    };
    let outcome = outcome_of(layout, window, &drag.dragged, &target);
    let label = describe(root, &target, side);
    Some(Zone { kind: ZoneKind::Edge { side, level }, target, bounds: strip, drawn, label, outcome, hovered: false })
}

/// The pane whose stack meets `side` of the window where the pointer is,
/// measured across the edge.
fn pane_at_edge(root: &LayoutNode, rects: &HashMap<NodeId, Rect>, field: Field, side: Side, pointer: Point<Pixels>) -> Option<PaneId> {
    let content = field.content();
    if content.size.width <= px(0.) || content.size.height <= px(0.) {
        return None;
    }
    let across = match side {
        Side::Top | Side::Bottom => f64::from(f32::from((pointer.x - content.origin.x) / content.size.width)),
        Side::Left | Side::Right => f64::from(f32::from((pointer.y - content.origin.y) / content.size.height)),
    }
    .clamp(0.0, 1.0);
    let touches = |rect: &Rect| match side {
        Side::Top => rect.y < 1e-6,
        Side::Bottom => rect.y + rect.height > 1.0 - 1e-6,
        Side::Left => rect.x < 1e-6,
        Side::Right => rect.x + rect.width > 1.0 - 1e-6,
    };
    let spans = |rect: &Rect| match side {
        Side::Top | Side::Bottom => rect.x <= across && across <= rect.x + rect.width,
        Side::Left | Side::Right => rect.y <= across && across <= rect.y + rect.height,
    };
    root.stack_rects().into_iter().find(|(id, rect)| touches(rect) && spans(rect) && rects.contains_key(id)).and_then(|(id, _)| root.find(&id).and_then(LayoutNode::active_pane).cloned())
}

/// The rectangle a target docks beside: the node's, or the run's for a range.
fn span_of(root: &LayoutNode, rects: &HashMap<NodeId, Rect>, target: &DockTarget, node: &NodeId) -> Option<Rect> {
    match target {
        DockTarget::BesideRange { split, from, to, .. } => {
            let children = root.find(split)?.children();
            let first = rects.get(children.get(*from)?.id())?;
            let last = rects.get(children.get(*to)?.id())?;
            Some(Rect { x: first.x.min(last.x), y: first.y.min(last.y), width: (last.x + last.width).max(first.x + first.width) - first.x.min(last.x), height: (last.y + last.height).max(first.y + first.height) - first.y.min(last.y) })
        }
        _ => rects.get(node).copied(),
    }
}

/// What dropping `dragged` on `target` would do, tried on a copy of the
/// model: a pane is moved there, a new screen is opened there.
pub fn outcome_of(layout: &WorkspaceLayout, window: &WindowId, dragged: &Dragged, target: &DockTarget) -> Outcome {
    let mut trial = layout.clone();
    let landed = match dragged {
        Dragged::Pane(pane) => trial.move_pane(pane, window, target.clone()).map(|()| pane.clone()),
        Dragged::New(route) => trial.open_pane(window, kinds::definition_of(*route), target.clone()),
    };
    match landed {
        Ok(pane) => {
            let preview = trial.window(window).and_then(|window| window.root.as_ref()).and_then(|root| root.pane_rects().into_iter().find(|(id, _)| *id == pane)).map(|(_, rect)| rect).unwrap_or(Rect::UNIT);
            Outcome::Allowed { preview }
        }
        Err(OpError::NoOp) => Outcome::Unchanged,
        Err(err) => Outcome::Refused(err.to_string()),
    }
}

/// What dropping in a gap does.
fn describe_gap(axis: atlas_workspace::Axis) -> String {
    match axis {
        atlas_workspace::Axis::Horizontal => "Place between these panes, as a column".to_string(),
        atlas_workspace::Axis::Vertical => "Place between these panes, as a row".to_string(),
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

    /// `1 | 2 | 3` in equal thirds. The area is 1200 × 600 at the origin, the
    /// cards pulled in by 24 px with 28 px gaps, so the columns are drawn
    /// across the 1152 × 552 content.
    fn three_columns() -> (WorkspaceLayout, HashMap<PaneId, Bounds<Pixels>>, Field) {
        let mut layout = WorkspaceLayout::new("test");
        let window = WindowId::main();
        let one = layout.open_pane(&window, PaneDefinition::new("today"), DockTarget::edge(Side::Right)).unwrap();
        let two = layout.open_pane(&window, PaneDefinition::new("accounts"), DockTarget::WindowEdge { side: Side::Right, share: Some(0.5) }).unwrap();
        let three = layout.open_pane(&window, PaneDefinition::new("rules"), DockTarget::WindowEdge { side: Side::Right, share: Some(1.0 / 3.0) }).unwrap();
        let field = Field { area: Bounds::new(point(px(0.), px(0.)), size(px(1200.), px(600.))), inset: px(24.), gap: px(28.) };
        let mut drawn = HashMap::new();
        let column = field.content().size.width / 3.;
        for (index, pane) in [one, two, three].into_iter().enumerate() {
            drawn.insert(pane, Bounds::new(point(field.content().origin.x + column * index as f32, field.content().origin.y), size(column, field.content().size.height)));
        }
        (layout, drawn, field)
    }

    fn drag_at(x: f32, y: f32) -> DragInFlight {
        DragInFlight { dragged: Dragged::New(Route::ForecastPath), pointer: point(px(x), px(y)), level_offset: 0, elsewhere: None }
    }

    #[test]
    fn the_pane_under_the_pointer_is_found_and_hidden_tabs_are_ignored() {
        let (mut layout, mut drawn, _) = three_columns();
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
    fn three_columns_offer_two_gaps_and_four_edge_strips() {
        let (layout, _, field) = three_columns();
        let zones = zones_for(&layout, &WindowId::main(), field, &drag_at(600., 300.));
        let gaps: Vec<&Zone> = zones.iter().filter(|zone| matches!(zone.kind, ZoneKind::Gap { .. })).collect();
        let edges: Vec<&Zone> = zones.iter().filter(|zone| matches!(zone.kind, ZoneKind::Edge { .. })).collect();
        assert_eq!(gaps.len(), 2, "{zones:#?}");
        assert_eq!(edges.len(), 4);
        // The gaps are centred on the dividers, a gap plus slop wide, and take
        // a Beside on the child before them.
        let column = field.content().size.width / 3.;
        let first = gaps[0];
        assert!((first.bounds.center().x - (field.content().origin.x + column)).abs() < px(1.), "{:?}", first.bounds);
        assert_eq!(first.bounds.size.width, field.gap + GAP_SLOP * 2.);
        assert!(matches!(first.target, DockTarget::Beside { side: Side::Right, .. }));
        assert_eq!(first.label, "Place between these panes, as a column");
        assert!(first.element_id().starts_with("dock-gap-"));
        // The strips lie in the inset, along the whole edge.
        let bottom = edges.iter().find(|zone| zone.side() == Side::Bottom).unwrap();
        assert_eq!(bottom.element_id(), "dock-band-bottom-0");
        assert_eq!(bottom.label, "Dock along the bottom of the window");
        assert!(bottom.bounds.origin.y > field.content().origin.y + field.content().size.height);
        assert_eq!(bottom.bounds.size.width, field.content().size.width);
        assert!(!bottom.hovered);
        // Nothing is hovered from the middle of a pane.
        assert!(zones.iter().all(|zone| !zone.hovered));
    }

    #[test]
    fn the_pointer_in_a_gap_hovers_it_and_a_drop_there_lands_between() {
        let (layout, _, field) = three_columns();
        let column = field.content().size.width / 3.;
        let divider = field.content().origin.x + column * 2.;
        let zones = zones_for(&layout, &WindowId::main(), field, &drag_at(f32::from(divider), 300.));
        let hovered: Vec<&Zone> = zones.iter().filter(|zone| zone.hovered).collect();
        assert_eq!(hovered.len(), 1, "{zones:#?}");
        assert!(matches!(hovered[0].kind, ZoneKind::Gap { index: 2, .. }), "{:?}", hovered[0].kind);
        match &hovered[0].outcome {
            Outcome::Allowed { preview } => {
                assert!(preview.x > 1.0 / 3.0 && preview.x + preview.width <= 2.0 / 3.0 + 1e-9, "between the second and third column: {preview:?}");
                assert!((preview.height - 1.0).abs() < 1e-9, "a full column");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn space_walks_an_edge_strip_from_the_window_to_the_groups_that_meet_it() {
        // `1 | 2 | 3` with 4 as a tab of 3: the strip below shows the window,
        // then the 2–3 run, then the 3 (+4) pane as Space is pressed.
        let (mut layout, _, field) = three_columns();
        let panes = layout.main_window().unwrap().panes();
        let stack3 = layout.stack_of(&panes[2]).unwrap();
        layout.open_pane(&WindowId::main(), PaneDefinition::new("forecast"), DockTarget::tab(stack3)).unwrap();
        let x = f32::from(field.content().origin.x + field.content().size.width * 5. / 6.);
        let y = f32::from(field.area.origin.y + field.area.size.height - px(10.));
        let labels_at = |offset: usize| {
            let drag = DragInFlight { dragged: Dragged::New(Route::ForecastPath), pointer: point(px(x), px(y)), level_offset: offset, elsewhere: None };
            let zones = zones_for(&layout, &WindowId::main(), field, &drag);
            let bottom = zones.into_iter().find(|zone| matches!(zone.kind, ZoneKind::Edge { side: Side::Bottom, .. })).unwrap();
            assert!(bottom.hovered, "the pointer is in the bottom strip");
            (bottom.label, bottom.drawn.size.width)
        };
        let full = field.content().size.width;
        assert_eq!(labels_at(0), ("Dock along the bottom of the window".to_string(), full));
        // A stack of tabs is one pane on screen, so the 2–3 run is "2 panes".
        let (label, width) = labels_at(1);
        assert_eq!(label, "Dock below these 2 panes");
        assert!((width - full * 2. / 3.).abs() < px(1.), "the highlight spans the run: {width:?}");
        let (label, width) = labels_at(2);
        assert_eq!(label, "Dock below this pane");
        assert!((width - full / 3.).abs() < px(1.), "the highlight spans the pane: {width:?}");
        assert_eq!(labels_at(3).0, "Dock along the bottom of the window", "cycling wraps around");
    }

    #[test]
    fn a_refused_move_and_an_unchanged_one_are_reported() {
        let (layout, _, field) = three_columns();
        let panes = layout.main_window().unwrap().panes();
        // As many thin columns as the minimum share allows: one more, at
        // the default share, would squeeze the others below it.
        let mut crowded = layout.clone();
        while crowded.open_pane(&WindowId::main(), PaneDefinition::new("today"), DockTarget::WindowEdge { side: Side::Right, share: Some(ops::MIN_SHARE_ARG) }).is_ok() {}
        let drag = DragInFlight { dragged: Dragged::New(Route::ForecastPath), pointer: point(px(1190.), px(300.)), level_offset: 0, elsewhere: None };
        let zones = zones_for(&crowded, &WindowId::main(), field, &drag);
        let right = zones.iter().find(|zone| zone.side() == Side::Right && matches!(zone.kind, ZoneKind::Edge { .. })).unwrap();
        assert!(matches!(right.outcome, Outcome::Refused(_)), "{:?}", right.outcome);
        assert!(!right.accepts_drops());
        // Dragging the third pane to the right edge, where it already is, changes nothing.
        let drag = DragInFlight { dragged: Dragged::Pane(panes[2].clone()), pointer: point(px(1190.), px(300.)), level_offset: 0, elsewhere: None };
        let zones = zones_for(&layout, &WindowId::main(), field, &drag);
        let right = zones.iter().find(|zone| zone.side() == Side::Right && matches!(zone.kind, ZoneKind::Edge { .. })).unwrap();
        assert_eq!(right.outcome, Outcome::Unchanged);
        assert!(right.accepts_drops());
    }
}
