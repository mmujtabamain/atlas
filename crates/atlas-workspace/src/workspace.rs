//! The whole workspace: windows, their layout trees, and the panes they show.
//!
//! [`WorkspaceLayout`] is the document the UI edits and the file that gets
//! saved. It owns one **main** window (always present, possibly empty) and
//! any number of **floating** windows (removed as soon as their last pane
//! leaves), the [`PaneDefinition`] of every open pane, the id counters and the
//! active window. Every pane lives in exactly one stack of exactly one window
//! and every window's tree is normalized — [`crate::validate`] checks all of
//! it, and every mutating method here asserts (in debug builds) that it left
//! the invariants intact.
//!
//! A pane is defined by what it shows (`kind`, e.g. `"account"`) and which
//! resource (`resource`, e.g. an account id) plus its own `viewState` (scroll
//! position, filters, chosen tab), stored as JSON so the model does not have
//! to know every screen. The tree is portable between machines; window
//! frames (positions and sizes in logical pixels) are kept separately on the
//! window and may simply be dropped on a machine with a different display.
//!
//! Serialized form (camelCase throughout):
//!
//! ```json
//! { "schemaVersion": 1, "workspaceId": "default", "name": "Default",
//!   "scope": { "householdId": null },
//!   "windows": [ { "id": "window_main", "role": "main", "root": { "type": "stack", … }, "activePane": "pane_1", "frame": null } ],
//!   "panes": { "pane_1": { "kind": "today", "resource": null, "viewState": {} } },
//!   "activeWindow": "window_main",
//!   "ids": { "nextPane": 2, "nextNode": 2, "nextWindow": 1 } }
//! ```

use crate::ids::{IdSource, NodeId, PaneId, WindowId};
use crate::layout::{LayoutNode, Side};
use crate::ops::{self, DockTarget, OpError, SplitLimits};
use crate::persist::SCHEMA_VERSION;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

/// Which household a workspace belongs to. A global scope (`household_id:
/// None`) fits any household; a household scope is only meaningful for
/// panes whose resources exist in that household.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Scope {
    pub household_id: Option<String>,
}

impl Scope {
    /// A scope that fits every household.
    pub fn global() -> Self {
        Scope { household_id: None }
    }

    /// A scope bound to one household.
    pub fn household(id: impl Into<String>) -> Self {
        Scope { household_id: Some(id.into()) }
    }

    /// True when the scope is not bound to a household.
    pub fn is_global(&self) -> bool {
        self.household_id.is_none()
    }

    /// True when something saved under `self` may be opened under `other`:
    /// global content fits everywhere, household content only its household.
    pub fn accepts(&self, other: &Scope) -> bool {
        match &self.household_id {
            None => true,
            Some(mine) => other.household_id.as_deref() == Some(mine.as_str()),
        }
    }
}

/// The two kinds of window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowRole {
    /// The one window that always exists, even when it shows nothing.
    #[default]
    Main,
    /// A window made by detaching a pane; it disappears with its last pane.
    Floating,
}

/// A window's position and size in logical units. Kept apart from the tree
/// because it is the one part of a layout that is not portable.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowFrame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl WindowFrame {
    /// A frame; sizes are clamped to be non-negative.
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        WindowFrame {
            x,
            y,
            width: width.max(0.0),
            height: height.max(0.0),
        }
    }

    /// True when every coordinate is a finite number and the size is positive.
    pub fn is_sane(&self) -> bool {
        [self.x, self.y, self.width, self.height].iter().all(|v| v.is_finite()) && self.width > 0.0 && self.height > 0.0
    }
}

/// What one pane shows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneDefinition {
    /// The screen type, e.g. `"today"`, `"account"`, `"scenario"`.
    pub kind: String,
    /// What the screen is about (an account id, a scenario id, …), as JSON,
    /// so two panes of the same kind can be told apart and found again.
    #[serde(default)]
    pub resource: Option<Value>,
    /// The screen's own state — scroll position, filters, sub-tab — as JSON.
    /// Always an object; see [`crate::persist::scrub_view_state`] for what
    /// must never be in it.
    #[serde(default = "empty_object")]
    pub view_state: Value,
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

