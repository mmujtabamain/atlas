//! Forecast / Assumptions: the register of conditions every forecast relies
//! on, with acceptance; Derive from history (a fixed sample, a named formula,
//! a deliberate apply); Sensitivity (how far one assumption can move before
//! the path breaks, one at a time, with the joint caveat).

use atlas_core::Money;
use atlas_core::assumptions::Derivation;
use atlas_core::ids::AssumptionId;
use atlas_core::model::{Assumption, Freshness, Household};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    alert::Alert,
    button::{Button, ButtonVariants as _},
    h_flex,
    radio::RadioGroup,
    select::Select,
    tag::Tag,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::common::{detail_header, workspace_header};
use crate::app::AtlasApp;
use crate::entry::Entry;
use crate::models::assumptions::AssumptionsModel;
use crate::nav::{Destination, Route};
use crate::widgets::facts::facts;
use crate::widgets::copy::copy_button;
use crate::widgets::explain;
use crate::widgets::labels;
use crate::widgets::record::{self, Lane};
use crate::widgets::scope;
use crate::widgets::statement::{self, Line, Statement};
use crate::widgets::states::{action_bar, columns, columns_leading, count_line, empty_state, hairline, info_card, note, section};

fn date(d: chrono::NaiveDate) -> String {
    d.format("%d %b %Y").to_string()
}

/// Rows needing attention (not accepted, stale, expired) come first.
fn attention(f: Freshness) -> u8 {
    match f {
        Freshness::Expired => 0,
        Freshness::NotAccepted => 1,
        Freshness::Stale => 2,
        Freshness::Fresh => 3,
    }
}

/// One control of an inline filter or scope row: the label beside its control
/// rather than above it.
///
/// `widgets::scope::control` stacks each label over its control, which costs a
/// screen a whole line before its first row. The bar is shared with every
/// analysis screen, so the inline form is composed here — the same way
/// `screens::forecast` and `screens::accounts` compose theirs.
fn inline_control(label: &'static str, control: impl IntoElement, cx: &App) -> impl IntoElement {
    h_flex()
        .flex_shrink_0()
        .gap_2()
        .items_center()
        .child(div().flex_shrink_0().text_xs().text_color(cx.theme().muted_foreground).child(label))
        .child(control)
}

/// A labelled plain fact that fills its column.
///
/// `states::fact` is a fixed 16 rem card, which is wider than a cell of a grid
/// of five and would paint over its neighbour.
fn field(label: impl Into<SharedString>, value: impl Into<SharedString>, cx: &App) -> AnyElement {
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .child(div().w_full().text_xs().text_color(cx.theme().muted_foreground).child(label.into()))
        .child(div().w_full().text_sm().font_weight(FontWeight::MEDIUM).child(value.into()))
        .into_any_element()
}

/// The share of the register's width the condition sentence takes; the four
/// metadata columns share what is left.
const CONDITION: f32 = 0.36;

/// The register's column labels, on the grid its rows use.
fn register_header(cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let cell = move |text: &'static str| div().w_full().text_xs().text_color(muted).child(text).into_any_element();
    div()
        .w_full()
        .pb_2()
        .child(columns_leading(
            CONDITION,
            [
                cell("Condition / source"),
                cell("Certainty"),
                cell("Applies to"),
                cell("Accepted / expires"),
                h_flex().w_full().justify_end().child(div().text_xs().text_color(muted).child("Review")).into_any_element(),
            ],
        ))
        .into_any_element()
}

