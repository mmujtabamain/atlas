//! Atlas Financer desktop application — the verbose, provenance-first UI over
//! [`atlas_core`].
//!
//! Layout only: every control is a gpui-kit component and every colour a
//! `cx.theme()` token. The crate is a library so the production view can be
//! driven by the UI integration tests in `tests/ui.rs`; `main.rs` is a thin
//! binary around [`launch`].
//!
//! | module | contents |
//! |---|---|
//! | [`shell`] | `Shell`, the window's root view: title bar, cached sidebar, cached content, status bar |
//! | [`app`] | `AtlasApp`, the content view: household, derived models, the active screen |
//! | [`derived`] | `Lazy`, a screen model dropped on change and computed on first use |
//! | [`nav`] | destinations, routes and the slugs that address them |
//! | [`models`] | derived screen models: computed once per state change, read by the screens |
//! | [`screens`] | one module per screen of the design |
//! | [`widgets`] | shared compositions: explained figures, the calculation sheet, figure meanings, scope bar, record lists |
//! | [`lifecycle`] | new / open / save / sample, the lock, the who-is-looking picker |
//! | [`entry`] | data-entry forms for every object |
//! | [`rules_entry`] | the rule creation flow |
//! | [`scenario_entry`] | scenario change and composition forms |
//! | [`decision_entry`] | the purchase builder's form state and steps |
//! | [`privacy_entry`] | policy and purpose-grant forms |
//! | [`alerting`] | failure reporting to the DevBench notify endpoint |
//! | [`logging`] | stderr + `logs.log` logger with timestamps and per-sink filters |
//! | [`perf`] | frame meter (FPS counter, per-frame and summary perf log lines), engine timing |
//! | [`launch`] | command-line options and theme selection |

pub mod actions;
pub mod alerting;
pub mod app;
pub mod controls;
pub mod decision_entry;
pub mod derived;
pub mod entry;
pub mod launch;
pub mod lifecycle;
pub mod logging;
pub mod models;
pub mod nav;
pub mod perf;
pub mod privacy_entry;
pub mod rules_entry;
pub mod scenario_entry;
pub mod screens;
pub mod shell;
pub mod widgets;

pub use app::AtlasApp;
pub use launch::Launch;
pub use shell::Shell;
