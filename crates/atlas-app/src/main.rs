//! Atlas Financer — desktop binary.
//!
//! ```text
//! atlas [--theme light|dark] [--size WxH] [--screen household|people|…] [--viewer a|b]
//! ```

use atlas_app::{AtlasApp, Launch, alerting};
use gpui_kit::component::{Root, TitleBar};
use gpui_kit::*;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
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
                let view = cx.new(|cx| AtlasApp::new(&launch, window, cx));
                // Root must be the first view in every gpui-kit window.
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("failed to open the Atlas Financer window");
        })
        .detach();
    });
}
