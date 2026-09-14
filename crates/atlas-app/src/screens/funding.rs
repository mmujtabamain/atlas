//! Accounts / Funding: which accounts the rules let a funding search use, in
//! what order, and which account would pay each expense category today.

use atlas_core::model::Household;
use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, button::{Button, ButtonVariants as _}, h_flex, select::Select, tag::Tag, v_flex};
use gpui_kit::*;

use super::common::workspace_header;
use crate::app::AtlasApp;
use crate::models::rules::RulesModel;
use crate::nav::{Destination, Route};
use crate::widgets::record::{self, Lane};
use crate::widgets::states::{action_bar, info_card, note, section};

/// One control of the inline scope row: the label beside its control rather
/// than above it (`widgets::scope::control` stacks them, which costs the
/// screen a line before its first row).
fn inline_control(label: &'static str, control: impl IntoElement, cx: &App) -> impl IntoElement {
    h_flex()
        .flex_shrink_0()
        .gap_2()
        .items_center()
        .child(div().flex_shrink_0().text_xs().text_color(cx.theme().muted_foreground).child(label))
        .child(control)
}

// The order's lanes. The reason has the flexible lane to itself and the rule
// that produced it has its own at the trailing edge: a sentence beside a link
// in one auto-width cell is re-measured at every ancestor's sizing pass
// (`docs/perf.md` §3.3).
const FUNDING_LANES: [(&str, Lane); 5] = [("", Lane::fixed(32.)), ("Account", Lane::fixed(240.)), ("Allowed", Lane::fixed(200.)), ("Why", Lane::flex()), ("", Lane::fixed(70.))];
const BANK_LANES: [(&str, Lane); 3] = [("Category", Lane::fixed(220.)), ("Selected account", Lane::fixed(260.)), ("Why", Lane::flex())];

pub fn render(app: &AtlasApp, model: &RulesModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let header = workspace_header(Destination::Accounts, Route::Funding, vec![], cx);
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let name = |id: atlas_core::ids::AccountId| household.account(id).map(|a| a.name.clone()).unwrap_or_else(|| id.to_string());
    let overlay_name = model.overlay_scenario.and_then(|id| household.scenario(id)).map(|s| s.name.clone());

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
                    (FUNDING_LANES[0].1, record::muted(format!("{}.", index + 1), cx)),
                    (FUNDING_LANES[1].1, record::link(format!("funding-account-{index}"), name(account), cx.listener(move |this, _, _, cx| this.navigate(Route::Account(account), cx)))),
                    (FUNDING_LANES[2].1, h_flex().child(status).into_any_element()),
                    (FUNDING_LANES[3].1, record::muted(step.reason.clone(), cx)),
                    (FUNDING_LANES[4].1, h_flex().justify_end().child(record::link(format!("funding-rule-{index}"), "Rule", cx.listener(move |this, _, _, cx| this.navigate(Route::Rule(rule), cx)))).into_any_element()),
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
                    (BANK_LANES[0].1, record::text(format!("“{category}” expenses"))),
                    (BANK_LANES[1].1, record::link(format!("bank-account-{index}"), name(account), cx.listener(move |this, _, _, cx| this.navigate(Route::Account(account), cx)))),
                    (BANK_LANES[2].1, record::muted(why.clone(), cx)),
                ],
                |_, _, _| {},
            )
        })
        .collect();
    let counts = format!(
        "{} {} in the funding order · {} {} with a bank-selection rule",
        funding_rows.len(),
        if funding_rows.len() == 1 { "account" } else { "accounts" },
        bank_rows.len(),
        if bank_rows.len() == 1 { "category" } else { "categories" }
    );

    v_flex()
        .id("screen-funding")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        // The plan on one row with the date these facts are read on at the
        // trailing edge, and what the two lists below are — and are not —
        // underneath it.
        .child(
            v_flex()
                .w_full()
                .gap_2()
                .child(
                    h_flex()
                        .w_full()
                        .justify_between()
                        .items_center()
                        .gap_4()
                        .child(inline_control("Plan", Select::new(&app.plan_choices.funding).small().w(px(220.)), cx))
                        .child(div().flex_shrink_0().text_xs().text_color(muted).child(format!("Funding as of {}", household.as_of.format("%d %b %Y")))),
                )
                .child(div().w_full().text_xs().text_color(muted).child("Reconciliation-date facts under the expected evaluation. This is what a funding search may use, not a purchase result."))
                .child(div().id("funding-counts").test_support().w_full().text_xs().text_color(muted).child(counts)),
        )
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
                    record::list("funding-order-list", record::header(&FUNDING_LANES, cx), funding_rows).into_any_element()
                }),
        )
        .child(
            section("funding-bank-selection", "Expense account selection")
                .divider(true)
                .description("Per bank-selection rule: which account pays a category's ordinary expenses today, on the settled balances.")
                .child(if bank_rows.is_empty() {
                    note("No bank-selection rule.", cx).into_any_element()
                } else {
                    record::list("bank-selection-list", record::header(&BANK_LANES, cx), bank_rows).into_any_element()
                }),
        )
        // The screen's standing fact: it describes eligibility, and eligibility
        // is not a movement. As true when both lists are full as when they are
        // empty, so it is a card and not a muted line beside the commands.
        .child(info_card(
            "funding-no-movement",
            IconName::ShieldCheck,
            "Nothing here moves money",
            "These are the accounts a funding search may draw on and the order it would try them in, on today's settled balances. Choosing one is a plan; a purchase is only recorded when you record it.",
            cx,
        ))
        .child(action_bar(
            "funding-footer",
            vec![Button::new("funding-rules").small().ghost().icon(IconName::Gavel).label("Funding rules").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Rules, cx))).into_any_element()],
            vec![Button::new("funding-test-purchase").small().outline().icon(IconName::Target).label("Test a purchase").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Purchase, cx))).into_any_element()],
            cx,
        ))
        .into_any_element()
}
