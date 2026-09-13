//! Forecast / Path: the conditional cash path of a boundary under a case —
//! four figures, the chart against the hard floor with its exact values,
//! the account paths, the transfers a failing account would need, the
//! basis (conditional statement) and the reproducibility record.

use atlas_core::forecast::AccountPath;
use atlas_core::model::Household;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, WindowExt as _,
    accordion::Accordion,
    alert::Alert,
    button::{Button, ButtonVariants as _},
    chart::AreaChart,
    description_list::{DescriptionItem, DescriptionList},
    h_flex,
    select::Select,
    tab::{Tab, TabBar},
    tag::Tag,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::common::workspace_header;
use crate::app::AtlasApp;
use crate::models::projections::{ChartPoint, ProjectionModel};
use crate::nav::{Destination, Route};
use crate::widgets::chart::{self, Legend, PathCommand};
use crate::widgets::copy::copy_button;
use crate::widgets::explain;
use crate::widgets::figure::{ExplainedFigure, Figure};
use crate::widgets::grid;
use crate::widgets::record::{self, Lane};
use crate::widgets::scope;
use crate::widgets::statement;
use crate::widgets::states::{columns, fact, info_card, note, section};

fn date(d: chrono::NaiveDate) -> String {
    d.format("%d %b %Y").to_string()
}

/// The same figure under a shorter label. Four figures in one grid have room
/// for `At the horizon`, not `Conditional projected cash at horizon`: a label
/// that wraps onto a second line pushes its value out of line with the other
/// three. The calculation sheet still opens under the figure's full name.
fn relabel(figure: &ExplainedFigure, label: impl Into<SharedString>, emphasis: bool) -> Figure {
    Figure::new(figure.id.clone(), label, &figure.calc, figure.content()).emphasis(emphasis)
}

/// One control of the scope row, label beside its select rather than above it.
///
/// `widgets::scope::bar` stacks each label over its control, which costs this
/// screen three lines before the first figure. Composed here because the bar
/// is shared with every other analysis screen; it wants an inline mode.
fn inline_select(label: &'static str, choice: &scope::Choice, width: Pixels, cx: &App) -> impl IntoElement {
    h_flex()
        .flex_shrink_0()
        .gap_2()
        .items_center()
        .child(div().flex_shrink_0().text_xs().text_color(cx.theme().muted_foreground).child(label))
        .child(Select::new(choice).small().w(width))
}

/// The chart element of a path: cash as discrete steps against the floor.
fn path_chart(id: &'static str, points: Vec<ChartPoint>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let balance_color = theme.chart_1;
    let floor_color = theme.danger;
    let tick_margin = (points.len() / 8).max(1);
    if points.is_empty() {
        return div().h_64().w_full().flex().items_center().justify_center().text_sm().text_color(theme.muted_foreground).child("No planned postings: the balance stays where it starts through the horizon.").into_any_element();
    }
    // The area chart is the kit's multi-series chart; the cash series gets a
    // faint fill, the floor none, and both step at each posting.
    div()
        .h_64()
        .w_full()
        .child(
            AreaChart::new(points)
                .id(id)
                .x(|p: &ChartPoint| p.label.clone())
                .y(|p: &ChartPoint| p.balance)
                .stroke(balance_color)
                .fill(balance_color.opacity(0.08))
                .name("Cash")
                .step_after()
                .y(|p: &ChartPoint| p.floor)
                .stroke(floor_color)
                .fill(floor_color.opacity(0.0))
                .name("Hard floor")
                .step_after()
                .tick_margin(tick_margin),
        )
        .into_any_element()
}

