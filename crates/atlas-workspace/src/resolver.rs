//! Where "open X" goes.
//!
//! Every link, menu item and command that opens a screen goes through
//! [`resolve_in`], which turns the intent into either *focus that existing
//! pane* or *create one here*. "Here" is the window the request came from —
//! a launcher, a menu or an empty window's first-pane button acts on the
//! window it is in, whichever window holds the active pane. The rules:
//!
//! - [`Intent::Open`]: if a pane already shows exactly this (same kind, same
//!   resource) focus it — preferring one in this window, then one in this
//!   window's active stack; otherwise create it as a tab of this window's
//!   active stack, or at the right edge when the window is empty. An empty
//!   window never reuses a pane elsewhere: it asked for its first pane;
//! - [`Intent::NewInstance`]: always create a tab of the active stack;
//! - [`Intent::OpenRight`] / [`Intent::OpenBelow`]: create beside the active
//!   stack on that side (at the window edge when the window is empty);
//! - [`Intent::OpenNewWindow`]: create a new floating window.
//!
//! [`resolve`] is the same with the active window as "here". The resolver
//! only decides; the caller performs the operation.

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

/// Decides where a screen of `kind` about `resource` opens, asked for from
/// the active window.
pub fn resolve(layout: &WorkspaceLayout, kind: &str, resource: Option<&Value>, intent: Intent) -> Resolution {
    let here = layout.active_window().map(|window| window.id.clone()).unwrap_or_else(WindowId::main);
    resolve_in(layout, &here, kind, resource, intent)
}

/// Decides where a screen of `kind` about `resource` opens, asked for from
/// the window `here` (the main window when `here` is not a window of the
/// layout).
pub fn resolve_in(layout: &WorkspaceLayout, here: &WindowId, kind: &str, resource: Option<&Value>, intent: Intent) -> Resolution {
    let window = layout.window(here).or_else(|| layout.main_window());
    let here = window.map(|window| window.id.clone()).unwrap_or_else(WindowId::main);
    let active_stack = window.and_then(|window| window.active_stack());

    match intent {
        Intent::Open => {
            let matches = layout.find_panes(kind, resource);
            if let Some(existing) = pick_existing(layout, &matches, &here, active_stack.as_ref()) {
                log::info!("resolver: '{kind}' already open as {existing}; focusing it");
                return Resolution::Focus(existing);
            }
            Resolution::Create {
                window: here,
                target: centre_target(active_stack),
            }
        }
        Intent::NewInstance => Resolution::Create {
            window: here,
            target: centre_target(active_stack),
        },
        Intent::OpenRight => Resolution::Create {
            window: here,
            target: beside_target(active_stack, Side::Right),
        },
        Intent::OpenBelow => Resolution::Create {
            window: here,
            target: beside_target(active_stack, Side::Bottom),
        },
        Intent::OpenNewWindow => Resolution::CreateWindow,
    }
}

/// The best existing pane: in this window's active stack, else in this
/// window, else the first anywhere — unless this window is empty, which
/// asked for a pane of its own and reuses nothing.
fn pick_existing(layout: &WorkspaceLayout, matches: &[PaneId], here: &WindowId, active_stack: Option<&crate::ids::NodeId>) -> Option<PaneId> {
    if matches.is_empty() || active_stack.is_none() {
        return None;
    }
    let in_active_stack = matches.iter().find(|pane| layout.stack_of(pane).as_ref() == active_stack);
    let in_this_window = matches.iter().find(|pane| layout.window_id_of(pane).as_ref() == Some(here));
    in_active_stack.or(in_this_window).or_else(|| matches.first()).cloned()
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