impl PaneDefinition {
    /// The kind a pane takes when what it showed no longer exists (a deleted
    /// account, a removed screen type). The original definition is kept under
    /// `viewState.original` so the pane can explain itself and be restored.
    pub const UNAVAILABLE_KIND: &'static str = "unavailable";

    /// A pane of `kind` with no resource and empty view state.
    pub fn new(kind: impl Into<String>) -> Self {
        PaneDefinition {
            kind: kind.into(),
            resource: None,
            view_state: empty_object(),
        }
    }

    /// Sets the resource.
    pub fn with_resource(mut self, resource: Value) -> Self {
        self.resource = Some(resource);
        self
    }

    /// Sets the view state (anything that is not an object becomes `{}`).
    pub fn with_view_state(mut self, view_state: Value) -> Self {
        self.view_state = if view_state.is_object() { view_state } else { empty_object() };
        self
    }

    /// The stand-in for a pane whose resource or kind is gone.
    pub fn unavailable(original: &PaneDefinition, reason: impl Into<String>) -> Self {
        let mut view_state = serde_json::Map::new();
        view_state.insert("original".to_owned(), serde_json::to_value(original).unwrap_or(Value::Null));
        view_state.insert("reason".to_owned(), Value::String(reason.into()));
        PaneDefinition {
            kind: Self::UNAVAILABLE_KIND.to_owned(),
            resource: None,
            view_state: Value::Object(view_state),
        }
    }

    /// True for the stand-in made by [`PaneDefinition::unavailable`].
    pub fn is_unavailable(&self) -> bool {
        self.kind == Self::UNAVAILABLE_KIND
    }

    /// The definition an unavailable pane stands in for, if it is one.
    pub fn original(&self) -> Option<PaneDefinition> {
        if !self.is_unavailable() {
            return None;
        }
        serde_json::from_value(self.view_state.get("original")?.clone()).ok()
    }

    /// True when `kind` and `resource` both match.
    pub fn matches(&self, kind: &str, resource: Option<&Value>) -> bool {
        self.kind == kind && self.resource.as_ref() == resource
    }
}

/// One window and its layout tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowLayout {
    pub id: WindowId,
    #[serde(default)]
    pub role: WindowRole,
    /// `None` is an empty window (only the main window may stay empty).
    #[serde(default)]
    pub root: Option<LayoutNode>,
    /// The pane that has focus in this window.
    #[serde(default)]
    pub active_pane: Option<PaneId>,
    /// Position and size; not portable, so optional.
    #[serde(default)]
    pub frame: Option<WindowFrame>,
}

impl WindowLayout {
    /// An empty window.
    pub fn new(id: WindowId, role: WindowRole) -> Self {
        WindowLayout {
            id,
            role,
            root: None,
            active_pane: None,
            frame: None,
        }
    }

    /// The empty main window.
    pub fn main() -> Self {
        WindowLayout::new(WindowId::main(), WindowRole::Main)
    }

    /// True when the window shows no pane.
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    /// Every pane in the window, pre-order.
    pub fn panes(&self) -> Vec<PaneId> {
        self.root.as_ref().map(LayoutNode::panes).unwrap_or_default()
    }

    /// True when `pane` is in this window.
    pub fn contains_pane(&self, pane: &PaneId) -> bool {
        self.root.as_ref().is_some_and(|tree| tree.contains_pane(pane))
    }

    /// The stack holding `pane`.
    pub fn stack_of(&self, pane: &PaneId) -> Option<NodeId> {
        self.root.as_ref().and_then(|tree| tree.find_stack_of(pane))
    }

