//! Atlas Financer — desktop binary.
//!
//! ```text
//! atlas [--theme light|dark] [--size WxH] [--screen household|people|…] [--viewer a|b]
//! ```

use atlas_app::{Launch, Shell, alerting, logging};
use gpui_kit::component::{Root, TitleBar};
use gpui_kit::*;

fn main() {
    // stderr + logs.log (see `atlas_app::logging`); RUST_LOG still works.
    logging::init();
    alerting::init();
    alerting::install_panic_hook();
    let launch = Launch::parse(std::env::args().skip(1));
    log::info!("launching Atlas Financer: {launch:?}");

    // The full Lucide catalog: the sidebar and screens use finance icons that
    // are not among the 101 default component icons.
    let app = gpui_kit::application().with_assets(gpui_kit::assets::AllAssets);
    app.run(move |cx| {
        gpui_kit::init(cx);
        launch.apply_theme(cx);

        let bounds = Bounds::centered(None, size(px(launch.width), px(launch.height)), cx);
        let mut window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..TitleBar::window_options()
        };
        if let Some(titlebar) = window_options.titlebar.as_mut() {
            titlebar.title = Some("Atlas Financer".into());
        }

        let launch = launch.clone();
        cx.spawn(async move |cx| {
            cx.open_window(window_options, |window, cx| {
                if launch.perf_overlay {
                    // gpui's own frame-time readout, painted straight into the
                    // scene (no view invalidation, so it never causes a frame).
                    window.set_debug_frame_overlay_mode(DebugFrameOverlayMode::Full);
                    log::info!("perf: gpui frame-time overlay on (top-right: current draw, 1%/10% worst, max, frame count)");
                }
                // The shell creates the content view; Root must be the first
                // view in every gpui-kit window.
                let shell = cx.new(|cx| Shell::new(&launch, window, cx));
                cx.new(|cx| Root::new(shell, window, cx))
            })
            .expect("failed to open the Atlas Financer window");
        })
        .detach();
    });
}
