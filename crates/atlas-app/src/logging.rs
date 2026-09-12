//! Logging to two sinks at once: stderr (as before) and a `logs.log` file the
//! customer can send back after a slow session.
//!
//! - The file is `ATLAS_LOG_FILE` when set, otherwise `logs.log` in the
//!   current directory (`cargo run` → the repository root). `ATLAS_LOG_FILE=off`
//!   disables it. An existing file is rotated to `logs.prev.log` first, so one
//!   run is one file and the previous run is not lost on a quick restart.
//! - Filters use the `RUST_LOG` syntax. stderr gets `RUST_LOG`, or `info`.
//!   The file gets `info` plus `debug` for the atlas crates (that is where the
//!   per-frame perf lines live) — `RUST_LOG`, when set, is layered on top of
//!   that (so `RUST_LOG=warn` quiets gpui in the file but keeps the perf
//!   detail), and `ATLAS_LOG_FILE_FILTER` replaces the file filter outright.
//! - Every line carries the wall-clock time with milliseconds and the seconds
//!   since process start, so log lines can be lined up with what the person
//!   was doing and with each other.
//!
//! The logger is process-global; [`init`] installs it once and reports where
//! the file went so the person knows what to send.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use log::{LevelFilter, Log, Metadata, Record};

/// Default file name, relative to the current directory.
pub const DEFAULT_FILE: &str = "logs.log";
/// Where the previous run's file goes.
pub const PREVIOUS_FILE: &str = "logs.prev.log";

/// Default filter for stderr when `RUST_LOG` is not set.
pub const DEFAULT_STDERR_FILTER: &str = "info";
/// Default filter for the file when neither `RUST_LOG` nor
/// `ATLAS_LOG_FILE_FILTER` is set: everything at info, plus the atlas crates'
/// debug lines (per-frame perf detail).
pub const DEFAULT_FILE_FILTER: &str = "info,atlas_app=debug,atlas_core=debug,atlas_store=debug";

static PROCESS_START: OnceLock<Instant> = OnceLock::new();

/// The instant the process (well, the logger) started; every log line and the
/// perf meter measure from it.
pub fn process_start() -> Instant {
    *PROCESS_START.get_or_init(Instant::now)
}

/// Seconds since [`process_start`].
pub fn uptime_secs() -> f64 {
    process_start().elapsed().as_secs_f64()
}

/// What [`init`] decided, for the startup banner and the tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogSetup {
    pub stderr_filter: String,
    pub file_filter: String,
    /// `None` when the file sink is off or could not be opened.
    pub file: Option<PathBuf>,
    /// Why the file sink is off, when it is.
    pub file_error: Option<String>,
}

/// The two-sink logger.
pub struct TeeLogger {
    stderr: env_filter::Filter,
    file: Option<FileSink>,
}

struct FileSink {
    filter: env_filter::Filter,
    file: Mutex<File>,
}

impl TeeLogger {
    /// Builds a logger from two filter strings and an optional file. The file
    /// is opened (and rotated) here; a failure to open it is returned so the
    /// caller can say so, and logging still works on stderr.
    pub fn new(stderr_filter: &str, file_filter: &str, file: Option<&Path>) -> (TeeLogger, LogSetup) {
        let stderr = env_filter::Builder::new().parse(stderr_filter).build();
        let mut setup = LogSetup { stderr_filter: stderr_filter.to_string(), file_filter: file_filter.to_string(), file: None, file_error: None };
        let file_sink = file.and_then(|path| match open_rotated(path) {
            Ok(file) => {
                setup.file = Some(path.to_path_buf());
                Some(FileSink { filter: env_filter::Builder::new().parse(file_filter).build(), file: Mutex::new(file) })
            }
            Err(err) => {
                setup.file_error = Some(format!("{}: {err}", path.display()));
                None
            }
        });
        (TeeLogger { stderr, file: file_sink }, setup)
    }

    /// The most permissive level either sink wants; `log` drops records above
    /// it before they reach us.
    pub fn max_level(&self) -> LevelFilter {
        let file_level = self.file.as_ref().map(|sink| sink.filter.filter()).unwrap_or(LevelFilter::Off);
        self.stderr.filter().max(file_level)
    }

    /// Formats one record the way both sinks print it.
    pub fn format(record: &Record<'_>) -> String {
        let now = chrono::Local::now();
        format!(
            "{} +{:>9.3}s {:<5} {}: {}",
            now.format("%Y-%m-%d %H:%M:%S%.3f"),
            uptime_secs(),
            record.level(),
            record.target(),
            record.args()
        )
    }
}

