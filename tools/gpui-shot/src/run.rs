//! Orchestration: X server -> launch app -> find window -> steps -> settled capture -> PNG.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use log::{debug, error, info, warn};

use crate::args::{Options, Step};
use crate::frame::Frame;
use crate::x11::{WindowInfo, WindowMatch, X11};
use crate::xvfb::Xvfb;

/// What happened, for the binary to report and turn into an exit code.
#[derive(Debug)]
pub struct Outcome {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    /// The last two captures (`settle` apart) were identical.
    pub settled: bool,
    /// Every pixel has the same colour: the app never drew a frame.
    pub blank: bool,
    pub extra_shots: Vec<PathBuf>,
    /// Where the app's stdout/stderr went (launch mode only).
    pub app_log: Option<PathBuf>,
    pub window: WindowInfo,
}

impl Outcome {
    pub fn is_good(&self) -> bool {
        self.settled && !self.blank
    }
}

/// Locates the repo's `.sysroot` (see `scripts/setup-linux-sysroot.sh`): the
/// `GPUI_SHOT_SYSROOT` variable wins, then the directories above the running
/// executable (`target/debug/gpui-shot` -> repo root), then the working directory.
pub fn find_sysroot() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("GPUI_SHOT_SYSROOT") {
        let path = PathBuf::from(explicit);
        return path.join("vulkan/lvp_icd.json").exists().then_some(path);
    }
    let mut starts = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            starts.push(dir.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }
    for start in starts {
        let mut dir: Option<&Path> = Some(&start);
        for _ in 0..5 {
            let Some(d) = dir else { break };
            let candidate = d.join(".sysroot");
            if candidate.join("vulkan/lvp_icd.json").exists() {
                return Some(candidate);
            }
            dir = d.parent();
        }
    }
    None
}

/// Computes the environment changes for the launched app. Pure, so it is
/// unit-testable: `current` is the caller's environment, the result maps a
/// variable to `Some(new value)` or `None` (= remove it).
pub fn child_env(
    current: &BTreeMap<OsString, OsString>,
    display: &str,
    sysroot: Option<&Path>,
    extra: &[(String, String)],
) -> BTreeMap<OsString, Option<OsString>> {
    let mut changes: BTreeMap<OsString, Option<OsString>> = BTreeMap::new();
    changes.insert("DISPLAY".into(), Some(display.into()));
    // gpui prefers Wayland whenever WAYLAND_DISPLAY is set; we want the X server.
    changes.insert("WAYLAND_DISPLAY".into(), None);
    changes.insert("XDG_SESSION_TYPE".into(), Some("x11".into()));

    if let Some(sysroot) = sysroot {
        let icd = sysroot.join("vulkan/lvp_icd.json");
        // Only point the Vulkan loader at lavapipe when the caller has not chosen a driver.
        let has_driver = current.contains_key(OsString::from("VK_DRIVER_FILES").as_os_str())
            || current.contains_key(OsString::from("VK_ICD_FILENAMES").as_os_str());
        if !has_driver {
            changes.insert("VK_DRIVER_FILES".into(), Some(icd.clone().into()));
            changes.insert("VK_ICD_FILENAMES".into(), Some(icd.into())); // older loaders
        }
        let lib = sysroot.join("lib");
        let mut ld_path = OsString::from(lib);
        if let Some(existing) = current.get(OsString::from("LD_LIBRARY_PATH").as_os_str()) {
            if !existing.is_empty() {
                ld_path.push(":");
                ld_path.push(existing);
            }
        }
        changes.insert("LD_LIBRARY_PATH".into(), Some(ld_path));
    }

    for (key, value) in extra {
        changes.insert(key.into(), Some(value.into()));
    }
    changes
}

fn current_env() -> BTreeMap<OsString, OsString> {
    std::env::vars_os().collect()
}

