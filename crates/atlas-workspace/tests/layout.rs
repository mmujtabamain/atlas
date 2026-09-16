//! The layout model through its public operations: building pictures,
//! removing, moving, determinism, normalization, history, closed panes,
//! presets, the resolver, focus geometry, floating windows, and a randomized
//! run that must never break an invariant.

mod common;

use atlas_workspace::focus::{self, Direction};
use atlas_workspace::ops;
use atlas_workspace::resolver::{self, Intent, Resolution};
use atlas_workspace::{Axis, ClosedPanes, DockTarget, LayoutHistory, OpError, PaneDefinition, PaneId, Preset, Side, SplitLimits, WindowFrame, WindowId, WindowRole, WorkspaceLayout};
use common::{assert_valid, json, open, open_beside, open_tab, picture, picture_by_kind, root, stack_of, three_columns_with_split};
use serde_json::json as j;

// ---------------------------------------------------------------------------
// 1. Pictures built through the operations only
// ---------------------------------------------------------------------------

#[test]
fn builds_three_columns_with_the_right_one_split() {
    let (ws, _) = three_columns_with_split();
    assert_eq!(picture(&ws, 3, 3), "123\n123\n124");
    assert_valid(&ws);
    let tree = root(&ws);
    assert_eq!(tree.axis(), Some(Axis::Horizontal));
    for weight in tree.weights() {
        assert!((weight - 1.0 / 3.0).abs() < 1e-12, "{:?}", tree.weights());
    }
}

#[test]
fn builds_a_group_of_two_over_a_wide_pane() {
    // 1 | [ [2 | 3] / 4 ]  →  123 / 144 / 144
    let mut ws = WorkspaceLayout::new("test");
    let _p1 = open(&mut ws, "one", DockTarget::edge(Side::Right));
    let p2 = open(
        &mut ws,
        "two",
        DockTarget::WindowEdge {
            side: Side::Right,
            share: Some(2.0 / 3.0),
        },
    );
    let _p4 = open_beside(&mut ws, "four", &p2, Side::Bottom, Some(2.0 / 3.0));
    let _p3 = open_beside(&mut ws, "three", &p2, Side::Right, Some(0.5));
    assert_eq!(picture(&ws, 3, 3), "123\n144\n144");
    assert_valid(&ws);
}

#[test]
fn ancestor_docking_below_a_row_spans_the_whole_row() {
    // 1 | 2 | 3, then 4 docked at the bottom of the split holding them: the
    // new pane spans the group and the group's children keep their ratio.
    let mut ws = WorkspaceLayout::new("test");
    let _p1 = open(&mut ws, "one", DockTarget::edge(Side::Right));
    let _p2 = open(&mut ws, "two", DockTarget::WindowEdge { side: Side::Right, share: Some(0.5) });
    let _p3 = open(
        &mut ws,
        "three",
        DockTarget::WindowEdge {
            side: Side::Right,
            share: Some(1.0 / 3.0),
        },
    );
    assert_eq!(picture(&ws, 3, 3), "123\n123\n123");
    let row = root(&ws).id().clone();
    let _p4 = open(
        &mut ws,
        "four",
        DockTarget::Beside {
            node: row.clone(),
            side: Side::Bottom,
            share: Some(1.0 / 3.0),
        },
    );
    assert_eq!(picture(&ws, 3, 3), "123\n123\n444");
    let tree = root(&ws);
    assert_eq!(tree.axis(), Some(Axis::Vertical));
    let inner = tree.find(&row).expect("the row keeps its id under the new split");
    for weight in inner.weights() {
        assert!((weight - 1.0 / 3.0).abs() < 1e-12);
    }
    assert_valid(&ws);
}

#[test]
fn the_two_three_over_four_example() {
    // 2 | 3 with 4 docked below the pair: 23 / 23 / 44.
    let mut ws = WorkspaceLayout::new("test");
    let _p2 = open(&mut ws, "2", DockTarget::edge(Side::Right));
    let _p3 = open(&mut ws, "3", DockTarget::WindowEdge { side: Side::Right, share: Some(0.5) });
    let pair = root(&ws).id().clone();
    let _p4 = open(
        &mut ws,
        "4",
        DockTarget::Beside {
            node: pair,
            side: Side::Bottom,
            share: Some(1.0 / 3.0),
        },
    );
    assert_eq!(picture_by_kind(&ws, 2, 3), "23\n23\n44");
    assert_valid(&ws);
}

#[test]
fn a_group_beside_a_full_height_pane() {
    // 1 full height on the left; 2 | 3 over 4 on the right: 123 / 123 / 144.
    let mut ws = WorkspaceLayout::new("test");
    let _p1 = open(&mut ws, "one", DockTarget::edge(Side::Right));
    let p2 = open(
        &mut ws,
        "two",
        DockTarget::WindowEdge {
            side: Side::Right,
            share: Some(2.0 / 3.0),
        },
    );
    let _p4 = open_beside(&mut ws, "four", &p2, Side::Bottom, Some(1.0 / 3.0));
    let _p3 = open_beside(&mut ws, "three", &p2, Side::Right, Some(0.5));
    assert_eq!(picture(&ws, 3, 3), "123\n123\n144");
    assert_valid(&ws);

    // Ancestor docking by moving: 4 to the bottom of the whole window makes
    // it span every column, and the same-axis row flattens into one split.
    let p4 = ws.find_panes("four", None)[0].clone();
    ws.move_pane(
        &p4,
        &WindowId::main(),
        DockTarget::WindowEdge {
            side: Side::Bottom,
            share: Some(1.0 / 3.0),
        },
    )
    .unwrap();
    assert_eq!(picture(&ws, 3, 3), "123\n123\n444");
    assert_eq!(root(&ws).children()[0].children().len(), 3, "1, 2 and 3 are one flat row");
    assert_valid(&ws);
}

