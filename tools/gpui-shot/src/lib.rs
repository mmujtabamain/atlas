//! `gpui-shot`: take screenshots of a gpui window on a headless Linux box.
//!
//! It lives in this repo (`tools/gpui-shot`, a workspace member) so that
//! `scripts/shoot.sh`, `scripts/walkthrough.sh` and `scripts/perf-screens.sh`
//! need nothing outside the checkout: `cargo build -p gpui-shot` puts the
//! binary next to `atlas` in `target/debug/`.
//!
//! Why this exists
//! ---------------
//! GPUI (the `gpui-pre-*` snapshot pinned by gpui-kit 0.6) only ships an
//! offscreen renderer on macOS (Metal); `Window::render_to_image` is a no-op
//! on Linux. On Linux gpui renders through wgpu into a real X11/Wayland window,
//! so the only way to get pixels on a box without a display is:
//!
//! 1. start a virtual X server (`Xvfb`),
//! 2. give wgpu a Vulkan device — Mesa's `lavapipe` CPU driver,
//! 3. launch the app against that display, wait for its window,
//! 4. read the window's pixels back with the X11 `GetImage` request,
//! 5. encode them as PNG.
//!
//! Everything here is pure Rust on top of `x11rb`; no `xcap`, no `xwd`, no
//! ImageMagick. (`xcap` was evaluated: on Linux it drags in pipewire, wayland
//! and zbus system dependencies that cannot be installed on this box.)
//!
//! The library is split so that the pure parts (argument parsing, pixel
//! conversion, display-number selection, step scripting) are unit-testable
//! without an X server; `run::run` is the orchestration used by the binary.

pub mod args;
pub mod frame;
pub mod logger;
pub mod run;
pub mod x11;
pub mod xvfb;

pub use args::{Options, Step};
pub use frame::Frame;
pub use run::{Outcome, run};