pub fn render_path(app: &AtlasApp, model: &ProjectionModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    grid::sync(&app.grids.forecast_values, &model.path_rows, cx);
    let f = &model.forecast;
    let header = workspace_header(
        Destination::Forecast,
        Route::ForecastPath,
        vec![Button::new("forecast-record").small().outline().icon(IconName::Info).label("Forecast record…").on_click(cx.listener(|this, _, window, cx| this.open_forecast_record(window, cx))).into_any_element()],
        cx,
    );
    let scenario_name = f.scenario.and_then(|id| household.scenario(id)).map(|s| s.name.clone());
    let breach = f.breach.first_breach.is_some();
    let lowest_index = f.path.iter().enumerate().min_by_key(|(_, p)| p.balance.minor()).map(|(i, _)| i);
    let first_breach_index = f.breach.first_breach.and_then(|d| f.path.iter().position(|p| p.date >= d && p.balance.minor() < f.floor.minor()));
    let mut commands = Vec::new();
    if let Some(i) = lowest_index {
        commands.push(PathCommand { id: "show-lowest", label: "Show lowest point", select: i });
    }
    if let Some(i) = first_breach_index {
        commands.push(PathCommand { id: "show-first-breach", label: "Show first breach", select: i });
    }
    let showing_values = app.forecast_path_state.read(cx).values;
    let selected = app.forecast_path_state.read(cx).selected.and_then(|i| f.path.get(i).map(|p| (i, p)));
    let readout = selected.map(|(i, p)| {
        let against = match p.balance.checked_sub(f.floor) {
            Ok(h) if !h.is_negative() => format!("{} above the floor", h.format()),
            Ok(d) => format!("{} below the floor", d.abs().format()),
            Err(_) => String::new(),
        };
        chart::readout(
            format!("Selected: {} · posting {} of {}", date(p.date), i + 1, f.path.len()),
            vec![fact("Balance after posting", p.balance.format(), cx).into_any_element(), fact("Against the floor", against, cx).into_any_element(), fact("Hard floor", f.floor.format(), cx).into_any_element()],
            cx,
        )
    });
    let context = format!("{} · {} case · {} · through {}", f.boundary.label(household), f.case.label(), scenario_name.as_deref().map(|n| format!("with scenario “{n}”")).unwrap_or_else(|| "baseline".into()), date(f.through));
    let report_tab = app.forecast_report_tab;
    let report: AnyElement = match report_tab {
        1 => render_transfers(model, household, cx),
        2 => render_basis(app, model, household, cx),
        _ => render_account_paths(app, model, household, cx),
    };
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let border = theme.border;
    let radius = theme.radius;
    let legend = vec![Legend { name: format!("{} cash", f.boundary.label(household)).into(), color: theme.chart_1 }, Legend { name: "Hard floor".into(), color: theme.danger }];
    let values = grid::render("forecast-values-grid", &app.grids.forecast_values, cx).into_any_element();
    let chart_el = path_chart("forecast-line-chart", model.chart.clone(), cx);
    let cash_path = chart::cash_path("forecast-path", &app.forecast_path_state, context, chart_el, values, legend, commands, readout, cx);

    // The lowest point's date belongs in its label: as a qualifier under the
    // metadata it sat on a line the other three columns leave empty.
    let lowest = match f.lowest_date {
        Some(d) => relabel(&model.lowest, format!("Lowest · {}", date(d)), breach),
        None => relabel(&model.lowest, "Lowest", breach).qualifier("Never below the start"),
    };
    let days_line = format!(
        "Days below the floor {} · shortfall over time {} {}-days (how long and how deep, not an amount of cash){}",
        f.breach.days_below,
        f.breach.integrated_shortfall_currency_days,
        household.base_currency.code(),
        match (f.breach.first_breach, f.breach.recovery) {
            (Some(_), Some(r)) => format!(" · recovers {}", date(r)),
            (Some(_), None) => " · not recovered within this window".to_string(),
            _ => String::new(),
        }
    );
    // Whether the path holds is a fact about this screen, so it is a card and
    // not a muted sentence under the plot — and an alert only when the floor
    // is actually breached.
    let breach_fact: AnyElement = if breach {
        div()
            .id("forecast-breach-summary")
            .test_support()
            .w_full()
            .child(Alert::warning("forecast-breach-alert", days_line).title(f.breach.summary()))
            .into_any_element()
    } else {
        info_card("forecast-breach-summary", IconName::ChartLine, f.breach.summary(), days_line, cx)
    };

    v_flex()
        .id("screen-forecast")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        // The scope on one row — `Whose money [Household]  Case [Expected]
        // Plan [Baseline]` — with the horizon it cannot change at the trailing
        // edge and the case's own sentence beneath.
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
                        .child(
                            h_flex()
                                .flex_wrap()
                                .gap_5()
                                .items_center()
                                .child(inline_select("Whose money", &app.forecast_boundary_choice, px(200.), cx))
                                .child(inline_select("Case", &app.forecast_case_choice, px(150.), cx))
                                .child(inline_select("Plan", &app.plan_choices.forecast, px(190.), cx)),
                        )
                        .child(div().flex_shrink_0().text_xs().text_color(muted).child(format!("Balances {} → through {}", date(f.as_of), date(f.through)))),
                )
                .child(div().w_full().text_xs().text_color(muted).child(scope::case_description(f.case))),
        )
        // No heading over the figures: they are the answer, and the scope line
        // above already names the path they are on. The paragraph that stood
        // here repeated what every one of these figures carries as its own
        // `Conditional future` and `Scenario-tested` terms, each of which
        // opens Figure meanings.
        .child(
            div().id("forecast-figures").test_support().w_full().child(columns([
                relabel(&model.start, "Reconciled start", false).into_any_element(),
                relabel(&model.end, "At the horizon", !breach).into_any_element(),
                lowest.into_any_element(),
                relabel(&model.injection, "Extra needed at start", false).qualifier("The minimum addition, not the whole starting balance").into_any_element(),
            ])),
        )
        .child(
            v_flex()
                .id("forecast-chart")
                .test_support()
                .w_full()
                .gap_4()
                // The plot, its title, its Chart/Values toggle and its legend
                // are one bordered object: a borderless plot under a floating
                // title reads as three unrelated things stacked.
                .child(
                    v_flex()
                        .w_full()
                        .gap_2()
                        .p_4()
                        .rounded(radius)
                        .border_1()
                        .border_color(border)
                        // The title sits over the context line the shared
                        // `cash_path` frame already pairs with the
                        // Chart/Values toggle, so the card reads
                        // title / context — toggle from its top edge.
                        .child(div().w_full().text_sm().font_weight(FontWeight::MEDIUM).child(if showing_values { "Exact cash-path values" } else { "Conditional cash path" }))
                        .child(cash_path)
                        .child(div().w_full().text_xs().text_color(muted).child("The chart rounds to whole currency units and cannot show same-day order; the Values view lists every posting exactly.")),
                )
                .child(breach_fact)
                // The equation, not the chain as a table: fifteen rows of
                // per-series terms cost this screen a third of its height and
                // are what `Full calculation…` opens, listed exactly again in
                // the Values view and the Basis tab.
                .child(explain::render_equation("forecast-end-equation", model.end.calc.node(), model.end.content(), cx)),
        )
        .child(
            v_flex()
                .w_full()
                .gap_4()
                .child(
                    TabBar::new("forecast-report-tabs")
                        .selected_index(report_tab)
                        .on_click(cx.listener(|this, index: &usize, _, cx| {
                            this.forecast_report_tab = *index;
                            cx.notify();
                        }))
                        .children([Tab::new().label("Account paths"), Tab::new().label("Transfers"), Tab::new().label("Basis")]),
                )
                .child(report),
        )
        .into_any_element()
}

