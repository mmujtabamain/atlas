//! Accounts / Earmarks: a boundary's current cash constraints and the money
//! set aside across accounts. Present money only; the household runway uses
//! the fixed Expected baseline and horizon.

use atlas_core::ids::ObjectRef;
use atlas_core::liquidity::Boundary;
use atlas_core::model::{Coverage, Household};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    alert::Alert,
    button::{Button, ButtonVariants as _},
    h_flex,
    select::Select,
    tab::{Tab, TabBar},
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::common::workspace_header;
use crate::app::AtlasApp;
use crate::models::liquidity::LiquidityModel;
use crate::nav::{Destination, Route};
use crate::widgets::labels;
use crate::widgets::record::{self, Lane};
use crate::widgets::states::{action_bar, columns, columns_leading, count_line, hairline, info_card, note, section};

const LANES: [(&str, Lane); 7] = [
    ("Earmark", Lane::fixed(200.)),
    ("Account", Lane::fixed(200.)),
    ("Amount", Lane::money(120.)),
    ("Relationship", Lane::fixed(220.)),
    ("Constraint", Lane::fixed(150.)),
    ("Purpose / status", Lane::flex()),
    ("", Lane::fixed(220.)),
];

/// One control of the inline scope row: the label beside its control rather
/// than above it (`widgets::scope::control` stacks them, which costs the
/// screen a line before its first figure).
fn inline_control(label: &'static str, control: impl IntoElement, cx: &App) -> impl IntoElement {
    h_flex()
        .flex_shrink_0()
        .gap_2()
        .items_center()
        .child(div().flex_shrink_0().text_xs().text_color(cx.theme().muted_foreground).child(label))
        .child(control)
}

/// A labelled plain fact that fills its column (`states::fact` is a fixed 16
/// rem card, which overflows a cell of a grid of four).
fn field(label: impl Into<SharedString>, value: impl Into<SharedString>, cx: &App) -> AnyElement {
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .child(div().w_full().text_xs().text_color(cx.theme().muted_foreground).child(label.into()))
        .child(div().w_full().text_sm().font_weight(FontWeight::MEDIUM).child(value.into()))
        .into_any_element()
}

/// `Spendable now`: headroom clamped at zero, with the deficit stated when the
/// floor is breached.
///
/// A raw restatement rather than a derived figure — it has no chain of its
/// own, so it carries no `ⓘ`, and the empty cell where its neighbours carry
/// one keeps the three values on the same line.
fn spendable_cell(model: &LiquidityModel, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let deficit = model.deficit.is_positive();
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .child(
            h_flex()
                .w_full()
                .justify_between()
                .items_baseline()
                .gap_2()
                .child(div().flex_1().min_w_0().text_xs().text_color(theme.muted_foreground).child("Spendable now"))
                .child(div().flex_shrink_0().size_5()),
        )
        .child(
            div()
                .id(SharedString::from(format!("{}-spendable", model.boundary.slug())))
                .test_support()
                .font_family(theme.mono_font_family.clone())
                .font_weight(FontWeight::SEMIBOLD)
                .text_xl()
                .child(model.spendable_display.format()),
        )
        .child(div().w_full().text_xs().text_color(if deficit { theme.danger } else { theme.muted_foreground }).child(if deficit {
            format!("Shown as 0: the floor is breached by {}", model.deficit.format())
        } else {
            "No deficit against the hard floor".to_string()
        }))
        .into_any_element()
}

