//! Where "open X" goes.
//!
//! Every link, menu item and command that opens a screen goes through
//! [`resolve`], which turns the intent into either *focus that existing pane*
//! or *create one here*. The rules:
//!
//! - [`Intent::Open`]: if a pane already shows exactly this (same kind, same
//!   resource) focus it — preferring one in the active window, then one in
//!   the active stack; otherwise create it as a tab of the active stack, or
//!   at the right edge when the window is empty;
//! - [`Intent::NewInstance`]: always create a tab of the active stack;
//! - [`Intent::OpenRight`] / [`Intent::OpenBelow`]: create beside the active
//!   stack on that side (at the window edge when the window is empty);
//! - [`Intent::OpenNewWindow`]: create a new floating window.
//!
//! The resolver only decides; the caller performs the operation.

use crate::ids::{PaneId, WindowId};
use crate::layout::Side;
use crate::ops::DockTarget;
use crate::workspace::WorkspaceLayout;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How the user asked for the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Intent {
    /// Plain open: reuse an existing pane if there is one.
    Open,
    /// Open in a new pane to the right of the current one.
    OpenRight,
    /// Open in a new pane below the current one.
    OpenBelow,
    /// Open in a new window.
    OpenNewWindow,
    /// Open another pane even if one already shows the same thing.
    NewInstance,
}

/// What to do.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Resolution {
    /// Give this existing pane the focus.
    Focus(PaneId),
    /// Open a new pane in `window` at `target`.
    Create { window: WindowId, target: DockTarget },
    /// Open a new pane in a new floating window.
    CreateWindow,
}

/// Decides where a screen of `kind` about `resource` opens.
pub fn resolve(layout: &WorkspaceLayout, kind: &str, resource: Option<&Value>, intent: Intent) -> Resolution {
    let active_window = layout.active_window();
    let active_window_id = active_window.map(|window| window.id.clone()).unwrap_or_else(WindowId::main);
    let active_stack = active_window.and_then(|window| window.active_stack());

    match intent {
        Intent::Open => {
            let matches = layout.find_panes(kind, resource);
            if let Some(existing) = pick_existing(layout, &matches, &active_window_id) {
                log::info!("resolver: '{kind}' already open as {existing}; focusing it");
                return Resolution::Focus(existing);
            }
            Resolution::Create {
                window: active_window_id,
                target: centre_target(active_stack),
            }
        }
        Intent::NewInstance => Resolution::Create {
            window: active_window_id,
            target: centre_target(active_stack),
        },
        Intent::OpenRight => Resolution::Create {
            window: active_window_id,
            target: beside_target(active_stack, Side::Right),
        },
        Intent::OpenBelow => Resolution::Create {
            window: active_window_id,
            target: beside_target(active_stack, Side::Bottom),
        },
        Intent::OpenNewWindow => Resolution::CreateWindow,
    }
}

/// The best existing pane: in the active stack, else in the active window, else the first.
fn pick_existing(layout: &WorkspaceLayout, matches: &[PaneId], active_window: &WindowId) -> Option<PaneId> {
    if matches.is_empty() {
        return None;
    }
    let active_stack = layout.active_pane().and_then(|pane| layout.stack_of(&pane));
    let in_active_stack = matches.iter().find(|pane| active_stack.is_some() && layout.stack_of(pane) == active_stack);
    let in_active_window = matches.iter().find(|pane| layout.window_id_of(pane).as_ref() == Some(active_window));
    in_active_stack.or(in_active_window).or_else(|| matches.first()).cloned()
}

fn centre_target(active_stack: Option<crate::ids::NodeId>) -> DockTarget {
    match active_stack {
        Some(stack) => DockTarget::tab(stack),
        None => DockTarget::edge(Side::Right),
    }
}

fn beside_target(active_stack: Option<crate::ids::NodeId>, side: Side) -> DockTarget {
    match active_stack {
        Some(stack) => DockTarget::beside(stack, side),
        None => DockTarget::edge(side),
    }
}
