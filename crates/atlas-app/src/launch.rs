//! Command-line options and theme selection.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::App;

use crate::screens::Section;

/// What the command line asked for.
#[derive(Debug, Clone)]
pub struct Launch {
    /// `light` or `dark`.
    pub theme: String,
    pub width: f32,
    pub height: f32,
    /// The sidebar section to open with.
    pub section: Section,
    /// Which fixture person is looking (`a` or `b`).
    pub viewer: char,
}

impl Default for Launch {
    fn default() -> Self {
        Launch {
            theme: "light".into(),
            width: 1600.,
            height: 1000.,
            section: Section::Household,
            viewer: 'a',
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
                    launch.viewer = args.get(i).and_then(|s| s.chars().next()).map(|c| c.to_ascii_lowercase()).unwrap_or('a');
                }
                "-h" | "--help" => {
                    println!(
                        "atlas [--theme light|dark] [--size WxH] [--screen {}] [--viewer a|b]",
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
        let launch = Launch::parse(["--theme", "dark", "--size", "1280x800", "--screen", "accounts", "--viewer", "B", "--bogus"].map(String::from));
        assert_eq!(launch.theme, "dark");
        assert_eq!((launch.width, launch.height), (1280., 800.));
        assert_eq!(launch.section, Section::Accounts);
        assert_eq!(launch.viewer, 'b');
    }
}