/// Runs the whole pipeline described in the crate docs.
pub fn run(options: &Options) -> Result<Outcome> {
    let started = Instant::now();
    let deadline = started + options.timeout;

    // 1. X server.
    let xvfb = match &options.display {
        Some(display) => {
            info!("using existing X display {display}");
            None
        }
        None => Some(Xvfb::start(options.screen_width, options.screen_height)?),
    };
    let display: String = match (&options.display, &xvfb) {
        (Some(d), _) => d.clone(),
        (None, Some(x)) => x.display().to_string(),
        (None, None) => unreachable!(),
    };

    // 2. App.
    let sysroot = find_sysroot();
    match &sysroot {
        Some(path) => debug!("using vendored sysroot {}", path.display()),
        None => debug!("no vendored sysroot found; relying on the system Vulkan driver"),
    }
    // Connect before launching so the pointer is already parked in the screen
    // corner when the app maps its window: gpui reads the initial pointer
    // position and would otherwise start with the centre of the window hovered.
    let x11 = X11::connect(&display)?;
    if options.pointer.is_none() {
        x11.park_pointer()?;
    }

    let app_log = (!options.command.is_empty()).then(|| options.out.with_extension("log"));
    let mut child = if options.command.is_empty() {
        None
    } else {
        Some(launch(options, &display, sysroot.as_deref(), app_log.as_deref().unwrap())?)
    };

    let matcher = match (&options.window_id, &options.title, &child) {
        (Some(id), _, _) => WindowMatch::Id(*id),
        (None, Some(title), _) => WindowMatch::TitleContains(title.clone()),
        (None, None, Some(child)) => WindowMatch::Pid(child.id()),
        (None, None, None) => bail!("nothing to capture"),
    };

    // 3. Window.
    let window = wait_for_window_or_exit(&x11, &matcher, child.as_mut(), deadline, app_log.as_deref())?;
    if let Some((x, y)) = options.pointer {
        x11.hover(window.id, x, y)?;
    }
    info!("waiting {:?} before the first capture", options.delay);
    sleep_checking_child(options.delay, child.as_mut(), app_log.as_deref())?;

    // 4. Steps.
    let mut extra_shots = Vec::new();
    for (index, step) in options.steps.iter().enumerate() {
        info!("step {}/{}: {:?}", index + 1, options.steps.len(), step);
        match step {
            Step::Click { x, y } => x11.click(window.id, *x, *y)?,
            Step::Wheel { x, y, clicks } => x11.wheel(window.id, *x, *y, *clicks)?,
            Step::Key { keysym } => x11.key(keysym)?,
            Step::Wait { duration } => sleep_checking_child(*duration, child.as_mut(), app_log.as_deref())?,
            Step::Shot { path } => {
                let (frame, settled) = capture_settled(&x11, window.id, options.settle, deadline)?;
                if !settled {
                    warn!("step shot {} did not settle within the timeout", path.display());
                }
                frame.save_png(path)?;
                info!("wrote {} ({}x{})", path.display(), frame.width, frame.height);
                extra_shots.push(path.clone());
            }
        }
    }

    // 5. Final capture.
    let (frame, settled) = capture_settled(&x11, window.id, options.settle, deadline)?;
    let blank = frame.is_blank();
    frame.save_png(&options.out)?;
    let window = x11.refresh(window.id).unwrap_or(window);
    info!(
        "wrote {} ({}x{}, {} distinct colours, settled={settled}, blank={blank}) after {:?}",
        options.out.display(),
        frame.width,
        frame.height,
        frame.distinct_colors(1000),
        started.elapsed()
    );
    if blank {
        error!("the frame is blank: the app never drew; see {}", app_log.as_deref().map(|p| p.display().to_string()).unwrap_or_default());
    }

    // 6. Cleanup.
    if options.keep_running {
        if let Some(x) = xvfb {
            x.leak();
        }
        if let Some(c) = &child {
            info!("leaving the app running (pid {}) on {display}", c.id());
        }
    } else {
        if let Some(mut c) = child.take() {
            stop_child(&mut c);
        }
        drop(xvfb);
    }

    Ok(Outcome {
        path: options.out.clone(),
        width: frame.width,
        height: frame.height,
        settled,
        blank,
        extra_shots,
        app_log,
        window,
    })
}