#[test]
fn docking_beside_a_split_of_the_same_axis_scales_its_children() {
    // A right of the whole [B | C]: B and C shrink by (1 - s) and keep 2:1.
    let mut ws = WorkspaceLayout::new("test");
    let _b = open(&mut ws, "b", DockTarget::edge(Side::Right));
    let _c = open(
        &mut ws,
        "c",
        DockTarget::WindowEdge {
            side: Side::Right,
            share: Some(1.0 / 3.0),
        },
    );
    let group = root(&ws).id().clone();
    let _a = open(
        &mut ws,
        "a",
        DockTarget::Beside {
            node: group,
            side: Side::Right,
            share: Some(0.25),
        },
    );
    let weights = root(&ws).weights().to_vec();
    assert_eq!(weights.len(), 3);
    assert!((weights[0] - 0.5).abs() < 1e-12, "{weights:?}");
    assert!((weights[1] - 0.25).abs() < 1e-12, "{weights:?}");
    assert!((weights[2] - 0.25).abs() < 1e-12, "{weights:?}");
    assert_eq!(picture_by_kind(&ws, 4, 1), "bbca");
}

#[test]
fn centre_docking_makes_a_tab_and_activates_it() {
    let (mut ws, [p1, p2, _, _]) = three_columns_with_split();
    let p5 = open_tab(&mut ws, "five", &p2, Some(0));
    assert_eq!(stack_of(&ws, &p5), stack_of(&ws, &p2));
    let stack = root(&ws).find(&stack_of(&ws, &p2)).unwrap();
    assert_eq!(stack.stack_panes(), &[p5.clone(), p2.clone()]);
    assert_eq!(stack.active_pane(), Some(&p5));
    assert_eq!(ws.active_pane(), Some(p5.clone()));
    // Only the active pane of a stack is painted.
    assert_eq!(picture_by_kind(&ws, 3, 3), "oft\noft\noff");
    ws.set_active_pane(&p2).unwrap();
    assert_eq!(picture_by_kind(&ws, 3, 1), "ott");
    assert_ne!(stack_of(&ws, &p1), stack_of(&ws, &p2));
}

#[test]
fn the_minimum_size_rule_rejects_slivers_but_never_tabs() {
    let mut ws = WorkspaceLayout::new("test");
    let _p1 = open(&mut ws, "one", DockTarget::edge(Side::Right));
    let p2 = open(&mut ws, "two", DockTarget::WindowEdge { side: Side::Right, share: Some(0.15) });
    let before = json(&ws);
    let error = ws
        .open_pane(
            &WindowId::main(),
            PaneDefinition::new("three"),
            DockTarget::Beside {
                node: stack_of(&ws, &p2),
                side: Side::Right,
                share: Some(0.15),
            },
        )
        .unwrap_err();
    assert!(matches!(error, OpError::TooSmall { .. }), "{error}");
    assert_eq!(json(&ws), before, "a rejected split leaves the workspace untouched");
    assert!(error.to_string().contains("tab"), "the message points at tab docking: {error}");
    // Tab docking into the narrow stack is still fine.
    let p3 = ws.open_pane(&WindowId::main(), PaneDefinition::new("three"), DockTarget::tab(stack_of(&ws, &p2))).unwrap();
    assert_eq!(stack_of(&ws, &p3), stack_of(&ws, &p2));
    // Relaxed limits allow the sliver.
    ws.set_limits(SplitLimits::unlimited());
    ws.open_pane(
        &WindowId::main(),
        PaneDefinition::new("four"),
        DockTarget::Beside {
            node: stack_of(&ws, &p2),
            side: Side::Right,
            share: Some(0.15),
        },
    )
    .unwrap();
    assert_valid(&ws);
}

#[test]
fn the_depth_limit_is_enforced() {
    let mut ws = WorkspaceLayout::new("test").with_limits(SplitLimits { min_share: 0.0, max_depth: 2 });
    let p1 = open(&mut ws, "one", DockTarget::edge(Side::Right));
    let p2 = open_beside(&mut ws, "two", &p1, Side::Right, None);
    let p3 = open_beside(&mut ws, "three", &p2, Side::Bottom, None);
    assert_eq!(root(&ws).depth(), 2);
    let error = ws
        .open_pane(
            &WindowId::main(),
            PaneDefinition::new("four"),
            DockTarget::Beside {
                node: stack_of(&ws, &p3),
                side: Side::Right,
                share: None,
            },
        )
        .unwrap_err();
    assert!(matches!(error, OpError::TooDeep { max_depth: 2, depth: 3 }), "{error}");
}

// ---------------------------------------------------------------------------
// 2. Removing
// ---------------------------------------------------------------------------

#[test]
fn removing_collapses_the_split_and_leaves_the_rest_identical() {
    let (mut ws, [p1, p2, p3, p4]) = three_columns_with_split();
    let before = root(&ws).clone();
    let untouched_left = before.children()[0].clone();
    let untouched_middle = before.children()[1].clone();

    ws.close_pane(&p3).unwrap();
    assert_valid(&ws);
    assert_eq!(picture(&ws, 3, 3), "123\n123\n123");
    let after = root(&ws);
    assert_eq!(after.id(), before.id(), "the root split keeps its id");
    assert_eq!(after.children()[0], untouched_left);
    assert_eq!(after.children()[1], untouched_middle);
    assert_eq!(after.children()[2].id(), &stack_of(&ws, &p4), "the vertical split collapsed into the surviving stack");
    assert_eq!(after.weights(), before.weights());
    assert!(!ws.panes.contains_key(&p3));
    assert_eq!(ws.find_panes("one", None), vec![p1.clone()]);
    assert_eq!(ws.find_panes("two", None), vec![p2]);

    // Closing the active pane hands focus to its neighbour.
    ws.set_active_pane(&p4).unwrap();
    ws.close_pane(&p4).unwrap();
    assert!(ws.active_pane().is_some());
    assert_valid(&ws);
    ws.close_pane(&p1).unwrap();
    ws.close_pane(&ws.active_pane().unwrap()).unwrap();
    assert!(ws.is_empty());
    assert!(ws.main_window().unwrap().is_empty());
    assert_eq!(ws.active_pane(), None);
    assert_valid(&ws);
}

#[test]
fn removing_a_tab_keeps_the_stack_and_moves_the_active_tab() {
    let (mut ws, [_, p2, _, _]) = three_columns_with_split();
    let p5 = open_tab(&mut ws, "five", &p2, None);
    let p6 = open_tab(&mut ws, "six", &p2, None);
    let stack = stack_of(&ws, &p2);
    assert_eq!(ws.active_pane(), Some(p6.clone()));
    ws.set_active_pane(&p5).unwrap();
    ws.close_pane(&p5).unwrap();
    let node = root(&ws).find(&stack).expect("the stack survives");
    assert_eq!(node.stack_panes(), &[p2.clone(), p6.clone()]);
    assert_eq!(node.active_pane(), Some(&p6), "the tab that slid into the closed position takes over");
    assert_eq!(ws.active_pane(), Some(p6));
    assert_valid(&ws);
}