pub fn render_register(app: &AtlasApp, _model: &AssumptionsModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let header = workspace_header(
        Destination::Forecast,
        Route::Assumptions,
        vec![
            Button::new("new-assumption").small().outline().icon(IconName::Plus).label("Add assumption…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Assumption, window, cx))).into_any_element(),
            Button::new("derive-from-history").small().outline().label("Derive from history").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Derive, cx))).into_any_element(),
        ],
        cx,
    );
    let viewer = app.viewer();
    let controls = &app.assumption_controls;
    let applies_row = scope::selected(&controls.applies, cx);
    let freshness_row = scope::selected(&controls.freshness, cx);
    let filtered = applies_row > 0 || freshness_row > 0;
    let run_ids: Vec<AssumptionId> = app.projection().map(|p| p.forecast.assumptions.iter().map(|a| a.id).collect()).unwrap_or_default();
    let as_of = household.as_of;
    let mut visible: Vec<&Assumption> = household.assumptions.iter().filter(|a| a.private_to.is_none_or(|p| p == viewer.person)).collect();
    let total_visible = visible.len();
    let hidden = household.assumptions.len() - total_visible;
    visible.retain(|a| match applies_row {
        0 => true,
        1 => run_ids.contains(&a.id),
        n => controls.applies_series.get(n - 2).is_some_and(|s| a.applies_to.contains(s)),
    });
    visible.retain(|a| match freshness_row {
        0 => true,
        1 => a.freshness(as_of) == Freshness::Fresh,
        2 => a.freshness(as_of) == Freshness::Stale,
        3 => a.freshness(as_of) == Freshness::NotAccepted,
        _ => a.freshness(as_of) == Freshness::Expired,
    });
    visible.sort_by_key(|a| attention(a.freshness(as_of)));
    let expanded = app.assumption_expanded;
    let theme = cx.theme();
    let border = theme.border;
    let muted = theme.muted_foreground;
    let rows: Vec<AnyElement> = visible
        .iter()
        .map(|a| {
            let id = a.id;
            let freshness = a.freshness(as_of);
            let applies: Vec<String> = a.applies_to.iter().filter_map(|s| household.series_by_id(*s)).map(|s| s.name.clone()).collect();
            let is_open = expanded == Some(id);
            let source_short = match &a.source {
                atlas_core::model::AssumptionSource::UserEntered => "Entered by the user".to_string(),
                atlas_core::model::AssumptionSource::DerivedFromHistory { sample_size, .. } => format!("Derived from {sample_size} payments"),
                atlas_core::model::AssumptionSource::RulePack(p) => format!("Rule pack {p}"),
                atlas_core::model::AssumptionSource::Scenario(_) => "Created by a scenario".to_string(),
            };
            // The row's own commands: `Accept` while the condition needs a
            // review, and the chevron that opens its full source. The row is
            // not itself a click target — a register whose every row expands
            // on click cannot also carry a button that does something else.
            let accept: Option<AnyElement> = (freshness != Freshness::Fresh).then(|| {
                Button::new(SharedString::from(format!("accept-assumption-{}", id.raw())))
                    .xsmall()
                    .outline()
                    .label("Accept")
                    .tooltip("Record that you accept this condition as of the reconciliation date")
                    .on_click(cx.listener(move |this, _, window, cx| this.accept_assumption(id, window, cx)))
                    .into_any_element()
            });
            let disclose = Button::new(SharedString::from(format!("assumption-open-{}", id.raw())))
                .xsmall()
                .ghost()
                .compact()
                .icon(if is_open { IconName::ChevronUp } else { IconName::ChevronDown })
                .tooltip(if is_open { "Hide the full source" } else { "Show the full source" })
                .on_click(cx.listener(move |this, _, _, cx| this.toggle_assumption(id, cx)));
            // Text, certainty, applies-to, acceptance, freshness in columns of
            // definite width. The sentence never shares an auto-width cell
            // with a tag: that shape cost this very block 11,400 taffy measure
            // callbacks a frame (`docs/perf.md` §3.3). The source describes the
            // sentence, so it sits under it rather than in a narrow column of
            // its own where it would wrap to five lines.
            let grid = columns_leading(
                CONDITION,
                [
                    v_flex()
                        .w_full()
                        .gap_1()
                        .child(div().w_full().text_sm().child(a.text.clone()))
                        .child(div().w_full().text_xs().text_color(muted).child(source_short))
                        .into_any_element(),
                    h_flex().w_full().items_center().child(labels::certainty_tag(a.certainty)).into_any_element(),
                    div().w_full().text_xs().text_color(muted).child(if applies.is_empty() { "Every forecast".to_string() } else { applies.join(", ") }).into_any_element(),
                    v_flex()
                        .w_full()
                        .gap_0p5()
                        .child(div().w_full().text_xs().child(a.accepted_on.map(|d| format!("Accepted {}", date(d))).unwrap_or_else(|| "Not accepted".into())))
                        .child(div().w_full().text_xs().text_color(muted).child(a.expires_on.map(|d| format!("Expires {}", date(d))).unwrap_or_else(|| "No expiry".into())))
                        .into_any_element(),
                    h_flex().w_full().items_center().justify_end().gap_2().child(labels::freshness_tag(freshness)).children(accept).child(disclose).into_any_element(),
                ],
            );
            let row = v_flex().id(SharedString::from(format!("assumption-row-{}", id.raw()))).test_support().w_full().gap_3().py_3().border_b_1().border_color(border).child(grid);
            if !is_open {
                return row.into_any_element();
            }
            let series_links: Vec<AnyElement> = a
                .applies_to
                .iter()
                .filter_map(|s| household.series_by_id(*s))
                .map(|s| {
                    let sid = s.id;
                    Button::new(SharedString::from(format!("assumption-series-{}-{}", id.raw(), sid.raw()))).xsmall().ghost().compact().label(s.name.clone()).on_click(cx.listener(move |this, _, _, cx| this.navigate(Route::SeriesDetail(sid), cx))).into_any_element()
                })
                .collect();
            row.child(
                v_flex()
                    .w_full()
                    .gap_2()
                    .child(
                        facts()
                            .columns(2)
                            .pair("Source", a.source.describe())
                            .pair("Certainty", a.certainty.label())
                            .pair("Accepted", a.accepted_on.map(date).unwrap_or_else(|| "Not accepted".into()))
                            .pair("Expires", a.expires_on.map(date).unwrap_or_else(|| "Never".into()))
                            .pair("Freshness", match freshness {
                                Freshness::Fresh => "Accepted within the last ninety days".to_string(),
                                Freshness::Stale => "Accepted more than ninety days ago and not reviewed since".to_string(),
                                Freshness::NotAccepted => "Nobody has accepted it yet".to_string(),
                                Freshness::Expired => "Past its expiry; accepting it again does not move the expiry".to_string(),
                            })
                            .pair("Visibility", if a.private_to.is_some() { "Private to you".to_string() } else { "Household".to_string() }),
                    )
                    .when(!series_links.is_empty(), |this| this.child(h_flex().w_full().gap_1().items_center().child(div().text_xs().text_color(muted).child("Applies to")).children(series_links))),
            )
            .into_any_element()
        })
        .collect();
    let body: AnyElement = if household.assumptions.is_empty() {
        empty_state("assumptions-empty", "No assumptions recorded", "A forecast without recorded assumptions is not assumption-free; write down what it relies on.", Some(Button::new("assumptions-add-first").small().outline().icon(IconName::Plus).label("Add assumption…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Assumption, window, cx))).into_any_element()), cx)
    } else if visible.is_empty() {
        empty_state("assumptions-no-match", "No matches", "No disclosed assumption matches these filters.", Some(Button::new("assumptions-clear-2").small().ghost().label("Clear filters").on_click(cx.listener(|this, _, window, cx| this.clear_assumption_filters(window, cx))).into_any_element()), cx)
    } else {
        v_flex().w_full().child(register_header(cx)).child(hairline(cx)).children(rows).into_any_element()
    };
    v_flex()
        .id("screen-assumptions")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        // One inline row of filters with `Clear` at its trailing edge, and the
        // count of what they left underneath: the count answers the filters,
        // so it reads after them rather than before.
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
                                .child(inline_control("Applies to", Select::new(&controls.applies).small().w(px(240.)), cx))
                                .child(inline_control("Freshness", Select::new(&controls.freshness).small().w(px(170.)), cx)),
                        )
                        .child(Button::new("assumptions-clear").small().ghost().label("Clear").disabled(!filtered).on_click(cx.listener(|this, _, window, cx| this.clear_assumption_filters(window, cx)))),
                )
                .child(
                    h_flex()
                        .w_full()
                        .gap_1()
                        .items_center()
                        .child(count_line(visible.len(), if filtered { visible.len() } else { total_visible + hidden }, "assumptions", cx))
                        .when(hidden > 0, |this| this.child(note(format!("· {hidden} private to other people are not listed"), cx))),
                ),
        )
        .child(body)
        // What acceptance means is as true when nothing needs accepting, so it
        // is a card and not a muted afterthought under the rows.
        .child(info_card(
            "assumptions-review",
            IconName::ShieldCheck,
            "Review is explicit",
            format!("Freshness is judged on the reconciliation date, {}. Accepting records a review, not a refresh: a stale condition stays a condition until someone looks at it, and an expired one stays expired after it is accepted.", date(as_of)),
            cx,
        ))
        .into_any_element()
}

