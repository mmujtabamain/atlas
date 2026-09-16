//! The pane workspace — "everything is a pane" — for one window.
//!
//! Every screen of the app shows inside a **pane**. Panes sit side by side in
//! horizontal and vertical splits with draggable dividers, stack as tabs, and
//! one of them is the **active** pane: the one the launcher, the keyboard
//! commands and in-pane Back act on. Closing the last pane leaves an empty
//! workspace that offers to open one.
//!
//! Two things hold the layout, and the division of labour between them is
//! the whole design:
//!
//! - the **model**, [`atlas_workspace::WorkspaceLayout`], is the source of
//!   truth for structure, the active pane, undo history and (later) the saved
//!   layout. It is pure data, tested on its own, and every operation on it is
//!   transactional;
//! - gpui-kit's dock engine, [`gpui_kit::component::dock::DockArea`], is the
//!   **live projection** the person sees and interacts with: it lays the
//!   panes out, draws the tab bars and dividers, and handles the drags.
//!
//! Every command applies to the model first and then to the area. A pane
//! dropped after a drag is applied to the model first as well — the engine
//! resolves where it landed, the model decides whether that is allowed and
//! what the weights become, and the area is rebuilt from the model — while
//! every other edit that originates in the engine (a divider dragged, a tab
//! chosen or closed) is mirrored back into the model from the engine's own
//! dump, and validated before it is accepted. [`view`] spells out which
//! model change maps onto which engine operation.
//!
//! | module | contents |
//! |---|---|
//! | [`view`] | [`WorkspaceView`]: the content column; owns the model, the pane views and the dock area; the model ↔ area mapping |
//! | [`pane`] | [`PaneView`]: one pane — a dock panel rendering one screen, with its own scroll region and in-pane history |
//! | [`kinds`] | the pane registry: a route as a pane definition (`kind` + `resource`) and back; titles and icons |
//! | [`mirror`] | reading the engine's dumped layout back into a model tree |
//! | [`dock_targets`] | the drag-target overlay: bands for docking beside a group, a run of siblings, or the window |
//! | [`launcher`] | the pane launcher strip under the title bar, and its saved arrangement |
//! | [`commands`] | the keyboard commands: split, close, next pane, back |
//!
//! Dragging a pane onto another pane (its centre for a tab, an edge for a
//! split) works within the window and is transactional: Escape cancels with
//! nothing changed, a drop that changes nothing leaves no history, and a drop
//! the minimum-size rule refuses is put back and explained. Docking beside a
//! whole group, beside a run of sibling panes or along the window's edges is
//! the workspace's own overlay ([`dock_targets`]). The launcher, floating
//! windows and persistence build on this and come later.

pub mod commands;
pub mod dock_targets;
pub mod kinds;
pub mod launcher;
pub mod mirror;
pub mod pane;
pub mod view;

pub use launcher::LauncherView;
pub use pane::PaneView;
pub use view::WorkspaceView;