    /// True when the tree has a node with this id.
    pub fn has_node(&self, node: &NodeId) -> bool {
        self.root.as_ref().is_some_and(|tree| tree.find(node).is_some())
    }

    /// The active pane's stack, or the first stack when there is no active pane.
    pub fn active_stack(&self) -> Option<NodeId> {
        let tree = self.root.as_ref()?;
        match &self.active_pane {
            Some(pane) => tree.find_stack_of(pane).or_else(|| tree.stack_ids().into_iter().next()),
            None => tree.stack_ids().into_iter().next(),
        }
    }

    /// Where a new pane goes when the caller has no better idea: as a tab of
    /// the active stack, or at the right edge of an empty window.
    pub fn default_target(&self) -> DockTarget {
        match self.active_stack() {
            Some(stack) => DockTarget::tab(stack),
            None => DockTarget::edge(Side::Right),
        }
    }

    /// Replaces the whole tree with one projected back from a live view of
    /// this window (the UI mirrors an edit the person made on screen — a
    /// divider dragged, a tab chosen, a tab closed — into the model this way).
    /// The tree is normalized and `active_pane` is repaired so it names a
    /// pane that is still there; the report says what normalization changed.
    ///
    /// Only the window is touched: the caller is responsible for keeping the
    /// workspace's pane definitions in step (a pane that left the tree must
    /// lose its definition) and for validating the whole workspace afterwards.
    pub fn replace_root(&mut self, root: Option<LayoutNode>) -> ops::NormalizeReport {
        self.root = root;
        let report = ops::normalize(&mut self.root);
        self.repair_active_pane();
        report
    }

    /// Repairs `active_pane` so it names a pane of this window (the tree's
    /// first stack's active pane by default). Returns true when it changed.
    fn repair_active_pane(&mut self) -> bool {
        let wanted = match (&self.root, &self.active_pane) {
            (Some(tree), Some(pane)) if tree.contains_pane(pane) => Some(pane.clone()),
            (Some(tree), _) => tree.stack_ids().first().and_then(|stack| tree.find(stack)).and_then(|stack| stack.active_pane().cloned()),
            (None, _) => None,
        };
        let changed = wanted != self.active_pane;
        self.active_pane = wanted;
        changed
    }
}

/// The whole workspace. See the module docs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkspaceLayout {
    /// The file format version; [`SCHEMA_VERSION`] when created here.
    pub schema_version: u32,
    /// A stable id for the workspace (a slug of its first name).
    pub workspace_id: String,
    /// The name shown to the user.
    pub name: String,
    /// Which household the workspace belongs to, if any.
    pub scope: Scope,
    /// The main window first, then floating windows in creation order.
    pub windows: Vec<WindowLayout>,
    /// The definition of every open pane.
    pub panes: BTreeMap<PaneId, PaneDefinition>,
    /// The window that has focus.
    pub active_window: Option<WindowId>,
    /// The id counters.
    pub ids: IdSource,
    /// The minimum-size rule applied to splits. Policy, not layout: not saved.
    #[serde(skip)]
    pub limits: SplitLimits,
}

impl Default for WorkspaceLayout {
    fn default() -> Self {
        WorkspaceLayout::new("Default")
    }
}