// ---------------------------------------------------------------------------
// 3. Moving
// ---------------------------------------------------------------------------

#[test]
fn moving_changes_only_the_source_and_destination() {
    let (mut ws, [p1, p2, p3, _]) = three_columns_with_split();
    let before = root(&ws).clone();
    let right_group = before.children()[2].clone();

    ws.move_pane(&p1, &WindowId::main(), DockTarget::tab(stack_of(&ws, &p2))).unwrap();
    assert_valid(&ws);
    let after = root(&ws);
    assert_eq!(after.children().len(), 2);
    assert_eq!(after.children()[1], right_group, "the untouched group keeps ids and weights");
    assert_eq!(after.children()[0].stack_panes(), &[p2.clone(), p1.clone()]);
    assert!(
        (after.weights()[0] - 0.5).abs() < 1e-12,
        "the emptied column's weight goes to the survivors proportionally: {:?}",
        after.weights()
    );
    assert!((after.weights()[1] - 0.5).abs() < 1e-12);
    assert_eq!(picture(&ws, 3, 3), "223\n223\n224", "pre-order numbering: 2, then the moved 1 (active), 3, 4");

    // Moving 3 to the left edge of the window.
    ws.move_pane(
        &p3,
        &WindowId::main(),
        DockTarget::WindowEdge {
            side: Side::Left,
            share: Some(1.0 / 3.0),
        },
    )
    .unwrap();
    assert_eq!(picture(&ws, 3, 3), "134\n134\n134");
    assert_valid(&ws);
}

#[test]
fn moving_a_pane_onto_its_own_place_is_a_noop_that_changes_nothing() {
    let (mut ws, [p1, p2, p3, p4]) = three_columns_with_split();
    let before = json(&ws);
    let cases = vec![
        DockTarget::tab(stack_of(&ws, &p4)),
        DockTarget::Beside {
            node: stack_of(&ws, &p3),
            side: Side::Bottom,
            share: None,
        },
        DockTarget::Beside {
            node: stack_of(&ws, &p4),
            side: Side::Left,
            share: None,
        },
        DockTarget::Beside {
            node: root(&ws).parent_of(&stack_of(&ws, &p4)).unwrap(),
            side: Side::Bottom,
            share: None,
        },
    ];
    for target in cases {
        assert_eq!(ws.move_pane(&p4, &WindowId::main(), target.clone()), Err(OpError::NoOp), "{target:?}");
        assert_eq!(json(&ws), before, "{target:?}");
    }
    assert_eq!(ws.move_pane(&p1, &WindowId::main(), DockTarget::edge(Side::Left)), Err(OpError::NoOp));
    assert_eq!(
        ws.move_pane(
            &p1,
            &WindowId::main(),
            DockTarget::Beside {
                node: stack_of(&ws, &p2),
                side: Side::Left,
                share: None
            }
        ),
        Err(OpError::NoOp)
    );
    assert_eq!(
        ws.move_pane(
            &p2,
            &WindowId::main(),
            DockTarget::Beside {
                node: stack_of(&ws, &p1),
                side: Side::Right,
                share: None
            }
        ),
        Err(OpError::NoOp)
    );
    assert_eq!(json(&ws), before);
}

#[test]
fn a_failed_move_leaves_the_workspace_byte_identical() {
    let (mut ws, [p1, _, _, _]) = three_columns_with_split();
    let before = json(&ws);
    let error = ws.move_pane(&p1, &WindowId::main(), DockTarget::tab(atlas_workspace::NodeId::new("node_missing"))).unwrap_err();
    assert!(matches!(error, OpError::UnknownNode(_)));
    assert_eq!(json(&ws), before);
    let error = ws.move_pane(&PaneId::new("pane_missing"), &WindowId::main(), DockTarget::edge(Side::Left)).unwrap_err();
    assert!(matches!(error, OpError::UnknownPane(_)));
    assert_eq!(json(&ws), before);
    let error = ws.move_pane(&p1, &WindowId::new("window_missing"), DockTarget::edge(Side::Left)).unwrap_err();
    assert!(matches!(error, OpError::UnknownWindow(_)));
    assert_eq!(json(&ws), before);
}

#[test]
fn moving_onto_a_split_that_collapses_retargets_to_the_survivor() {
    // Target the split [3 / 4] while moving 3 out of it: the split collapses
    // into 4's stack, and the drop lands beside that stack.
    let (mut ws, [_, _, p3, p4]) = three_columns_with_split();
    let split = root(&ws).parent_of(&stack_of(&ws, &p3)).unwrap();
    ws.move_pane(
        &p3,
        &WindowId::main(),
        DockTarget::Beside {
            node: split,
            side: Side::Bottom,
            share: Some(1.0 / 3.0),
        },
    )
    .unwrap();
    assert_eq!(picture_by_kind(&ws, 3, 3), "otf\notf\nott", "four is now above three");
    assert_eq!(focus::neighbour(root(&ws), &p4, Direction::Down), Some(p3));
    assert_valid(&ws);
}

#[test]
fn tabs_reorder_within_a_stack() {
    let (mut ws, [_, p2, _, _]) = three_columns_with_split();
    let stack = stack_of(&ws, &p2);
    let p5 = open(&mut ws, "five", DockTarget::tab(stack.clone()));
    let p6 = open(&mut ws, "six", DockTarget::tab(stack.clone()));
    ws.move_pane(&p6, &WindowId::main(), DockTarget::Stack { node: stack.clone(), index: Some(0) }).unwrap();
    assert_eq!(root(&ws).find(&stack).unwrap().stack_panes(), &[p6.clone(), p2.clone(), p5.clone()]);
    assert_eq!(ws.move_pane(&p6, &WindowId::main(), DockTarget::Stack { node: stack.clone(), index: Some(0) }), Err(OpError::NoOp));
    ws.move_pane(&p6, &WindowId::main(), DockTarget::Stack { node: stack.clone(), index: Some(99) }).unwrap();
    assert_eq!(root(&ws).find(&stack).unwrap().stack_panes(), &[p2, p5, p6]);
    assert_valid(&ws);
}