pub fn render_derive(app: &AtlasApp, model: &AssumptionsModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let header = detail_header(Destination::Forecast, Route::Assumptions, "Assumptions", "Derive from history", Some(div().text_sm().text_color(cx.theme().muted_foreground).child("A fixed sample of the last six reconciled payments and a named formula. This records what was paid, not what will be.").into_any_element()), vec![], cx);
    let formula_index = Derivation::ALL.iter().position(|d| *d == model.derivation).unwrap_or(0);
    let series = model.derivation_series;
    let series_name = household.series_by_id(series).map(|s| s.name.clone());
    let history = household.history_of(series);
    let history_lanes: [(&str, Lane); 2] = [("Date", Lane::fixed(160.)), ("Paid amount", Lane::money(160.))];
    let history_rows: Vec<_> = history
        .iter()
        .rev()
        .take(6)
        .enumerate()
        .map(|(i, h)| record::row(SharedString::from(format!("history-{i}")), false, vec![(history_lanes[0].1, record::text(date(h.date))), (history_lanes[1].1, record::money(h.amount, cx))], |_, _, _| {}))
        .collect();
    let target = household.assumptions.iter().find(|a| a.applies_to.contains(&series));
    let viewer = app.viewer();
    let theme = cx.theme();
    let muted = theme.muted_foreground;

    let result: AnyElement = if model.derivable.is_empty() {
        note("No series has derivation history. Recording or matching transactions does not extend it; a manually entered assumption remains available.", cx).into_any_element()
    } else {
        match (&model.derived, &model.derived_figure) {
            (Ok(derived), Some(figure)) => {
                let content = figure.content();
                v_flex()
                    .w_full()
                    .gap_4()
                    // The derived amount beside the window it was read from:
                    // the sample is what makes the number auditable, so it
                    // sits next to it rather than only inside the chain the
                    // `ⓘ` opens.
                    .child(columns_leading(
                        0.34,
                        [
                            figure.leading().into_any_element(),
                            field("Range or amount", derived.amount.describe(), cx),
                            field("Sample", format!("{} payments · {} – {}", derived.sample_size, date(derived.sample_from), date(derived.sample_to)), cx),
                        ],
                    ))
                    .child(div().w_full().text_sm().child(derived.statement.clone()))
                    // The named formula and its result on one line, with the
                    // command that opens the six payment terms — the shape
                    // every other screen states its equation in.
                    //
                    // Not `explain::render_equation`: that joins a node's terms
                    // with `+` and `−`, which is true of a sum and false of a
                    // median or a range. This chain is a formula node, so the
                    // line names the formula instead of pretending to add its
                    // sample up.
                    .child(
                        h_flex()
                            .w_full()
                            .items_start()
                            .justify_between()
                            .gap_4()
                            .child(div().flex_1().min_w_0().text_sm().child(format!("{} = {}", derived.derivation.describe(), figure.calc.node().value().render())))
                            .child(div().flex_shrink_0().child(
                                Button::new("derive-equation").xsmall().ghost().compact().icon(IconName::ListTree).label("Full calculation…").on_click(move |_, window, cx| explain::open_sheet(window, cx, content.clone())),
                            )),
                    )
                    .into_any_element()
            }
            (Err(err), _) => Alert::warning("derivation-error", format!("{err} — the engine reports what it has; nothing is fabricated to reach six payments.")).title("Not enough history").into_any_element(),
            _ => div().into_any_element(),
        }
    };

    // The target's before and after, and the one command that changes it. The
    // command sits in the bar at the foot of the screen, where a screen's own
    // commands live; the review it needs is stated beside it.
    let (target_detail, target_command): (AnyElement, Option<AnyElement>) = match target {
        Some(t) if t.private_to.is_some_and(|p| p != viewer.person) => (note("The assumption that applies to this series is private to someone else; it cannot be changed from here.", cx).into_any_element(), None),
        Some(t) => {
            let id = t.id;
            let can_apply = model.derived.is_ok();
            (
                facts()
                    .pair("Target assumption", t.text.clone())
                    .pair("Currently", format!("{} · {}", t.certainty.label(), t.source.describe()))
                    .pair("After applying", model.derived.as_ref().map(|d| format!("{} · derived from history · acceptance cleared until reviewed", d.amount.describe())).unwrap_or_else(|_| "Nothing to apply yet".into()))
                    .into_any_element(),
                Some(Button::new("apply-derivation").small().primary().label("Apply derivation…").disabled(!can_apply).on_click(cx.listener(move |this, _, window, cx| this.confirm_apply_derivation(id, window, cx))).into_any_element()),
            )
        }
        None => (
            div().text_sm().text_color(muted).child("No assumption applies to this series yet.").into_any_element(),
            Some(Button::new("derive-add-assumption").small().outline().icon(IconName::Plus).label("Add assumption…").on_click(cx.listener(move |this, _, window, cx| this.open_assumption_for_series(series, window, cx))).into_any_element()),
        ),
    };

    v_flex()
        .id("screen-derive")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        // The sample and the formula on one row — `Series [Groceries]  Formula
        // (Min–max) (Median) (Mean)` — with the sample size it cannot change at
        // the trailing edge. Stacked label-over-control pairs cost this screen
        // three lines before its first number.
        .child(
            v_flex()
                .id("derive-inputs")
                .test_support()
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
                                .child(inline_control("Series", Select::new(&app.assumption_controls.derive_series).small().w(px(260.)), cx))
                                .child(inline_control(
                                    "Formula",
                                    RadioGroup::horizontal("derive-formula")
                                        .children(Derivation::ALL.iter().map(|d| d.label()))
                                        .selected_index(Some(formula_index))
                                        .on_change(cx.listener(|this, index: &usize, _, cx| {
                                            if let Some(d) = Derivation::ALL.get(*index) {
                                                this.select_derivation(*d, cx);
                                            }
                                        })),
                                    cx,
                                )),
                        )
                        .child(
                            h_flex()
                                .flex_shrink_0()
                                .gap_3()
                                .items_center()
                                .child(div().text_xs().text_color(muted).child("Sample · last 6 payments"))
                                .when_some(series_name.clone(), |this, name| {
                                    this.child(Button::new("derive-open-series").xsmall().ghost().compact().label(format!("Open {name}")).on_click(cx.listener(move |this, _, _, cx| this.navigate(Route::SeriesDetail(series), cx))))
                                }),
                        ),
                )
                .child(div().w_full().text_xs().text_color(muted).child("Changing either recomputes the amount below. Nothing about the series or its assumption changes until the result is applied.")),
        )
        .child(section("derive-result", "Derived amount").child(result))
        .child(
            section("derive-history", "Payments in the sample")
                .divider(true)
                .description("The reconciled history the formula read, newest first.")
                .child(if history_rows.is_empty() { note("No history for this series.", cx).into_any_element() } else { record::list("derive-history-list", record::header(&history_lanes, cx), history_rows).into_any_element() }),
        )
        .child(section("derive-target", "Apply to its assumption").divider(true).child(target_detail))
        .child(action_bar(
            "derive-footer",
            vec![note("Applying rewrites the target's amount and source and clears its acceptance; it must be reviewed again in the register.", cx).into_any_element()],
            target_command.into_iter().collect(),
            cx,
        ))
        .into_any_element()
}