impl WorkspaceLayout {
    /// A workspace with one empty main window.
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        WorkspaceLayout {
            schema_version: SCHEMA_VERSION,
            workspace_id: slug(&name),
            name,
            scope: Scope::global(),
            windows: vec![WindowLayout::main()],
            panes: BTreeMap::new(),
            active_window: Some(WindowId::main()),
            ids: IdSource::new(),
            limits: SplitLimits::default(),
        }
    }

    /// Sets the scope.
    pub fn with_scope(mut self, scope: Scope) -> Self {
        self.scope = scope;
        self
    }

    /// Sets the minimum-size rule.
    pub fn with_limits(mut self, limits: SplitLimits) -> Self {
        self.limits = limits;
        self
    }

    // -- lookups ------------------------------------------------------------

    /// True when no pane is open in any window.
    pub fn is_empty(&self) -> bool {
        self.panes.is_empty()
    }

    /// The main window (a valid workspace always has one).
    pub fn main_window(&self) -> Option<&WindowLayout> {
        self.windows.iter().find(|window| window.role == WindowRole::Main)
    }

    /// A window by id.
    pub fn window(&self, id: &WindowId) -> Option<&WindowLayout> {
        self.windows.iter().find(|window| window.id == *id)
    }

    /// A window by id, mutable. Callers must keep the invariants; prefer the operations.
    pub fn window_mut(&mut self, id: &WindowId) -> Option<&mut WindowLayout> {
        self.windows.iter_mut().find(|window| window.id == *id)
    }

    /// The window that shows `pane`.
    pub fn window_of(&self, pane: &PaneId) -> Option<&WindowLayout> {
        self.windows.iter().find(|window| window.contains_pane(pane))
    }

    /// The id of the window that shows `pane`.
    pub fn window_id_of(&self, pane: &PaneId) -> Option<WindowId> {
        self.window_of(pane).map(|window| window.id.clone())
    }

    /// The window whose tree contains `node`.
    pub fn window_of_node(&self, node: &NodeId) -> Option<&WindowLayout> {
        self.windows.iter().find(|window| window.has_node(node))
    }

    /// The stack holding `pane`, in whatever window.
    pub fn stack_of(&self, pane: &PaneId) -> Option<NodeId> {
        self.windows.iter().find_map(|window| window.stack_of(pane))
    }

    /// The pane with focus: the active window's active pane.
    pub fn active_pane(&self) -> Option<PaneId> {
        let window = self.active_window.as_ref().and_then(|id| self.window(id)).or_else(|| self.main_window())?;
        window.active_pane.clone()
    }

    /// The active window (the main window when nothing is recorded).
    pub fn active_window(&self) -> Option<&WindowLayout> {
        self.active_window.as_ref().and_then(|id| self.window(id)).or_else(|| self.main_window())
    }

    /// The definition of a pane.
    pub fn pane(&self, pane: &PaneId) -> Option<&PaneDefinition> {
        self.panes.get(pane)
    }

    /// Every pane of a window, pre-order; empty for an unknown window.
    pub fn panes_in(&self, window: &WindowId) -> Vec<PaneId> {
        self.window(window).map(WindowLayout::panes).unwrap_or_default()
    }

    /// Every pane showing `kind` about `resource`, in id order.
    pub fn find_panes(&self, kind: &str, resource: Option<&Value>) -> Vec<PaneId> {
        self.panes.iter().filter(|(_, definition)| definition.matches(kind, resource)).map(|(id, _)| id.clone()).collect()
    }

    /// Every pane of `kind`, whatever its resource.
    pub fn find_panes_of_kind(&self, kind: &str) -> Vec<PaneId> {
        self.panes.iter().filter(|(_, definition)| definition.kind == kind).map(|(id, _)| id.clone()).collect()
    }

    /// The minimum-size rule in force.
    pub fn limits(&self) -> &SplitLimits {
        &self.limits
    }

    /// Replaces the minimum-size rule.
    pub fn set_limits(&mut self, limits: SplitLimits) {
        self.limits = limits;
    }

    // -- operations ---------------------------------------------------------

    /// Opens a new pane in `window` at `target`, returning its id. The new
    /// pane becomes the active pane of its window and the window becomes the
    /// active window.
    pub fn open_pane(&mut self, window: &WindowId, definition: PaneDefinition, target: DockTarget) -> Result<PaneId, OpError> {
        let index = self.window_index(window).ok_or_else(|| OpError::UnknownWindow(window.clone()))?;
        let mut ids = self.ids.clone();
        let pane = ids.mint_pane();
        ops::insert(&mut self.windows[index].root, &mut ids, &pane, &target, &self.limits)?;
        self.ids = ids;
        log::info!("workspace: opened pane {pane} ({}) in {window} at {target:?}", definition.kind);
        self.panes.insert(pane.clone(), definition);
        self.windows[index].active_pane = Some(pane.clone());
        self.active_window = Some(window.clone());
        self.assert_valid();
        Ok(pane)
    }

    /// Opens a pane where the resolver's default would put it: as a tab of
    /// the active window's active stack, or at the right edge of an empty window.
    pub fn open_pane_default(&mut self, definition: PaneDefinition) -> Result<PaneId, OpError> {
        let window = self.active_window().map(|window| (window.id.clone(), window.default_target()));
        let (window_id, target) = window.ok_or_else(|| OpError::UnknownWindow(WindowId::main()))?;
        self.open_pane(&window_id, definition, target)
    }

    /// Closes a pane and returns its definition. A floating window whose last
    /// pane closes is removed; the main window stays, empty.
    pub fn close_pane(&mut self, pane: &PaneId) -> Result<PaneDefinition, OpError> {
        self.close_pane_detailed(pane).map(|closed| closed.definition)
    }

    /// Closes a pane and returns everything needed to reopen it in place.
    pub fn close_pane_detailed(&mut self, pane: &PaneId) -> Result<crate::closed::ClosedPane, OpError> {
        let index = self.window_index_of_pane(pane).ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
        let window_id = self.windows[index].id.clone();
        let removed = ops::remove(&mut self.windows[index].root, pane)?;
        let definition = match self.panes.remove(pane) {
            Some(definition) => definition,
            None => {
                log::warn!("workspace: pane {pane} was in the tree of {window_id} without a definition");
                PaneDefinition::new("unknown")
            }
        };
        log::info!("workspace: closed pane {pane} ({}) from {window_id}", definition.kind);
        self.settle_after_removal(index, pane, removed.neighbour.as_ref());
        let closed = crate::closed::ClosedPane {
            definition,
            window: window_id,
            stack: Some(removed.stack),
            index: removed.index,
            neighbour: removed.neighbour,
            neighbour_side: removed.neighbour_side,
            closed_at: chrono::Utc::now(),
        };
        self.drop_window_if_empty_floating(index);
        self.assert_valid();
        Ok(closed)
    }

    /// Moves a pane to `target` in `window` — the same window or another one.
    /// Within one window this is [`ops::move_pane`] (with its [`OpError::NoOp`]
    /// rule); across windows the pane leaves the source tree and lands in the
    /// target tree, and a floating source left empty is removed.
    pub fn move_pane(&mut self, pane: &PaneId, window: &WindowId, target: DockTarget) -> Result<(), OpError> {
        let source = self.window_index_of_pane(pane).ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
        let destination = self.window_index(window).ok_or_else(|| OpError::UnknownWindow(window.clone()))?;
        if source == destination {
            ops::move_pane(&mut self.windows[source].root, &mut self.ids, pane, &target, &self.limits)?;
            self.windows[source].active_pane = Some(pane.clone());
            self.active_window = Some(window.clone());
            self.assert_valid();
            return Ok(());
        }
        let mut source_root = self.windows[source].root.clone();
        let mut destination_root = self.windows[destination].root.clone();
        let mut ids = self.ids.clone();
        let removed = ops::remove(&mut source_root, pane)?;
        ops::insert(&mut destination_root, &mut ids, pane, &target, &self.limits)?;
        self.windows[source].root = source_root;
        self.windows[destination].root = destination_root;
        self.ids = ids;
        log::info!("workspace: moved pane {pane} from {} to {window} at {target:?}", self.windows[source].id);
        self.settle_after_removal(source, pane, removed.neighbour.as_ref());
        self.windows[destination].active_pane = Some(pane.clone());
        self.active_window = Some(window.clone());
        self.drop_window_if_empty_floating(source);
        self.assert_valid();
        Ok(())
    }

    /// Moves a pane into its own new floating window with the given frame.
    pub fn detach_pane(&mut self, pane: &PaneId, frame: WindowFrame) -> Result<WindowId, OpError> {
        let source = self.window_index_of_pane(pane).ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
        if self.windows[source].role == WindowRole::Floating && self.windows[source].panes().len() == 1 {
            return Err(OpError::AlreadyDetached(pane.clone()));
        }
        let mut ids = self.ids.clone();
        let window_id = ids.mint_window();
        let mut source_root = self.windows[source].root.clone();
        let removed = ops::remove(&mut source_root, pane)?;
        let mut new_root = None;
        ops::insert(&mut new_root, &mut ids, pane, &DockTarget::edge(Side::Right), &self.limits)?;
        self.windows[source].root = source_root;
        self.ids = ids;
        self.windows.push(WindowLayout {
            id: window_id.clone(),
            role: WindowRole::Floating,
            root: new_root,
            active_pane: Some(pane.clone()),
            frame: Some(frame),
        });
        log::info!("workspace: detached pane {pane} into floating window {window_id}");
        self.settle_after_removal(source, pane, removed.neighbour.as_ref());
        self.active_window = Some(window_id.clone());
        self.drop_window_if_empty_floating(source);
        self.assert_valid();
        Ok(window_id)
    }

    /// Merges `pane` into the stack that shows `onto` (same window only).
    pub fn stack_panes(&mut self, pane: &PaneId, onto: &PaneId) -> Result<(), OpError> {
        let window = self.window_id_of(onto).ok_or_else(|| OpError::UnknownPane(onto.clone()))?;
        let stack = self.stack_of(onto).ok_or_else(|| OpError::UnknownPane(onto.clone()))?;
        self.move_pane(pane, &window, DockTarget::tab(stack))
    }

    /// Pulls `pane` out of its multi-pane stack to `side` of that stack.
    pub fn unstack_pane(&mut self, pane: &PaneId, side: Side) -> Result<(), OpError> {
        let index = self.window_index_of_pane(pane).ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
        ops::unstack(&mut self.windows[index].root, &mut self.ids, pane, side, &self.limits)?;
        self.windows[index].active_pane = Some(pane.clone());
        self.active_window = Some(self.windows[index].id.clone());
        self.assert_valid();
        Ok(())
    }

    /// Opens a second pane with the same definition at `target` in the same window.
    pub fn duplicate_pane(&mut self, pane: &PaneId, target: DockTarget) -> Result<PaneId, OpError> {
        let window = self.window_id_of(pane).ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
        let definition = self.panes.get(pane).cloned().ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
        self.open_pane(&window, definition, target)
    }

    /// Replaces a split's weights, in whichever window it is.
    pub fn resize(&mut self, split: &NodeId, weights: &[f64]) -> Result<(), OpError> {
        let index = self.windows.iter().position(|window| window.has_node(split)).ok_or_else(|| OpError::UnknownNode(split.clone()))?;
        ops::resize(&mut self.windows[index].root, split, weights)?;
        self.assert_valid();
        Ok(())
    }

    /// Gives `pane` the focus: active in its stack, its window's active pane,
    /// and its window the active window.
    pub fn set_active_pane(&mut self, pane: &PaneId) -> Result<(), OpError> {
        let index = self.window_index_of_pane(pane).ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
        ops::set_active(&mut self.windows[index].root, pane)?;
        self.windows[index].active_pane = Some(pane.clone());
        self.active_window = Some(self.windows[index].id.clone());
        self.assert_valid();
        Ok(())
    }

    /// Makes `window` the active window.
    pub fn set_active_window(&mut self, window: &WindowId) -> Result<(), OpError> {
        if self.window(window).is_none() {
            return Err(OpError::UnknownWindow(window.clone()));
        }
        self.active_window = Some(window.clone());
        Ok(())
    }

    /// Replaces a pane's definition (kind, resource and view state) in place.
    pub fn replace_pane(&mut self, pane: &PaneId, definition: PaneDefinition) -> Result<PaneDefinition, OpError> {
        let slot = self.panes.get_mut(pane).ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
        Ok(std::mem::replace(slot, definition))
    }

    /// Replaces a pane's view state.
    pub fn set_view_state(&mut self, pane: &PaneId, view_state: Value) -> Result<(), OpError> {
        let definition = self.panes.get_mut(pane).ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
        definition.view_state = if view_state.is_object() { view_state } else { empty_object() };
        Ok(())
    }

    /// Moves a window's frame.
    pub fn set_frame(&mut self, window: &WindowId, frame: Option<WindowFrame>) -> Result<(), OpError> {
        let target = self.window_mut(window).ok_or_else(|| OpError::UnknownWindow(window.clone()))?;
        target.frame = frame;
        Ok(())
    }

    // -- repair -------------------------------------------------------------

    /// Restores every workspace-level invariant that can be restored without
    /// guessing: normalizes each window's tree, replaces blank or duplicate
    /// node ids, drops floating windows left empty, adds a main window if none
    /// exists, repairs each window's active pane and the active window, and
    /// moves the id counters past every id in use. Returns true when
    /// anything changed. Used when a layout arrives from a file; the
    /// operations keep a live workspace normalized on their own.
    pub fn normalize(&mut self) -> bool {
        let before = self.clone();
        for window in &mut self.windows {
            ops::normalize(&mut window.root);
        }
        self.repair_node_ids();
        self.windows.retain(|window| window.role == WindowRole::Main || window.root.is_some());
        if self.main_window().is_none() {
            self.windows.insert(0, WindowLayout::main());
        }
        for window in &mut self.windows {
            window.repair_active_pane();
        }
        let active_exists = self.active_window.as_ref().is_some_and(|id| self.window(id).is_some());
        if !active_exists {
            self.active_window = self.main_window().map(|window| window.id.clone());
        }
        for pane in self.panes.keys() {
            self.ids.observe_pane(pane);
        }
        for window in &self.windows {
            self.ids.observe_window(&window.id);
            if let Some(tree) = &window.root {
                for node in tree.node_ids() {
                    self.ids.observe_node(&node);
                }
            }
        }
        *self != before
    }

    /// Gives every blank or duplicate node id a fresh minted id.
    fn repair_node_ids(&mut self) {
        let mut seen: HashSet<NodeId> = HashSet::new();
        for window in &self.windows {
            if let Some(tree) = &window.root {
                for node in tree.node_ids() {
                    self.ids.observe_node(&node);
                }
            }
        }
        for window in &mut self.windows {
            if let Some(tree) = &mut window.root {
                relabel_nodes(tree, &mut seen, &mut self.ids);
            }
        }
    }

    // -- internals ----------------------------------------------------------

    fn window_index(&self, window: &WindowId) -> Option<usize> {
        self.windows.iter().position(|candidate| candidate.id == *window)
    }

    fn window_index_of_pane(&self, pane: &PaneId) -> Option<usize> {
        self.windows.iter().position(|window| window.contains_pane(pane))
    }

    /// After `pane` left window `index`: point the window's active pane at
    /// the neighbour when it is still there, else at the tree's first pane.
    fn settle_after_removal(&mut self, index: usize, pane: &PaneId, neighbour: Option<&PaneId>) {
        let window = &mut self.windows[index];
        if window.active_pane.as_ref() != Some(pane) && window.active_pane.as_ref().is_some_and(|active| window.contains_pane(active)) {
            return;
        }
        window.active_pane = match neighbour {
            Some(candidate) if window.contains_pane(candidate) => Some(candidate.clone()),
            _ => window.panes().into_iter().next(),
        };
    }

    /// Removes the window at `index` when it is a floating window with no panes.
    fn drop_window_if_empty_floating(&mut self, index: usize) {
        let Some(window) = self.windows.get(index) else {
            return;
        };
        if window.role != WindowRole::Floating || window.root.is_some() {
            return;
        }
        let removed = self.windows.remove(index);
        log::info!("workspace: removed empty floating window {}", removed.id);
        if self.active_window.as_ref() == Some(&removed.id) {
            self.active_window = self.main_window().map(|window| window.id.clone());
        }
    }

    /// Debug-build check that every mutation left the invariants intact.
    fn assert_valid(&self) {
        if cfg!(debug_assertions) {
            let violations = self.validate();
            debug_assert!(violations.is_empty(), "workspace invariants broken: {violations:#?}");
        }
    }
}

