//! Command-line options and theme selection.

use std::path::PathBuf;

use chrono::NaiveDate;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::App;

use crate::screens::Section;

/// Which household the app starts with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Start {
    /// The fictitious plan household (default).
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
    /// The sidebar section to open with.
    pub section: Section,
    /// Which fixture person is looking (`a` or `b`); `--viewer <person id>` for real households.
    pub viewer: char,
    /// Person id to view as (overrides `viewer` when given).
    pub viewer_id: Option<u32>,
    pub start: Start,
    /// Reconciliation date for a new empty household.
    pub as_of: Option<NaiveDate>,
    /// Lock owner name (defaults to the OS user).
    pub owner: String,
    /// Take over another owner's lock on the household file.
    pub take_over: bool,
}

impl Default for Launch {
    fn default() -> Self {
        Launch {
            theme: "light".into(),
            width: 1600.,
            height: 1000.,
            section: Section::Household,
            viewer: 'a',
            viewer_id: None,
            start: Start::Sample,
            as_of: None,
            owner: std::env::var("USER").unwrap_or_else(|_| "user".into()),
            take_over: false,
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
                    match args.get(i).and_then(|s| Section::from_slug(s)) {
                        Some(section) => launch.section = section,
                        None => log::warn!("unknown --screen {:?}; known: {}", args.get(i), Section::slugs().join(", ")),
                    }
                }
                "--viewer" => {
                    i += 1;
                    match args.get(i) {
                        Some(value) if value.chars().all(|c| c.is_ascii_digit()) => launch.viewer_id = value.parse().ok(),
                        Some(value) => launch.viewer = value.chars().next().map(|c| c.to_ascii_lowercase()).unwrap_or('a'),
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
                "-h" | "--help" => {
                    println!(
                        "atlas [--theme light|dark] [--size WxH] [--screen {}] [--viewer a|b|<person id>] [--household FILE.atlas.sqlite | --new | --sample] [--as-of YYYY-MM-DD] [--owner NAME] [--take-over]",
                        Section::slugs().join("|")
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
                log::warn!("unknown theme {other:?}; using light (bundled JSON themes are an open decision, see docs/ui-implementation-plan.md §6)");
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
        assert_eq!(launch.section, Section::Accounts);
        assert_eq!(launch.viewer, 'b');
        assert_eq!(launch.start, Start::File(PathBuf::from("/tmp/x.atlas.sqlite")));
        assert_eq!(launch.owner, "ada");
        assert!(launch.take_over);
        let empty = Launch::parse(["--new", "--viewer", "7", "--as-of", "2026-09-11"].map(String::from));
        assert_eq!(empty.start, Start::Empty);
        assert_eq!(empty.viewer_id, Some(7));
        assert_eq!(empty.as_of, NaiveDate::from_ymd_opt(2026, 9, 11));
    }
}
