//! Command-line parsing. Hand-rolled on purpose: the tool has a dozen flags and
//! we would rather not add `clap` (and its compile time) to a 2 GiB build box.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};

/// One scripted interaction performed after the window is up and before the
/// final capture. Lets a caller reach "special scenarios" (an open dialog, a
/// hovered row, a second page) without a window manager or xdotool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Move the pointer to window-relative `(x, y)` and click button 1.
    Click { x: i16, y: i16 },
    /// Press and release one key by X keysym name (`Return`, `Escape`, `Tab`, `a`, ...).
    Key { keysym: String },
    /// Sleep for the given time (lets animations/async work finish).
    Wait { duration: Duration },
    /// Capture the window as it is right now into `path` (in addition to the final `--out`).
    Shot { path: PathBuf },
    /// Move the pointer to window-relative `(x, y)` and turn the wheel `clicks`
    /// notches: positive scrolls down (button 5), negative up (button 4).
    /// Note: gpui's X11 backend reads XI2 scroll valuators, which XTEST legacy
    /// wheel buttons do not produce, so gpui windows ignore this; use a taller
    /// `--size` to photograph long pages instead.
    Wheel { x: i16, y: i16, clicks: i32 },
    /// Press button 1 at window-relative `(x1, y1)`, move to `(x2, y2)` in a
    /// few increments and keep holding — a drag in flight, for photographing
    /// drop indicators and previews. A later [`Step::Release`] drops.
    Drag { x1: i16, y1: i16, x2: i16, y2: i16 },
    /// Release button 1 where the pointer is, ending a [`Step::Drag`].
    Release,
}

impl Step {
    /// Parses `click:X,Y`, `key:NAME`, `wait:MS`, `shot:PATH`, `wheel:X,Y,CLICKS`,
    /// `drag:X1,Y1,X2,Y2` and `release`.
    pub fn parse(spec: &str) -> Result<Step> {
        if spec == "release" {
            return Ok(Step::Release);
        }
        let (kind, rest) = spec
            .split_once(':')
            .ok_or_else(|| anyhow!("step `{spec}` must look like kind:value (click:100,200 / key:Return / wait:500 / shot:file.png / drag:10,20,300,400 / release)"))?;
        match kind {
            "drag" => {
                let parts: Vec<&str> = rest.split(',').map(str::trim).collect();
                if parts.len() != 4 {
                    bail!("drag step `{spec}` needs X1,Y1,X2,Y2");
                }
                let coordinate = |index: usize, name: &str| parts[index].parse::<i16>().with_context(|| format!("bad {name} in `{spec}`"));
                Ok(Step::Drag {
                    x1: coordinate(0, "X1")?,
                    y1: coordinate(1, "Y1")?,
                    x2: coordinate(2, "X2")?,
                    y2: coordinate(3, "Y2")?,
                })
            }
            "release" => bail!("release step takes no value: write `release`"),
            "click" => {
                let (x, y) = rest
                    .split_once(',')
                    .ok_or_else(|| anyhow!("click step `{spec}` needs X,Y"))?;
                Ok(Step::Click {
                    x: x.trim().parse().with_context(|| format!("bad X in `{spec}`"))?,
                    y: y.trim().parse().with_context(|| format!("bad Y in `{spec}`"))?,
                })
            }
            "key" => {
                if rest.is_empty() {
                    bail!("key step `{spec}` needs a keysym name");
                }
                Ok(Step::Key {
                    keysym: rest.to_string(),
                })
            }
            "wait" => {
                let ms: u64 = rest
                    .parse()
                    .with_context(|| format!("wait step `{spec}` needs milliseconds"))?;
                Ok(Step::Wait {
                    duration: Duration::from_millis(ms),
                })
            }
            "shot" => {
                if rest.is_empty() {
                    bail!("shot step `{spec}` needs a file path");
                }
                Ok(Step::Shot {
                    path: PathBuf::from(rest),
                })
            }
            "wheel" => {
                let parts: Vec<&str> = rest.split(',').map(str::trim).collect();
                if parts.len() != 3 {
                    bail!("wheel step `{spec}` needs X,Y,CLICKS (positive = down)");
                }
                Ok(Step::Wheel {
                    x: parts[0].parse().with_context(|| format!("bad X in `{spec}`"))?,
                    y: parts[1].parse().with_context(|| format!("bad Y in `{spec}`"))?,
                    clicks: parts[2].parse().with_context(|| format!("bad CLICKS in `{spec}`"))?,
                })
            }
            other => bail!("unknown step kind `{other}` in `{spec}`"),
        }
    }
}

