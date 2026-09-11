//! Liquidity & reservations (§6, §17, M02, M13, E01, E08): the money
//! definitions of a chosen boundary, its hard floors and headroom, the runway
//! of the household path against those floors, and the earmarks themselves —
//! with a reservation editor and the E01 pay-and-release action.

use atlas_core::authz::Viewer;
use atlas_core::breach::{self, BreachReport};
use atlas_core::forecast::household_projection;
use atlas_core::ids::{EntityRef, ObjectRef, ReservationId};
use atlas_core::liquidity::{Boundary, boundary_liquidity};
use atlas_core::model::{Coverage, Household};
use atlas_core::{Disclosure, EngineResult, Money};
use chrono::NaiveDate;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    group_box::GroupBox, h_flex,
    tab::{Tab, TabBar},
    table::{Table, TableBody, TableCell, TableHead, TableHeader, TableRow},
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::AtlasApp;
use crate::widgets::figure::ExplainedFigure;
use crate::widgets::labels;
use crate::widgets::master::page_header;
use crate::widgets::table::{money_cell, muted_cell};

/// Everything the screen shows for one boundary.
#[derive(Clone, Debug)]
pub struct LiquidityModel {
    pub boundary: Boundary,
    pub boundaries: Vec<Boundary>,
    pub figures: Vec<ExplainedFigure>,
    pub hard_floor: ExplainedFigure,
    pub headroom: ExplainedFigure,
    pub spendable_display: Money,
    pub deficit: Money,
    /// Household only: the liquid-cash path against the hard floors (M13).
    pub runway: Option<BreachReport>,
    pub minimum_injection: Option<ExplainedFigure>,
    pub horizon: NaiveDate,
    /// Reservations on the boundary's accounts, active and released.
    pub reservations: Vec<ReservationId>,
}

impl LiquidityModel {
    pub fn compute(household: &Household, viewer: Viewer, boundary: Boundary, horizon: NaiveDate) -> EngineResult<Self> {
        log::info!("computing liquidity for boundary {:?} viewer {}", boundary, viewer.person);
        let mut boundaries = vec![Boundary::Household];
        boundaries.extend(household.people.iter().map(|p| Boundary::Person(p.id)));
        boundaries.extend(
            household
                .companies
                .iter()
                .filter(|c| matches!(household.disclosure_for(viewer, ObjectRef::Company(c.id)), Disclosure::Full | Disclosure::SelectedFields))
                .map(|c| Boundary::Company(c.id)),
        );
        let boundary = if boundaries.contains(&boundary) { boundary } else { Boundary::Household };

        let report = boundary_liquidity(household, boundary)?;
        let slug = boundary.slug();
        let figures = report
            .figures
            .iter()
            .enumerate()
            .map(|(index, (label, calc))| ExplainedFigure::new(format!("{slug}-figure-{index}"), label.clone(), calc, household, viewer))
            .collect();
        let hard_floor = ExplainedFigure::new(format!("{slug}-hard-floor"), "Hard floors", &report.hard_floor, household, viewer);
        let headroom = ExplainedFigure::new(format!("{slug}-headroom"), "Headroom over hard floors (signed)", &report.headroom, household, viewer);
        let deficit = report.headroom.money().negated().clamped_at_zero();

        let (runway, minimum_injection) = if boundary == Boundary::Household {
            let projection = household_projection(household, horizon, None)?;
            let breach = breach::analyse(&projection.path, report.hard_floor.money(), horizon)?;
            let injection = ExplainedFigure::new(format!("{slug}-injection"), "Minimum immediate injection K*", &breach.minimum_injection, household, viewer);
            (Some(breach), Some(injection))
        } else {
            (None, None)
        };

        let reservations = household
            .reservations
            .iter()
            .filter(|r| report.accounts.contains(&r.account))
            .filter(|r| !matches!(household.disclosure_for(viewer, ObjectRef::Reservation(r.id)), Disclosure::Hidden | Disclosure::Aggregate))
            .map(|r| r.id)
            .collect();

        Ok(LiquidityModel {
            boundary,
            boundaries,
            figures,
            hard_floor,
            headroom,
            spendable_display: report.headroom.money().clamped_at_zero(),
            deficit,
            runway,
            minimum_injection,
            horizon,
            reservations,
        })
    }
}

