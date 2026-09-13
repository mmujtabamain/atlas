//! Accounts / Earmarks: a boundary's current cash constraints and the money
//! set aside across accounts. Present money only; the household runway uses
//! the fixed Expected baseline and horizon.

use atlas_core::ids::ObjectRef;
use atlas_core::liquidity::Boundary;
use atlas_core::model::{Coverage, Household};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    tab::{Tab, TabBar},
    v_flex,
};
use gpui_kit::*;

use super::common::workspace_header;
use crate::app::AtlasApp;
use crate::models::liquidity::LiquidityModel;
use crate::nav::{Destination, Route};
use crate::widgets::figure::card;
use crate::widgets::labels;
use crate::widgets::record::{self, Lane};
use crate::widgets::scope;
use crate::widgets::states::{fact, lanes, note, section};

const LANES: [(&str, Lane); 7] = [
    ("Earmark", Lane::fixed(200.)),
    ("Account", Lane::fixed(200.)),
    ("Amount", Lane::money(120.)),
    ("Relationship", Lane::fixed(220.)),
    ("Constraint", Lane::fixed(150.)),
    ("Purpose / status", Lane::flex()),
    ("", Lane::fixed(220.)),
];

pub fn render(app: &AtlasApp, model: &LiquidityModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let viewer = app.viewer();
    let eligible = crate::models::liquidity::editable_accounts(household, viewer);
    let header = workspace_header(
        Destination::Accounts,
        Route::Earmarks,
        vec![Button::new("new-reservation")
            .small()
            .outline()
            .icon(IconName::Plus)
            .label("Add earmark…")
            .disabled(eligible.is_empty())
            .tooltip(if eligible.is_empty() { "Earmarks need a personal account you see in full" } else { "Set money aside for a purpose" })
            .on_click(cx.listener(|this, _, window, cx| this.open_new_reservation_for(None, window, cx)))
            .into_any_element()],
        cx,
    );
    let theme = cx.theme();
    let released_tab = app.earmarks_released_tab;
    let rows: Vec<_> = model
        .reservations
        .iter()
        .filter_map(|id| household.reservation(*id))
        .filter(|r| r.released_on.is_some() == released_tab)
        .map(|r| {
            let rid = r.id;
            let account_name = household.account(r.account).map(|a| a.name.clone()).unwrap_or_default();
            let account_id = r.account;
            let coverage = match r.coverage {
                Coverage::Disjoint => "Separate amount".to_string(),
                Coverage::CoversAccountMinimum => "Includes the bank minimum".to_string(),
                Coverage::NestedIn(outer) => format!("Inside {}", household.reservation(outer).map(|o| o.name.clone()).unwrap_or_else(|| outer.to_string())),
            };
            let status = match r.released_on {
                Some(d) => format!("Released {}", d.format("%d %b %Y")),
                None => if r.purpose.is_empty() { "Active".to_string() } else { r.purpose.clone() },
            };
            let actions: AnyElement = if r.released_on.is_none() {
                h_flex()
                    .gap_1()
                    .child(Button::new(SharedString::from(format!("release-{}", rid.raw()))).xsmall().ghost().label("Pay and release…").on_click(cx.listener(move |this, _, window, cx| this.open_release_reservation(rid, window, cx))))
                    .child(Button::new(SharedString::from(format!("delete-earmark-{}", rid.raw()))).xsmall().ghost().icon(IconName::Trash).tooltip("Delete earmark…").on_click(cx.listener(move |this, _, window, cx| this.confirm_delete(ObjectRef::Reservation(rid), window, cx))))
                    .into_any_element()
            } else {
                Button::new(SharedString::from(format!("delete-earmark-{}", rid.raw()))).xsmall().ghost().icon(IconName::Trash).tooltip("Delete earmark…").on_click(cx.listener(move |this, _, window, cx| this.confirm_delete(ObjectRef::Reservation(rid), window, cx))).into_any_element()
            };
            record::row(
                SharedString::from(format!("earmark-{}", rid.raw())),
                false,
                vec![
                    (LANES[0].1, record::text(r.name.clone())),
                    (LANES[1].1, record::link(format!("earmark-account-{}", rid.raw()), account_name, cx.listener(move |this, _, _, cx| this.navigate(Route::Account(account_id), cx)))),
                    (LANES[2].1, record::money(r.amount, cx)),
                    (LANES[3].1, record::muted(coverage, cx)),
                    (LANES[4].1, h_flex().child(labels::hardness_tag(r.hardness)).into_any_element()),
                    (LANES[5].1, record::muted(status, cx)),
                    (LANES[6].1, actions),
                ],
                |_, _, _| {},
            )
        })
        .collect();

    let boundary_label = model.boundary.label(household);
    let is_household = model.boundary == Boundary::Household;
    let money_figures: Vec<AnyElement> = model.figures.iter().enumerate().map(|(i, f)| card(if i == 2 { f.leading() } else { f.standard() }).into_any_element()).collect();

    v_flex()
        .id("screen-earmarks")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(scope::bar(vec![scope::select("Whose money", &app.boundary_choice, px(220.), cx).into_any_element()], Some("Present money on the reconciliation date. Company boundaries need full disclosure.".into()), cx))
        .child(section("earmarks-money", format!("{boundary_label} — money today")).child(lanes(money_figures)))
        .child(
            section("earmarks-floors", "Hard floor and headroom")
                .description("Hard earmarks and bank minimums must stay in the accounts; headroom is what is above them, shown negative when they are breached.")
                .child(lanes([
                    card(model.hard_floor.standard()).into_any_element(),
                    card(model.headroom.leading()).into_any_element(),
                    card(
                        v_flex()
                            .gap_1()
                            .child(div().text_xs().text_color(theme.muted_foreground).child("Spendable now"))
                            .child(div().id(SharedString::from(format!("{}-spendable", model.boundary.slug()))).test_support().text_xl().font_weight(FontWeight::SEMIBOLD).font_family(theme.mono_font_family.clone()).child(model.spendable_display.format()))
                            .child(div().text_xs().text_color(if model.deficit.is_positive() { theme.danger } else { theme.muted_foreground }).child(if model.deficit.is_positive() {
                                format!("Shown as 0: the floor is breached by {}", model.deficit.format())
                            } else {
                                "No deficit against the hard floor".to_string()
                            })),
                    )
                    .into_any_element(),
                ])),
        )
        .child(match (&model.runway, &model.minimum_injection) {
            (Some(runway), Some(injection)) => section("earmarks-runway", format!("Expected baseline through {}", model.horizon.format("%d %b %Y")))
                .action(Button::new("earmarks-view-path").small().ghost().icon(IconName::ChartLine).label("View path").on_click(cx.listener(|this, _, _, cx| this.open_forecast_for_boundary(Boundary::Household, cx))))
                .child(div().id("runway-summary").test_support().text_sm().child(runway.summary()))
                .child(lanes([
                    fact("First breach", runway.first_breach.map(|d| d.format("%d %b %Y").to_string()).unwrap_or_else(|| "None through the horizon".into()), cx).into_any_element(),
                    fact("Worst deficit", format!("{}{}", runway.worst_deficit.format(), runway.worst_date.map(|d| format!(" on {}", d.format("%d %b %Y"))).unwrap_or_default()), cx).into_any_element(),
                    fact("Lowest path balance", format!("{}{}", runway.lowest.format(), runway.lowest_date.map(|d| format!(" on {}", d.format("%d %b %Y"))).unwrap_or_default()), cx).into_any_element(),
                    fact("Days below the floor", runway.days_below.to_string(), cx).into_any_element(),
                    fact("Shortfall over time", format!("{} {}-days", runway.integrated_shortfall_currency_days, household.base_currency.code()), cx).into_any_element(),
                    card(injection.standard()).into_any_element(),
                ]))
                .child(note("The path uses the expected value of every planned movement, so it is conditional, not money in hand. Shortfall over time measures how long and how deep, not an amount of cash.", cx))
                .into_any_element(),
            _ => section("earmarks-runway", "Dated path")
                .child(note(format!("{boundary_label}'s dated path lives in the forecast."), cx))
                .child(h_flex().child(Button::new("earmarks-view-forecast").small().ghost().icon(IconName::ChartLine).label("View forecast").on_click({
                    let boundary = model.boundary;
                    cx.listener(move |this, _, _, cx| this.open_forecast_for_boundary(boundary, cx))
                })))
                .into_any_element(),
        })
        .child(
            section("earmarks-list", "Earmarks")
                .description("Money set aside for a purpose without a separate bank account. Paying the obligation releases its earmark: settled cash falls, free cash does not.")
                .child(
                    TabBar::new("earmarks-tabs")
                        .selected_index(if released_tab { 1 } else { 0 })
                        .on_click(cx.listener(|this, index: &usize, _, cx| {
                            this.earmarks_released_tab = *index == 1;
                            cx.notify();
                        }))
                        .children([Tab::new().label("Active"), Tab::new().label("Released")]),
                )
                .child(if rows.is_empty() {
                    note(if released_tab { "No released earmarks." } else if is_household { "No earmarks on the household's accounts." } else { "No earmarks on this boundary's accounts." }, cx).into_any_element()
                } else {
                    record::list("earmarks-rows", record::header(&LANES, cx), rows).into_any_element()
                }),
        )
        .into_any_element()
}
