//! End-to-end checks: a real Xvfb, a real gpui window, real pixels.
//!
//! They need `Xvfb` on the box and the `atlas` binary already built
//! (`cargo build -p atlas-app`), so they only run when explicitly asked for:
//!
//!     GPUI_SHOT_E2E=1 cargo test -p gpui-shot --test e2e
//!
//! Without the variable every test here passes trivially after printing why.

use std::path::PathBuf;
use std::time::Duration;

use gpui_shot::{Options, Step, run};

fn enabled() -> bool {
    if std::env::var_os("GPUI_SHOT_E2E").is_some() {
        true
    } else {
        eprintln!("skipped: set GPUI_SHOT_E2E=1 to run the Xvfb end-to-end tests");
        false
    }
}

/// The product binary, in the same target directory this test was built into
/// (`CARGO_TARGET_DIR` when set, `<repo>/target` otherwise).
fn atlas_binary() -> PathBuf {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|| repo.join("target"));
    let atlas = target.join("debug/atlas");
    assert!(atlas.exists(), "build the app first: cargo build -p atlas-app (missing {})", atlas.display());
    atlas
}

fn out_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gpui-shot-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn png_size(path: &std::path::Path) -> (u32, u32) {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "not a PNG: {}", path.display());
    let w = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let h = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    (w, h)
}

/// `atlas` arguments that open the sample household straight on the Today
/// screen, for one viewer, with the window filling the virtual screen.
fn sample_today(theme: &str, size: &str) -> Vec<String> {
    [
        atlas_binary().display().to_string(),
        "--theme".into(),
        theme.into(),
        "--size".into(),
        size.into(),
        "--sample".into(),
        "--viewer".into(),
        "a".into(),
        "--screen".into(),
        "today".into(),
    ]
    .to_vec()
}

#[test]
fn captures_a_settled_frame_of_the_app() {
    if !enabled() {
        return;
    }
    let out = out_dir().join("dark.png");
    let options = Options {
        command: sample_today("dark", "1200x760"),
        out: out.clone(),
        screen_width: 1200,
        screen_height: 760,
        timeout: Duration::from_secs(120),
        ..Options::default()
    };
    let outcome = run(&options).expect("capture failed");
    assert!(outcome.is_good(), "frame should be settled and non-blank: {outcome:?}");
    assert_eq!((outcome.width, outcome.height), (1200, 760));
    assert_eq!(png_size(&out), (1200, 760));
    assert_eq!(outcome.window.title, "Atlas Financer");
    assert!(outcome.app_log.as_ref().unwrap().exists(), "the app log must be written next to the PNG");
}

#[test]
fn steps_can_open_a_sheet_and_take_an_extra_shot() {
    if !enabled() {
        return;
    }
    let dir = out_dir();
    let mid = dir.join("before-click.png");
    let options = Options {
        command: sample_today("light", "1600x1000"),
        out: dir.join("sheet.png"),
        timeout: Duration::from_secs(120),
        steps: vec![
            Step::Shot { path: mid.clone() },
            // Today's leading figure at 1600x1000: clicking it opens the calculation sheet
            // (the same coordinates `scripts/shoot.sh` uses for `explain-free-cash-*`).
            Step::Click { x: 560, y: 200 },
            Step::Wait { duration: Duration::from_millis(800) },
        ],
        ..Options::default()
    };
    let outcome = run(&options).expect("capture failed");
    assert!(!outcome.blank, "{outcome:?}");
    assert_eq!(outcome.extra_shots, vec![mid.clone()]);
    let before = std::fs::read(&mid).unwrap();
    let after = std::fs::read(&outcome.path).unwrap();
    assert_ne!(before, after, "clicking the figure must change the frame (sheet opened)");
}

#[test]
fn reports_an_app_that_exits_before_showing_a_window() {
    if !enabled() {
        return;
    }
    let options = Options {
        command: vec!["/bin/true".into()],
        out: out_dir().join("never.png"),
        timeout: Duration::from_secs(20),
        ..Options::default()
    };
    let err = run(&options).expect_err("a program without a window must fail");
    let text = format!("{err:#}");
    assert!(text.contains("exited"), "unexpected error: {text}");
}

#[test]
fn reports_a_missing_program_clearly() {
    if !enabled() {
        return;
    }
    let options = Options {
        command: vec!["/definitely/not/here".into()],
        out: out_dir().join("missing.png"),
        timeout: Duration::from_secs(10),
        ..Options::default()
    };
    let err = run(&options).expect_err("missing program must fail");
    assert!(format!("{err:#}").contains("launching"), "{err:#}");
}