pub fn render(model: &LiquidityModel, household: &Household, viewer: Viewer, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    let viewer_name = household.entity_name(EntityRef::Person(viewer.person));
    let selected_index = model.boundaries.iter().position(|b| *b == model.boundary).unwrap_or(0);
    let boundaries = model.boundaries.clone();

    v_flex()
        .id("screen-liquidity")
        .test_support()
        .gap_6()
        .child(page_header(
            "Liquidity & reservations",
            "A bank balance is not available money (§17). Pick a boundary; every figure carries its chain, earmarks are separate objects, and headroom keeps its sign (§6.6).",
            cx,
        ))
        .child(
            TabBar::new("liquidity-boundaries")
                .selected_index(selected_index)
                .on_click(cx.listener(move |this, index: &usize, _, cx| {
                    if let Some(boundary) = boundaries.get(*index) {
                        this.select_boundary(*boundary, cx);
                    }
                }))
                .children(model.boundaries.iter().map(|b| Tab::new().label(b.label(household)))),
        )
        .child(
            GroupBox::new().id("boundary-figures").title(format!("{} — money definitions (§6)", model.boundary.label(household))).child(
                h_flex().flex_wrap().gap_8().children(model.figures.iter().enumerate().map(|(index, f)| {
                    div().min_w_48().child(f.figure(&viewer_name, index == 2))
                })),
            ),
        )
        .child(
            GroupBox::new().id("headroom").title("Hard floors and headroom (§6.6, §17)").child(
                v_flex()
                    .gap_4()
                    .child(
                        h_flex()
                            .flex_wrap()
                            .gap_8()
                            .child(div().min_w_48().child(model.hard_floor.figure(&viewer_name, false)))
                            .child(div().min_w_48().child(model.headroom.figure(&viewer_name, true)))
                            .child(
                                div().min_w_48().child(
                                    v_flex()
                                        .gap_1()
                                        .child(div().text_xs().text_color(theme.muted_foreground).child("Displayed spendable (floored at zero)"))
                                        .child(
                                            div()
                                                .id(SharedString::from(format!("{}-spendable", model.boundary.slug())))
                                                .test_support()
                                                .text_xl()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .font_family(theme.mono_font_family.clone())
                                                .child(model.spendable_display.format()),
                                        )
                                        .child(div().text_xs().text_color(if model.deficit.is_positive() { theme.danger } else { theme.muted_foreground }).child(
                                            if model.deficit.is_positive() {
                                                format!("Deficit {} reported separately — never hidden by the zero floor (§6.6)", model.deficit.format())
                                            } else {
                                                "No deficit against the hard floors".to_string()
                                            },
                                        )),
                                ),
                            ),
                    )
                    .child(div().text_xs().text_color(theme.muted_foreground).child(
                        "Nested minimums are constraints, not additive deductions: a bank minimum inside an earmark is counted once (§17, M02).",
                    )),
            ),
        )
        .child(match (&model.runway, &model.minimum_injection) {
            (Some(runway), Some(injection)) => render_runway(runway, injection, model, &viewer_name, cx).into_any_element(),
            _ => GroupBox::new()
                .id("runway")
                .title("Runway against hard floors (M13)")
                .child(div().text_sm().text_color(theme.muted_foreground).child(
                    "Per-person and per-company dated paths arrive with M4 (board #2757); the household path is analysed on the Household boundary.",
                ))
                .into_any_element(),
        })
        .child(render_reservations(model, household, viewer, cx))
}

fn render_runway(runway: &BreachReport, injection: &ExplainedFigure, model: &LiquidityModel, viewer_name: &str, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let fact = |label: &str, value: String| {
        v_flex()
            .gap_1()
            .min_w_48()
            .child(div().text_xs().text_color(theme.muted_foreground).child(label.to_string()))
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(value))
    };
    GroupBox::new()
        .id("runway")
        .title(format!("Runway of the household liquid-cash path against hard floors through {} (M13, E08)", model.horizon.format("%d %b %Y")))
        .child(
            v_flex()
                .gap_4()
                .child(div().id("runway-summary").test_support().text_sm().child(runway.summary()))
                .child(
                    h_flex()
                        .flex_wrap()
                        .gap_8()
                        .child(fact("First breach", runway.first_breach.map(|d| d.format("%d %b %Y").to_string()).unwrap_or_else(|| "none through the horizon".into())))
                        .child(fact("Worst deficit", format!("{}{}", runway.worst_deficit.format(), runway.worst_date.map(|d| format!(" on {}", d.format("%d %b %Y"))).unwrap_or_default())))
                        .child(fact("Lowest path balance", format!("{}{}", runway.lowest.format(), runway.lowest_date.map(|d| format!(" on {}", d.format("%d %b %Y"))).unwrap_or_default())))
                        .child(fact("Days below the floor", runway.days_below.to_string()))
                        .child(fact("Integrated shortfall", format!("{} currency-days", runway.integrated_shortfall_currency_days)))
                        .child(div().min_w_48().child(injection.figure(viewer_name, false))),
                )
                .child(div().text_xs().text_color(theme.muted_foreground).child(
                    "The path uses expected values of every planned occurrence in the baseline (§2.4: conditional, not available money). Maximum deficit is currency; integrated shortfall is currency-days and is not a capital requirement (E08). No breach is reported as “no breach through the horizon”, never as infinite runway (V044).",
                )),
        )
}