const ACCOUNT_LANES: [(&str, Lane); 8] = [
    ("Account", Lane::fixed(180.)),
    ("Start", Lane::money(100.)),
    ("End", Lane::money(100.)),
    ("Lowest / date", Lane::fixed(170.)),
    ("Floor", Lane::money(95.)),
    ("Postings", Lane::fixed(50.)),
    ("Status", Lane::fixed(150.)),
    ("", Lane::flex()),
];

fn account_status(a: &AccountPath) -> AnyElement {
    if let Some(d) = a.negative_from {
        Tag::danger().xsmall().outline().child(format!("Negative {}", d.format("%d %b"))).into_any_element()
    } else if let Some(d) = a.breach.first_breach {
        Tag::warning().xsmall().outline().child(format!("Below floor {}", d.format("%d %b"))).into_any_element()
    } else {
        Tag::secondary().xsmall().outline().child("Floor held").into_any_element()
    }
}

fn render_account_paths(app: &AtlasApp, model: &ProjectionModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let f = &model.forecast;
    let expanded = app.forecast_selected_account;
    let rows: Vec<_> = f
        .accounts
        .iter()
        .map(|a| {
            let id = a.account;
            let name = household.account(id).map(|acc| acc.name.clone()).unwrap_or_default();
            let share = if a.share_basis_points < 10_000 { format!(" · {}% share", a.share_basis_points / 100) } else { String::new() };
            let is_open = expanded == Some(id);
            record::row(
                SharedString::from(format!("account-path-{}", id.raw())),
                is_open,
                vec![
                    (ACCOUNT_LANES[0].1, record::stack(name, format!("{}{share}", household.account(id).map(|acc| acc.kind.label()).unwrap_or_default()), cx)),
                    (ACCOUNT_LANES[1].1, record::money(a.start, cx)),
                    (ACCOUNT_LANES[2].1, record::money(a.end, cx)),
                    (ACCOUNT_LANES[3].1, record::muted(format!("{}{}", a.lowest.format(), a.lowest_date.map(|d| format!(" · {}", date(d))).unwrap_or_default()), cx)),
                    (ACCOUNT_LANES[4].1, record::money(a.floor, cx)),
                    (ACCOUNT_LANES[5].1, record::muted(a.postings.len().to_string(), cx)),
                    (ACCOUNT_LANES[6].1, h_flex().child(account_status(a)).into_any_element()),
                    (
                        ACCOUNT_LANES[7].1,
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(Button::new(SharedString::from(format!("account-path-open-{}", id.raw()))).xsmall().ghost().compact().label(if is_open { "Close path" } else { "Open path" }).on_click(cx.listener(move |this, _, _, cx| {
                                this.forecast_selected_account = if this.forecast_selected_account == Some(id) { None } else { Some(id) };
                                this.account_path_state.update(cx, |s, cx| {
                                    s.selected = None;
                                    cx.notify();
                                });
                                cx.notify();
                            })))
                            .child(Button::new(SharedString::from(format!("account-open-{}", id.raw()))).xsmall().ghost().compact().icon(IconName::Landmark).tooltip("Open account").on_click(cx.listener(move |this, _, _, cx| this.navigate(Route::Account(id), cx))))
                            .into_any_element(),
                    ),
                ],
                |_, _, _| {},
            )
        })
        .collect();
    let expanded_path = expanded.and_then(|id| f.accounts.iter().find(|a| a.account == id)).map(|a| render_account_path(app, a, household, cx));
    section("forecast-accounts", "Account by account")
        .description("The total can be fine while one account fails. Company accounts stay in their company's path; a shared account counts by its share.")
        .child(if rows.is_empty() { note("No account is included in this boundary's path.", cx).into_any_element() } else { record::list("account-paths-list", record::header(&ACCOUNT_LANES, cx), rows).into_any_element() })
        .children(expanded_path)
        .into_any_element()
}