#[test]
fn stack_and_unstack_are_moves() {
    let (mut ws, [p1, p2, _, _]) = three_columns_with_split();
    ws.stack_panes(&p1, &p2).unwrap();
    assert_eq!(stack_of(&ws, &p1), stack_of(&ws, &p2));
    assert_eq!(ws.stack_panes(&p1, &p2), Err(OpError::NoOp));
    ws.unstack_pane(&p1, Side::Left).unwrap();
    assert_ne!(stack_of(&ws, &p1), stack_of(&ws, &p2));
    assert_eq!(focus::neighbour(root(&ws), &p1, Direction::Right), Some(p2.clone()));
    assert_eq!(ws.unstack_pane(&p1, Side::Left), Err(OpError::NotStacked(p1)));
    assert_valid(&ws);
}

#[test]
fn resize_goes_through_the_workspace() {
    let (mut ws, _) = three_columns_with_split();
    let split = root(&ws).id().clone();
    ws.resize(&split, &[2.0, 1.0, 1.0]).unwrap();
    assert_eq!(root(&ws).weights(), &[0.5, 0.25, 0.25]);
    assert_eq!(picture(&ws, 4, 1), "1123");
    assert!(matches!(ws.resize(&split, &[1.0]), Err(OpError::InvalidWeights { .. })));
    assert!(matches!(ws.resize(&atlas_workspace::NodeId::new("nope"), &[1.0, 1.0]), Err(OpError::UnknownNode(_))));
    assert_valid(&ws);
}

// ---------------------------------------------------------------------------
// 4. Determinism
// ---------------------------------------------------------------------------

fn scripted_workspace() -> WorkspaceLayout {
    let (mut ws, [p1, p2, p3, p4]) = three_columns_with_split();
    let p5 = open_tab(&mut ws, "five", &p2, None);
    ws.move_pane(
        &p1,
        &WindowId::main(),
        DockTarget::Beside {
            node: stack_of(&ws, &p4),
            side: Side::Right,
            share: None,
        },
    )
    .unwrap();
    ws.close_pane(&p3).unwrap();
    ws.detach_pane(&p5, WindowFrame::new(10.0, 20.0, 800.0, 600.0)).unwrap();
    let columns = root(&ws).children().len();
    let weights: Vec<f64> = (1..=columns).map(|i| i as f64).collect();
    ws.resize(&root(&ws).id().clone(), &weights).unwrap();
    ws
}

#[test]
fn the_same_operations_yield_identical_json() {
    let first = json(&scripted_workspace());
    let second = json(&scripted_workspace());
    assert_eq!(first, second);
    assert!(first.contains("\"pane_5\""));
}

// ---------------------------------------------------------------------------
// 5. JSON round trip and hand-written documents
// ---------------------------------------------------------------------------

#[test]
fn json_round_trips_and_uses_the_planned_keys() {
    let ws = scripted_workspace();
    let value = serde_json::to_value(&ws).unwrap();
    for key in ["schemaVersion", "workspaceId", "name", "scope", "windows", "panes", "activeWindow", "ids"] {
        assert!(value.get(key).is_some(), "missing {key}: {value}");
    }
    assert_eq!(value["schemaVersion"], atlas_workspace::SCHEMA_VERSION);
    assert_eq!(value["windows"][0]["id"], "window_main");
    assert_eq!(value["windows"][0]["role"], "main");
    assert_eq!(value["windows"][1]["role"], "floating");
    assert!(value["windows"][1]["frame"]["width"].is_number());
    assert_eq!(value["windows"][0]["root"]["type"], "split");
    assert!(value["windows"][0]["root"]["children"][0]["activePaneId"].is_string());
    assert!(value["panes"]["pane_1"]["viewState"].is_object());
    assert!(value["panes"]["pane_1"].get("kind").is_some());
    let text = serde_json::to_string(&ws).unwrap();
    let back: WorkspaceLayout = serde_json::from_str(&text).unwrap();
    assert_eq!(back, ws);
}

#[test]
fn a_hand_written_document_loads_and_is_normalized() {
    let document = j!({
        "schemaVersion": 1,
        "workspaceId": "hand",
        "name": "Hand written",
        "scope": { "householdId": "hh-1" },
        "windows": [
            {
                "id": "window_main",
                "role": "main",
                "root": {
                    "type": "split", "id": "node_1", "axis": "horizontal", "weights": [2, 1],
                    "children": [
                        { "type": "stack", "id": "node_2", "panes": ["pane_1"], "activePaneId": "pane_1" },
                        { "type": "split", "id": "node_3", "axis": "vertical",
                          "children": [
                              { "type": "stack", "id": "node_4", "panes": ["pane_2", "pane_3"], "activePaneId": "pane_3" },
                              { "type": "stack", "panes": ["pane_4"] }
                          ] }
                    ]
                },
                "activePane": "pane_1"
            }
        ],
        "panes": {
            "pane_1": { "kind": "today" },
            "pane_2": { "kind": "account", "resource": { "accountId": "acc-1" }, "viewState": { "scroll": 40 } },
            "pane_3": { "kind": "account", "resource": { "accountId": "acc-2" } },
            "pane_4": { "kind": "forecast" }
        },
        "activeWindow": "window_main"
    });
    let ws = atlas_workspace::persist::migrate(document).unwrap_or_else(|error| panic!("{error}"));
    assert_valid(&ws);
    assert_eq!(ws.scope.household_id.as_deref(), Some("hh-1"));
    assert_eq!(picture_by_kind(&ws, 3, 2), "tta\nttf");
    let tree = root(&ws);
    assert_eq!(tree.weights(), &[2.0 / 3.0, 1.0 / 3.0], "weights are normalised on load");
    let inner = &tree.children()[1];
    assert_eq!(inner.weights(), &[0.5, 0.5], "missing weights become equal");
    assert!(!inner.children()[1].id().is_blank(), "a blank node id is replaced");
    assert_eq!(ws.ids.next_pane, 5, "the id source moves past the ids in the file");
    assert!(ws.ids.next_node >= 5);
    assert_eq!(ws.find_panes("account", Some(&j!({ "accountId": "acc-2" }))), vec![PaneId::new("pane_3")]);
}

// ---------------------------------------------------------------------------
// 6. Normalization and the randomized run
// ---------------------------------------------------------------------------

