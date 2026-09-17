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
    /// `--open <slug>`, repeatable: further screens opened as panes at launch,
    /// each a new column at the right edge of the window, all columns sharing
    /// the width equally. Screenshots and tests build multi-pane workspaces with it.
    pub extra: Vec<Route>,
    /// `--open +<slug>`: screens opened as a **tab** of an earlier pane rather
    /// than beside it. The index counts the panes opened at launch in order
    /// (`route` is 0, the first `--open` is 1, …); the tab goes onto the pane
    /// opened just before the flag, so `--open accounts --open +rules` stacks
    /// Rules onto Accounts. Applied after every `extra` pane exists.
    pub stacked: Vec<(usize, Route)>,
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
    /// The chip that follows a dragged pane rides in a transparent window
    /// of its own, above every window. On unless `ATLAS_DRAG_GHOST_WINDOW=0`
    /// (a bare X server without a compositor shows a transparent window as
    /// black); off, the chip is drawn inside the window that owns the drag.
    pub drag_ghost_window: bool,
    /// Where the app keeps its own files (the launcher's arrangement,
    /// workspace sessions, saved layouts): `--data-dir`, else
    /// `ATLAS_DATA_DIR`, else the platform's per-user application directory.
    /// `None` — the default for a `Launch` built in code, as the tests do —
    /// keeps everything for this run only and writes nothing.
    pub data_dir: Option<std::path::PathBuf>,
    /// `--screen` or `--open` was given: the person asked for that screen,
    /// so the household's saved session is not restored over it.
    pub explicit_screen: bool,
}

/// The app's directory name inside the platform's per-user application directory.
pub const APP_DIR: &str = "atlas-financer";
/// The environment variable that overrides where the app keeps its files.
pub const DATA_DIR_ENV: &str = "ATLAS_DATA_DIR";

impl Default for Launch {
    fn default() -> Self {
        Launch {
            theme: "light".into(),
            width: 1600.,
            height: 1000.,
            route: Route::Today,
            detail: None,
            extra: Vec::new(),
            stacked: Vec::new(),
            viewer: None,
            viewer_id: None,
            start: Start::Welcome,
            data_dir: None,
            explicit_screen: false,
            as_of: None,
            owner: std::env::var("USER").unwrap_or_else(|_| "user".into()),
            take_over: false,
            perf_overlay: std::env::var("ATLAS_PERF_OVERLAY").map(|v| matches!(v.trim(), "1" | "on" | "true" | "yes")).unwrap_or(false),
            drag_ghost_window: std::env::var("ATLAS_DRAG_GHOST_WINDOW").map(|v| !matches!(v.trim(), "0" | "off" | "false" | "no")).unwrap_or(true),
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
                    launch.explicit_screen = true;
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
                "--open" => {
                    launch.explicit_screen = true;
                    i += 1;
                    match args.get(i) {
                        Some(value) => {
                            let (as_tab, slug) = match value.strip_prefix('+') {
                                Some(slug) => (true, slug),
                                None => (false, value.as_str()),
                            };
                            match Route::from_slug(slug) {
                                Some(route) if as_tab => {
                                    // The pane opened just before this flag: the
                                    // launch route counts as pane 0.
                                    let onto = launch.extra.len();
                                    launch.stacked.push((onto, route));
                                }
                                Some(route) => launch.extra.push(route),
                                None => log::warn!("unknown --open {value:?}; known: {} (prefix with + to open as a tab of the previous pane)", Route::slugs().join(", ")),
                            }
                        }
                        None => log::warn!("--open needs a screen slug"),
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
                "--data-dir" => {
                    i += 1;
                    match args.get(i) {
                        Some(dir) => launch.data_dir = Some(std::path::PathBuf::from(dir)),
                        None => log::warn!("--data-dir needs a directory"),
                    }
                }
                "-h" | "--help" => {
                    println!(
                        "atlas [--theme light|dark] [--size WxH] [--screen {}] [--open <slug>|+<slug>]... [--viewer a|b|<person id>] [--household FILE.atlas.sqlite | --new | --sample] [--as-of YYYY-MM-DD] [--owner NAME] [--take-over] [--perf-overlay] [--data-dir DIR]\n\n--open adds a pane to the right of the previous one; +slug adds it as a tab of the previous pane.\n--data-dir is where the app keeps the launcher's arrangement and its workspaces (default: ATLAS_DATA_DIR, else the per-user application directory).\nLogs go to stderr and logs.log (ATLAS_LOG_FILE=path|off, RUST_LOG=filter, ATLAS_LOG_FILE_FILTER=filter); the status bar shows gpui's frame timing.",
                        [Route::slugs(), crate::nav::FirstDetail::slugs()].concat().join("|")
                    );
                    std::process::exit(0);
                }
                other => log::warn!("ignoring unknown argument {other}"),
            }
            i += 1;
        }
        // A launch from the command line keeps its files somewhere; one built
        // in code (the tests) keeps nothing unless it says where.
        if launch.data_dir.is_none() {
            launch.data_dir = Some(atlas_workspace::persist::app_data_dir(APP_DIR, DATA_DIR_ENV));
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
        assert!(Launch::default().data_dir.is_none(), "a launch built in code keeps nothing on disk");
        assert!(Launch::parse(Vec::<String>::new()).data_dir.is_some(), "a command-line launch always has a data directory");
        assert_eq!(Launch::parse(["--data-dir", "/tmp/atlas-here"].map(String::from)).data_dir, Some(std::path::PathBuf::from("/tmp/atlas-here")));
        let panes = Launch::parse(["--screen", "today", "--open", "accounts", "--open", "+rules", "--open", "forecast", "--open", "nonsense"].map(String::from));
        assert_eq!(panes.extra, vec![Route::Accounts, Route::ForecastPath], "unknown slugs are skipped");
        assert_eq!(panes.stacked, vec![(1, Route::Rules)], "a +slug stacks onto the pane opened just before it");
        let empty = Launch::parse(["--new", "--viewer", "7", "--as-of", "2026-09-11"].map(String::from));
        assert_eq!(empty.start, Start::Empty);
        assert_eq!(empty.viewer_id, Some(7));
        assert_eq!(empty.as_of, NaiveDate::from_ymd_opt(2026, 9, 11));
    }
}