/// One account's exact path inside the run: its own chart against its own
/// floor, and its postings in order.
fn render_account_path(app: &AtlasApp, a: &AccountPath, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let name = household.account(a.account).map(|acc| acc.name.clone()).unwrap_or_default();
    let per_major = household.base_currency.minor_per_major() as f64;
    let floor = a.floor.minor() as f64 / per_major;
    let points: Vec<ChartPoint> = a.points.iter().map(|p| ChartPoint { label: SharedString::from(p.date.format("%d %b").to_string()), balance: p.balance.minor() as f64 / per_major, floor }).collect();
    let theme = cx.theme();
    let legend = vec![Legend { name: format!("{name} balance").into(), color: theme.chart_1 }, Legend { name: "Account floor".into(), color: theme.danger }];
    let lowest_index = a.points.iter().enumerate().min_by_key(|(_, p)| p.balance.minor()).map(|(i, _)| i);
    let commands = lowest_index.map(|i| PathCommand { id: "account-show-lowest", label: "Show lowest point", select: i }).into_iter().collect();
    let state = app.account_path_state.clone();
    let selected = state.read(cx).selected.and_then(|i| a.points.get(i).map(|p| (i, p)));
    let readout = selected.map(|(i, p)| chart::readout(format!("Selected: {} · posting {} of {}", date(p.date), i + 1, a.points.len()), vec![fact("Balance after posting", p.balance.format(), cx).into_any_element(), fact("Account floor", a.floor.format(), cx).into_any_element()], cx));
    const MAX_ROWS: usize = 40;
    let posting_lanes: [(&str, Lane); 5] = [("Date", Lane::fixed(120.)), ("Order", Lane::fixed(60.)), ("Posting", Lane::flex()), ("Amount", Lane::money(140.)), ("Balance after", Lane::money(150.))];
    let values_rows: Vec<_> = a
        .points
        .iter()
        .zip(a.postings.iter())
        .enumerate()
        .take(MAX_ROWS)
        .map(|(i, (p, posting))| {
            let state = state.clone();
            record::row(
                SharedString::from(format!("account-posting-{i}")),
                selected.is_some_and(|(s, _)| s == i),
                vec![
                    (posting_lanes[0].1, record::text(date(p.date))),
                    (posting_lanes[1].1, record::muted(posting.intraday_order.to_string(), cx)),
                    (posting_lanes[2].1, record::text(posting.label.clone())),
                    (posting_lanes[3].1, record::money(posting.amount, cx)),
                    (posting_lanes[4].1, record::money(p.balance, cx)),
                ],
                move |_, _, cx| {
                    state.update(cx, |s, cx| {
                        s.selected = Some(i);
                        cx.notify();
                    })
                },
            )
        })
        .collect();
    let values = v_flex()
        .w_full()
        .gap_1()
        .child(record::list("account-values-list", record::header(&posting_lanes, cx), values_rows))
        .when(a.points.len() > MAX_ROWS, |this| this.child(div().text_xs().text_color(theme.muted_foreground).child(format!("{} more postings after these.", a.points.len() - MAX_ROWS))))
        .into_any_element();
    let chart_el = path_chart("account-line-chart", points, cx);
    v_flex()
        .w_full()
        .gap_3()
        .pt_2()
        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(format!("{name} — exact path in this run")))
        .child(chart::cash_path("account-path", &state, format!("{} · floor {}", a.breach.summary(), a.floor.format()), chart_el, values, legend, commands, readout, cx))
        .into_any_element()
}

