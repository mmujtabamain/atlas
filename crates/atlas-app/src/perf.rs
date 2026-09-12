//! Frame and engine timing: the numbers behind the status-bar FPS counter and
//! the `perf:` lines in `logs.log`.
//!
//! gpui draws a frame only when something invalidated the window (a
//! `cx.notify()`, hover, scroll, typing), so "FPS" here means *how fast frames
//! come while they are coming*; an idle window at 0 frames/s is healthy.
//! What matters for the person is how long one frame takes, and that is what
//! is measured, in three independent ways:
//!
//! 1. **build** — time inside `AtlasApp::render`, i.e. building the element
//!    tree (model clones, screen functions, formatting). Our code only.
//! 2. **draw≈** — from the start of `render` until our tree finished
//!    painting, split by the [phase probe] that wraps the root element into
//!    gpui's phases: `layout` (request_layout — this is where `RenderOnce`
//!    components such as Button, Table, Tag build their own element trees),
//!    `taffy` (the flexbox solve), `prepaint` (hitboxes, element state) and
//!    `paint` (quads, glyphs). Overlays drawn through `defer_draw` (tooltips,
//!    popovers) come after our tree, so this is a floor, not the whole frame.
//! 3. **gpui** — gpui's own profiler histograms (`profiler` feature):
//!    `Window::draw` duration, first-invalidation-to-present, and the interval
//!    between presented frames while animating. Ground truth, summarised once
//!    a second.
//!
//! The status bar shows the *previous* frame's numbers (this frame's are not
//! known while it is being built). The gpui overlay in the top-right corner
//! (`--no-perf-overlay` hides it) paints the current draw time straight into
//! the scene without going through views, so it never causes a frame itself.
//!
//! Engine work is timed separately with [`timed`]: every `compute_*` in
//! `AtlasApp` logs how long the calculation took, so a slow *state change* and
//! a slow *frame* can be told apart in the log.
//!
//! [phase probe]: FrameMeter::phase_probe

use std::cell::Cell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::{Duration, Instant};

// Named imports on purpose: `gpui_kit::*` would also bring in gpui's `test`
// attribute macro, which shadows `#[test]` in the unit tests below.
use gpui_kit::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, Window, profiler,
};

use crate::logging;

/// A frame whose draw took longer than this counts as slow in the summary.
pub const SLOW_FRAME: Duration = Duration::from_millis(50);
/// A single frame longer than this is logged on its own, at warn (a hitch).
pub const HITCH: Duration = Duration::from_millis(250);
/// How often the summary line is written while frames are happening.
pub const SUMMARY_EVERY: Duration = Duration::from_secs(1);
/// An engine computation longer than this is logged at warn instead of info.
pub const SLOW_COMPUTE: Duration = Duration::from_millis(100);
/// Frames within this window feed the instantaneous FPS figure.
const FPS_WINDOW: Duration = Duration::from_secs(1);

/// Milliseconds as a float, for log lines.
pub fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn fmt_ms(duration: Option<Duration>) -> String {
    match duration {
        Some(duration) => format!("{:.1}ms", ms(duration)),
        None => "n/a".to_string(),
    }
}

/// Runs an engine computation and logs its duration (`info`, or `warn` past
/// [`SLOW_COMPUTE`]). The label should say what was computed and for what.
pub fn timed<R>(what: &str, f: impl FnOnce() -> R) -> R {
    let start = Instant::now();
    let result = f();
    let elapsed = start.elapsed();
    if elapsed >= SLOW_COMPUTE {
        log::warn!("perf: {what} took {:.1}ms (slow, > {}ms)", ms(elapsed), SLOW_COMPUTE.as_millis());
    } else {
        log::info!("perf: {what} took {:.1}ms", ms(elapsed));
    }
    result
}

