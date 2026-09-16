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

use crate::ids::{IdSource, PaneId, WindowId};
use crate::layout::{Axis, LayoutNode};
use crate::ops::{self, OpError};
use crate::workspace::{PaneDefinition, WorkspaceLayout};
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
