//! The pane workspace — "everything is a pane" — for one window.
//!
//! Every screen of the app shows inside a **pane**. Panes sit side by side in
//! horizontal and vertical splits with draggable dividers, stack as tabs, and
//! one of them is the **active** pane: the one the sidebar, the keyboard
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
//! Every command applies to the model first and then to the area; every edit
//! that originates in the engine — a divider dragged, a tab chosen or closed,
//! later a pane dragged elsewhere — is mirrored back into the model from the
//! engine's own dump, and validated before it is accepted. [`view`] spells
//! out which model change maps onto which engine operation.
//!
//! | module | contents |
//! |---|---|
//! | [`view`] | [`WorkspaceView`]: the content column; owns the model, the pane views and the dock area; the model ↔ area mapping |
//! | [`pane`] | [`PaneView`]: one pane — a dock panel rendering one screen, with its own scroll region and in-pane history |
//! | [`kinds`] | the pane registry: a route as a pane definition (`kind` + `resource`) and back; titles and icons |
//! | [`mirror`] | reading the engine's dumped layout back into a model tree |
//! | [`commands`] | the keyboard commands: split, close, next pane, back |
//!
//! Drag-and-drop docking, docking beside a whole group, the launcher, floating
//! windows and persistence build on this and come later; the engine already
//! supports dragging, so nothing here disables it.

pub mod commands;
pub mod kinds;
pub mod mirror;
pub mod pane;
pub mod view;

pub use pane::PaneView;
pub use view::WorkspaceView;