/// One completed frame, as the next frame sees it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameSample {
    /// 1-based frame count since the view was created.
    pub number: u64,
    /// Time inside `AtlasApp::render`.
    pub build: Duration,
    /// Render start → our tree finished painting (build + the phases below),
    /// when the probe ran in that frame.
    pub draw: Option<Duration>,
    /// gpui phases of our tree, from the phase probe (zero when it did not run).
    pub phases: Phases,
    /// Render start → next render start.
    pub interval: Option<Duration>,
    /// Time spent in the screen's render function inside `render_content`.
    pub content_render: Duration,
    /// Section slug the frame rendered.
    pub section: &'static str,
    /// Mouse-move events on the window since the previous frame.
    pub mouse_moves: u32,
    /// Scroll-wheel events on the window since the previous frame.
    pub wheel_events: u32,
}

/// How long each gpui phase spent on our element tree in one frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Phases {
    /// `request_layout` of the whole tree: components render here.
    pub layout: Duration,
    /// Between the end of request_layout and the start of prepaint: taffy.
    pub taffy: Duration,
    pub prepaint: Duration,
    pub paint: Duration,
}

impl Phases {
    fn add(&mut self, other: Phases) {
        self.layout += other.layout;
        self.taffy += other.taffy;
        self.prepaint += other.prepaint;
        self.paint += other.paint;
    }

    fn div(self, n: u32) -> Phases {
        Phases { layout: self.layout / n, taffy: self.taffy / n, prepaint: self.prepaint / n, paint: self.paint / n }
    }

    fn describe(&self) -> String {
        format!(
            "layout={} taffy={} prepaint={} paint={}",
            fmt_ms(Some(self.layout)),
            fmt_ms(Some(self.taffy)),
            fmt_ms(Some(self.prepaint)),
            fmt_ms(Some(self.paint))
        )
    }
}

/// What the phase probe records while gpui draws our tree.
#[derive(Clone, Copy, Debug, Default)]
struct ProbeState {
    layout_started: Option<Instant>,
    layout_ended: Option<Instant>,
    prepaint_started: Option<Instant>,
    prepaint_ended: Option<Instant>,
    paint_started: Option<Instant>,
    paint_ended: Option<Instant>,
}

impl ProbeState {
    fn phases(&self) -> Phases {
        let span = |a: Option<Instant>, b: Option<Instant>| match (a, b) {
            (Some(a), Some(b)) if b >= a => b - a,
            _ => Duration::ZERO,
        };
        Phases {
            layout: span(self.layout_started, self.layout_ended),
            taffy: span(self.layout_ended, self.prepaint_started),
            prepaint: span(self.prepaint_started, self.prepaint_ended),
            paint: span(self.paint_started, self.paint_ended),
        }
    }
}

/// Accumulators for one summary window.
#[derive(Debug, Default)]
struct WindowStats {
    frames: u32,
    build_sum: Duration,
    build_max: Duration,
    draw_sum: Duration,
    draw_max: Duration,
    draw_samples: u32,
    phases_sum: Phases,
    slow: u32,
    hitches: u32,
    mouse_moves: u32,
    wheel_events: u32,
}

/// Measures frames of one view. Owned by `AtlasApp`; see the module docs.
pub struct FrameMeter {
    frames: u64,
    /// Start of the frame currently being built.
    started: Option<Instant>,
    /// Shared with the phase-probe element wrapping the tree.
    probe: Rc<Cell<ProbeState>>,
    /// The frame being built (finished by the next `begin_frame`).
    pending: FrameSample,
    /// The last completed frame — what the status bar shows.
    last: Option<FrameSample>,
    /// Frame starts within [`FPS_WINDOW`].
    recent: VecDeque<Instant>,
    window_started: Instant,
    window: WindowStats,
    last_summary: Option<Instant>,
    last_hitch_warned: Option<Instant>,
    logged_window_info: bool,
    mouse_moves: Cell<u32>,
    wheel_events: Cell<u32>,
    content_render: Cell<Duration>,
    /// gpui histograms at the previous summary, to report the delta.
    previous_snapshot: Option<profiler::FrameDurationSnapshot>,
}

