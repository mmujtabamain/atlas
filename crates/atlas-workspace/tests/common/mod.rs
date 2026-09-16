//! Helpers shared by the integration tests: workspaces built through the
//! public operations only, and grid pictures of them.

#![allow(dead_code)]

use atlas_workspace::grid;
use atlas_workspace::{DockTarget, LayoutNode, NodeId, PaneDefinition, PaneId, Side, WindowId, WorkspaceLayout};

/// Opens a pane of `kind` in the main window.
pub fn open(ws: &mut WorkspaceLayout, kind: &str, target: DockTarget) -> PaneId {
    let described = format!("{target:?}");
    ws.open_pane(&WindowId::main(), PaneDefinition::new(kind), target)
        .unwrap_or_else(|error| panic!("opening {kind} at {described} failed: {error}"))
}

/// Opens a pane of `kind` beside the stack that holds `anchor`.
pub fn open_beside(ws: &mut WorkspaceLayout, kind: &str, anchor: &PaneId, side: Side, share: Option<f64>) -> PaneId {
    let node = stack_of(ws, anchor);
    open(ws, kind, DockTarget::Beside { node, side, share })
}

/// Opens a pane of `kind` as a tab of the stack that holds `anchor`.
pub fn open_tab(ws: &mut WorkspaceLayout, kind: &str, anchor: &PaneId, index: Option<usize>) -> PaneId {
    let node = stack_of(ws, anchor);
    open(ws, kind, DockTarget::Stack { node, index })
}

/// The stack holding `pane`.
pub fn stack_of(ws: &WorkspaceLayout, pane: &PaneId) -> NodeId {
    ws.stack_of(pane).unwrap_or_else(|| panic!("{pane} has no stack"))
}

/// The main window's tree.
pub fn root(ws: &WorkspaceLayout) -> &LayoutNode {
    ws.main_window().and_then(|window| window.root.as_ref()).expect("main window is empty")
}

/// The main window's picture with panes numbered in pre-order.
pub fn picture(ws: &WorkspaceLayout, cols: usize, rows: usize) -> String {
    grid::render_numbered(root(ws), cols, rows)
}

/// The main window's picture with each pane labelled by the first character of its kind.
pub fn picture_by_kind(ws: &WorkspaceLayout, cols: usize, rows: usize) -> String {
    grid::render(root(ws), cols, rows, |pane| ws.pane(pane).and_then(|definition| definition.kind.chars().next()).unwrap_or('?'))
}

/// The workspace as canonical JSON, for byte-identical comparisons.
pub fn json(ws: &WorkspaceLayout) -> String {
    serde_json::to_string_pretty(ws).expect("workspace serializes")
}

/// Builds `123 / 123 / 124`: three equal columns, the right one split two
/// thirds over one third. Returns the workspace and the four pane ids.
pub fn three_columns_with_split() -> (WorkspaceLayout, [PaneId; 4]) {
    let mut ws = WorkspaceLayout::new("test");
    let p1 = open(&mut ws, "one", DockTarget::edge(Side::Right));
    let p2 = open(&mut ws, "two", DockTarget::WindowEdge { side: Side::Right, share: Some(0.5) });
    let p3 = open(
        &mut ws,
        "three",
        DockTarget::WindowEdge {
            side: Side::Right,
            share: Some(1.0 / 3.0),
        },
    );
    let p4 = open_beside(&mut ws, "four", &p3, Side::Bottom, Some(1.0 / 3.0));
    assert_eq!(picture(&ws, 3, 3), "123\n123\n124");
    (ws, [p1, p2, p3, p4])
}

/// Asserts a workspace has no invariant violations.
pub fn assert_valid(ws: &WorkspaceLayout) {
    let violations = ws.validate();
    assert!(violations.is_empty(), "invariants broken: {violations:#?}\n{}", json(ws));
}