fn render_transfers(model: &ProjectionModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let lanes_def: [(&str, Lane); 5] = [("Date", Lane::fixed(120.)), ("Account", Lane::fixed(240.)), ("Shortfall", Lane::money(140.)), ("Coverable", Lane::flex()), ("", Lane::fixed(260.))];
    let rows: Vec<_> = model
        .forecast
        .transfer_points
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let account = t.account;
            let on = t.date;
            let amount = t.shortfall;
            let name = household.account(account).map(|a| a.name.clone()).unwrap_or_default();
            record::row(
                SharedString::from(format!("transfer-{i}")),
                false,
                vec![
                    (lanes_def[0].1, record::text(date(t.date))),
                    (lanes_def[1].1, record::link(format!("transfer-account-{i}"), name, cx.listener(move |this, _, _, cx| this.navigate(Route::Account(account), cx)))),
                    (lanes_def[2].1, record::money(t.shortfall, cx)),
                    (lanes_def[3].1, record::muted(if t.coverable { "Other accounts in this boundary have the headroom at that point" } else { "Not coverable from this boundary's other accounts" }, cx)),
                    (lanes_def[4].1, h_flex().justify_end().child(Button::new(SharedString::from(format!("plan-transfer-{i}"))).xsmall().outline().label("Plan transfer…").on_click(cx.listener(move |this, _, window, cx| this.open_transfer_plan(account, on, amount, window, cx)))).into_any_element()),
                ],
                |_, _, _| {},
            )
        })
        .collect();
    section("forecast-transfers", "Transfers an account would need")
        .description("Where an account falls below its floor on this path. Coverable means another included account has headroom then — not that a transfer happens, and not that fees or delays are solved.")
        .child(if rows.is_empty() { note("No account falls below its hard floor on this path; no internal transfer is needed.", cx).into_any_element() } else { record::list("transfers-list", record::header(&lanes_def, cx), rows).into_any_element() })
        .into_any_element()
}