impl Default for FrameMeter {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameMeter {
    pub fn new() -> Self {
        FrameMeter {
            frames: 0,
            started: None,
            probe: Rc::new(Cell::new(ProbeState::default())),
            pending: FrameSample::default(),
            last: None,
            recent: VecDeque::new(),
            window_started: Instant::now(),
            window: WindowStats::default(),
            last_summary: None,
            last_hitch_warned: None,
            logged_window_info: false,
            mouse_moves: Cell::new(0),
            wheel_events: Cell::new(0),
            content_render: Cell::new(Duration::ZERO),
            previous_snapshot: None,
        }
    }

    // ----- called from AtlasApp::render, in this order ------------------------

    /// First thing in `render`: closes the previous frame (now that its paint
    /// probe has fired) and opens this one. Returns the finished frame, if any.
    pub fn begin_frame(&mut self, section: &'static str) -> Option<FrameSample> {
        let now = Instant::now();
        let finished = self.started.take().map(|previous_start| {
            let probe = self.probe.get();
            let painted = probe.paint_ended.filter(|painted| *painted >= previous_start);
            let mut sample = self.pending;
            sample.draw = painted.map(|painted| painted - previous_start);
            sample.phases = if painted.is_some() { probe.phases() } else { Phases::default() };
            sample.interval = Some(now - previous_start);
            sample
        });
        if let Some(sample) = finished {
            self.finish(sample, now);
        }

        self.frames += 1;
        self.started = Some(now);
        self.probe.set(ProbeState::default());
        self.content_render.set(Duration::ZERO);
        self.pending = FrameSample {
            number: self.frames,
            section,
            mouse_moves: self.mouse_moves.take(),
            wheel_events: self.wheel_events.take(),
            ..FrameSample::default()
        };
        self.recent.push_back(now);
        while self.recent.front().is_some_and(|first| now - *first > FPS_WINDOW) {
            self.recent.pop_front();
        }
        if self.frames == 1 {
            log::info!("perf: first frame started {:.0}ms after process start", ms(logging::process_start().elapsed()));
        }
        finished
    }

    /// Restarts this frame's build clock — call right after any perf work
    /// done inside `render` (the summary line) so it is not booked as build time.
    pub fn restart_build_clock(&mut self) {
        if self.started.is_some() {
            self.started = Some(Instant::now());
        }
    }

    /// Once per window: what we are rendering into.
    pub fn log_window_info(&mut self, window: &Window, is_dark: bool) {
        if self.logged_window_info {
            return;
        }
        self.logged_window_info = true;
        let viewport = window.viewport_size();
        let bounds = window.bounds();
        log::info!(
            "perf: window viewport={:.0}x{:.0} logical px, scale_factor={}, bounds origin=({:.0},{:.0}), active={}, theme={}",
            f32::from(viewport.width),
            f32::from(viewport.height),
            window.scale_factor(),
            f32::from(bounds.origin.x),
            f32::from(bounds.origin.y),
            window.is_window_active(),
            if is_dark { "dark" } else { "light" }
        );
    }

    /// Records how long `render_content` spent in the screen function.
    /// `&self` because it is called while the view is borrowed immutably.
    pub fn record_content(&self, render: Duration) {
        self.content_render.set(render);
    }

    /// Last thing in `render`: the element tree is built.
    pub fn end_build(&mut self) {
        if let Some(started) = self.started {
            self.pending.build = started.elapsed();
        }
        self.pending.content_render = self.content_render.get();
    }

    /// Wraps the finished tree so gpui's phases on it are timed; the end of
    /// its paint closes the `draw≈` measurement. Transparent for layout: it
    /// hands the inner element's layout id straight up.
    pub fn phase_probe(&self, tree: AnyElement) -> PhaseProbe {
        PhaseProbe { inner: tree, state: self.probe.clone() }
    }