#[test]
fn normalize_reports_no_change_on_a_live_workspace() {
    let ws = scripted_workspace();
    for window in &ws.windows {
        let mut root = window.root.clone();
        let report = ops::normalize(&mut root);
        assert!(!report.changed, "{}: {report:?}", window.id);
        assert_eq!(root, window.root);
    }
}

/// A tiny deterministic generator so the run is reproducible without extra crates.
struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound.max(1) as u64) as usize
    }

    fn unit(&mut self) -> f64 {
        (self.next() % 10_000) as f64 / 10_000.0
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() { None } else { items.get(self.below(items.len())) }
    }
}

fn random_side(rng: &mut XorShift) -> Side {
    Side::all()[rng.below(4)]
}

fn random_target(rng: &mut XorShift, ws: &WorkspaceLayout, window: &WindowId) -> DockTarget {
    let Some(tree) = ws.window(window).and_then(|w| w.root.as_ref()) else {
        return DockTarget::edge(random_side(rng));
    };
    let nodes = tree.node_ids();
    let node = rng.pick(&nodes).cloned().unwrap_or_else(|| tree.id().clone());
    match rng.below(10) {
        0 | 1 => DockTarget::WindowEdge {
            side: random_side(rng),
            share: Some(0.1 + rng.unit() * 0.8),
        },
        2..=5 => DockTarget::Beside {
            node,
            side: random_side(rng),
            share: if rng.below(2) == 0 { None } else { Some(rng.unit()) },
        },
        _ => match tree.find(&node) {
            Some(found) if found.is_stack() => DockTarget::Stack {
                node,
                index: if rng.below(2) == 0 { None } else { Some(rng.below(4)) },
            },
            _ => DockTarget::tab(tree.stack_ids()[rng.below(tree.leaf_count())].clone()),
        },
    }
}

#[test]
fn two_thousand_random_operations_never_break_an_invariant() {
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    let mut ws = WorkspaceLayout::new("random");
    let mut history = LayoutHistory::new(50);
    let mut closed = ClosedPanes::default();
    let mut applied = 0;
    let mut noops = 0;
    let mut rejected = 0;

    for step in 0..2000 {
        let before = ws.clone();
        let before_json = json(&before);
        let panes: Vec<PaneId> = ws.panes.keys().cloned().collect();
        let windows: Vec<WindowId> = ws.windows.iter().map(|w| w.id.clone()).collect();
        let window = rng.pick(&windows).cloned().unwrap_or_else(WindowId::main);
        let choice = if panes.len() > 14 { 1 } else { rng.below(12) };
        let label;
        let result: Result<(), OpError> = match choice {
            0 | 8 | 9 => {
                label = "open";
                let target = random_target(&mut rng, &ws, &window);
                let kind = format!("kind{}", rng.below(5));
                let definition = PaneDefinition::new(kind).with_resource(j!({ "n": rng.below(3) }));
                ws.open_pane(&window, definition, target).map(|_| ())
            }
            1 => {
                label = "close";
                match rng.pick(&panes).cloned() {
                    Some(pane) => ws.close_pane_detailed(&pane).map(|entry| closed.record(entry)),
                    None => Err(OpError::NoOp),
                }
            }
            2 | 10 => {
                label = "move";
                match rng.pick(&panes).cloned() {
                    Some(pane) => {
                        let target = random_target(&mut rng, &ws, &window);
                        ws.move_pane(&pane, &window, target)
                    }
                    None => Err(OpError::NoOp),
                }
            }
            3 => {
                label = "resize";
                let splits: Vec<_> = ws
                    .windows
                    .iter()
                    .filter_map(|w| w.root.as_ref())
                    .flat_map(|tree| tree.node_ids().into_iter().filter(|id| tree.find(id).is_some_and(|n| n.is_split())).collect::<Vec<_>>())
                    .collect();
                match rng.pick(&splits).cloned() {
                    Some(split) => {
                        let count = ws
                            .window_of_node(&split)
                            .and_then(|w| w.root.as_ref())
                            .and_then(|t| t.find(&split))
                            .map(|n| n.children().len())
                            .unwrap_or(0);
                        let weights: Vec<f64> = (0..count).map(|_| 0.05 + rng.unit()).collect();
                        ws.resize(&split, &weights)
                    }
                    None => Err(OpError::NoOp),
                }
            }
            4 => {
                label = "stack";
                match (rng.pick(&panes).cloned(), rng.pick(&panes).cloned()) {
                    (Some(pane), Some(onto)) if ws.window_id_of(&pane) == ws.window_id_of(&onto) => ws.stack_panes(&pane, &onto),
                    _ => Err(OpError::NoOp),
                }
            }
            5 => {
                label = "unstack";
                match rng.pick(&panes).cloned() {
                    Some(pane) => ws.unstack_pane(&pane, random_side(&mut rng)),
                    None => Err(OpError::NoOp),
                }
            }
            6 => {
                label = "set_active";
                match rng.pick(&panes).cloned() {
                    Some(pane) => ws.set_active_pane(&pane),
                    None => Err(OpError::NoOp),
                }
            }
            7 => {
                label = "detach";
                match rng.pick(&panes).cloned() {
                    Some(pane) if ws.windows.len() < 4 => ws
                        .detach_pane(&pane, WindowFrame::new(rng.unit() * 500.0, rng.unit() * 500.0, 400.0 + rng.unit() * 800.0, 300.0 + rng.unit() * 600.0))
                        .map(|_| ()),
                    _ => Err(OpError::NoOp),
                }
            }
            _ => {
                label = "undo/reopen";
                if rng.below(2) == 0 {
                    match history.undo(&ws) {
                        Some((restored, _)) => {
                            ws = restored;
                            Ok(())
                        }
                        None => Err(OpError::NoOp),
                    }
                } else {
                    match closed.pop() {
                        Some(entry) => {
                            let fallback = ws.active_window().map(|w| w.default_target()).unwrap_or_else(|| DockTarget::edge(Side::Right));
                            ws.reopen(entry, fallback).map(|_| ())
                        }
                        None => Err(OpError::NoOp),
                    }
                }
            }
        };

        match result {
            Ok(()) => {
                applied += 1;
                if label != "undo/reopen" {
                    history.push(label, before);
                }
            }
            Err(OpError::NoOp) => {
                noops += 1;
                assert_eq!(json(&ws), before_json, "step {step}: a NoOp {label} changed the workspace");
            }
            Err(_) => {
                rejected += 1;
                assert_eq!(json(&ws), before_json, "step {step}: a rejected {label} changed the workspace");
            }
        }

        let violations = ws.validate();
        assert!(violations.is_empty(), "step {step} ({label}) broke the invariants: {violations:#?}\n{}", json(&ws));
        for window in &ws.windows {
            let mut copy = window.root.clone();
            let report = ops::normalize(&mut copy);
            assert!(!report.changed, "step {step} ({label}) left {} un-normalized: {report:?}", window.id);
        }
    }
    assert!(applied > 500, "applied {applied}, noops {noops}, rejected {rejected}");
}