impl Log for TeeLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        self.stderr.enabled(metadata) || self.file.as_ref().is_some_and(|sink| sink.filter.enabled(metadata))
    }

    fn log(&self, record: &Record<'_>) {
        let to_stderr = self.stderr.matches(record);
        let to_file = self.file.as_ref().is_some_and(|sink| sink.filter.matches(record));
        if !to_stderr && !to_file {
            return;
        }
        let mut line = Self::format(record);
        if to_stderr {
            eprintln!("{line}");
        }
        if to_file
            && let Some(sink) = &self.file
            && let Ok(mut file) = sink.file.lock()
        {
            // One unbuffered write per record: the file is what the person
            // sends after a hang or a crash, so it must be current at every
            // moment, and a whole line per write keeps lines intact.
            line.push('\n');
            let _ = file.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {
        if let Some(sink) = &self.file
            && let Ok(mut file) = sink.file.lock()
        {
            let _ = file.flush();
        }
    }
}

/// Rotates `path` to `logs.prev.log` (same directory, `PREVIOUS_FILE` name
/// when the file is the default, `<stem>.prev.<ext>` otherwise) and opens a
/// fresh file.
fn open_rotated(path: &Path) -> std::io::Result<File> {
    if path.exists() {
        let previous = previous_path(path);
        if let Err(err) = std::fs::rename(path, &previous) {
            // Not fatal: fall through to truncating in place.
            eprintln!("logging: could not rotate {} to {}: {err}", path.display(), previous.display());
        }
    }
    OpenOptions::new().create(true).write(true).truncate(true).open(path)
}

/// `logs.log` → `logs.prev.log`; `x.txt` → `x.prev.txt`; `x` → `x.prev`.
pub fn previous_path(path: &Path) -> PathBuf {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("logs");
    let name = match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => format!("{stem}.prev.{ext}"),
        None => format!("{stem}.prev"),
    };
    path.with_file_name(name)
}

/// Resolves the file path from the environment: `ATLAS_LOG_FILE` (empty,
/// `off`, `0` or `none` disable the file), else `logs.log` in the current directory.
pub fn file_path_from_env() -> Option<PathBuf> {
    match std::env::var("ATLAS_LOG_FILE") {
        Ok(value) => {
            let value = value.trim();
            if value.is_empty() || matches!(value.to_ascii_lowercase().as_str(), "off" | "0" | "none" | "false") {
                None
            } else {
                Some(PathBuf::from(value))
            }
        }
        Err(_) => Some(PathBuf::from(DEFAULT_FILE)),
    }
}

/// The file filter: the default, with `RUST_LOG` layered on top when set. A
/// later directive wins for the same module and the global level, so
/// `RUST_LOG=warn` quiets everything else while `atlas_app=debug` stays.
pub fn file_filter_with(rust_log: Option<&str>) -> String {
    match rust_log {
        Some(rust_log) => format!("{DEFAULT_FILE_FILTER},{rust_log}"),
        None => DEFAULT_FILE_FILTER.to_string(),
    }
}

/// Installs the process-wide logger and prints the banner. Safe to call once;
/// a second call (tests) keeps the first logger and returns the fresh setup.
pub fn init() -> LogSetup {
    process_start();
    let rust_log = std::env::var("RUST_LOG").ok().filter(|v| !v.trim().is_empty());
    let stderr_filter = rust_log.clone().unwrap_or_else(|| DEFAULT_STDERR_FILTER.to_string());
    let file_filter = std::env::var("ATLAS_LOG_FILE_FILTER")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| file_filter_with(rust_log.as_deref()));
    let path = file_path_from_env();
    let (logger, setup) = TeeLogger::new(&stderr_filter, &file_filter, path.as_deref());
    let max_level = logger.max_level();
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(max_level);
    }
    banner(&setup);
    setup
}

