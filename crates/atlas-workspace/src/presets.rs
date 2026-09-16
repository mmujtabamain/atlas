//! Layout templates and the built-in presets.
//!
//! A [`LayoutTemplate`] is a tree shape with named **slots** instead of
//! panes; instantiating it with a list of pane definitions fills the slots
//! in order (slots left unfilled are dropped and the tree normalized), and
//! any window can be turned back into a template with its resources
//! stripped, which is how a user saves "my analysis arrangement" without
//! tying it to particular accounts.
//!
//! The five presets, as 3×3 (or 4×3) pictures where each digit is a slot:
//!
//! | preset | picture | shape |
//! |---|---|---|
//! | Focus | `111 / 111 / 111` | one slot |
//! | Compare | `112 / 112 / 112` | `[1 \| 2]` 0.5 / 0.5 |
//! | Main + Inspector | `1112 / 1112 / 1112` | `[1 \| 2]` 0.75 / 0.25 |
//! | Analysis | `112 / 113 / 113` | `[1 \| [2 / 3]]` 0.66 / 0.34, `2 / 3` 0.34 / 0.66 |
//! | Review | `122 / 133 / 133` | `[1 \| [2 / 3]]` 0.34 / 0.66, `2 / 3` 0.34 / 0.66 |

use crate::ids::{IdSource, NodeId, PaneId, WindowId};
use crate::layout::{Axis, LayoutNode};
use crate::ops::{self, OpError};
use crate::workspace::{PaneDefinition, WindowFrame, WindowLayout, WindowRole, WorkspaceLayout};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The built-in arrangements.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Preset {
    Focus,
    Compare,
    MainInspector,
    Analysis,
    Review,
}

impl Preset {
    /// Every preset, in menu order.
    pub fn all() -> [Preset; 5] {
        [Preset::Focus, Preset::Compare, Preset::MainInspector, Preset::Analysis, Preset::Review]
    }

    /// The menu label.
    pub fn label(self) -> &'static str {
        match self {
            Preset::Focus => "Focus",
            Preset::Compare => "Compare",
            Preset::MainInspector => "Main + Inspector",
            Preset::Analysis => "Analysis",
            Preset::Review => "Review",
        }
    }

    /// One sentence on what the arrangement is for.
    pub fn description(self) -> &'static str {
        match self {
            Preset::Focus => "One pane filling the window.",
            Preset::Compare => "Two panes side by side, equal width.",
            Preset::MainInspector => "A wide main pane with a narrow inspector on the right.",
            Preset::Analysis => "A wide main pane; on the right a small pane over a tall one.",
            Preset::Review => "A narrow pane on the left; on the right a small pane over a tall one.",
        }
    }

    /// How many slots the arrangement has.
    pub fn slot_count(self) -> usize {
        self.template().slots.len()
    }

    /// The arrangement as a template.
    pub fn template(self) -> LayoutTemplate {
        let slot = |name: &str| TemplateNode::slot(name);
        let (slots, root) = match self {
            Preset::Focus => (vec!["main"], slot("main")),
            Preset::Compare => (vec!["left", "right"], TemplateNode::split(Axis::Horizontal, vec![slot("left"), slot("right")], vec![0.5, 0.5])),
            Preset::MainInspector => (
                vec!["main", "inspector"],
                TemplateNode::split(Axis::Horizontal, vec![slot("main"), slot("inspector")], vec![0.75, 0.25]),
            ),
            Preset::Analysis => (
                vec!["main", "supporting", "detail"],
                TemplateNode::split(
                    Axis::Horizontal,
                    vec![slot("main"), TemplateNode::split(Axis::Vertical, vec![slot("supporting"), slot("detail")], vec![0.34, 0.66])],
                    vec![0.66, 0.34],
                ),
            ),
            Preset::Review => (
                vec!["list", "summary", "detail"],
                TemplateNode::split(
                    Axis::Horizontal,
                    vec![slot("list"), TemplateNode::split(Axis::Vertical, vec![slot("summary"), slot("detail")], vec![0.34, 0.66])],
                    vec![0.34, 0.66],
                ),
            ),
        };
        LayoutTemplate {
            name: self.label().to_owned(),
            slots: slots.into_iter().map(str::to_owned).collect(),
            root,
        }
    }
}

