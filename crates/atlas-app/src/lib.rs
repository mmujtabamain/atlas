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
//! | [`app`] | `AtlasApp`, the window's root view: shell, navigation, derived models |
//! | [`screens`] | one module per sidebar section |
//! | [`widgets`] | shared pieces: figures with "Why?", vocabulary tags, the explain sheet |
//! | [`lifecycle`] | new / open / save / sample, the lock, the who-is-looking picker (M12) |
//! | [`entry`] | data-entry dialogs for every object (M12) |
//! | [`alerting`] | failure reporting to the DevBench notify endpoint |
//! | [`launch`] | command-line options and theme selection |

pub mod alerting;
pub mod app;
pub mod entry;
pub mod launch;
pub mod lifecycle;
pub mod screens;
pub mod widgets;

pub use app::AtlasApp;
pub use launch::Launch;
