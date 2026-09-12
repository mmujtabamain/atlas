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
//! | [`screens`] | one module per sidebar section |
//! | [`widgets`] | shared pieces: figures with "Why?", vocabulary tags, the explain sheet |
//! | [`lifecycle`] | new / open / save / sample, the lock, the who-is-looking picker (M12) |
//! | [`entry`] | data-entry dialogs for every object (M12) |
//! | [`rules_entry`] | the rule editor dialog (M7) |
//! | [`scenario_entry`] | scenario change and composition dialogs (M8) |
//! | [`decision_entry`] | the decision builder's form state and steps (M9) |
//! | [`privacy_entry`] | policy editor and purpose-grant dialogs (M10) |
//! | [`alerting`] | failure reporting to the DevBench notify endpoint |
//! | [`logging`] | stderr + `logs.log` logger with timestamps and per-sink filters |
//! | [`perf`] | frame meter (FPS counter, per-frame and summary perf log lines), engine timing |
//! | [`launch`] | command-line options and theme selection |

pub mod alerting;
pub mod app;
pub mod decision_entry;
pub mod derived;
pub mod entry;
pub mod launch;
pub mod lifecycle;
pub mod logging;
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