    /// Writes the once-a-second summary, with gpui's own histograms, when a
    /// second has passed and frames happened. Call after `begin_frame`.
    pub fn log_summary_if_due(&mut self, window: &Window) {
        let now = Instant::now();
        let since = now - self.window_started;
        if since < SUMMARY_EVERY || self.window.frames == 0 {
            return;
        }
        let stats = std::mem::take(&mut self.window);
        self.window_started = now;
        self.last_summary = Some(now);
        let avg = |sum: Duration, n: u32| if n == 0 { None } else { Some(sum / n) };
        let fps = self.fps().map(|fps| format!("{fps:.1}")).unwrap_or_else(|| "n/a".into());
        let phases = if stats.draw_samples == 0 { Phases::default() } else { stats.phases_sum.div(stats.draw_samples) };
        let mut line = format!(
            "perf: summary {:.1}s: {} frames (fps≈{}) build avg={} max={} · draw≈ avg={} max={} (avg {}) · slow(>{}ms)={} hitches(>{}ms)={} · input moves={} wheel={} · section={}",
            since.as_secs_f64(),
            stats.frames,
            fps,
            fmt_ms(avg(stats.build_sum, stats.frames)),
            fmt_ms(Some(stats.build_max)),
            fmt_ms(avg(stats.draw_sum, stats.draw_samples)),
            fmt_ms(Some(stats.draw_max)),
            phases.describe(),
            SLOW_FRAME.as_millis(),
            stats.slow,
            HITCH.as_millis(),
            stats.hitches,
            stats.mouse_moves,
            stats.wheel_events,
            self.pending.section,
        );
        line.push_str(&self.gpui_summary(window));
        log::info!("{line}");
    }

    // ----- input counters (called from listeners on the root element) ---------

    pub fn count_mouse_move(&self) {
        self.mouse_moves.set(self.mouse_moves.get().saturating_add(1));
    }

    pub fn count_wheel(&self) {
        self.wheel_events.set(self.wheel_events.get().saturating_add(1));
    }

    // ----- readings -------------------------------------------------------------

    /// Frames per second over the last second, from the spacing of frame
    /// starts; `None` until two frames fell inside the window.
    pub fn fps(&self) -> Option<f64> {
        let (first, last) = (self.recent.front()?, self.recent.back()?);
        let span = *last - *first;
        if self.recent.len() < 2 || span.is_zero() {
            return None;
        }
        Some((self.recent.len() - 1) as f64 / span.as_secs_f64())
    }

    /// The last completed frame.
    pub fn last(&self) -> Option<FrameSample> {
        self.last
    }

    /// Frames begun so far (including the one being built).
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// The status-bar text: previous frame's numbers.
    ///
    /// Leads with the frame *cost* and the rate it allows, because the rate of
    /// frames actually drawn is a property of the input in gpui: a mouse
    /// crossing five buttons in a second draws five frames, and "5 frames/s"
    /// read as "5 fps" looks like a performance problem when each of those
    /// frames took 6 ms.
    pub fn status_text(&self) -> String {
        let drawn = match self.fps() {
            Some(fps) => format!("{fps:.0} frames/s drawn"),
            None => "idle".to_string(),
        };
        match self.last {
            Some(last) => match last.draw {
                Some(draw) if !draw.is_zero() => format!(
                    "{:.0} ms/frame = {:.0} fps possible · {drawn} · #{}",
                    ms(draw),
                    1000.0 / ms(draw),
                    last.number
                ),
                _ => format!("build {:.0} ms/frame · {drawn} · #{}", ms(last.build), last.number),
            },
            None => "first frame".to_string(),
        }
    }

    // ----- internals --------------------------------------------------------------

