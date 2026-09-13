//! Welcome — no household is open — and the viewer gate that covers content
//! until "Who is looking?" is answered.

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, alert::Alert, button::{Button, ButtonVariants as _}, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::AtlasApp;

pub fn render(app: &AtlasApp, cx: &mut Context<AtlasApp>) -> AnyElement {
    let theme = cx.theme();
    let notice = app.startup_notice().map(str::to_string);
    v_flex()
        .id("screen-welcome")
        .test_support()
        .size_full()
        .items_center()
        .justify_center()
        .gap_6()
        .child(
            v_flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size_12()
                        .rounded(theme.radius)
                        .bg(theme.sidebar_primary)
                        .text_color(theme.sidebar_primary_foreground)
                        .child(Icon::new(IconName::Wallet).large()),
                )
                .child(div().text_2xl().font_weight(FontWeight::SEMIBOLD).child("Atlas Financer"))
                .child(div().text_sm().text_color(theme.muted_foreground).child("See what is free now. Follow what is planned. Test a concrete purchase.")),
        )
        .child(
            v_flex()
                .w_80()
                .gap_2()
                .child(Button::new("welcome-create").w_full().primary().icon(IconName::Plus).label("Create household…").on_click(cx.listener(|this, _, window, cx| this.open_new_household(window, cx))))
                .child(Button::new("welcome-open").w_full().outline().icon(IconName::FolderOpen).label("Open household…").on_click(cx.listener(|this, _, window, cx| this.open_open(window, cx))))
                .child(Button::new("welcome-sample").w_full().outline().label("Explore sample").on_click(cx.listener(|this, _, window, cx| this.load_sample(window, cx))))
                .child(div().text_xs().text_color(theme.muted_foreground).text_center().child("Fictitious household · PKR")),
        )
        .when_some(notice, |this, text| {
            this.child(
                v_flex().w_96().gap_2().child(Alert::warning("startup-notice", text).title("The file could not be opened")).child(
                    h_flex()
                        .gap_2()
                        .child(Button::new("welcome-try-another").small().outline().label("Try another file…").on_click(cx.listener(|this, _, window, cx| this.open_open(window, cx))))
                        .child(Button::new("welcome-sample-after-failure").small().ghost().label("Explore sample").on_click(cx.listener(|this, _, window, cx| this.load_sample(window, cx)))),
                ),
            )
        })
        .into_any_element()
}

/// The gate: the household is loaded but nobody has said who is looking.
/// The chooser dialog opens over it; nothing of the household renders behind.
pub fn render_gate(app: &AtlasApp, cx: &mut Context<AtlasApp>) -> AnyElement {
    let theme = cx.theme();
    let name = app.household().name.clone();
    v_flex()
        .id("screen-viewer-gate")
        .test_support()
        .size_full()
        .items_center()
        .justify_center()
        .gap_4()
        .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(name))
        .child(div().text_sm().text_color(theme.muted_foreground).child("Choose who is looking to continue. This changes what Atlas shows; it does not secure the household file."))
        .child(
            h_flex()
                .gap_2()
                .child(Button::new("gate-choose").primary().icon(IconName::Eye).label("Who is looking…").on_click(cx.listener(|this, _, window, cx| this.open_viewer_picker(window, cx))))
                .child(Button::new("gate-back").outline().label("Back to Welcome").on_click(cx.listener(|this, _, window, cx| this.close_household(window, cx)))),
        )
        .into_any_element()
}