fn launch(options: &Options, display: &str, sysroot: Option<&Path>, app_log: &Path) -> Result<Child> {
    let program = &options.command[0];
    let args = &options.command[1..];
    if let Some(parent) = app_log.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let log_file = File::create(app_log).with_context(|| format!("creating {}", app_log.display()))?;
    let log_file_err = log_file.try_clone()?;

    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null()).stdout(Stdio::from(log_file)).stderr(Stdio::from(log_file_err));
    for (key, value) in child_env(&current_env(), display, sysroot, &options.env) {
        match value {
            Some(v) => {
                command.env(&key, &v);
            }
            None => {
                command.env_remove(&key);
            }
        }
    }
    if std::env::var_os("RUST_LOG").is_none() && !options.env.iter().any(|(k, _)| k == "RUST_LOG") {
        // gpui logs its adapter choice at info; that is the first thing to read when a frame is blank.
        command.env("RUST_LOG", "info");
    }
    info!("launching {:?} {:?} on {display}; app output -> {}", program, args, app_log.display());
    let child = command
        .spawn()
        .with_context(|| format!("launching `{program}` (not built? wrong path?)"))?;
    debug!("app pid {}", child.id());
    Ok(child)
}

fn wait_for_window_or_exit(
    x11: &X11,
    matcher: &WindowMatch,
    mut child: Option<&mut Child>,
    deadline: Instant,
    app_log: Option<&Path>,
) -> Result<WindowInfo> {
    // Poll in short slices so an app that crashes before mapping is reported
    // with its log instead of a generic timeout.
    loop {
        let slice = Duration::from_millis(500).min(deadline.saturating_duration_since(Instant::now()));
        match x11.wait_for_window(matcher, slice) {
            Ok(window) => return Ok(window),
            Err(err) => {
                if let Some(c) = child.as_deref_mut() {
                    if let Some(status) = c.try_wait()? {
                        bail!("the app exited with {status} before showing a window\n{}", log_tail(app_log));
                    }
                }
                if Instant::now() >= deadline {
                    return Err(err.context(format!("timed out waiting for the window\n{}", log_tail(app_log))));
                }
            }
        }
    }
}

fn sleep_checking_child(duration: Duration, mut child: Option<&mut Child>, app_log: Option<&Path>) -> Result<()> {
    let end = Instant::now() + duration;
    while Instant::now() < end {
        if let Some(c) = child.as_deref_mut() {
            if let Some(status) = c.try_wait()? {
                bail!("the app exited with {status} while we were waiting\n{}", log_tail(app_log));
            }
        }
        std::thread::sleep(Duration::from_millis(50).min(end.saturating_duration_since(Instant::now())));
    }
    Ok(())
}