/// Everything the tool needs to know for one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Program to launch plus its arguments. Empty when `window_id` is given
    /// (capture-only mode against an already running app).
    pub command: Vec<String>,
    /// Where the final PNG goes.
    pub out: PathBuf,
    /// Use this existing X display instead of starting an Xvfb.
    pub display: Option<String>,
    /// Xvfb screen size (only used when we start Xvfb ourselves).
    pub screen_width: u16,
    pub screen_height: u16,
    /// Pick the window whose title contains this instead of matching `_NET_WM_PID`.
    pub title: Option<String>,
    /// Capture this exact X window id (hex `0x...` or decimal); no program is launched.
    pub window_id: Option<u32>,
    /// Give up on the window/settling after this long.
    pub timeout: Duration,
    /// Time to wait after the window is mapped before the first capture.
    pub delay: Duration,
    /// Two captures this far apart must be identical to count as "settled".
    pub settle: Duration,
    /// Scripted interactions before the final capture.
    pub steps: Vec<Step>,
    /// Extra environment for the child, `KEY=VALUE`.
    pub env: Vec<(String, String)>,
    /// Where to put the pointer before capturing, window-relative. `None` parks
    /// it in the bottom-right corner of the screen so hover effects (tooltips,
    /// row highlights) do not leak into a shot that did not ask for them.
    pub pointer: Option<(i16, i16)>,
    /// Leave the app (and Xvfb) running after the capture.
    pub keep_running: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            command: Vec::new(),
            out: PathBuf::from("shot.png"),
            display: None,
            screen_width: 1600,
            screen_height: 1000,
            title: None,
            window_id: None,
            timeout: Duration::from_secs(90),
            delay: Duration::from_millis(1000),
            settle: Duration::from_millis(600),
            steps: Vec::new(),
            env: Vec::new(),
            pointer: None,
            keep_running: false,
        }
    }
}

pub const USAGE: &str = "\
gpui-shot — screenshot a gpui window on a headless Linux box

USAGE:
    gpui-shot [OPTIONS] -- <program> [args...]
    gpui-shot [OPTIONS] --display :N --window-id 0x1c00001      (capture an existing window)

OPTIONS:
    --out <file.png>       output file                                   [default: shot.png]
    --display <:N>         use this X display instead of starting Xvfb
    --size <WxH>           Xvfb screen size                              [default: 1600x1000]
    --title <substr>       pick the window whose title contains this (default: by child PID)
    --window-id <id>       capture this X window id; no program is launched
    --timeout <secs>       max time to wait for the window + a settled frame [default: 90]
    --delay <ms>           wait after the window appears before capturing [default: 1000]
    --settle <ms>          frame must be unchanged for this long          [default: 600]
    --step <spec>          scripted action before the final shot; repeatable, in order:
                             click:X,Y   key:Return   wait:500   shot:extra.png   wheel:X,Y,CLICKS
                             drag:X1,Y1,X2,Y2 (press, move, keep holding)   release (drop)
    --env KEY=VALUE        extra environment for the program; repeatable
    --pointer <X,Y>        hover the pointer at window-relative X,Y before capturing
                           (default: parked in the screen corner, so no hover effects)
    --keep-running         do not kill the program / Xvfb afterwards
    -v, --verbose          debug logging (same as RUST_LOG=debug)
    -h, --help             this text

EXIT CODES:
    0  captured a settled, non-blank frame
    3  a frame was written but it never settled or is blank (check the app log)
    1  error (no window, X server failed, ...)

ENVIRONMENT PASSED TO THE PROGRAM:
    DISPLAY (the Xvfb display), WAYLAND_DISPLAY removed so gpui picks X11,
    VK_DRIVER_FILES -> the vendored lavapipe ICD when the loader has no driver,
    LD_LIBRARY_PATH += <repo>/.sysroot/lib (vendored libxkbcommon-x11).
";

