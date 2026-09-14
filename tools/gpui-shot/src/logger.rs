//! The binary's stderr logger: `[  +1.234s INFO  gpui_shot::run] message`.
//!
//! Hand-rolled for the same reason the argument parser is: `env_logger` would
//! add a dozen transitive crates (anstream, jiff, …) to the product's lock file
//! and to every cold build on the 2 GiB box, for a tool that prints a few
//! dozen lines per run. Elapsed time since start is more useful here than a
//! wall clock anyway: it says how long the window took to appear and settle.

use std::io::Write;
use std::sync::OnceLock;
use std::time::Instant;

use log::{LevelFilter, Log, Metadata, Record};

/// A `static` logger (`log::set_logger` needs a `'static` reference and, unlike
/// `set_boxed_logger`, no `std` feature on `log`); the start time lives beside it.
static LOGGER: StderrLogger = StderrLogger;
static STARTED: OnceLock<Instant> = OnceLock::new();

struct StderrLogger;

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let elapsed = STARTED.get().map(Instant::elapsed).unwrap_or_default().as_secs_f64();
        let stderr = std::io::stderr();
        let mut out = stderr.lock();
        let _ = writeln!(out, "[{elapsed:>+8.3}s {:<5} {}] {}", record.level(), record.target(), record.args());
    }

    fn flush(&self) {
        let _ = std::io::stderr().flush();
    }
}

/// Installs the logger at `level`. Calling it twice is harmless (the second
/// call is ignored by the `log` crate).
pub fn init(level: LevelFilter) {
    STARTED.get_or_init(Instant::now);
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(level);
    }
}

/// The level `RUST_LOG` asks for, or `default` when it is unset or says
/// nothing this logger understands. Accepted: a level name (`trace`, `debug`,
/// `info`, `warn`, `error`, `off`) or `env_logger`-style directives such as
/// `gpui_shot=debug,info`, of which only the levels count — the most verbose
/// one wins, so a filter meant for a richer logger never hides output here.
pub fn level_from_env(default: LevelFilter) -> LevelFilter {
    match std::env::var("RUST_LOG") {
        Ok(value) => parse_filter(&value).unwrap_or(default),
        Err(_) => default,
    }
}

/// See [`level_from_env`]; `None` when no directive names a known level.
pub fn parse_filter(filter: &str) -> Option<LevelFilter> {
    filter
        .split(',')
        .map(|directive| directive.rsplit('=').next().unwrap_or("").trim().to_ascii_lowercase())
        .filter_map(|level| match level.as_str() {
            "trace" => Some(LevelFilter::Trace),
            "debug" => Some(LevelFilter::Debug),
            "info" => Some(LevelFilter::Info),
            "warn" | "warning" => Some(LevelFilter::Warn),
            "error" => Some(LevelFilter::Error),
            "off" => Some(LevelFilter::Off),
            _ => None,
        })
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_levels_and_directives() {
        assert_eq!(parse_filter("debug"), Some(LevelFilter::Debug));
        assert_eq!(parse_filter(" WARN "), Some(LevelFilter::Warn));
        assert_eq!(parse_filter("gpui_shot=trace"), Some(LevelFilter::Trace));
        assert_eq!(parse_filter("gpui_shot::x11=debug,info"), Some(LevelFilter::Debug));
        assert_eq!(parse_filter("off"), Some(LevelFilter::Off));
    }

    #[test]
    fn unknown_filters_fall_back_to_the_default() {
        assert_eq!(parse_filter(""), None);
        assert_eq!(parse_filter("gpui_shot"), None);
        assert_eq!(parse_filter("loud,louder"), None);
    }
}