// ---------------------------------------------------------------------------
// 8. History
// ---------------------------------------------------------------------------

#[test]
fn undo_and_redo_restore_the_exact_json() {
    let (mut ws, [p1, p2, _, _]) = three_columns_with_split();
    let mut history = LayoutHistory::new(3);
    let original = json(&ws);

    let snapshot = ws.clone();
    ws.move_pane(&p1, &WindowId::main(), DockTarget::tab(stack_of(&ws, &p2))).unwrap();
    history.push("Move pane", snapshot);
    let moved = json(&ws);
    assert_ne!(moved, original);

    // A NoOp move adds no entry.
    let snapshot = ws.clone();
    assert_eq!(ws.move_pane(&p1, &WindowId::main(), DockTarget::tab(stack_of(&ws, &p2))), Err(OpError::NoOp));
    drop(snapshot);
    assert_eq!(history.len(), 1);

    let (restored, label) = history.undo(&ws).unwrap();
    assert_eq!(label, "Move pane");
    ws = restored;
    assert_eq!(json(&ws), original);
    let (redone, _) = history.redo(&ws).unwrap();
    ws = redone;
    assert_eq!(json(&ws), moved);

    // The cap evicts the oldest entry.
    for n in 0..5 {
        let snapshot = ws.clone();
        open_tab(&mut ws, &format!("extra{n}"), &p2, None);
        history.push(format!("Open extra{n}"), snapshot);
    }
    assert_eq!(history.labels(), vec!["Open extra2", "Open extra3", "Open extra4"]);
}

// ---------------------------------------------------------------------------
// 9. Closed panes
// ---------------------------------------------------------------------------

#[test]
fn reopen_lands_in_the_old_stack_when_it_still_exists() {
    let (mut ws, [_, p2, _, _]) = three_columns_with_split();
    let stack = stack_of(&ws, &p2);
    let p5 = open(&mut ws, "five", DockTarget::Stack { node: stack.clone(), index: Some(0) });
    let closed = ws.close_pane_detailed(&p5).unwrap();
    assert_eq!(closed.stack, Some(stack.clone()));
    assert_eq!(closed.index, 0);
    assert_eq!(closed.neighbour, Some(p2.clone()));
    assert_eq!(closed.neighbour_side, None);
    let reopened = ws.reopen(closed, DockTarget::edge(Side::Left)).unwrap();
    assert_eq!(stack_of(&ws, &reopened), stack);
    assert_eq!(root(&ws).find(&stack).unwrap().stack_panes(), &[reopened.clone(), p2]);
    assert_eq!(ws.pane(&reopened).unwrap().kind, "five");
    assert_valid(&ws);
}

#[test]
fn reopen_goes_beside_the_neighbour_when_the_stack_is_gone() {
    let (mut ws, [_, _, p3, p4]) = three_columns_with_split();
    let closed = ws.close_pane_detailed(&p3).unwrap();
    assert_eq!(closed.neighbour, Some(p4.clone()));
    assert_eq!(closed.neighbour_side, Some(Side::Top));
    assert_eq!(picture(&ws, 3, 3), "123\n123\n123");
    let reopened = ws.reopen(closed, DockTarget::edge(Side::Left)).unwrap();
    assert_eq!(focus::neighbour(root(&ws), &reopened, Direction::Down), Some(p4.clone()));
    assert_eq!(focus::neighbour(root(&ws), &p4, Direction::Up), Some(reopened.clone()));
    assert_eq!(picture(&ws, 3, 3), "123\n124\n124", "the reopened pane takes the default share above 4");
    assert_valid(&ws);
}

#[test]
fn reopen_falls_back_when_stack_and_neighbour_are_gone() {
    let (mut ws, [p1, _, p3, p4]) = three_columns_with_split();
    let closed = ws.close_pane_detailed(&p3).unwrap();
    ws.close_pane(&p4).unwrap();
    ws.set_active_pane(&p1).unwrap();
    let reopened = ws.reopen(closed, DockTarget::tab(stack_of(&ws, &p1))).unwrap();
    assert_eq!(stack_of(&ws, &reopened), stack_of(&ws, &p1));
    assert_valid(&ws);

    // A fallback node that no longer exists degrades to the window edge.
    let closed = ws.close_pane_detailed(&reopened).unwrap();
    ws.close_pane(&p1).unwrap();
    let reopened = ws.reopen(closed, DockTarget::tab(atlas_workspace::NodeId::new("node_gone"))).unwrap();
    assert!(ws.window_of(&reopened).is_some());
    assert_valid(&ws);
}

// ---------------------------------------------------------------------------
// 10. Presets
// ---------------------------------------------------------------------------

#[test]
fn every_preset_draws_its_picture() {
    let cases: [(Preset, usize, &str); 5] = [
        (Preset::Focus, 3, "111\n111\n111"),
        (Preset::Compare, 3, "112\n112\n112"),
        (Preset::MainInspector, 4, "1112\n1112\n1112"),
        (Preset::Analysis, 3, "112\n113\n113"),
        (Preset::Review, 3, "122\n133\n133"),
    ];
    for (preset, cols, expected) in cases {
        let template = preset.template();
        let panes: Vec<PaneDefinition> = template
            .slots
            .iter()
            .enumerate()
            .map(|(i, slot)| PaneDefinition::new(slot.clone()).with_resource(j!({ "i": i })))
            .collect();
        let ws = WorkspaceLayout::from_template(preset.label(), &template, panes);
        assert_valid(&ws);
        assert_eq!(picture(&ws, cols, 3), expected, "{preset:?}");
        assert_eq!(ws.panes.len(), template.slots.len());
        assert!(ws.active_pane().is_some());

        let back = ws.to_template(&WindowId::main()).unwrap();
        assert_eq!(back.slots, template.slots, "{preset:?}: slots named by kind, in tree order");
        assert_eq!(back.root, template.root, "{preset:?}: the shape round-trips");
        let round = WorkspaceLayout::from_template("again", &back, back.slots.iter().map(|slot| PaneDefinition::new(slot.clone())).collect());
        assert_eq!(picture(&round, cols, 3), expected);
        for definition in round.panes.values() {
            assert!(definition.resource.is_none(), "templates carry no resources");
        }
    }
}