/// Renames blank or already-seen node ids to fresh ones, pre-order.
fn relabel_nodes(node: &mut LayoutNode, seen: &mut HashSet<NodeId>, ids: &mut IdSource) {
    if node.id().is_blank() || !seen.insert(node.id().clone()) {
        let fresh = ids.mint_node();
        seen.insert(fresh.clone());
        node.set_id(fresh);
    }
    if let LayoutNode::Split { children, .. } = node {
        for child in children {
            relabel_nodes(child, seen, ids);
        }
    }
}

/// A file-system-friendly id from a name: lowercase ASCII letters and digits, dashes between words.
pub fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut pending_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(ch.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    if out.is_empty() { "workspace".to_owned() } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_workspace_has_one_empty_main_window() {
        let layout = WorkspaceLayout::new("My Desk!");
        assert_eq!(layout.workspace_id, "my-desk");
        assert_eq!(layout.windows.len(), 1);
        assert!(layout.main_window().unwrap().is_empty());
        assert_eq!(layout.active_window, Some(WindowId::main()));
        assert!(layout.validate().is_empty());
        assert_eq!(slug("   "), "workspace");
    }

    #[test]
    fn unavailable_keeps_the_original() {
        let original = PaneDefinition::new("account").with_resource(serde_json::json!({ "accountId": "acc-1" }));
        let stand_in = PaneDefinition::unavailable(&original, "account acc-1 was deleted");
        assert!(stand_in.is_unavailable());
        assert_eq!(stand_in.original(), Some(original));
        assert_eq!(stand_in.view_state["reason"], "account acc-1 was deleted");
    }

    #[test]
    fn replace_root_normalizes_and_repairs_the_active_pane() {
        let mut window = WindowLayout::main();
        // A one-child split around a stack whose active pane is not a member:
        // what a projection from a live view may hand back.
        let stack = LayoutNode::Stack {
            id: NodeId::new("s"),
            panes: vec![PaneId::new("pane_1"), PaneId::new("pane_2")],
            active_pane_id: Some(PaneId::new("pane_9")),
        };
        let report = window.replace_root(Some(LayoutNode::split(NodeId::new("root"), crate::layout::Axis::Horizontal, vec![stack], vec![1.0])));
        assert!(report.changed);
        assert_eq!(window.root.as_ref().map(LayoutNode::id), Some(&NodeId::new("s")), "the one-child split collapsed to its stack");
        assert_eq!(window.active_pane, Some(PaneId::new("pane_1")));
        window.active_pane = Some(PaneId::new("pane_2"));
        window.replace_root(None);
        assert!(window.is_empty());
        assert_eq!(window.active_pane, None);
    }

    #[test]
    fn scope_rules() {
        assert!(Scope::global().accepts(&Scope::household("h1")));
        assert!(Scope::household("h1").accepts(&Scope::household("h1")));
        assert!(!Scope::household("h1").accepts(&Scope::household("h2")));
        assert!(!Scope::household("h1").accepts(&Scope::global()));
    }
}