/// A tree shape with slots where panes go.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum TemplateNode {
    /// Children along an axis with weights, like [`LayoutNode::Split`].
    Split { axis: Axis, children: Vec<TemplateNode>, weights: Vec<f64> },
    /// A stack of one or more slots (tabs); `active` is the index of the
    /// slot shown in front.
    Stack {
        slots: Vec<String>,
        #[serde(default)]
        active: Option<usize>,
    },
}

impl TemplateNode {
    /// A single-slot stack.
    pub fn slot(name: &str) -> TemplateNode {
        TemplateNode::Stack {
            slots: vec![name.to_owned()],
            active: Some(0),
        }
    }

    /// A split.
    pub fn split(axis: Axis, children: Vec<TemplateNode>, weights: Vec<f64>) -> TemplateNode {
        TemplateNode::Split { axis, children, weights }
    }

    /// Every slot name in the subtree, pre-order.
    pub fn slots(&self) -> Vec<String> {
        match self {
            TemplateNode::Stack { slots, .. } => slots.clone(),
            TemplateNode::Split { children, .. } => children.iter().flat_map(TemplateNode::slots).collect(),
        }
    }
}

/// A named tree shape. `slots` lists the slot names in fill order; every
/// leaf of `root` refers to one of them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutTemplate {
    pub name: String,
    pub slots: Vec<String>,
    pub root: TemplateNode,
}

impl LayoutTemplate {
    /// A template whose slot order is the tree's pre-order.
    pub fn new(name: impl Into<String>, root: TemplateNode) -> Self {
        let slots = root.slots();
        LayoutTemplate { name: name.into(), slots, root }
    }

    /// Fills the slots with `panes` in slot order: the first definition goes
    /// into the first slot, and so on. Slots without a definition are left
    /// out and the tree normalized, so a three-slot template given two panes
    /// yields a two-pane tree. Returns the tree (`None` when no pane was
    /// given) and the definitions keyed by their new ids.
    pub fn instantiate(&self, panes: Vec<PaneDefinition>, ids: &mut IdSource) -> (Option<LayoutNode>, BTreeMap<PaneId, PaneDefinition>) {
        let mut by_slot: BTreeMap<String, PaneId> = BTreeMap::new();
        let mut definitions = BTreeMap::new();
        for (slot, definition) in self.slots.iter().zip(panes) {
            let pane = ids.mint_pane();
            by_slot.insert(slot.clone(), pane.clone());
            definitions.insert(pane, definition);
        }
        let mut root = Some(build_node(&self.root, &by_slot, ids));
        ops::normalize(&mut root);
        (root, definitions)
    }
}