/// One measured value of the sensitivity band.
///
/// The engine reports the floor, the lowest point and the one-off limit as
/// plain amounts — `SensitivityReport` carries `Money`, not `Calc` — so there
/// is no chain for a `widgets::figure::Figure`'s `ⓘ` to open. They are stated
/// as the raw values they are, the way Today states `Spendable now`, with the
/// line that says what each one means underneath.
fn measure(id: &'static str, label: impl Into<SharedString>, value: Money, meaning: impl Into<SharedString>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .child(div().w_full().text_xs().text_color(theme.muted_foreground).child(label.into()))
        .child(
            div()
                .id(id)
                .test_support()
                .font_family(theme.mono_font_family.clone())
                .font_weight(FontWeight::SEMIBOLD)
                .text_xl()
                .text_color(if value.is_negative() { theme.danger } else { theme.foreground })
                .child(value.format()),
        )
        .child(div().w_full().text_xs().text_color(theme.muted_foreground).child(meaning.into()))
        .into_any_element()
}

const BREAKPOINT_LANES: [(&str, Lane); 5] = [
    ("Series", Lane::fixed(220.)),
    ("Tested change", Lane::fixed(170.)),
    ("Declared range", Lane::fixed(220.)),
    ("Breakpoint / result", Lane::flex()),
    ("", Lane::fixed(70.)),
];