#[test]
fn to_template_names_repeated_kinds_and_keeps_tabs() {
    let (mut ws, [_, p2, _, _]) = three_columns_with_split();
    open_tab(&mut ws, "two", &p2, None);
    let template = ws.to_template(&WindowId::main()).unwrap();
    assert_eq!(template.slots, vec!["one", "two", "two-2", "three", "four"]);
    assert_eq!(template.name, ws.name);
    let text = serde_json::to_string(&template).unwrap();
    assert!(text.contains("\"type\":\"split\""));
    assert!(text.contains("\"slots\":[\"two\",\"two-2\"]"));
}

// ---------------------------------------------------------------------------
// 12. Resolver
// ---------------------------------------------------------------------------

#[test]
fn the_resolver_focuses_exact_matches_and_creates_otherwise() {
    let mut ws = WorkspaceLayout::new("test");
    assert_eq!(
        resolver::resolve(&ws, "today", None, Intent::Open),
        Resolution::Create {
            window: WindowId::main(),
            target: DockTarget::edge(Side::Right)
        }
    );
    assert_eq!(
        resolver::resolve(&ws, "today", None, Intent::OpenRight),
        Resolution::Create {
            window: WindowId::main(),
            target: DockTarget::edge(Side::Right)
        }
    );
    assert_eq!(
        resolver::resolve(&ws, "today", None, Intent::OpenBelow),
        Resolution::Create {
            window: WindowId::main(),
            target: DockTarget::edge(Side::Bottom)
        }
    );

    let today = open(&mut ws, "today", DockTarget::edge(Side::Right));
    let account = ws
        .open_pane(&WindowId::main(), PaneDefinition::new("account").with_resource(j!({ "id": "a1" })), DockTarget::edge(Side::Right))
        .unwrap();
    let active_stack = stack_of(&ws, &account);

    assert_eq!(resolver::resolve(&ws, "today", None, Intent::Open), Resolution::Focus(today.clone()));
    assert_eq!(resolver::resolve(&ws, "account", Some(&j!({ "id": "a1" })), Intent::Open), Resolution::Focus(account.clone()));
    assert_eq!(
        resolver::resolve(&ws, "account", Some(&j!({ "id": "a2" })), Intent::Open),
        Resolution::Create {
            window: WindowId::main(),
            target: DockTarget::tab(active_stack.clone())
        }
    );
    assert_eq!(
        resolver::resolve(&ws, "today", None, Intent::NewInstance),
        Resolution::Create {
            window: WindowId::main(),
            target: DockTarget::tab(active_stack.clone())
        }
    );
    assert_eq!(
        resolver::resolve(&ws, "today", None, Intent::OpenRight),
        Resolution::Create {
            window: WindowId::main(),
            target: DockTarget::beside(active_stack.clone(), Side::Right)
        }
    );
    assert_eq!(
        resolver::resolve(&ws, "today", None, Intent::OpenBelow),
        Resolution::Create {
            window: WindowId::main(),
            target: DockTarget::beside(active_stack, Side::Bottom)
        }
    );
    assert_eq!(resolver::resolve(&ws, "today", None, Intent::OpenNewWindow), Resolution::CreateWindow);

    // Two matches: the one in the active window (then active stack) wins.
    let floating = ws.detach_pane(&today, WindowFrame::new(0.0, 0.0, 500.0, 400.0)).unwrap();
    let today_in_main = open(&mut ws, "today", DockTarget::edge(Side::Left));
    ws.set_active_window(&floating).unwrap();
    assert_eq!(resolver::resolve(&ws, "today", None, Intent::Open), Resolution::Focus(today));
    ws.set_active_pane(&today_in_main).unwrap();
    assert_eq!(resolver::resolve(&ws, "today", None, Intent::Open), Resolution::Focus(today_in_main));
}

// ---------------------------------------------------------------------------
// 13. Focus geometry
// ---------------------------------------------------------------------------

#[test]
fn directional_focus_follows_the_overlap_rule() {
    let (ws, [p1, p2, p3, p4]) = three_columns_with_split();
    let tree = root(&ws);
    assert_eq!(focus::neighbour(tree, &p2, Direction::Right), Some(p3.clone()), "3 shares two thirds of the edge, 4 one third");
    assert_eq!(focus::neighbour(tree, &p4, Direction::Left), Some(p2.clone()));
    assert_eq!(focus::neighbour(tree, &p3, Direction::Down), Some(p4.clone()));
    assert_eq!(focus::neighbour(tree, &p4, Direction::Up), Some(p3.clone()));
    assert_eq!(focus::neighbour(tree, &p1, Direction::Left), None);
    assert_eq!(focus::neighbour(tree, &p1, Direction::Up), None);
    assert_eq!(focus::neighbour(tree, &p4, Direction::Right), None);
    assert_eq!(focus::neighbour(tree, &p2, Direction::Left), Some(p1.clone()));
    assert_eq!(focus::neighbour(tree, &PaneId::new("pane_missing"), Direction::Left), None);

    // Ties on overlap go to the nearest centre: from a full-height 1 with
    // 2 on top and 3 below of equal height, Right picks by centre distance.
    let mut even = WorkspaceLayout::new("even");
    let a = open(&mut even, "a", DockTarget::edge(Side::Right));
    let b = open(&mut even, "b", DockTarget::WindowEdge { side: Side::Right, share: Some(0.5) });
    let c = open_beside(&mut even, "c", &b, Side::Bottom, Some(0.5));
    assert_eq!(picture_by_kind(&even, 2, 2), "ab\nac");
    let first = focus::neighbour(root(&even), &a, Direction::Right).unwrap();
    assert!(first == b || first == c);
    assert_eq!(focus::neighbour(root(&even), &c, Direction::Left), Some(a.clone()));
    assert_eq!(focus::neighbour(root(&even), &b, Direction::Down), Some(c));
}

