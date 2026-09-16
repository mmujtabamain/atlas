//! Identifiers of the workspace model.
//!
//! Every pane, tree node and window carries its own newtype so the compiler
//! keeps them apart, and every one of them is a **string** rather than an
//! integer: a saved layout file is meant to be readable (and, in a pinch,
//! editable) by a person, and `"pane_7"` reads better than `7`.
//!
//! Ids are minted by an [`IdSource`], which is part of the saved
//! [`WorkspaceLayout`](crate::WorkspaceLayout) so that a restored workspace
//! keeps counting where it left off and the same sequence of operations on
//! the same starting state always yields the same ids (the model is
//! deterministic by design; the tests rely on it).

use serde::{Deserialize, Serialize};
use std::fmt;

/// Declares one string-backed identifier newtype.
macro_rules! define_string_id {
    ($(#[$meta:meta])* $name:ident, $prefix:literal) => {
        $(#[$meta])*
        #[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// The prefix every minted id of this kind starts with (`"pane"`, `"node"`, `"window"`).
            pub const PREFIX: &'static str = $prefix;

            /// Wraps an arbitrary string. Hand-written layout files may use any
            /// name; minted ids look like `pane_1`, `pane_2`, …
            pub fn new(raw: impl Into<String>) -> Self {
                $name(raw.into())
            }

            /// Builds the id [`IdSource`] would mint for `counter`.
            pub fn numbered(counter: u64) -> Self {
                $name(format!("{}_{}", $prefix, counter))
            }

            /// The raw string.
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// True when the id is the empty string, which a hand-written file
            /// may leave behind; such ids are replaced on load.
            pub fn is_blank(&self) -> bool {
                self.0.is_empty()
            }

            /// The counter this id was minted from, if it has the minted shape
            /// (`<prefix>_<number>`). Used to move an [`IdSource`] past ids that
            /// arrived from a file.
            pub fn minted_counter(&self) -> Option<u64> {
                let rest = self.0.strip_prefix($prefix)?.strip_prefix('_')?;
                rest.parse::<u64>().ok()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<&str> for $name {
            fn from(raw: &str) -> Self {
                $name(raw.to_owned())
            }
        }

        impl From<String> for $name {
            fn from(raw: String) -> Self {
                $name(raw)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

define_string_id!(
    /// One open pane (a screen instance). Unique across every window of a workspace.
    PaneId,
    "pane"
);
define_string_id!(
    /// One node of a window's layout tree — a split or a stack.
    NodeId,
    "node"
);
define_string_id!(
    /// One window. The main window is always [`WindowId::main`].
    WindowId,
    "window"
);

impl WindowId {
    /// The id of the main window, which every workspace has exactly one of.
    pub fn main() -> Self {
        WindowId("window_main".to_owned())
    }

    /// True for the main window's id.
    pub fn is_main(&self) -> bool {
        self.0 == "window_main"
    }
}

/// Mints pane, node and window ids deterministically: `pane_1`, `pane_2`, …
///
/// The counters are saved with the layout so restored workspaces never reuse
/// an id that a history entry or a closed-pane record may still refer to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct IdSource {
    /// The counter the next pane id is minted from.
    pub next_pane: u64,
    /// The counter the next node id is minted from.
    pub next_node: u64,
    /// The counter the next (floating) window id is minted from.
    pub next_window: u64,
}

impl Default for IdSource {
    fn default() -> Self {
        IdSource {
            next_pane: 1,
            next_node: 1,
            next_window: 1,
        }
    }
}

impl IdSource {
    /// A source that starts counting at 1.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mints the next pane id.
    pub fn mint_pane(&mut self) -> PaneId {
        let id = PaneId::numbered(self.next_pane);
        self.next_pane += 1;
        id
    }

    /// Mints the next node id.
    pub fn mint_node(&mut self) -> NodeId {
        let id = NodeId::numbered(self.next_node);
        self.next_node += 1;
        id
    }

    /// Mints the next floating-window id.
    pub fn mint_window(&mut self) -> WindowId {
        let id = WindowId::numbered(self.next_window);
        self.next_window += 1;
        id
    }

    /// Moves the pane counter past `id` if it has the minted shape, so ids
    /// that arrived from a file are never minted a second time.
    pub fn observe_pane(&mut self, id: &PaneId) {
        if let Some(counter) = id.minted_counter() {
            self.next_pane = self.next_pane.max(counter + 1);
        }
    }

    /// Moves the node counter past `id` if it has the minted shape.
    pub fn observe_node(&mut self, id: &NodeId) {
        if let Some(counter) = id.minted_counter() {
            self.next_node = self.next_node.max(counter + 1);
        }
    }

    /// Moves the window counter past `id` if it has the minted shape.
    pub fn observe_window(&mut self, id: &WindowId) {
        if let Some(counter) = id.minted_counter() {
            self.next_window = self.next_window.max(counter + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_mint_in_sequence_and_round_trip_as_plain_strings() {
        let mut ids = IdSource::new();
        assert_eq!(ids.mint_pane().as_str(), "pane_1");
        assert_eq!(ids.mint_pane().as_str(), "pane_2");
        assert_eq!(ids.mint_node().as_str(), "node_1");
        assert_eq!(ids.mint_window().as_str(), "window_1");
        assert_eq!(serde_json::to_string(&PaneId::new("pane_3")).unwrap(), "\"pane_3\"");
        assert_eq!(serde_json::from_str::<NodeId>("\"node_9\"").unwrap(), NodeId::numbered(9));
        assert_eq!(WindowId::main().to_string(), "window_main");
        assert!(WindowId::main().is_main());
    }

    #[test]
    fn observing_a_minted_id_moves_the_counter_past_it() {
        let mut ids = IdSource::new();
        ids.observe_pane(&PaneId::new("pane_41"));
        assert_eq!(ids.mint_pane().as_str(), "pane_42");
        ids.observe_pane(&PaneId::new("custom-name"));
        assert_eq!(ids.mint_pane().as_str(), "pane_43");
        ids.observe_node(&NodeId::new("node_2"));
        assert_eq!(ids.mint_node().as_str(), "node_3");
        ids.observe_window(&WindowId::main());
        assert_eq!(ids.mint_window().as_str(), "window_1");
    }
}