    fn finish(&mut self, sample: FrameSample, now: Instant) {
        self.last = Some(sample);
        let stats = &mut self.window;
        stats.frames += 1;
        stats.build_sum += sample.build;
        stats.build_max = stats.build_max.max(sample.build);
        if let Some(draw) = sample.draw {
            stats.draw_sum += draw;
            stats.draw_max = stats.draw_max.max(draw);
            stats.draw_samples += 1;
            stats.phases_sum.add(sample.phases);
        }
        stats.mouse_moves += sample.mouse_moves;
        stats.wheel_events += sample.wheel_events;
        let cost = sample.draw.unwrap_or(sample.build);
        if cost >= SLOW_FRAME {
            stats.slow += 1;
        }
        if cost >= HITCH {
            stats.hitches += 1;
        }
        let detail = format!(
            "frame #{} section={} build={} draw≈{} ({}) interval={} content(render={}) input(moves={} wheel={})",
            sample.number,
            sample.section,
            fmt_ms(Some(sample.build)),
            fmt_ms(sample.draw),
            sample.phases.describe(),
            fmt_ms(sample.interval),
            fmt_ms(Some(sample.content_render)),
            sample.mouse_moves,
            sample.wheel_events,
        );
        // A hitch is worth a line of its own, but never more than one a second.
        let warn_hitch = cost >= HITCH && self.last_hitch_warned.is_none_or(|at| now - at >= Duration::from_secs(1));
        if warn_hitch {
            self.last_hitch_warned = Some(now);
            log::warn!("perf: slow {detail}");
        } else {
            log::debug!("perf: {detail}");
        }
    }