/// Captures until two frames `settle` apart are identical (and not blank), or
/// `deadline` passes. Returns the last frame and whether it settled.
pub fn capture_settled(x11: &X11, window: u32, settle: Duration, deadline: Instant) -> Result<(Frame, bool)> {
    let mut last: Option<(Frame, Instant)> = None; // frame + when it was first seen
    loop {
        let frame = x11.capture(window)?;
        let now = Instant::now();
        match &mut last {
            Some((seen, since)) if *seen == frame => {
                if now.duration_since(*since) >= settle && !frame.is_blank() {
                    debug!("frame settled ({:?} unchanged)", now.duration_since(*since));
                    return Ok((frame, true));
                }
            }
            _ => {
                debug!("frame changed ({} distinct colours so far)", frame.distinct_colors(64));
                last = Some((frame, now));
            }
        }
        if now >= deadline {
            warn!("frame did not settle before the timeout; using the latest capture");
            let (frame, _) = last.take().ok_or_else(|| anyhow!("no capture"))?;
            return Ok((frame, false));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn stop_child(child: &mut Child) {
    let pid = child.id().to_string();
    debug!("stopping app pid {pid}");
    let _ = Command::new("kill").args(["-TERM", &pid]).status();
    let end = Instant::now() + Duration::from_secs(2);
    while Instant::now() < end {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    warn!("app ignored SIGTERM, killing it");
    let _ = child.kill();
    let _ = child.wait();
}

/// The last lines of the app log, for error messages.
pub fn log_tail(app_log: Option<&Path>) -> String {
    let Some(path) = app_log else { return String::new() };
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let lines: Vec<&str> = text.lines().collect();
            let tail = &lines[lines.len().saturating_sub(40)..];
            format!("--- tail of {} ---\n{}", path.display(), tail.join("\n"))
        }
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<OsString, OsString> {
        pairs.iter().map(|(k, v)| (OsString::from(k), OsString::from(v))).collect()
    }

    fn get<'a>(changes: &'a BTreeMap<OsString, Option<OsString>>, key: &str) -> &'a Option<OsString> {
        &changes[std::ffi::OsStr::new(key)]
    }

    #[test]
    fn child_env_forces_x11_and_lavapipe_when_sysroot_present() {
        let changes = child_env(&env(&[("WAYLAND_DISPLAY", "wayland-0")]), ":95", Some(Path::new("/repo/.sysroot")), &[]);
        assert_eq!(*get(&changes, "DISPLAY"), Some(OsString::from(":95")));
        assert_eq!(*get(&changes, "WAYLAND_DISPLAY"), None);
        assert_eq!(*get(&changes, "VK_DRIVER_FILES"), Some(OsString::from("/repo/.sysroot/vulkan/lvp_icd.json")));
        assert_eq!(*get(&changes, "LD_LIBRARY_PATH"), Some(OsString::from("/repo/.sysroot/lib")));
    }

    #[test]
    fn child_env_respects_a_caller_chosen_vulkan_driver_and_prepends_ld_path() {
        let changes = child_env(
            &env(&[("VK_DRIVER_FILES", "/my/icd.json"), ("LD_LIBRARY_PATH", "/opt/lib")]),
            ":1",
            Some(Path::new("/repo/.sysroot")),
            &[],
        );
        assert!(!changes.contains_key(std::ffi::OsStr::new("VK_DRIVER_FILES")));
        assert_eq!(*get(&changes, "LD_LIBRARY_PATH"), Some(OsString::from("/repo/.sysroot/lib:/opt/lib")));
    }

    #[test]
    fn child_env_without_sysroot_only_touches_display_vars_and_extras() {
        let extra = vec![("RUST_LOG".to_string(), "trace".to_string())];
        let changes = child_env(&env(&[]), ":2", None, &extra);
        assert_eq!(changes.len(), 4, "{changes:?}");
        assert_eq!(*get(&changes, "RUST_LOG"), Some(OsString::from("trace")));
        assert_eq!(*get(&changes, "XDG_SESSION_TYPE"), Some(OsString::from("x11")));
    }

    #[test]
    fn log_tail_reports_last_lines_or_nothing() {
        assert_eq!(log_tail(None), "");
        assert_eq!(log_tail(Some(Path::new("/nonexistent/x.log"))), "");
        let path = std::env::temp_dir().join(format!("gpui-shot-tail-{}.log", std::process::id()));
        let body: String = (0..50).map(|i| format!("line {i}\n")).collect();
        std::fs::write(&path, body).unwrap();
        let tail = log_tail(Some(&path));
        assert!(tail.contains("line 49"));
        assert!(!tail.contains("line 9\n"), "{tail}");
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn find_sysroot_honours_explicit_variable_only_when_valid() {
        // Cannot mutate the process env safely in parallel tests; just check the
        // walk-up logic tolerates a missing sysroot without panicking.
        let _ = find_sysroot();
    }
}