#[test]
fn keyboard_move_targets_land_beside_the_neighbour_or_at_the_edge() {
    let (mut ws, [p1, p2, p3, p4]) = three_columns_with_split();
    let tree = root(&ws);
    assert_eq!(
        focus::move_direction_target(tree, &p2, Direction::Right),
        Some(DockTarget::beside(stack_of(&ws, &p3), Side::Left)),
        "2 lands left of 3, spanning only 3's height"
    );
    assert_eq!(
        focus::move_direction_target(tree, &p1, Direction::Right),
        Some(DockTarget::beside(stack_of(&ws, &p2), Side::Right)),
        "a lone pane jumps over its adjacent sibling"
    );
    assert_eq!(focus::move_direction_target(tree, &p1, Direction::Left), Some(DockTarget::edge(Side::Left)));
    assert_eq!(focus::move_direction_target(tree, &p4, Direction::Down), Some(DockTarget::edge(Side::Bottom)));
    assert_eq!(focus::move_direction_target(tree, &PaneId::new("pane_missing"), Direction::Down), None);

    let target = focus::move_direction_target(tree, &p1, Direction::Right).unwrap();
    ws.move_pane(&p1, &WindowId::main(), target).unwrap();
    assert_eq!(picture(&ws, 3, 3), "123\n123\n124");
    assert_eq!(ws.find_panes("two", None)[0], root(&ws).panes()[0], "2 is now first");
    assert_valid(&ws);

    // 4 is at the bottom of its column, not of the window: "move down" puts
    // it along the whole bottom edge.
    let target = focus::move_direction_target(root(&ws), &p4, Direction::Down).unwrap();
    assert_eq!(target, DockTarget::edge(Side::Bottom));
    ws.move_pane(&p4, &WindowId::main(), target).unwrap();
    assert_eq!(picture(&ws, 3, 3), "123\n123\n444");
    assert_valid(&ws);
}

// ---------------------------------------------------------------------------
// 15. Floating windows
// ---------------------------------------------------------------------------

#[test]
fn detaching_makes_a_floating_window_that_closes_with_its_last_pane() {
    let (mut ws, [p1, p2, p3, _]) = three_columns_with_split();
    let frame = WindowFrame::new(100.0, 50.0, 900.0, 700.0);
    let floating = ws.detach_pane(&p3, frame).unwrap();
    assert_valid(&ws);
    assert_eq!(ws.windows.len(), 2);
    let window = ws.window(&floating).unwrap();
    assert_eq!(window.role, WindowRole::Floating);
    assert_eq!(window.frame, Some(frame));
    assert_eq!(window.panes(), vec![p3.clone()]);
    assert_eq!(window.active_pane, Some(p3.clone()));
    assert_eq!(ws.active_window, Some(floating.clone()));
    assert_eq!(ws.window_id_of(&p3), Some(floating.clone()));
    assert_eq!(picture(&ws, 3, 3), "123\n123\n123", "the main window closed the gap");
    assert_eq!(ws.detach_pane(&p3, frame), Err(OpError::AlreadyDetached(p3.clone())));

    // Move another pane into the floating window as a tab, then move it back.
    ws.move_pane(&p2, &floating, DockTarget::tab(stack_of(&ws, &p3))).unwrap();
    assert_valid(&ws);
    assert_eq!(ws.panes_in(&floating), vec![p3.clone(), p2.clone()]);
    assert_eq!(ws.panes_in(&WindowId::main()).len(), 2);
    ws.move_pane(&p2, &WindowId::main(), DockTarget::edge(Side::Bottom)).unwrap();
    assert_valid(&ws);
    assert_eq!(ws.panes_in(&floating), vec![p3.clone()]);

    // Closing the floating window's last pane removes the window.
    ws.set_active_window(&floating).unwrap();
    ws.close_pane(&p3).unwrap();
    assert!(ws.window(&floating).is_none());
    assert_eq!(ws.windows.len(), 1);
    assert_eq!(ws.active_window, Some(WindowId::main()));
    assert_valid(&ws);

    // Moving the last pane out of a floating window removes it too.
    let floating2 = ws.detach_pane(&p1, frame).unwrap();
    ws.move_pane(&p1, &WindowId::main(), DockTarget::edge(Side::Left)).unwrap();
    assert!(ws.window(&floating2).is_none());
    assert_valid(&ws);

    // The main window's last pane leaves it empty, not gone.
    for pane in ws.panes_in(&WindowId::main()) {
        ws.close_pane(&pane).unwrap();
    }
    assert_eq!(ws.windows.len(), 1);
    assert!(ws.main_window().unwrap().root.is_none());
    assert_valid(&ws);
}

#[test]
fn detaching_the_only_pane_of_the_main_window_is_allowed() {
    let mut ws = WorkspaceLayout::new("test");
    let p1 = open(&mut ws, "one", DockTarget::edge(Side::Right));
    let floating = ws.detach_pane(&p1, WindowFrame::new(0.0, 0.0, 400.0, 300.0)).unwrap();
    assert!(ws.main_window().unwrap().is_empty());
    assert_eq!(ws.main_window().unwrap().active_pane, None);
    assert_eq!(ws.window_id_of(&p1), Some(floating));
    assert_valid(&ws);
    let p2 = ws.open_pane_default(PaneDefinition::new("two")).unwrap();
    assert_eq!(ws.window_id_of(&p2), ws.window_id_of(&p1), "the default target is the active window's active stack");
    assert_eq!(stack_of(&ws, &p2), stack_of(&ws, &p1));
    assert_valid(&ws);
}

#[test]
fn duplicate_opens_a_copy_beside_the_original() {
    let (mut ws, [p1, _, _, _]) = three_columns_with_split();
    ws.set_view_state(&p1, j!({ "scroll": 7 })).unwrap();
    let copy = ws
        .duplicate_pane(
            &p1,
            DockTarget::Beside {
                node: stack_of(&ws, &p1),
                side: Side::Bottom,
                share: None,
            },
        )
        .unwrap();
    assert_eq!(ws.pane(&copy), ws.pane(&p1));
    assert_eq!(focus::neighbour(root(&ws), &p1, Direction::Down), Some(copy));
    assert_valid(&ws);
}