pub fn render_sensitivity(app: &AtlasApp, model: &AssumptionsModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let report = &model.sensitivity;
    let pending = app.sensitivity_pending;
    let header = workspace_header(
        Destination::Forecast,
        Route::Sensitivity,
        vec![Button::new("run-sensitivity").small().primary().icon(IconName::Play).label("Run sensitivity").disabled(!pending).tooltip(if pending { "Recompute for the scope chosen below" } else { "The result below matches the scope" }).on_click(cx.listener(|this, _, _, cx| this.run_sensitivity(cx))).into_any_element()],
        cx,
    );
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let expanded_index = app.sensitivity_expanded;
    let rows: Vec<AnyElement> = report
        .breakpoints
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let sid = b.series;
            let is_open = expanded_index == Some(i);
            let dimension = if b.breaking_date.is_some() || !b.searched.contains("per occurrence") { "Arrival date" } else { "Amount per occurrence" };
            let result = match (b.breaks_within_range, b.breaking_amount, b.breaking_date) {
                (true, Some(a), _) => format!("Breaks at {}", a.format()),
                (true, _, Some(d)) => format!("Breaks from {}", date(d)),
                _ => "No breach inside the declared range".to_string(),
            };
            // The result sentence has the flexible lane to itself and the
            // command that opens it has its own at the trailing edge: text
            // beside a button in one auto-width cell is re-measured at every
            // ancestor's sizing pass (`docs/perf.md` §3.3).
            let row = record::row(
                SharedString::from(format!("breakpoint-{i}")),
                is_open,
                vec![
                    (BREAKPOINT_LANES[0].1, record::link(format!("breakpoint-series-{i}"), b.label.clone(), cx.listener(move |this, _, _, cx| this.navigate(Route::SeriesDetail(sid), cx)))),
                    (BREAKPOINT_LANES[1].1, record::muted(dimension, cx)),
                    (BREAKPOINT_LANES[2].1, record::muted(b.searched.clone(), cx)),
                    (BREAKPOINT_LANES[3].1, record::text(result)),
                    (
                        BREAKPOINT_LANES[4].1,
                        h_flex()
                            .justify_end()
                            .child(Button::new(SharedString::from(format!("breakpoint-open-{i}"))).xsmall().ghost().compact().label(if is_open { "Close" } else { "Open…" }).on_click(cx.listener(move |this, _, _, cx| {
                                this.sensitivity_expanded = if this.sensitivity_expanded == Some(i) { None } else { Some(i) };
                                cx.notify();
                            })))
                            .into_any_element(),
                    ),
                ],
                |_, _, _| {},
            );
            if !is_open {
                return row.into_any_element();
            }
            v_flex()
                .w_full()
                .child(row)
                .child(
                    v_flex()
                        .w_full()
                        .gap_1()
                        .px_3()
                        .py_2()
                        .child(div().w_full().text_sm().child(b.statement.clone()))
                        .child(div().w_full().text_xs().text_color(muted).child(format!("Searched {} with everything else held at its expected value.", b.searched))),
                )
                .into_any_element()
        })
        .collect();
    let viewer = app.viewer();
    let forecast_assumptions: Vec<Line> = household
        .assumptions
        .iter()
        .filter(|a| a.private_to.is_none_or(|p| p == viewer.person))
        .filter(|a| a.applies_to.is_empty() || a.applies_to.iter().any(|s| household.series_by_id(*s).is_some()))
        .map(|a| Line::from_assumption(a, household.as_of))
        .collect();
    let some_hidden = household.assumptions.iter().any(|a| a.private_to.is_some_and(|p| p != viewer.person));
    let plan = model.sensitivity_scenario.then(|| model.overlay_scenario.and_then(|id| household.scenario(id)).map(|s| format!("with scenario “{}”", s.name))).flatten().unwrap_or_else(|| "baseline".into());
    let statement = Statement {
        claim: model.statement.claim.clone(),
        through: model.horizon,
        scope: format!("{} · Expected case · {plan}", model.sensitivity_boundary.label(household)),
        assumptions: forecast_assumptions,
        some_hidden,
        strength: model.statement.coverage,
        coverage: "Each limit is found on its own by bisection, exact to the smallest currency unit, and confirmed by re-running the forecast at that value.".into(),
        does_not_establish: "Safety when several assumptions change at once, or under the conservative and optimistic cases.".into(),
        excluded_shocks: model.statement.excluded_shocks.join("; "),
    };
    let text = statement.as_text();
    // Whether the floor holds is the reading the three figures are under, and
    // it keeps its id whichever way it reads: a breach is an alert, a floor
    // that holds is the sentence that states the arithmetic between them.
    let summary: AnyElement = div()
        .id("sensitivity-summary")
        .test_support()
        .w_full()
        .child(if report.baseline_breaches {
            Alert::warning("sensitivity-baseline-breach", format!("The expected path already falls below the {} floor (lowest {}). No hypothetical limit repairs that; the limits below are measured from the breached path.", report.floor.format(), report.baseline_lowest.format())).into_any_element()
        } else {
            v_flex()
                .w_full()
                .gap_1()
                .child(div().w_full().text_sm().child(format!(
                    "{} lowest on the expected path − {} hard floor = {} one-off unplanned-spending limit",
                    report.baseline_lowest.format(),
                    report.floor.format(),
                    report.unplanned_spending_limit.format()
                )))
                .child(div().w_full().text_xs().text_color(muted).child(format!("The expected path holds the floor through {}, before any assumption is moved.", date(report.through))))
                .into_any_element()
        })
        .into_any_element();

    v_flex()
        .id("screen-sensitivity")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        // The scope on one row — `Path [Household]  Plan [Baseline]` — with the
        // case and the horizon it cannot change at the trailing edge.
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
                                .child(inline_control("Path", Select::new(&app.assumption_controls.sensitivity_path).small().w(px(240.)), cx))
                                .child(inline_control("Plan", Select::new(&app.plan_choices.sensitivity).small().w(px(190.)), cx)),
                        )
                        .child(div().flex_shrink_0().text_xs().text_color(muted).child(format!("Expected case · through {}", date(model.horizon)))),
                )
                // The standing rule of the scope, not the state of the result:
                // that it is out of date is said once, in the alert below.
                .child(div().w_full().text_xs().text_color(muted).child("Household or one personal cash account, on the expected case only.")),
        )
        .when(pending, |this| this.child(Alert::info("sensitivity-out-of-date", "The result below is for the previous scope. Run sensitivity to recompute for the scope you chose.").title("Out of date")))
        // No heading over the three figures: they are the answer, and the
        // scope row above already names the path they are measured on.
        .child(
            v_flex()
                .id("sensitivity-summary-section")
                .test_support()
                .w_full()
                .gap_4()
                .child(columns([
                    measure("sensitivity-floor", "Hard floor", report.floor, "Hard earmarks and bank minimums that must stay in the accounts.", cx),
                    measure("sensitivity-lowest", format!("Lowest on the {} path", report.boundary.label(household)), report.baseline_lowest, "The worst day of the expected path, before any assumption is moved.", cx),
                    measure("sensitivity-limit", "One-off unplanned-spending limit", report.unplanned_spending_limit, "The largest single unexpected expense that worst day absorbs without breaching.", cx),
                ]))
                .child(summary),
        )
        .child(
            section("sensitivity-breakpoints", "Breakpoints, one assumption at a time")
                .divider(true)
                .badge("Expected · One variable")
                .child(if rows.is_empty() {
                    note("No ranged assumptions to test: every series has an exact amount and date.", cx).into_any_element()
                } else {
                    v_flex().id("breakpoints-list").w_full().gap_0p5().child(record::header(&BREAKPOINT_LANES, cx)).children(rows).into_any_element()
                })
                // The joint caveat is as true when every limit is comfortable,
                // so it is a card and not an alert: an alert is for something
                // that is actually wrong.
                .child(info_card("joint-caveat", IconName::Info, "Single-assumption limits only", report.caveat, cx))
                .child(div().w_full().text_xs().text_color(muted).child("Method: one variable at a time, bisection to the smallest currency unit, confirmed by replaying the forecast at the found value.")),
        )
        .child(section("sensitivity-statement", "What this result establishes").divider(true).child(statement::render(
            "sensitivity-statement-block",
            &statement,
            app.forecast_show_all_assumptions,
            cx.listener(|this, _, _, cx| {
                this.forecast_show_all_assumptions = !this.forecast_show_all_assumptions;
                cx.notify();
            }),
            vec![
                copy_button("copy-sensitivity-statement", "Copy statement", text).into_any_element(),
                Button::new("sensitivity-review-assumptions").small().ghost().label("Review assumptions").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Assumptions, cx))).into_any_element(),
            ],
            cx,
        )))
        .into_any_element()
}