fn render_basis(app: &AtlasApp, model: &ProjectionModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let text = model.statement.as_text();
    let content = model.end.content();
    let excluded: Vec<String> = model.forecast.record.excluded_accounts.iter().map(|(id, reason)| format!("{} — {reason}", household.account(*id).map(|a| a.name.clone()).unwrap_or_else(|| id.to_string()))).collect();
    let actions = vec![
        Button::new("basis-full-calculation").small().outline().icon(IconName::ListTree).label("Full calculation…").on_click(move |_, window, cx| explain::open_sheet(window, cx, content.clone())).into_any_element(),
        Button::new("basis-review-assumptions").small().ghost().label("Review assumptions").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Assumptions, cx))).into_any_element(),
        copy_button("basis-copy-statement", "Copy statement", text).into_any_element(),
        Button::new("basis-record").small().ghost().icon(IconName::Info).label("Forecast record…").on_click(cx.listener(|this, _, window, cx| this.open_forecast_record(window, cx))).into_any_element(),
    ];
    section("forecast-basis", "Basis")
        .description("What this run used and what its result establishes.")
        .child(statement::render(
            "forecast-statement",
            &model.statement,
            app.forecast_show_all_assumptions,
            cx.listener(|this, _, _, cx| {
                this.forecast_show_all_assumptions = !this.forecast_show_all_assumptions;
                cx.notify();
            }),
            actions,
            cx,
        ))
        .when(!excluded.is_empty(), |this| this.child(v_flex().gap_1().child(div().text_xs().text_color(cx.theme().muted_foreground).child("Excluded from this path")).children(excluded.into_iter().map(|e| div().text_sm().child(e)))))
        .into_any_element()
}

