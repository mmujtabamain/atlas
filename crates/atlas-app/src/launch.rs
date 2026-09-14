//! Command-line options and theme selection.

use std::path::PathBuf;

use chrono::NaiveDate;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::App;

use crate::nav::Route;

/// Which household the app starts with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Start {
    /// No household: the Welcome screen (default).
    Welcome,
    /// The fictitious sample household.
    Sample,
    /// An empty household (unsaved until Save as…).
    Empty,
    /// A household file: opened when it exists, created when it does not.
    File(PathBuf),
}

/// What the command line asked for.
#[derive(Debug, Clone)]
pub struct Launch {
    /// `light` or `dark`.
    pub theme: String,
    pub width: f32,
    pub height: f32,
    /// The route to open with (after the viewer is chosen).
    pub route: Route,
    /// `--screen person` and the other detail slugs, which name a kind of
    /// record rather than one record; resolved against the household once it
    /// is open, since only then is there an id to open.
    pub detail: Option<crate::nav::FirstDetail>,
    /// Which fixture person is looking (`a` or `b`) when given explicitly;
    /// `--viewer <person id>` for real households. Without it a household with
    /// several people opens behind the "Who is looking?" chooser.
    pub viewer: Option<char>,
    /// Person id to view as (overrides `viewer` when given).
    pub viewer_id: Option<u32>,
    pub start: Start,
    /// Reconciliation date for a new empty household.
    pub as_of: Option<NaiveDate>,
    /// Lock owner name (defaults to the OS user).
    pub owner: String,
    /// Take over another owner's lock on the household file.
    pub take_over: bool,
    /// gpui's frame-time overlay in the window's top-right corner (perf work).
    /// Off unless `--perf-overlay` or `ATLAS_PERF_OVERLAY=1` asks for it; the
    /// status bar keeps gpui's fps reading either way.
    pub perf_overlay: bool,
}

impl Default for Launch {
    fn default() -> Self {
        Launch {
            theme: "light".into(),
            width: 1600.,
            height: 1000.,
            route: Route::Today,
            detail: None,
            viewer: None,
            viewer_id: None,
            start: Start::Welcome,
            as_of: None,
            owner: std::env::var("USER").unwrap_or_else(|_| "user".into()),
            take_over: false,
            perf_overlay: std::env::var("ATLAS_PERF_OVERLAY").map(|v| matches!(v.trim(), "1" | "on" | "true" | "yes")).unwrap_or(false),
        }
    }
}

impl Launch {
    /// Parses `args` (without the program name). Unknown arguments are logged
    /// and ignored so a screenshot run never dies on a typo.
    pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Launch {
        let mut launch = Launch::default();
        let args: Vec<String> = args.into_iter().collect();
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--theme" => {
                    i += 1;
                    launch.theme = args.get(i).cloned().unwrap_or_else(|| "light".into());
                }
                "--size" => {
                    i += 1;
                    if let Some((w, h)) = args.get(i).and_then(|s| s.split_once('x')) {
                        launch.width = w.parse().unwrap_or(launch.width);
                        launch.height = h.parse().unwrap_or(launch.height);
                    }
                }
                "--screen" => {
                    i += 1;
                    match args.get(i) {
                        Some(slug) if Route::from_slug(slug).is_some() => {
                            launch.route = Route::from_slug(slug).expect("just checked");
                            launch.detail = None;
                        }
                        Some(slug) if crate::nav::FirstDetail::from_slug(slug).is_some() => {
                            launch.detail = crate::nav::FirstDetail::from_slug(slug);
                        }
                        other => log::warn!(
                            "unknown --screen {other:?}; known: {}, {}",
                            Route::slugs().join(", "),
                            crate::nav::FirstDetail::slugs().join(", ")
                        ),
                    }
                }
                "--viewer" => {
                    i += 1;
                    match args.get(i) {
                        Some(value) if value.chars().all(|c| c.is_ascii_digit()) => launch.viewer_id = value.parse().ok(),
                        Some(value) => launch.viewer = value.chars().next().map(|c| c.to_ascii_lowercase()),
                        None => {}
                    }
                }
                "--household" => {
                    i += 1;
                    if let Some(path) = args.get(i) {
                        launch.start = Start::File(PathBuf::from(path));
                    }
                }
                "--new" => launch.start = Start::Empty,
                "--sample" => launch.start = Start::Sample,
                "--as-of" => {
                    i += 1;
                    launch.as_of = args.get(i).and_then(|s| s.parse().ok());
                }
                "--owner" => {
                    i += 1;
                    if let Some(owner) = args.get(i) {
                        launch.owner = owner.clone();
                    }
                }
                "--take-over" => launch.take_over = true,
                "--perf-overlay" => launch.perf_overlay = true,
                "--no-perf-overlay" => launch.perf_overlay = false,
                "-h" | "--help" => {
                    println!(
                        "atlas [--theme light|dark] [--size WxH] [--screen {}] [--viewer a|b|<person id>] [--household FILE.atlas.sqlite | --new | --sample] [--as-of YYYY-MM-DD] [--owner NAME] [--take-over] [--perf-overlay]\n\nLogs go to stderr and logs.log (ATLAS_LOG_FILE=path|off, RUST_LOG=filter, ATLAS_LOG_FILE_FILTER=filter); the status bar shows gpui's frame timing.",
                        [Route::slugs(), crate::nav::FirstDetail::slugs()].concat().join("|")
                    );
                    std::process::exit(0);
                }
                other => log::warn!("ignoring unknown argument {other}"),
            }
            i += 1;
        }
        launch
    }

    /// Applies `--theme`.
    pub fn apply_theme(&self, cx: &mut App) {
        match self.theme.as_str() {
            "dark" => Theme::change(ThemeMode::Dark, None, cx),
            "light" => Theme::change(ThemeMode::Light, None, cx),
            other => {
                log::warn!("unknown theme {other:?}; using light");
                Theme::change(ThemeMode::Light, None, cx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_options_and_ignores_unknown() {
        let launch = Launch::parse(["--theme", "dark", "--size", "1280x800", "--screen", "accounts", "--viewer", "B", "--bogus", "--household", "/tmp/x.atlas.sqlite", "--owner", "ada", "--take-over"].map(String::from));
        assert_eq!(launch.theme, "dark");
        assert_eq!((launch.width, launch.height), (1280., 800.));
        assert_eq!(launch.route, Route::Accounts);
        assert_eq!(launch.viewer, Some('b'));
        assert_eq!(launch.start, Start::File(PathBuf::from("/tmp/x.atlas.sqlite")));
        assert_eq!(launch.owner, "ada");
        assert!(launch.take_over);
        assert!(!launch.perf_overlay, "the overlay is off unless asked for");
        assert!(Launch::parse(["--perf-overlay"].map(String::from)).perf_overlay);
        assert!(!Launch::parse(["--perf-overlay", "--no-perf-overlay"].map(String::from)).perf_overlay);
        let empty = Launch::parse(["--new", "--viewer", "7", "--as-of", "2026-09-11"].map(String::from));
        assert_eq!(empty.start, Start::Empty);
        assert_eq!(empty.viewer_id, Some(7));
        assert_eq!(empty.as_of, NaiveDate::from_ymd_opt(2026, 9, 11));
    }
}
