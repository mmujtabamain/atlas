//! Forecast / Assumptions: the register of conditions every forecast relies
//! on, with acceptance; Derive from history (a fixed sample, a named formula,
//! a deliberate apply); Sensitivity (how far one assumption can move before
//! the path breaks, one at a time, with the joint caveat).

use atlas_core::assumptions::Derivation;
use atlas_core::ids::AssumptionId;
use atlas_core::model::{Assumption, Freshness, Household};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    alert::Alert,
    button::{Button, ButtonVariants as _},
    description_list::{DescriptionItem, DescriptionList},
    h_flex,
    radio::RadioGroup,
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
use crate::widgets::copy::copy_button;
use crate::widgets::explain;
use crate::widgets::figure::card;
use crate::widgets::labels;
use crate::widgets::record::{self, Lane};
use crate::widgets::scope;
use crate::widgets::statement::{self, Line, Statement};
use crate::widgets::states::{count_line, empty_state, fact, lanes, note, section};

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

const LANES: [(&str, Lane); 6] = [
    ("Condition", Lane::flex()),
    ("Certainty", Lane::fixed(130.)),
    ("Applies to", Lane::fixed(200.)),
    ("Accepted / expires", Lane::fixed(200.)),
    ("Freshness", Lane::fixed(110.)),
    ("", Lane::fixed(90.)),
];