/// Parses `args` (without the program name). `--` separates our flags from
/// the program to launch; everything after it is the child's command line.
pub fn parse<I, S>(args: I) -> Result<Options>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args: Vec<String> = args.into_iter().map(|a| a.as_ref().to_string()).collect();
    let mut options = Options::default();
    let mut verbose = false;
    let mut i = 0;

    fn value<'a>(args: &'a [String], i: &mut usize, flag: &str) -> Result<&'a str> {
        *i += 1;
        args.get(*i)
            .map(String::as_str)
            .ok_or_else(|| anyhow!("{flag} needs a value"))
    }

    while i < args.len() {
        let arg = args[i].as_str();
        match arg {
            "--" => {
                options.command = args[i + 1..].to_vec();
                break;
            }
            "-h" | "--help" => bail!("{USAGE}"),
            "-v" | "--verbose" => verbose = true,
            "--keep-running" => options.keep_running = true,
            "--out" => options.out = PathBuf::from(value(&args, &mut i, arg)?),
            "--display" => options.display = Some(value(&args, &mut i, arg)?.to_string()),
            "--title" => options.title = Some(value(&args, &mut i, arg)?.to_string()),
            "--size" => {
                let (w, h) = parse_size(value(&args, &mut i, arg)?)?;
                options.screen_width = w;
                options.screen_height = h;
            }
            "--window-id" => options.window_id = Some(parse_window_id(value(&args, &mut i, arg)?)?),
            "--timeout" => {
                let secs: u64 = value(&args, &mut i, arg)?.parse().context("--timeout needs seconds")?;
                options.timeout = Duration::from_secs(secs);
            }
            "--delay" => {
                let ms: u64 = value(&args, &mut i, arg)?.parse().context("--delay needs milliseconds")?;
                options.delay = Duration::from_millis(ms);
            }
            "--settle" => {
                let ms: u64 = value(&args, &mut i, arg)?.parse().context("--settle needs milliseconds")?;
                options.settle = Duration::from_millis(ms);
            }
            "--step" => options.steps.push(Step::parse(value(&args, &mut i, arg)?)?),
            "--pointer" => {
                let spec = value(&args, &mut i, arg)?;
                let (x, y) = spec.split_once(',').ok_or_else(|| anyhow!("--pointer needs X,Y, got `{spec}`"))?;
                options.pointer = Some((
                    x.trim().parse().with_context(|| format!("bad X in `{spec}`"))?,
                    y.trim().parse().with_context(|| format!("bad Y in `{spec}`"))?,
                ));
            }
            "--env" => {
                let spec = value(&args, &mut i, arg)?;
                let (k, v) = spec
                    .split_once('=')
                    .ok_or_else(|| anyhow!("--env needs KEY=VALUE, got `{spec}`"))?;
                options.env.push((k.to_string(), v.to_string()));
            }
            other if other.starts_with('-') => bail!("unknown option `{other}`\n\n{USAGE}"),
            _ => {
                // First bare word starts the program (so `--` is optional).
                options.command = args[i..].to_vec();
                break;
            }
        }
        i += 1;
    }

    if verbose && std::env::var_os("RUST_LOG").is_none() {
        // SAFETY: set before any threads are spawned (we are still parsing args).
        unsafe { std::env::set_var("RUST_LOG", "debug") };
    }

    if options.command.is_empty() && options.window_id.is_none() {
        bail!("nothing to do: give a program to launch after `--`, or --window-id\n\n{USAGE}");
    }
    if options.window_id.is_some() && options.display.is_none() {
        bail!("--window-id needs --display (the server the window lives on)");
    }
    if options.window_id.is_some() && !options.command.is_empty() {
        bail!("--window-id captures an existing window; do not also give a program");
    }
    Ok(options)
}

/// `1600x1000` -> `(1600, 1000)`.
pub fn parse_size(spec: &str) -> Result<(u16, u16)> {
    let (w, h) = spec
        .split_once(['x', 'X'])
        .ok_or_else(|| anyhow!("size must be WxH, got `{spec}`"))?;
    let w: u16 = w.parse().with_context(|| format!("bad width in `{spec}`"))?;
    let h: u16 = h.parse().with_context(|| format!("bad height in `{spec}`"))?;
    if w == 0 || h == 0 {
        bail!("size must be non-zero, got `{spec}`");
    }
    Ok((w, h))
}

