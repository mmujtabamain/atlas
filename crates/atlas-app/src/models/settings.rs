//! Settings: theme, household, viewer and alert status. Small on purpose; the
//! authorization controls live under Privacy.

use atlas_core::model::Household;
use gpui_kit::component::{ActiveTheme as _, description_list::{DescriptionItem, DescriptionList}, group_box::GroupBox, v_flex};
use gpui_kit::*;

use crate::alerting;

pub fn render(household: &Household, viewer_name: &str, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    v_flex()
        .id("screen-settings")
        .test_support()
        .w_full()
        .gap_6()
        .child(
            v_flex()
                .gap_1()
                .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child("Settings"))
                .child(div().text_sm().text_color(theme.muted_foreground).child("Light or dark is toggled from the title bar. Everything else here is read from the open household and the environment.")),
        )
        .child(
            GroupBox::new().id("settings-runtime").title("This session").child(
                DescriptionList::new()
                    .columns(1)
                    .child(DescriptionItem::new("Theme").value(if theme.is_dark() { "Dark" } else { "Light" }))
                    .child(DescriptionItem::new("Viewing as").value(viewer_name.to_string()))
                    .child(DescriptionItem::new("Household").value(household.name.clone()))
                    .child(DescriptionItem::new("Balances reconciled").value(household.as_of.format("%d %b %Y").to_string()))
                    .child(DescriptionItem::new("Base currency").value(household.base_currency.code().to_string()))
                    .child(DescriptionItem::new("Failure alerts").value(if alerting::is_configured() {
                        "On — failures are posted to the team chat".to_string()
                    } else {
                        "Off — failures are written to logs.log only (set DEVBENCH_NOTIFY_URL and DEVBENCH_NOTIFY_TOKEN to post them)".to_string()
                    }))
                    .child(DescriptionItem::new("Version").value(format!("Atlas Financer {}", env!("CARGO_PKG_VERSION")))),
            ),
        )
}