pub fn render_register(app: &AtlasApp, model: &AssumptionsModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
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
            let accept: AnyElement = if freshness == Freshness::Fresh {
                div().into_any_element()
            } else {
                Button::new(SharedString::from(format!("accept-assumption-{}", id.raw())))
                    .xsmall()
                    .outline()
                    .label("Accept")
                    .tooltip("Record that you accept this condition as of the reconciliation date")
                    .on_click(cx.listener(move |this, _, window, cx| this.accept_assumption(id, window, cx)))
                    .into_any_element()
            };
            let row = record::row(
                SharedString::from(format!("assumption-row-{}", id.raw())),
                is_open,
                vec![
                    (LANES[0].1, v_flex().min_w_0().child(div().text_sm().whitespace_normal().child(a.text.clone())).child(div().text_xs().text_color(cx.theme().muted_foreground).child(source_short)).into_any_element()),
                    (LANES[1].1, h_flex().child(labels::certainty_tag(a.certainty)).into_any_element()),
                    (LANES[2].1, record::muted(if applies.is_empty() { "Every forecast".to_string() } else { applies.join(", ") }, cx)),
                    (LANES[3].1, record::stack(a.accepted_on.map(|d| format!("Accepted {}", date(d))).unwrap_or_else(|| "Not accepted".into()), a.expires_on.map(|d| format!("Expires {}", date(d))).unwrap_or_else(|| "No expiry".into()), cx)),
                    (LANES[4].1, h_flex().child(labels::freshness_tag(freshness)).into_any_element()),
                    (LANES[5].1, h_flex().justify_end().child(accept).into_any_element()),
                ],
                move |_, _, cx| crate::app::with_app(cx, |app, cx| app.toggle_assumption(id, cx)),
            );
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
            v_flex()
                .w_full()
                .child(row)
                .child(
                    v_flex()
                        .w_full()
                        .gap_2()
                        .px_3()
                        .py_2()
                        .child(
                            DescriptionList::new()
                                .columns(2)
                                .child(DescriptionItem::new("Source").value(a.source.describe()))
                                .child(DescriptionItem::new("Certainty").value(a.certainty.label()))
                                .child(DescriptionItem::new("Accepted").value(a.accepted_on.map(date).unwrap_or_else(|| "Not accepted".into())))
                                .child(DescriptionItem::new("Expires").value(a.expires_on.map(date).unwrap_or_else(|| "Never".into())))
                                .child(DescriptionItem::new("Freshness").value(match freshness {
                                    Freshness::Fresh => "Accepted within the last ninety days".to_string(),
                                    Freshness::Stale => "Accepted more than ninety days ago and not reviewed since".to_string(),
                                    Freshness::NotAccepted => "Nobody has accepted it yet".to_string(),
                                    Freshness::Expired => "Past its expiry; accepting it again does not move the expiry".to_string(),
                                }))
                                .child(DescriptionItem::new("Visibility").value(if a.private_to.is_some() { "Private to you".to_string() } else { "Household".to_string() })),
                        )
                        .when(!series_links.is_empty(), |this| this.child(h_flex().gap_1().items_center().child(div().text_xs().text_color(cx.theme().muted_foreground).child("Applies to")).children(series_links))),
                )
                .into_any_element()
        })
        .collect();
    let theme = cx.theme();
    let body: AnyElement = if household.assumptions.is_empty() {
        empty_state("assumptions-empty", "No assumptions recorded", "A forecast without recorded assumptions is not assumption-free; write down what it relies on.", Some(Button::new("assumptions-add-first").small().outline().icon(IconName::Plus).label("Add assumption…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Assumption, window, cx))).into_any_element()), cx)
    } else if visible.is_empty() {
        empty_state("assumptions-no-match", "No matches", "No disclosed assumption matches these filters.", Some(Button::new("assumptions-clear-2").small().ghost().label("Clear filters").on_click(cx.listener(|this, _, window, cx| this.clear_assumption_filters(window, cx))).into_any_element()), cx)
    } else {
        v_flex().w_full().gap_0p5().child(record::header(&LANES, cx)).children(rows).into_any_element()
    };
    v_flex()
        .id("screen-assumptions")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(scope::bar(
            vec![
                scope::select("Applies to", &controls.applies, px(240.), cx).into_any_element(),
                scope::select("Freshness", &controls.freshness, px(170.), cx).into_any_element(),
                Button::new("assumptions-clear").small().ghost().label("Clear").disabled(!filtered).on_click(cx.listener(|this, _, window, cx| this.clear_assumption_filters(window, cx))).into_any_element(),
            ],
            Some(format!("Freshness is judged on the reconciliation date, {}. Accepting is a review, not a refresh: a stale condition stays a condition until someone looks at it.", date(as_of))),
            cx,
        ))
        .child(count_line(visible.len(), if filtered { visible.len() } else { total_visible + hidden }, "assumptions", cx))
        .child(body)
        .when(hidden > 0, |this| this.child(div().text_xs().text_color(theme.muted_foreground).child(format!("{hidden} private to other people are not listed."))))
        .child(note(format!("Sensitivity below tests one assumption at a time on the {} path.", model.sensitivity_boundary.label(household)), cx))
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

    let result: AnyElement = if model.derivable.is_empty() {
        note("No series has derivation history. Recording or matching transactions does not extend it; a manually entered assumption remains available.", cx).into_any_element()
    } else {
        match (&model.derived, &model.derived_figure) {
            (Ok(derived), Some(figure)) => {
                let content = figure.content();
                v_flex()
                    .gap_3()
                    .child(lanes([
                        card(figure.leading()).into_any_element(),
                        fact("Range or amount", derived.amount.describe(), cx).into_any_element(),
                        fact("Sample", format!("{} payments · {} – {}", derived.sample_size, date(derived.sample_from), date(derived.sample_to)), cx).into_any_element(),
                    ]))
                    .child(div().text_sm().child(derived.statement.clone()))
                    .child(explain::render_preview(figure.calc.node(), content, cx))
                    .into_any_element()
            }
            (Err(err), _) => Alert::warning("derivation-error", format!("{err} — the engine reports what it has; nothing is fabricated to reach six payments.")).title("Not enough history").into_any_element(),
            _ => div().into_any_element(),
        }
    };

    let apply: AnyElement = match target {
        Some(t) if t.private_to.is_some_and(|p| p != viewer.person) => note("The assumption that applies to this series is private to someone else; it cannot be changed from here.", cx).into_any_element(),
        Some(t) => {
            let id = t.id;
            let can_apply = model.derived.is_ok();
            v_flex()
                .gap_2()
                .child(
                    DescriptionList::new()
                        .columns(1)
                        .child(DescriptionItem::new("Target assumption").value(t.text.clone()))
                        .child(DescriptionItem::new("Currently").value(format!("{} · {}", t.certainty.label(), t.source.describe())))
                        .child(DescriptionItem::new("After applying").value(model.derived.as_ref().map(|d| format!("{} · derived from history · acceptance cleared until reviewed", d.amount.describe())).unwrap_or_else(|_| "Nothing to apply yet".into()))),
                )
                .child(h_flex().child(Button::new("apply-derivation").small().primary().label("Apply derivation…").disabled(!can_apply).on_click(cx.listener(move |this, _, window, cx| this.confirm_apply_derivation(id, window, cx)))))
                .into_any_element()
        }
        None => h_flex()
            .gap_2()
            .items_center()
            .child(div().text_sm().text_color(theme.muted_foreground).child("No assumption applies to this series yet."))
            .child(Button::new("derive-add-assumption").small().outline().icon(IconName::Plus).label("Add assumption…").on_click(cx.listener(move |this, _, window, cx| this.open_assumption_for_series(series, window, cx))))
            .into_any_element(),
    };

    v_flex()
        .id("screen-derive")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(
            section("derive-inputs", "Sample and formula")
                .child(
                    h_flex()
                        .flex_wrap()
                        .gap_6()
                        .items_end()
                        .child(scope::select("Series with history", &app.assumption_controls.derive_series, px(280.), cx))
                        .child(scope::control(
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
                        ))
                        .child(scope::fixed("Sample size", "Last 6 payments", cx)),
                )
                .when_some(series_name.clone(), |this, name| this.child(h_flex().child(Button::new("derive-open-series").xsmall().ghost().compact().label(format!("Open {name}")).on_click(cx.listener(move |this, _, _, cx| this.navigate(Route::SeriesDetail(series), cx)))))),
        )
        .child(section("derive-result", "Derived amount").child(result))
        .child(
            section("derive-history", "Payments in the sample")
                .description("The reconciled history the formula read, newest first.")
                .child(if history_rows.is_empty() { note("No history for this series.", cx).into_any_element() } else { record::list("derive-history-list", record::header(&history_lanes, cx), history_rows).into_any_element() }),
        )
        .child(section("derive-target", "Apply to its assumption").description("Applying rewrites the target's amount and source. It then needs a fresh acceptance in the register; nothing is accepted automatically.").child(apply))
        .into_any_element()
}

pub fn render_sensitivity(app: &AtlasApp, model: &AssumptionsModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let report = &model.sensitivity;
    let pending = app.sensitivity_pending;
    let header = workspace_header(
        Destination::Forecast,
        Route::Sensitivity,
        vec![Button::new("run-sensitivity").small().primary().icon(IconName::Play).label("Run sensitivity").disabled(!pending).tooltip(if pending { "Recompute for the scope chosen below" } else { "The result below matches the scope" }).on_click(cx.listener(|this, _, _, cx| this.run_sensitivity(cx))).into_any_element()],
        cx,
    );
    let lanes_def: [(&str, Lane); 4] = [("Series", Lane::fixed(240.)), ("Tested change", Lane::fixed(220.)), ("Declared range", Lane::fixed(220.)), ("Breakpoint / result", Lane::flex())];
    let rows: Vec<_> = report
        .breakpoints
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let sid = b.series;
            let dimension = if b.breaking_date.is_some() || !b.searched.contains("per occurrence") { "Arrival date" } else { "Amount per occurrence" };
            let result = match (b.breaks_within_range, b.breaking_amount, b.breaking_date) {
                (true, Some(a), _) => format!("Breaks at {}", a.format()),
                (true, _, Some(d)) => format!("Breaks from {}", date(d)),
                _ => "No breach inside the declared range".to_string(),
            };
            record::row(
                SharedString::from(format!("breakpoint-{i}")),
                app.sensitivity_expanded == Some(i),
                vec![
                    (lanes_def[0].1, record::link(format!("breakpoint-series-{i}"), b.label.clone(), cx.listener(move |this, _, _, cx| this.navigate(Route::SeriesDetail(sid), cx)))),
                    (lanes_def[1].1, record::muted(dimension, cx)),
                    (lanes_def[2].1, record::muted(b.searched.clone(), cx)),
                    (lanes_def[3].1, h_flex().gap_2().items_center().child(record::text(result)).child(Button::new(SharedString::from(format!("breakpoint-open-{i}"))).xsmall().ghost().compact().label("Open…").on_click(cx.listener(move |this, _, _, cx| {
                        this.sensitivity_expanded = if this.sensitivity_expanded == Some(i) { None } else { Some(i) };
                        cx.notify();
                    }))).into_any_element()),
                ],
                |_, _, _| {},
            )
        })
        .collect();
    let expanded = app.sensitivity_expanded.and_then(|i| report.breakpoints.get(i)).map(|b| {
        v_flex()
            .w_full()
            .gap_1()
            .px_3()
            .py_2()
            .child(div().text_sm().child(b.statement.clone()))
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(format!("Searched {} with everything else held at its expected value.", b.searched)))
            .into_any_element()
    });
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
    let theme = cx.theme();
    let summary_line = if report.baseline_breaches {
        Alert::warning("sensitivity-baseline-breach", format!("The expected path already falls below the {} floor (lowest {}). No hypothetical limit repairs that; the limits below are measured from the breached path.", report.floor.format(), report.baseline_lowest.format())).into_any_element()
    } else {
        div().id("sensitivity-summary").test_support().text_sm().child(format!("Floor {} · lowest on the expected path {} · floor held at the expected values.", report.floor.format(), report.baseline_lowest.format())).into_any_element()
    };

    v_flex()
        .id("screen-sensitivity")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(scope::bar(
            vec![
                scope::select("Path", &app.assumption_controls.sensitivity_path, px(260.), cx).into_any_element(),
                scope::select("Plan", &app.plan_choices.sensitivity, px(200.), cx).into_any_element(),
                scope::fixed("Case", "Expected", cx).into_any_element(),
                scope::fixed("Through", date(model.horizon), cx).into_any_element(),
            ],
            Some(if pending { "Out of date: the scope changed. Run sensitivity to recompute.".into() } else { "Household or one personal cash account; the expected case only.".into() }),
            cx,
        ))
        .when(pending, |this| this.child(Alert::info("sensitivity-out-of-date", "The result below is for the previous scope. Run sensitivity to recompute for the scope you chose.").title("Out of date")))
        .child(
            section("sensitivity-summary-section", format!("{} on the expected path", report.boundary.label(household)))
                .child(lanes([
                    fact("Hard floor", report.floor.format(), cx).into_any_element(),
                    fact("Baseline lowest", report.baseline_lowest.format(), cx).into_any_element(),
                    fact("One-off unplanned-spending limit", report.unplanned_spending_limit.format(), cx).into_any_element(),
                ]))
                .child(summary_line)
                .child(note("The unplanned-spending limit is the headroom on the worst day: the largest single unexpected expense the path absorbs without breaching.", cx)),
        )
        .child(
            section("sensitivity-breakpoints", "Breakpoints, one assumption at a time")
                .child(if rows.is_empty() { note("No ranged assumptions to test: every series has an exact amount and date.", cx).into_any_element() } else { record::list("breakpoints-list", record::header(&lanes_def, cx), rows).into_any_element() })
                .children(expanded)
                .child(Alert::warning("joint-caveat", report.caveat).title("Single-assumption limits only"))
                .child(div().text_xs().text_color(theme.muted_foreground).child("Method: one variable at a time, bisection to the smallest currency unit, confirmed by replaying the forecast at the found value.")),
        )
        .child(section("sensitivity-statement", "What this result establishes").child(statement::render(
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
