//! The workspace layout model of Atlas Financer — "everything is a pane" —
//! as pure data.
//!
//! Every screen of the app is a **pane**. Panes stack as tabs and split to
//! the left, right, top or bottom of another pane, of a whole group or of a
//! whole window; any pane can live in any window; layouts are saved and
//! restored. This crate is the half of that story with no UI in it: the
//! tree, the operations on it, the invariants, undo history, files, saved
//! layouts, the rules that decide where "open X" goes, keyboard focus
//! geometry and the background-job state machine. The UI crate projects a
//! [`WorkspaceLayout`] onto real windows and feeds user gestures back in as
//! operations; nothing here depends on gpui or gpui-kit, so all of it is
//! tested with plain `cargo test`.
//!
//! Two properties hold throughout. Operations are **transactional** — they
//! either apply completely or leave the workspace byte-for-byte unchanged —
//! and the model is **deterministic**: the same operations on the same
//! starting workspace produce the same ids and the same JSON.
//!
//! Module map:
//!
//! | module | contents |
//! |---|---|
//! | [`ids`] | `PaneId`, `NodeId`, `WindowId` (readable strings) and the deterministic `IdSource` |
//! | [`layout`] | `Axis`, `Side`, `Rect` and the `LayoutNode` tree of splits and stacks, with navigation helpers and unit-square geometry |
//! | [`ops`] | the tree algebra: `DockTarget`, insert / remove / move / stack / unstack / resize, `normalize`, the minimum-size rule |
//! | [`workspace`] | `WorkspaceLayout` (windows, panes, active state), `WindowLayout`, `PaneDefinition`, `Scope` |
//! | [`validate`] | every invariant, as `Violation`s |
//! | [`history`] | snapshot undo / redo |
//! | [`closed`] | recently closed panes and `reopen` |
//! | [`persist`] | schema version and migration chain, `load` / `save_atomic`, `Autosave`, view-state scrubbing |
//! | [`presets`] | `LayoutTemplate`, the built-in `Preset`s, `from_template` / `to_template` |
//! | [`saved`] | `SavedLayouts`: named layouts and templates on disk, household scoping |
//! | [`resolver`] | `resolve`: focus an existing pane or create one, per `Intent` |
//! | [`focus`] | directional neighbour and "move pane" targets from geometry |
//! | [`jobs`] | `JobBoard`: the background-job state machine behind the indicator |
//! | [`grid`] | character-grid pictures of a tree, for tests and logs |

pub mod closed;
pub mod focus;
pub mod grid;
pub mod history;
pub mod ids;
pub mod jobs;
pub mod layout;
pub mod ops;
pub mod persist;
pub mod presets;
pub mod resolver;
pub mod saved;
pub mod validate;
pub mod workspace;

pub use closed::{ClosedPane, ClosedPanes};
pub use focus::Direction;
pub use history::{HistoryEntry, LayoutHistory};
pub use ids::{IdSource, NodeId, PaneId, WindowId};
pub use jobs::{Job, JobBoard, JobError, JobId, JobState};
pub use layout::{Axis, LayoutNode, Rect, Side};
pub use ops::{DockTarget, NormalizeReport, OpError, SplitLimits};
pub use persist::{Autosave, LoadOutcome, PersistError, SCHEMA_VERSION};
pub use presets::{LayoutTemplate, Preset, TemplateNode};
pub use resolver::{Intent, Resolution};
pub use saved::{SavedError, SavedKind, SavedLayout, SavedLayouts};
pub use validate::Violation;
pub use workspace::{PaneDefinition, Scope, WindowFrame, WindowLayout, WindowRole, WorkspaceLayout};