impl AtlasApp {
    /// The read-only reproducibility record of the forecast on show.
    pub fn open_forecast_record(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(model) = self.projection() else { return };
        let household = &self.household;
        let r = &model.forecast.record;
        let name = |id: atlas_core::ids::AccountId| household.account(id).map(|a| a.name.clone()).unwrap_or_else(|| id.to_string());
        let snapshot: Vec<String> = r.starting_snapshot.iter().map(|(id, money)| format!("{} {}", name(*id), money.format())).collect();
        let excluded: Vec<String> = r.excluded_accounts.iter().map(|(id, reason)| format!("{} — {reason}", name(*id))).collect();
        let series: Vec<String> = r.included_series.iter().filter_map(|id| household.series_by_id(*id)).map(|s| s.name.clone()).collect();
        let rules = r.rules_applied.clone();
        let packs = r.tax_rule_packs.clone();
        let policies: Vec<String> = r.policy_versions.iter().map(|(id, v)| format!("{id} at version {v}")).collect();
        let identity: Vec<(String, String)> = vec![
            ("Algorithm".into(), r.algorithm.to_string()),
            ("Boundary".into(), r.boundary.label(household)),
            ("Case".into(), format!("{} — {}", r.case.label(), r.case.description())),
            ("Plan".into(), r.scenario.and_then(|id| household.scenario(id)).map(|s| format!("with scenario “{}”", s.name)).unwrap_or_else(|| "baseline".into())),
            ("Balances as of".into(), date(r.as_of)),
            ("Through".into(), date(r.through)),
            ("Assumptions".into(), format!("{} used ({} not disclosed to this viewer)", r.assumptions.len(), model.hidden_assumptions)),
            ("Input hash".into(), format!("{:016x}", r.input_hash)),
        ];
        let mut text = String::from("Forecast record\n");
        for (k, v) in &identity {
            text.push_str(&format!("{k}: {v}\n"));
        }
        let collections: Vec<(&'static str, Vec<String>)> = vec![("Starting balances", snapshot), ("Included series", series), ("Excluded accounts", excluded), ("Rules applied", rules), ("Tax rule packs", packs), ("Policy versions", policies)];
        for (title, items) in &collections {
            text.push_str(&format!("{title}: {}\n", if items.is_empty() { "none".to_string() } else { items.join("; ") }));
        }
        let identity = std::rc::Rc::new(identity);
        let collections = std::rc::Rc::new(collections);
        // The accordion is stateless: the open set lives here across renders.
        let open_set: std::rc::Rc<std::cell::RefCell<std::collections::HashSet<usize>>> = std::rc::Rc::new(std::cell::RefCell::new([0usize].into_iter().collect()));
        window.open_sheet(cx, move |sheet, _, cx| {
            let muted = cx.theme().muted_foreground;
            let identity = identity.clone();
            let collections = collections.clone();
            let text = text.clone();
            let open_now = open_set.borrow().clone();
            let open_set = open_set.clone();
            let mut accordion = Accordion::new("forecast-record-collections").bordered(true).multiple(true).on_toggle_click(move |open, _, _| {
                *open_set.borrow_mut() = open.iter().copied().collect();
            });
            for (i, (title, items)) in collections.iter().enumerate() {
                let items = items.clone();
                let is_open = open_now.contains(&i);
                accordion = accordion.item(move |item| {
                    item.title(format!("{title} ({})", items.len())).open(is_open).child(if items.is_empty() {
                        div().text_xs().text_color(muted).child("None").into_any_element()
                    } else {
                        v_flex().gap_0p5().text_sm().children(items.iter().map(|s| div().child(s.clone()))).into_any_element()
                    })
                });
            }
            sheet
                .title("Forecast record")
                .size(relative(0.45))
                .child(
                    v_flex()
                        .gap_4()
                        .child(div().text_xs().text_color(muted).child("Enough to reproduce this run for the viewer who is looking. Read-only; the record is not permission to see values that are not disclosed."))
                        .child(DescriptionList::new().columns(1).children(identity.iter().map(|(k, v)| DescriptionItem::new(k.clone()).value(v.clone()))))
                        .child(accordion),
                )
                .footer(
                    h_flex()
                        .w_full()
                        .justify_between()
                        .items_center()
                        .child(copy_button("copy-forecast-record", "Copy record", text))
                        .child(Button::new("close-forecast-record").outline().small().label("Close").on_click(|_, window, cx| window.close_sheet(cx))),
                )
        });
    }

    /// The planned-movement form prefilled as a transfer into `to` on `on`
    /// for `amount`; the source stays for the person to choose.
    pub fn open_transfer_plan(&mut self, to: atlas_core::ids::AccountId, on: chrono::NaiveDate, amount: atlas_core::Money, window: &mut Window, cx: &mut Context<Self>) {
        self.open_entry(crate::entry::Entry::Series, window, cx);
        let f = &self.entry_forms;
        f.draft.update(cx, |d, cx| {
            d.series_direction = 2;
            cx.notify();
        });
        f.series_name.update(cx, |s, cx| s.set_value(format!("Transfer to {}", self.household.account(to).map(|a| a.name.clone()).unwrap_or_default()), window, cx));
        f.series_amount.update(cx, |s, cx| s.set_value(amount.format(), window, cx));
        f.series_from.update(cx, |s, cx| s.set_date(on, window, cx));
        if let Some(row) = f.account_row(to) {
            Self::set_choice(&f.series_transfer_to, row, window, cx);
        }
        if let Some(row) = self.entry_forms.recurrence_row("One time") {
            Self::set_choice(&self.entry_forms.series_recurrence, row, window, cx);
        }
    }
}
