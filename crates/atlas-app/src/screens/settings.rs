//! Settings: theme, fixture, viewer and alert status. Small on purpose; the
//! authorization controls live under Privacy (M10).

use atlas_core::model::Household;
use gpui_kit::component::{ActiveTheme as _, description_list::{DescriptionItem, DescriptionList}, group_box::GroupBox, v_flex};
use gpui_kit::*;

use crate::alerting;

pub fn render(household: &Household, viewer_name: &str, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    v_flex()
        .id("screen-settings")
        .test_support()
        .gap_6()
        .child(
            v_flex()
                .gap_1()
                .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child("Settings"))
                .child(div().text_sm().text_color(theme.muted_foreground).child("Appearance is toggled from the title bar; everything else is read from the fixture and the environment.")),
        )
        .child(
            GroupBox::new().id("settings-runtime").title("Runtime").child(
                DescriptionList::new()
                    .columns(1)
                    .child(DescriptionItem::new("Theme").value(if theme.is_dark() { "Dark" } else { "Light" }))
                    .child(DescriptionItem::new("Viewing as").value(viewer_name.to_string()))
                    .child(DescriptionItem::new("Fixture").value(household.name.clone()))
                    .child(DescriptionItem::new("Reconciled to").value(household.as_of.format("%d %b %Y").to_string()))
                    .child(DescriptionItem::new("Base currency").value(household.base_currency.code().to_string()))
                    .child(DescriptionItem::new("Production alerts").value(if alerting::is_configured() {
                        "Posting to the DevBench notify endpoint (DEVBENCH_NOTIFY_URL)".to_string()
                    } else {
                        "Log only — set DEVBENCH_NOTIFY_URL and DEVBENCH_NOTIFY_TOKEN in the deployment environment".to_string()
                    }))
                    .child(DescriptionItem::new("Engine").value(format!("atlas-core {}", env!("CARGO_PKG_VERSION")))),
            ),
        )
}
