//! Accounts / Funding: which accounts the rules let a funding search use, in
//! what order, and which account would pay each expense category today.

use atlas_core::model::Household;
use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, button::Button, h_flex, tag::Tag, v_flex};
use gpui_kit::*;

use super::common::workspace_header;
use crate::app::AtlasApp;
use crate::models::rules::RulesModel;
use crate::nav::{Destination, Route};
use crate::widgets::record::{self, Lane};
use crate::widgets::scope;
use crate::widgets::states::{note, section};

pub fn render(app: &AtlasApp, model: &RulesModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let header = workspace_header(Destination::Accounts, Route::Funding, vec![], cx);
    let theme = cx.theme();
    let name = |id: atlas_core::ids::AccountId| household.account(id).map(|a| a.name.clone()).unwrap_or_else(|| id.to_string());
    let overlay_name = model.overlay_scenario.and_then(|id| household.scenario(id)).map(|s| s.name.clone());
    let funding_lanes: [(&str, Lane); 4] = [("", Lane::fixed(32.)), ("Account", Lane::fixed(260.)), ("Allowed", Lane::fixed(200.)), ("Why", Lane::flex())];
    let bank_lanes: [(&str, Lane); 3] = [("Category", Lane::fixed(220.)), ("Selected account", Lane::fixed(260.)), ("Why", Lane::flex())];

    let funding_rows: Vec<_> = model
        .funding
        .iter()
        .enumerate()
        .map(|(index, step)| {
            let account = step.account;
            let rule = step.rule;
            let status: AnyElement = if step.forbidden {
                Tag::danger().xsmall().outline().child("Not allowed").into_any_element()
            } else if let Some(floor) = step.preserve {
                Tag::secondary().xsmall().outline().child(format!("Keep at least {}", floor.format())).into_any_element()
            } else {
                Tag::secondary().xsmall().outline().child("Allowed").into_any_element()
            };
            record::row(
                SharedString::from(format!("funding-step-{index}")),
                false,
                vec![
                    (funding_lanes[0].1, record::muted(format!("{}.", index + 1), cx)),
                    (funding_lanes[1].1, record::link(format!("funding-account-{index}"), name(account), cx.listener(move |this, _, _, cx| this.navigate(Route::Account(account), cx)))),
                    (funding_lanes[2].1, h_flex().child(status).into_any_element()),
                    (funding_lanes[3].1, h_flex().gap_2().items_center().child(record::muted(step.reason.clone(), cx)).child(record::link(format!("funding-rule-{index}"), "Rule", cx.listener(move |this, _, _, cx| this.navigate(Route::Rule(rule), cx)))).into_any_element()),
                ],
                |_, _, _| {},
            )
        })
        .collect();

    let bank_rows: Vec<_> = model
        .bank_selection
        .iter()
        .enumerate()
        .map(|(index, (category, account, why))| {
            let account = *account;
            record::row(
                SharedString::from(format!("bank-selection-{index}")),
                false,
                vec![
                    (bank_lanes[0].1, record::text(format!("“{category}” expenses"))),
                    (bank_lanes[1].1, record::link(format!("bank-account-{index}"), name(account), cx.listener(move |this, _, _, cx| this.navigate(Route::Account(account), cx)))),
                    (bank_lanes[2].1, record::muted(why.clone(), cx)),
                ],
                |_, _, _| {},
            )
        })
        .collect();

    v_flex()
        .id("screen-funding")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(scope::bar(
            vec![
                scope::select("Plan", &app.plan_choices.funding, px(220.), cx).into_any_element(),
                scope::fixed("Funding as of", household.as_of.format("%d %b %Y").to_string(), cx).into_any_element(),
            ],
            Some("Reconciliation-date facts under the expected evaluation. This is what a funding search may use, not a purchase result.".into()),
            cx,
        ))
        .child(
            section("funding-order", "Funding order")
                .description("Preferred accounts with their floors first; prohibitions are hard exclusions. A prohibition with a date lifts itself once that date passes.")
                .child(if funding_rows.is_empty() {
                    if !model.scenario_on && overlay_name.is_some() && household.rules.iter().any(|r| r.scenario.is_some() && matches!(r.action, atlas_core::rules::RuleAction::PreferAccount { .. } | atlas_core::rules::RuleAction::ForbidAccount { .. })) {
                        note(format!("No household-wide funding rule is in force today. Rules scoped to scenario “{}” apply only under that plan.", overlay_name.clone().unwrap_or_default()), cx).into_any_element()
                    } else {
                        note("No funding rule is in force today. A purchase still uses the sources you allow explicitly.", cx).into_any_element()
                    }
                } else {
                    record::list("funding-order-list", record::header(&funding_lanes, cx), funding_rows).into_any_element()
                }),
        )
        .child(
            section("funding-bank-selection", "Expense account selection")
                .description("Per bank-selection rule: which account pays a category's ordinary expenses today, on the settled balances.")
                .child(if bank_rows.is_empty() {
                    note("No bank-selection rule.", cx).into_any_element()
                } else {
                    record::list("bank-selection-list", record::header(&bank_lanes, cx), bank_rows).into_any_element()
                }),
        )
        .child(
            h_flex()
                .gap_2()
                .child(Button::new("funding-rules").small().outline().icon(IconName::Gavel).label("Funding rules").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Rules, cx))))
                .child(Button::new("funding-test-purchase").small().outline().icon(IconName::Target).label("Test a purchase").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Purchase, cx))))
                .child(div().text_xs().text_color(theme.muted_foreground).child("Nothing here moves money.")),
        )
        .into_any_element()
}