/// `0x1c00001` or `29360129` -> window id.
pub fn parse_window_id(spec: &str) -> Result<u32> {
    let id = if let Some(hex) = spec.strip_prefix("0x").or_else(|| spec.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16)
    } else {
        spec.parse()
    };
    id.with_context(|| format!("bad window id `{spec}` (use 0x... or decimal)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_program_after_double_dash_with_options() {
        let o = parse(["--out", "a.png", "--size", "800x600", "--", "demo", "--flag"]).unwrap();
        assert_eq!(o.command, vec!["demo", "--flag"]);
        assert_eq!(o.out, PathBuf::from("a.png"));
        assert_eq!((o.screen_width, o.screen_height), (800, 600));
        assert!(o.display.is_none());
    }

    #[test]
    fn bare_program_without_double_dash() {
        let o = parse(["demo"]).unwrap();
        assert_eq!(o.command, vec!["demo"]);
    }

    #[test]
    fn requires_a_program_or_window_id() {
        let err = parse(["--out", "x.png"]).unwrap_err().to_string();
        assert!(err.contains("nothing to do"), "{err}");
    }

    #[test]
    fn window_id_mode_needs_display_and_no_program() {
        assert!(parse(["--window-id", "0x10"]).unwrap_err().to_string().contains("--display"));
        assert!(
            parse(["--display", ":1", "--window-id", "0x10", "demo"])
                .unwrap_err()
                .to_string()
                .contains("do not also give a program")
        );
        let o = parse(["--display", ":1", "--window-id", "0x1c00001"]).unwrap();
        assert_eq!(o.window_id, Some(0x1c00001));
        assert!(o.command.is_empty());
    }

    #[test]
    fn rejects_unknown_option_and_bad_values() {
        assert!(parse(["--bogus", "demo"]).is_err());
        assert!(parse(["--size", "800", "demo"]).is_err());
        assert!(parse(["--size", "0x600", "demo"]).is_err());
        assert!(parse(["--timeout", "soon", "demo"]).is_err());
        assert!(parse(["--env", "NOEQUALS", "demo"]).is_err());
        assert!(parse(["--out"]).is_err());
    }

    #[test]
    fn parses_steps_in_order() {
        let o = parse([
            "--step", "click:10,20", "--step", "key:Return", "--step", "wait:250", "--step", "shot:mid.png", "demo",
        ])
        .unwrap();
        assert_eq!(
            o.steps,
            vec![
                Step::Click { x: 10, y: 20 },
                Step::Key { keysym: "Return".into() },
                Step::Wait { duration: Duration::from_millis(250) },
                Step::Shot { path: PathBuf::from("mid.png") },
            ]
        );
    }

    #[test]
    fn step_parse_errors_are_descriptive() {
        assert!(Step::parse("click:10").unwrap_err().to_string().contains("X,Y"));
        assert_eq!(Step::parse("wheel:5,6,-3").unwrap(), Step::Wheel { x: 5, y: 6, clicks: -3 });
        assert!(Step::parse("wheel:5,6").is_err());
        assert_eq!(Step::parse("drag:10,20,300,400").unwrap(), Step::Drag { x1: 10, y1: 20, x2: 300, y2: 400 });
        assert!(Step::parse("drag:10,20,300").is_err());
        assert_eq!(Step::parse("release").unwrap(), Step::Release);
        assert!(Step::parse("release:now").is_err());
        assert!(Step::parse("dance:1").unwrap_err().to_string().contains("unknown step kind"));
        assert!(Step::parse("nocolon").is_err());
        assert!(Step::parse("wait:abc").is_err());
        assert!(Step::parse("key:").is_err());
        assert!(Step::parse("shot:").is_err());
    }

    #[test]
    fn env_pairs_and_durations() {
        let o = parse([
            "--env", "RUST_LOG=info", "--env", "A=b=c", "--delay", "5", "--settle", "7", "--timeout", "9", "demo",
        ])
        .unwrap();
        assert_eq!(o.env, vec![("RUST_LOG".into(), "info".into()), ("A".into(), "b=c".into())]);
        assert_eq!(o.delay, Duration::from_millis(5));
        assert_eq!(o.settle, Duration::from_millis(7));
        assert_eq!(o.timeout, Duration::from_secs(9));
    }

    #[test]
    fn pointer_option() {
        assert_eq!(parse(["--pointer", "10, 20", "demo"]).unwrap().pointer, Some((10, 20)));
        assert!(parse(["--pointer", "10", "demo"]).is_err());
        assert!(parse(["demo"]).unwrap().pointer.is_none());
    }

    #[test]
    fn window_ids_hex_and_decimal() {
        assert_eq!(parse_window_id("0x1C").unwrap(), 28);
        assert_eq!(parse_window_id("28").unwrap(), 28);
        assert!(parse_window_id("zz").is_err());
    }
}