pub fn render(app: &AtlasApp, model: &LiquidityModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let viewer = app.viewer();
    let eligible = crate::models::liquidity::editable_accounts(household, viewer);
    let add_disabled = eligible.is_empty();
    let add_tooltip = if add_disabled { "Earmarks need a personal account you see in full" } else { "Set money aside for a purpose" };
    let header = workspace_header(
        Destination::Accounts,
        Route::Earmarks,
        vec![Button::new("new-reservation")
            .small()
            .outline()
            .icon(IconName::Plus)
            .label("Add earmark…")
            .disabled(add_disabled)
            .tooltip(add_tooltip)
            .on_click(cx.listener(|this, _, window, cx| this.open_new_reservation_for(None, window, cx)))
            .into_any_element()],
        cx,
    );
    let theme = cx.theme();
    let muted = theme.muted_foreground;
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
                    .justify_end()
                    .gap_1()
                    .child(Button::new(SharedString::from(format!("release-{}", rid.raw()))).xsmall().ghost().label("Pay and release…").on_click(cx.listener(move |this, _, window, cx| this.open_release_reservation(rid, window, cx))))
                    .child(Button::new(SharedString::from(format!("delete-earmark-{}", rid.raw()))).xsmall().ghost().icon(IconName::Trash).tooltip("Delete earmark…").on_click(cx.listener(move |this, _, window, cx| this.confirm_delete(ObjectRef::Reservation(rid), window, cx))))
                    .into_any_element()
            } else {
                h_flex()
                    .justify_end()
                    .child(Button::new(SharedString::from(format!("delete-earmark-{}", rid.raw()))).xsmall().ghost().icon(IconName::Trash).tooltip("Delete earmark…").on_click(cx.listener(move |this, _, window, cx| this.confirm_delete(ObjectRef::Reservation(rid), window, cx))))
                    .into_any_element()
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
    // The leading figure is the one the boundary's constraint is about: free
    // cash for a household or a person, the cash-constraint ceiling for a
    // company. The engine lists it third, except for a person, whose first
    // figure is the attributed share.
    let leading_index = match model.boundary {
        Boundary::Person(_) => 3,
        _ => 2,
    }
    .min(model.figures.len().saturating_sub(1));
    // Six figures read as one grid across the full width; the same six as two
    // ragged rows of fixed cards leave the right third of the window empty.
    // Three cash figures on the leading row, the balance sheet under it.
    let one_band = model.figures.len() <= 4;
    let mut money_lead: Vec<AnyElement> = Vec::new();
    let mut money_rest: Vec<AnyElement> = Vec::new();
    if let Some(figure) = model.figures.get(leading_index) {
        money_lead.push(figure.leading().into_any_element());
    }
    for (index, figure) in model.figures.iter().enumerate() {
        if index == leading_index {
            continue;
        }
        if one_band || money_lead.len() < 3 {
            money_lead.push(figure.standard().into_any_element());
        } else {
            money_rest.push(figure.standard().into_any_element());
        }
    }
    let has_rest = !money_rest.is_empty();

    let runway_band: AnyElement = match (&model.runway, &model.minimum_injection) {
        (Some(runway), Some(injection)) => {
            let days_line = format!(
                "Days below the floor {} · shortfall over time {} {}-days (how long and how deep, not an amount of cash)",
                runway.days_below,
                runway.integrated_shortfall_currency_days,
                household.base_currency.code()
            );
            // Whether the path holds is a fact about this screen, so it is a
            // card — and an alert only when the floor is actually breached.
            let verdict: AnyElement = div()
                .id("runway-summary")
                .test_support()
                .w_full()
                .child(if runway.first_breach.is_some() {
                    Alert::warning("earmarks-runway-alert", days_line.clone()).title(runway.summary()).into_any_element()
                } else {
                    info_card("earmarks-runway-card", IconName::ChartLine, runway.summary(), days_line.clone(), cx)
                })
                .into_any_element();
            section("earmarks-runway", format!("Expected baseline through {}", model.horizon.format("%d %b %Y")))
                .divider(true)
                .badge("Expected · Baseline")
                .action(Button::new("earmarks-view-path").small().ghost().icon(IconName::ChartLine).label("View path").on_click(cx.listener(|this, _, _, cx| this.open_forecast_for_boundary(Boundary::Household, cx))))
                .child(verdict)
                .child(columns([
                    field("First breach", runway.first_breach.map(|d| d.format("%d %b %Y").to_string()).unwrap_or_else(|| "None through the horizon".into()), cx),
                    field("Lowest path balance", format!("{}{}", runway.lowest.format(), runway.lowest_date.map(|d| format!(" · {}", d.format("%d %b %Y"))).unwrap_or_default()), cx),
                    field("Worst deficit", format!("{}{}", runway.worst_deficit.format(), runway.worst_date.map(|d| format!(" · {}", d.format("%d %b %Y"))).unwrap_or_default()), cx),
                    injection.standard().into_any_element(),
                ]))
                .child(note("The path uses the expected value of every planned movement, so it is conditional, not money in hand.", cx))
                .into_any_element()
        }
        _ => section("earmarks-runway", "Dated path")
            .divider(true)
            .child(note(format!("{boundary_label}'s dated path lives in the forecast."), cx))
            .child(h_flex().child(Button::new("earmarks-view-forecast").small().ghost().icon(IconName::ChartLine).label("View forecast").on_click({
                let boundary = model.boundary;
                cx.listener(move |this, _, _, cx| this.open_forecast_for_boundary(boundary, cx))
            })))
            .into_any_element(),
    };

    v_flex()
        .id("screen-earmarks")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        // The boundary on one row, with the date its figures are read on at the
        // trailing edge and the rule that governs the choice beneath.
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
                        .child(inline_control("Whose money", Select::new(&app.boundary_choice).small().w(px(220.)), cx))
                        .child(div().flex_shrink_0().text_xs().text_color(muted).child(format!("Balances as of {}", household.as_of.format("%d %b %Y")))),
                )
                .child(div().w_full().text_xs().text_color(muted).child("Present money only. Company boundaries need full disclosure.")),
        )
        .child(
            section("earmarks-money", format!("{boundary_label} — money today"))
                .child(columns_leading(if one_band { 0.34 } else { 0.42 }, money_lead))
                .when(has_rest, |this| this.child(hairline(cx)).child(columns(money_rest))),
        )
        .child(
            section("earmarks-floors", "Hard floor and headroom")
                .divider(true)
                .description("Hard earmarks and bank minimums must stay in the accounts; headroom is what is above them, shown negative when they are breached.")
                .child(columns([model.hard_floor.standard().into_any_element(), model.headroom.standard().into_any_element(), spendable_cell(model, cx)])),
        )
        .child(runway_band)
        .child(
            section("earmarks-list", "Earmarks")
                .divider(true)
                .child(
                    h_flex()
                        .w_full()
                        .justify_between()
                        .items_center()
                        .gap_4()
                        .child(
                            TabBar::new("earmarks-tabs")
                                .selected_index(if released_tab { 1 } else { 0 })
                                .on_click(cx.listener(|this, index: &usize, _, cx| {
                                    this.earmarks_released_tab = *index == 1;
                                    cx.notify();
                                }))
                                .children([Tab::new().label("Active"), Tab::new().label("Released")]),
                        )
                        .child(count_line(rows.len(), rows.len(), if released_tab { "released earmarks" } else { "active earmarks" }, cx)),
                )
                .child(if rows.is_empty() {
                    note(if released_tab { "No released earmarks." } else if is_household { "No earmarks on the household's accounts." } else { "No earmarks on this boundary's accounts." }, cx).into_any_element()
                } else {
                    record::list("earmarks-rows", record::header(&LANES, cx), rows).into_any_element()
                })
                // What an earmark is stays true whether or not any exist, so it
                // is a card rather than a sentence under the heading that reads
                // as an afterthought.
                .child(info_card(
                    "earmarks-meaning",
                    IconName::Wallet,
                    "An earmark reserves money, it does not move it",
                    "Money set aside for a purpose without a separate bank account. The bank balance does not change; free cash is what is left once the reservations are taken off it. Paying the obligation releases its earmark: settled cash falls, free cash does not.",
                    cx,
                )),
        )
        .child(action_bar(
            "earmarks-footer",
            vec![Button::new("earmarks-funding").small().ghost().icon(IconName::ListOrdered).label("Funding order").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Funding, cx))).into_any_element()],
            vec![
                Button::new("earmarks-add-2")
                    .small()
                    .outline()
                    .icon(IconName::Plus)
                    .label("Add earmark…")
                    .disabled(add_disabled)
                    .tooltip(add_tooltip)
                    .on_click(cx.listener(|this, _, window, cx| this.open_new_reservation_for(None, window, cx)))
                    .into_any_element(),
            ],
            cx,
        ))
        .into_any_element()
}