impl LayoutTemplate {
    /// Arranges panes that already exist into this template's shape, keeping
    /// their ids (so a window can be re-shaped around the panes it shows).
    /// Slots are filled in three passes: a slot named like a pane's kind
    /// takes that pane; the remaining slots take the remaining panes in
    /// order; slots still empty whose name `is_kind` accepts get a new pane
    /// of that kind (returned in the second value, so the caller can create
    /// it). Panes left over become tabs of the last filled stack. Slots that
    /// stay empty are left out of the tree.
    pub fn arrange(&self, existing: Vec<(PaneId, PaneDefinition)>, ids: &mut IdSource, is_kind: impl Fn(&str) -> bool) -> (Option<LayoutNode>, Vec<(PaneId, PaneDefinition)>) {
        let mut by_slot: BTreeMap<String, PaneId> = BTreeMap::new();
        let mut unassigned: Vec<(PaneId, PaneDefinition)> = existing;
        for slot in &self.slots {
            if let Some(index) = unassigned.iter().position(|(_, definition)| definition.kind == *slot) {
                let (pane, _) = unassigned.remove(index);
                by_slot.insert(slot.clone(), pane);
            }
        }
        for slot in &self.slots {
            if by_slot.contains_key(slot) {
                continue;
            }
            if !unassigned.is_empty() {
                let (pane, _) = unassigned.remove(0);
                by_slot.insert(slot.clone(), pane);
            }
        }
        let mut created = Vec::new();
        for slot in &self.slots {
            if by_slot.contains_key(slot) || !is_kind(slot) {
                continue;
            }
            let pane = ids.mint_pane();
            by_slot.insert(slot.clone(), pane.clone());
            created.push((pane, PaneDefinition::new(slot.clone())));
        }
        let mut root = Some(build_node(&self.root, &by_slot, ids));
        ops::normalize(&mut root);
        if !unassigned.is_empty() {
            match root.as_mut() {
                Some(tree) => {
                    let last = tree.stack_ids().last().cloned();
                    if let Some(LayoutNode::Stack { panes, .. }) = last.and_then(|last| tree.find_mut(&last)) {
                        panes.extend(unassigned.into_iter().map(|(pane, _)| pane));
                    }
                }
                None => {
                    let panes: Vec<PaneId> = unassigned.into_iter().map(|(pane, _)| pane).collect();
                    let active_pane_id = panes.first().cloned();
                    root = Some(LayoutNode::Stack { id: ids.mint_node(), panes, active_pane_id });
                }
            }
            ops::normalize(&mut root);
        }
        (root, created)
    }
}