    /// gpui's histograms since the previous summary (cumulative when the
    /// delta cannot be formed), in the same line.
    fn gpui_summary(&mut self, window: &Window) -> String {
        let current = window.frame_duration_snapshot();
        let delta = match &self.previous_snapshot {
            Some(previous) => {
                let mut delta = current.clone();
                let ok = delta.draw_duration_histogram.subtract(&previous.draw_duration_histogram).is_ok()
                    && delta.dirty_to_present_histogram.subtract(&previous.dirty_to_present_histogram).is_ok()
                    && delta.present_interval_histogram.subtract(&previous.present_interval_histogram).is_ok();
                if ok { delta } else { current.clone() }
            }
            None => current.clone(),
        };
        self.previous_snapshot = Some(current);
        let draw = &delta.draw_duration_histogram;
        let dirty = &delta.dirty_to_present_histogram;
        let present = &delta.present_interval_histogram;
        let nanos = |value: u64| format!("{:.1}ms", value as f64 / 1_000_000.0);
        let mut out = String::new();
        if draw.len() > 0 {
            out.push_str(&format!(
                " | gpui draw p50={} p90={} max={} n={}",
                nanos(draw.value_at_percentile(50.0)),
                nanos(draw.value_at_percentile(90.0)),
                nanos(draw.max()),
                draw.len()
            ));
        }
        if dirty.len() > 0 {
            out.push_str(&format!(
                " · dirty→present p50={} max={} n={}",
                nanos(dirty.value_at_percentile(50.0)),
                nanos(dirty.max()),
                dirty.len()
            ));
        }
        if present.len() > 0 {
            out.push_str(&format!(
                " · present interval p50={} max={} n={} (≈{:.1} fps while animating)",
                nanos(present.value_at_percentile(50.0)),
                nanos(present.max()),
                present.len(),
                1e9 / present.value_at_percentile(50.0).max(1) as f64
            ));
        }
        if out.is_empty() {
            out.push_str(" | gpui: no draws recorded in this window");
        }
        out
    }
}

/// The element that wraps our tree; see [`FrameMeter::phase_probe`].
pub struct PhaseProbe {
    inner: AnyElement,
    state: Rc<Cell<ProbeState>>,
}

impl PhaseProbe {
    fn update(&self, f: impl FnOnce(&mut ProbeState)) {
        let mut state = self.state.get();
        f(&mut state);
        self.state.set(state);
    }
}

impl IntoElement for PhaseProbe {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for PhaseProbe {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        self.update(|s| s.layout_started = Some(Instant::now()));
        let id = self.inner.request_layout(window, cx);
        self.update(|s| s.layout_ended = Some(Instant::now()));
        (id, ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) {
        self.update(|s| s.prepaint_started = Some(Instant::now()));
        self.inner.prepaint(window, cx);
        self.update(|s| s.prepaint_ended = Some(Instant::now()));
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, cx: &mut App) {
        self.update(|s| s.paint_started = Some(Instant::now()));
        self.inner.paint(window, cx);
        self.update(|s| s.paint_ended = Some(Instant::now()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_build_draw_interval_and_fps() {
        let mut meter = FrameMeter::new();
        assert_eq!(meter.begin_frame("household"), None, "nothing to finish before the first frame");
        assert_eq!(meter.status_text(), "first frame");
        meter.count_mouse_move();
        meter.record_content(Duration::from_millis(5));
        std::thread::sleep(Duration::from_millis(5));
        meter.end_build();
        // The probe "paints" a little later, as gpui's layout and paint would.
        std::thread::sleep(Duration::from_millis(5));
        let t = Instant::now();
        meter.probe.set(ProbeState {
            layout_started: Some(t - Duration::from_millis(4)),
            layout_ended: Some(t - Duration::from_millis(3)),
            prepaint_started: Some(t - Duration::from_millis(2)),
            prepaint_ended: Some(t - Duration::from_millis(1)),
            paint_started: Some(t - Duration::from_millis(1)),
            paint_ended: Some(t),
        });
        std::thread::sleep(Duration::from_millis(5));

        let finished = meter.begin_frame("timeline").expect("first frame finished");
        assert_eq!(finished.number, 1);
        assert_eq!(finished.section, "household");
        assert!(finished.build >= Duration::from_millis(5), "{finished:?}");
        let draw = finished.draw.expect("probe painted");
        assert!(draw >= finished.build, "draw includes the build: {finished:?}");
        assert!(finished.interval.unwrap() >= draw, "interval spans the whole frame: {finished:?}");
        assert_eq!(finished.phases, Phases { layout: Duration::from_millis(1), taffy: Duration::from_millis(1), prepaint: Duration::from_millis(1), paint: Duration::from_millis(1) });
        assert_eq!(finished.content_render, Duration::from_millis(5));
        assert_eq!(finished.mouse_moves, 0, "the move was counted after frame 1 began, so it belongs to frame 2");
        assert_eq!(meter.last(), Some(finished));
        assert_eq!(meter.pending.mouse_moves, 1);
        assert!(meter.status_text().contains("ms/frame = "), "{}", meter.status_text());
        assert!(meter.status_text().ends_with("· #1"), "{}", meter.status_text());

        // Two frames inside the window give a rate; ~15 ms apart → tens of fps.
        let fps = meter.fps().expect("two frames in the window");
        assert!(fps > 10.0 && fps < 200.0, "{fps}");
        assert!(meter.status_text().contains("fps possible · ") && meter.status_text().contains(" frames/s drawn"), "{}", meter.status_text());
    }

    #[test]
    fn a_frame_without_a_probe_has_no_draw_time() {
        let mut meter = FrameMeter::new();
        meter.begin_frame("rules");
        meter.end_build();
        let finished = meter.begin_frame("rules").unwrap();
        assert_eq!(finished.draw, None);
        assert!(meter.status_text().starts_with("build "), "{}", meter.status_text());
        assert_eq!(meter.window.frames, 1);
    }

    #[test]
    fn slow_frames_and_hitches_are_counted() {
        let mut meter = FrameMeter::new();
        meter.begin_frame("projections");
        meter.end_build();
        // Pretend the probe painted 300 ms after the frame began.
        let started = meter.started.unwrap();
        meter.probe.set(ProbeState { paint_ended: Some(started + Duration::from_millis(300)), ..ProbeState::default() });
        // begin_frame() takes `now` after that instant only if we wait; use a
        // probe in the past instead: the frame is finished at the next begin.
        std::thread::sleep(Duration::from_millis(1));
        let finished = meter.begin_frame("projections").unwrap();
        assert!(finished.draw.unwrap() >= Duration::from_millis(300));
        assert_eq!(meter.window.slow, 1);
        assert_eq!(meter.window.hitches, 1);
        assert!(meter.last_hitch_warned.is_some(), "a hitch is logged at warn");
    }

    #[test]
    fn timed_returns_the_result() {
        assert_eq!(timed("unit test computation", || 41 + 1), 42);
    }
}