/// The first lines of every log: what was built, how, and where the log goes.
fn banner(setup: &LogSetup) {
    log::info!(
        "Atlas Financer {} — build profile={} opt-level(atlas crates)={} target={}/{} debug_assertions={}",
        env!("CARGO_PKG_VERSION"),
        env!("ATLAS_BUILD_PROFILE"),
        env!("ATLAS_BUILD_OPT_LEVEL"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        cfg!(debug_assertions)
    );
    log::info!("args: {:?}", std::env::args().skip(1).collect::<Vec<_>>());
    match (&setup.file, &setup.file_error) {
        (Some(path), _) => {
            let absolute = std::env::current_dir().map(|dir| dir.join(path)).unwrap_or_else(|_| path.clone());
            log::info!("log file: {} (filter: {}; previous run in {})", absolute.display(), setup.file_filter, previous_path(path).display());
        }
        (None, Some(err)) => log::warn!("log file could not be opened, logging to stderr only: {err}"),
        (None, None) => log::info!("log file: off (ATLAS_LOG_FILE); stderr filter: {}", setup.stderr_filter),
    }
    log::info!("stderr filter: {} — set RUST_LOG to change both sinks, ATLAS_LOG_FILE_FILTER for the file alone", setup.stderr_filter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::Level;

    fn record<'a>(level: Level, target: &'a str, args: std::fmt::Arguments<'a>) -> Record<'a> {
        Record::builder().level(level).target(target).args(args).build()
    }

    #[test]
    fn writes_matching_records_to_the_file_and_rotates() {
        let dir = std::env::temp_dir().join(format!("atlas-logging-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("logs.log");
        std::fs::write(&path, "old run\n").unwrap();

        let (logger, setup) = TeeLogger::new("warn", "info,atlas_app=debug", Some(&path));
        assert_eq!(setup.file.as_deref(), Some(path.as_path()));
        assert_eq!(setup.file_error, None);
        assert_eq!(logger.max_level(), LevelFilter::Debug, "the file wants atlas_app debug");
        assert_eq!(std::fs::read_to_string(dir.join("logs.prev.log")).unwrap(), "old run\n", "previous run rotated");

        logger.log(&record(Level::Debug, "atlas_app::perf", format_args!("frame #1 build=2.0ms")));
        logger.log(&record(Level::Debug, "wgpu_core", format_args!("noise")));
        logger.log(&record(Level::Info, "gpui", format_args!("kept")));
        logger.flush();

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("DEBUG atlas_app::perf: frame #1 build=2.0ms"), "{text}");
        assert!(text.contains("INFO  gpui: kept"), "{text}");
        assert!(!text.contains("noise"), "wgpu debug is filtered out of the file: {text}");
        let first = text.lines().next().unwrap();
        // "2026-09-12 03:29:01.123 +    0.001s DEBUG atlas_app::perf: …"
        assert!(first.len() > 30 && first.as_bytes()[19] == b'.' && first.contains("s DEBUG"), "timestamp with ms and uptime: {first}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unopenable_file_degrades_to_stderr() {
        let path = Path::new("/nonexistent-dir-for-atlas/logs.log");
        let (logger, setup) = TeeLogger::new("info", "debug", Some(path));
        assert_eq!(setup.file, None);
        assert!(setup.file_error.as_deref().unwrap_or("").contains("nonexistent-dir-for-atlas"));
        assert_eq!(logger.max_level(), LevelFilter::Info);
        // Must not panic without a file.
        logger.log(&record(Level::Info, "atlas_app", format_args!("still fine")));
    }

    #[test]
    fn rust_log_layers_on_the_file_filter_without_losing_perf_detail() {
        let dir = std::env::temp_dir().join(format!("atlas-logging-layer-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("logs.log");
        let filter = file_filter_with(Some("warn"));
        assert_eq!(filter, "info,atlas_app=debug,atlas_core=debug,atlas_store=debug,warn");
        let (logger, _) = TeeLogger::new("warn", &filter, Some(&path));
        logger.log(&record(Level::Debug, "atlas_app::perf", format_args!("frame #7")));
        logger.log(&record(Level::Info, "gpui", format_args!("quieted by RUST_LOG=warn")));
        logger.log(&record(Level::Warn, "wgpu", format_args!("still warned")));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("frame #7"), "{text}");
        assert!(!text.contains("quieted"), "{text}");
        assert!(text.contains("still warned"), "{text}");
        assert_eq!(file_filter_with(None), DEFAULT_FILE_FILTER);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn previous_path_keeps_the_extension() {
        assert_eq!(previous_path(Path::new("logs.log")), PathBuf::from("logs.prev.log"));
        assert_eq!(previous_path(Path::new("/tmp/run.txt")), PathBuf::from("/tmp/run.prev.txt"));
        assert_eq!(previous_path(Path::new("trace")), PathBuf::from("trace.prev"));
    }
}