impl WorkspaceLayout {
    /// Re-shapes `window` around the panes it shows, by `template` (see
    /// [`LayoutTemplate::arrange`]). Returns the panes the template created
    /// for slots nothing filled. Transactional.
    pub fn apply_template(&mut self, window: &WindowId, template: &LayoutTemplate, is_kind: impl Fn(&str) -> bool) -> Result<Vec<PaneId>, OpError> {
        let index = self.windows.iter().position(|candidate| candidate.id == *window).ok_or_else(|| OpError::UnknownWindow(window.clone()))?;
        let existing: Vec<(PaneId, PaneDefinition)> = self.windows[index].panes().into_iter().filter_map(|pane| self.pane(&pane).cloned().map(|definition| (pane, definition))).collect();
        let mut ids = self.ids.clone();
        let (root, created) = template.arrange(existing, &mut ids, is_kind);
        let mut work = self.clone();
        work.ids = ids;
        for (pane, definition) in &created {
            work.panes.insert(pane.clone(), definition.clone());
        }
        let active = root.as_ref().and_then(|tree| tree.stack_ids().into_iter().next()).and_then(|stack| root.as_ref()?.find(&stack)?.active_pane().cloned());
        work.windows[index].root = root;
        work.windows[index].active_pane = active;
        work.normalize();
        let violations = work.validate();
        if !violations.is_empty() {
            return Err(OpError::InvalidWeights { node: NodeId::new("template"), reason: format!("the template produced an invalid layout: {}", violations.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")) });
        }
        *self = work;
        log::info!("workspace: window {window} arranged by template '{}' ({} pane(s) created)", template.name, created.len());
        Ok(created.into_iter().map(|(pane, _)| pane).collect())
    }

    /// Copies `source`'s window `window` — its tree and its panes, with fresh
    /// ids — into a new floating window of this workspace at `frame`.
    /// Transactional.
    pub fn import_window(&mut self, source: &WorkspaceLayout, window: &WindowId, frame: WindowFrame) -> Result<WindowId, OpError> {
        let from = source.window(window).ok_or_else(|| OpError::UnknownWindow(window.clone()))?;
        let mut work = self.clone();
        let mut pane_ids: BTreeMap<PaneId, PaneId> = BTreeMap::new();
        for pane in from.panes() {
            let definition = source.pane(&pane).cloned().ok_or_else(|| OpError::UnknownPane(pane.clone()))?;
            let fresh = work.ids.mint_pane();
            work.panes.insert(fresh.clone(), definition);
            pane_ids.insert(pane, fresh);
        }
        let root = from.root.as_ref().map(|tree| reminted(tree, &pane_ids, &mut work.ids));
        let mut fresh_window = WindowLayout::new(work.ids.mint_window(), WindowRole::Floating);
        fresh_window.frame = Some(frame);
        fresh_window.active_pane = from.active_pane.as_ref().and_then(|pane| pane_ids.get(pane).cloned());
        fresh_window.root = root;
        let id = fresh_window.id.clone();
        work.windows.push(fresh_window);
        work.normalize();
        let violations = work.validate();
        if !violations.is_empty() {
            return Err(OpError::InvalidWeights { node: NodeId::new("import"), reason: format!("the imported window is invalid: {}", violations.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")) });
        }
        *self = work;
        log::info!("workspace: window {window} of '{}' imported as {id} ({} pane(s))", source.name, pane_ids.len());
        Ok(id)
    }
}

/// `node` with every pane id mapped through `panes` and every node id freshly minted.
fn reminted(node: &LayoutNode, panes: &BTreeMap<PaneId, PaneId>, ids: &mut IdSource) -> LayoutNode {
    match node {
        LayoutNode::Stack { panes: members, active_pane_id, .. } => LayoutNode::Stack {
            id: ids.mint_node(),
            panes: members.iter().filter_map(|pane| panes.get(pane).cloned()).collect(),
            active_pane_id: active_pane_id.as_ref().and_then(|active| panes.get(active).cloned()),
        },
        LayoutNode::Split { axis, children, weights, .. } => LayoutNode::Split { id: ids.mint_node(), axis: *axis, children: children.iter().map(|child| reminted(child, panes, ids)).collect(), weights: weights.clone() },
    }
}

/// Builds the layout tree for a template node; slots with no pane become
/// empty stacks that normalization removes.
fn build_node(node: &TemplateNode, by_slot: &BTreeMap<String, PaneId>, ids: &mut IdSource) -> LayoutNode {
    match node {
        TemplateNode::Stack { slots, active } => {
            let panes: Vec<PaneId> = slots.iter().filter_map(|slot| by_slot.get(slot).cloned()).collect();
            let active_pane_id = active.and_then(|index| slots.get(index)).and_then(|slot| by_slot.get(slot).cloned()).or_else(|| panes.first().cloned());
            LayoutNode::Stack {
                id: ids.mint_node(),
                panes,
                active_pane_id,
            }
        }
        TemplateNode::Split { axis, children, weights } => {
            let id = ids.mint_node();
            let built: Vec<LayoutNode> = children.iter().map(|child| build_node(child, by_slot, ids)).collect();
            LayoutNode::Split {
                id,
                axis: *axis,
                children: built,
                weights: weights.clone(),
            }
        }
    }
}

impl WorkspaceLayout {
    /// A workspace whose main window shows `template` filled with `panes`.
    pub fn from_template(name: impl Into<String>, template: &LayoutTemplate, panes: Vec<PaneDefinition>) -> WorkspaceLayout {
        let mut layout = WorkspaceLayout::new(name);
        let (root, definitions) = template.instantiate(panes, &mut layout.ids);
        let first_pane = root
            .as_ref()
            .and_then(|tree| tree.stack_ids().into_iter().next())
            .and_then(|stack| root.as_ref()?.find(&stack)?.active_pane().cloned());
        layout.panes = definitions;
        if let Some(main) = layout.windows.first_mut() {
            main.root = root;
            main.active_pane = first_pane;
        }
        log::info!("workspace: created '{}' from template '{}' with {} pane(s)", layout.name, template.name, layout.panes.len());
        debug_assert!(layout.validate().is_empty(), "template produced an invalid workspace: {:?}", layout.validate());
        layout
    }

    /// The shape of one window as a template. Every pane becomes a slot
    /// named by its kind (`account`, `account-2`, … when a kind repeats);
    /// resources and view state are not part of a template.
    pub fn to_template(&self, window: &WindowId) -> Result<LayoutTemplate, OpError> {
        self.to_template_named(window, self.name.clone())
    }

    /// [`WorkspaceLayout::to_template`] with an explicit template name.
    pub fn to_template_named(&self, window: &WindowId, name: impl Into<String>) -> Result<LayoutTemplate, OpError> {
        let window = self.window(window).ok_or_else(|| OpError::UnknownWindow(window.clone()))?;
        let mut names: BTreeMap<PaneId, String> = BTreeMap::new();
        let mut used: BTreeMap<String, usize> = BTreeMap::new();
        let mut order = Vec::new();
        for pane in window.panes() {
            let kind = self.pane(&pane).map(|definition| definition.kind.clone()).unwrap_or_else(|| "pane".to_owned());
            let count = used.entry(kind.clone()).or_insert(0);
            *count += 1;
            let slot = if *count == 1 { kind } else { format!("{kind}-{count}") };
            order.push(slot.clone());
            names.insert(pane, slot);
        }
        let root = match &window.root {
            Some(tree) => template_node(tree, &names),
            None => TemplateNode::Stack { slots: Vec::new(), active: None },
        };
        Ok(LayoutTemplate {
            name: name.into(),
            slots: order,
            root,
        })
    }
}

fn template_node(node: &LayoutNode, names: &BTreeMap<PaneId, String>) -> TemplateNode {
    match node {
        LayoutNode::Stack { panes, active_pane_id, .. } => {
            let slots: Vec<String> = panes.iter().filter_map(|pane| names.get(pane).cloned()).collect();
            let active = active_pane_id.as_ref().and_then(|active| panes.iter().position(|pane| pane == active));
            TemplateNode::Stack { slots, active }
        }
        LayoutNode::Split { axis, children, weights, .. } => TemplateNode::Split {
            axis: *axis,
            children: children.iter().map(|child| template_node(child, names)).collect(),
            weights: weights.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preset_arranges_the_panes_already_open_and_keeps_their_ids() {
        use crate::ops::DockTarget;
        use crate::layout::Side;
        let mut layout = WorkspaceLayout::new("Main");
        let main = WindowId::main();
        let a = layout.open_pane(&main, PaneDefinition::new("today"), DockTarget::edge(Side::Right)).unwrap();
        let b = layout.open_pane(&main, PaneDefinition::new("accounts"), DockTarget::edge(Side::Right)).unwrap();
        let c = layout.open_pane(&main, PaneDefinition::new("rules"), DockTarget::edge(Side::Right)).unwrap();
        let d = layout.open_pane(&main, PaneDefinition::new("forecast"), DockTarget::edge(Side::Right)).unwrap();
        // Analysis has three slots: the fourth pane becomes a tab of the last stack.
        let created = layout.apply_template(&main, &Preset::Analysis.template(), |_| false).unwrap();
        assert!(created.is_empty(), "generic slots create nothing");
        let root = layout.main_window().unwrap().root.clone().unwrap();
        assert_eq!(crate::grid::render(&root, 3, 3, |pane| if *pane == a { '1' } else if *pane == b { '2' } else if *pane == c { '3' } else { '4' }), "112\n113\n113");
        assert_eq!(layout.panes.len(), 4);
        let last = root.stack_ids().last().cloned().unwrap();
        assert_eq!(root.find(&last).unwrap().stack_panes(), [c.clone(), d.clone()], "the leftover pane joined the last stack");
        assert!(layout.validate().is_empty());
        // A slot named like a kind takes that pane, wherever it was.
        let by_kind = LayoutTemplate::new("by kind", TemplateNode::split(Axis::Horizontal, vec![TemplateNode::slot("rules"), TemplateNode::slot("today")], vec![0.5, 0.5]));
        let created = layout.apply_template(&main, &by_kind, |_| false).unwrap();
        assert!(created.is_empty());
        let root = layout.main_window().unwrap().root.clone().unwrap();
        let first = root.stack_ids()[0].clone();
        assert_eq!(root.find(&first).unwrap().stack_panes()[0], c, "Rules took the first slot by kind");
        // A slot whose name is a kind, with nothing left to fill it, becomes a new pane.
        let mut two = WorkspaceLayout::new("Main");
        two.open_pane(&main, PaneDefinition::new("today"), DockTarget::edge(Side::Right)).unwrap();
        let created = two.apply_template(&main, &by_kind, |kind| kind == "rules").unwrap();
        assert_eq!(created.len(), 1);
        assert_eq!(two.pane(&created[0]).map(|definition| definition.kind.as_str()), Some("rules"));
        assert_eq!(two.panes.len(), 2);
        // A template applied to an empty window yields nothing but stays valid.
        let mut empty = WorkspaceLayout::new("Main");
        assert!(empty.apply_template(&main, &Preset::Compare.template(), |_| false).unwrap().is_empty());
        assert!(empty.main_window().unwrap().root.is_none());
    }

    #[test]
    fn a_saved_window_is_imported_as_a_floating_window_with_fresh_ids() {
        use crate::ops::DockTarget;
        use crate::layout::Side;
        let main = WindowId::main();
        let mut saved = WorkspaceLayout::new("Saved");
        let a = saved.open_pane(&main, PaneDefinition::new("today"), DockTarget::edge(Side::Right)).unwrap();
        saved.open_pane(&main, PaneDefinition::new("accounts").with_resource(serde_json::json!({"accountId": 3})), DockTarget::edge(Side::Right)).unwrap();
        let mut current = WorkspaceLayout::new("Main");
        let existing = current.open_pane(&main, PaneDefinition::new("today"), DockTarget::edge(Side::Right)).unwrap();
        assert_eq!(existing, a, "both workspaces minted the same first id: the import must not collide");
        let window = current.import_window(&saved, &main, WindowFrame::new(10.0, 10.0, 800.0, 600.0)).unwrap();
        assert_eq!(current.windows.len(), 2);
        assert_eq!(current.panes_in(&window).len(), 2);
        assert_eq!(current.panes_in(&main), vec![existing]);
        assert_eq!(current.panes.len(), 3);
        let imported_accounts = current.panes_in(&window).into_iter().find(|pane| current.pane(pane).is_some_and(|definition| definition.kind == "accounts")).unwrap();
        assert_eq!(current.pane(&imported_accounts).and_then(|definition| definition.resource.clone()), Some(serde_json::json!({"accountId": 3})));
        assert!(current.validate().is_empty(), "{:?}", current.validate());
        assert!(current.import_window(&saved, &WindowId::new("nope"), WindowFrame::new(0.0, 0.0, 1.0, 1.0)).is_err());
    }

    #[test]
    fn presets_have_labels_and_slot_counts() {
        for preset in Preset::all() {
            assert!(!preset.label().is_empty());
            assert!(!preset.description().is_empty());
            let template = preset.template();
            assert_eq!(template.slots.len(), preset.slot_count());
            assert_eq!(template.root.slots(), template.slots, "{preset:?}: slot order must be the tree order");
        }
    }

    #[test]
    fn a_partially_filled_template_drops_empty_slots() {
        let mut ids = IdSource::new();
        let (root, panes) = Preset::Analysis.template().instantiate(vec![PaneDefinition::new("a"), PaneDefinition::new("b")], &mut ids);
        let tree = root.unwrap();
        assert_eq!(panes.len(), 2);
        assert_eq!(tree.leaf_count(), 2);
        assert_eq!(tree.axis(), Some(Axis::Horizontal));
        assert!((tree.weights()[0] - 0.66).abs() < 1e-12);
        let (none, empty) = Preset::Focus.template().instantiate(vec![], &mut ids);
        assert!(none.is_none());
        assert!(empty.is_empty());
    }
}