fn render_reservations(model: &LiquidityModel, household: &Household, _viewer: Viewer, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    let can_edit = true;
    let rows: Vec<_> = model.reservations.iter().filter_map(|id| household.reservation(*id)).collect();
    GroupBox::new()
        .id("reservations")
        .title("Reservations (§5.8, §17)")
        .child(
            v_flex()
                .gap_3()
                .child(
                    h_flex()
                        .justify_between()
                        .items_start()
                        .gap_4()
                        .child(div().flex_1().min_w_0().text_xs().text_color(theme.muted_foreground).child(
                            "Earmarks are separate objects, so several goals can use one account without fake bank accounts. Paying an obligation releases its earmark: cash falls, free cash does not (E01).",
                        ))
                        .when(can_edit, |this| {
                            this.child(
                                Button::new("new-reservation")
                                    .flex_shrink_0()
                                    .small()
                                    .outline()
                                    .icon(IconName::Plus)
                                    .label("New reservation…")
                                    .on_click(cx.listener(|this, _, window, cx| this.open_new_reservation(window, cx))),
                            )
                        }),
                )
                .child(if rows.is_empty() {
                    div().text_sm().text_color(theme.muted_foreground).child("No earmarks on this boundary's accounts.").into_any_element()
                } else {
                    Table::new()
                        .child(
                            TableHeader::new().child(
                                TableRow::new()
                                    .child(TableHead::new().w_40().flex_shrink_0().child("Reservation"))
                                    .child(TableHead::new().w_40().flex_shrink_0().child("Account"))
                                    .child(TableHead::new().w_32().flex_shrink_0().text_right().child("Amount"))
                                    .child(TableHead::new().w_56().flex_shrink_0().child("Coverage"))
                                    .child(TableHead::new().w_32().flex_shrink_0().child("Hardness"))
                                    .child(TableHead::new().min_w_0().child("Purpose"))
                                    .child(TableHead::new().w_40().flex_shrink_0().child("Status"))
                                    .child(TableHead::new().w_32().flex_shrink_0().child("")),
                            ),
                        )
                        .child(TableBody::new().children(rows.iter().enumerate().map(|(index, r)| {
                            let id = r.id;
                            let account_name = household.account(r.account).map(|a| a.name.clone()).unwrap_or_default();
                            let coverage = match r.coverage {
                                Coverage::Disjoint => "Disjoint — additive".to_string(),
                                Coverage::CoversAccountMinimum => "Includes the bank minimum".to_string(),
                                Coverage::NestedIn(outer) => format!("Nested in {}", household.reservation(outer).map(|o| o.name.clone()).unwrap_or_else(|| outer.to_string())),
                            };
                            let released = r.released_on;
                            TableRow::new()
                                .when(index % 2 == 1, |row| row.bg(theme.table_even))
                                .when(released.is_some(), |row| row.text_color(theme.muted_foreground))
                                .child(TableCell::new().w_40().flex_shrink_0().child(r.name.clone()))
                                .child(muted_cell(account_name, cx).w_40().flex_shrink_0())
                                .child(money_cell(r.amount, cx).w_32().flex_shrink_0())
                                .child(muted_cell(coverage, cx).w_56().flex_shrink_0())
                                .child(TableCell::new().w_32().flex_shrink_0().child(labels::hardness_tag(r.hardness)))
                                .child(muted_cell(r.purpose.clone(), cx).min_w_0().overflow_hidden().text_ellipsis())
                                .child(muted_cell(released.map(|d| format!("released {}", d.format("%d %b"))).unwrap_or_else(|| "active".into()), cx).w_40().flex_shrink_0())
                                .child(TableCell::new().w_32().flex_shrink_0().child(match released {
                                    None => Button::new(SharedString::from(format!("release-{}", id.raw())))
                                        .xsmall()
                                        .ghost()
                                        .label("Release…")
                                        .on_click(cx.listener(move |this, _, window, cx| this.open_release_reservation(id, window, cx)))
                                        .into_any_element(),
                                    Some(_) => div().into_any_element(),
                                }))
                        })))
                        .into_any_element()
                }),
        )
}

/// The viewer must have full disclosure on the account to add earmarks to it.
pub fn editable_accounts(household: &Household, viewer: Viewer) -> Vec<atlas_core::ids::AccountId> {
    household
        .accounts
        .iter()
        .filter(|a| !a.is_company_account() || household.disclosure_for(viewer, ObjectRef::Account(a.id)) == Disclosure::Full)
        .filter(|a| matches!(household.disclosure_for(viewer, ObjectRef::Account(a.id)), Disclosure::Full))
        .map(|a| a.id)
        .collect()
}