impl AtlasApp {
    /// Whether the sensitivity result on show is behind its chosen scope.
    pub fn sensitivity_pending(&self) -> bool {
        self.sensitivity_pending
    }

    pub fn toggle_assumption(&mut self, id: AssumptionId, cx: &mut Context<Self>) {
        self.assumption_expanded = if self.assumption_expanded == Some(id) { None } else { Some(id) };
        cx.notify();
    }

    pub fn clear_assumption_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        Self::set_choice(&self.assumption_controls.applies, 0, window, cx);
        Self::set_choice(&self.assumption_controls.freshness, 0, window, cx);
        cx.notify();
    }

    /// Recomputes sensitivity for the chosen scope.
    pub fn run_sensitivity(&mut self, cx: &mut Context<Self>) {
        log::info!("sensitivity run: {:?} scenario={}", self.sensitivity_boundary, self.sensitivity_scenario);
        self.sensitivity_pending = false;
        self.sensitivity_expanded = None;
        self.refresh_assumptions();
        cx.notify();
    }

    /// Reviews the before/after before applying a derivation.
    pub fn confirm_apply_derivation(&mut self, assumption: AssumptionId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(model) = self.assumptions() else { return };
        let Ok(derived) = &model.derived else { return };
        let Some(target) = self.household.assumptions.iter().find(|a| a.id == assumption) else { return };
        let lines = vec![
            format!("Assumption: {}", target.text),
            format!("Now: {}", target.source.describe()),
            format!("After: {} — {}", derived.amount.describe(), derived.derivation.describe()),
            "Its acceptance is cleared; review and accept it in the register.".to_string(),
        ];
        super::common::confirm_primary(window, cx, "Apply this derivation?", lines, "Apply derivation", move |window, cx| {
            crate::app::with_app(cx, |app, cx| app.apply_derivation(assumption, window, cx));
        });
    }
}

/// A tag for a breakpoint's result, for tests and other screens.
pub fn breakpoint_tag(breaks: bool) -> Tag {
    if breaks { Tag::warning().xsmall().outline().child("Breaks within range") } else { Tag::secondary().xsmall().outline().child("No breach in range") }
}
