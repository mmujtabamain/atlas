//! Timeline (§9, §15, §16, M05): every planned occurrence in a window with
//! its four clocks, status and reconciliation; the series that generate them
//! with their exceptions and effective-dated changes; the actual transactions
//! they are reconciled to.

use atlas_core::authz::Viewer;
use atlas_core::ids::{AccountId, EntityRef, ObjectRef, ScenarioId, SeriesId};
use atlas_core::model::Household;
use atlas_core::timeline::{Direction, Occurrence, OccurrenceStatus};
use atlas_core::vocab::Certainty;
use atlas_core::{Disclosure, EngineResult, Money};
use chrono::NaiveDate;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    group_box::GroupBox, h_flex,
    select::Select,
    table::{Table, TableBody, TableCell, TableHead, TableHeader, TableRow},
    tag::Tag, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::{AtlasApp, TimelineControls};
use crate::widgets::labels;
use crate::widgets::master::page_header;
use crate::widgets::table::{money_cell, muted_cell};

/// What the timeline shows (§9 filters).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineFilter {
    pub entity: Option<EntityRef>,
    pub account: Option<AccountId>,
    pub certainty: Option<Certainty>,
    pub status: Option<OccurrenceStatus>,
    pub scenario: Option<ScenarioId>,
    pub through: NaiveDate,
}

/// Rows and lists for the screen, computed once per state change.
#[derive(Clone, Debug)]
pub struct TimelineModel {
    pub filter: TimelineFilter,
    pub occurrences: Vec<Occurrence>,
    pub total_in: Money,
    pub total_out: Money,
    /// Series the viewer may see, in fixture order.
    pub series: Vec<SeriesId>,
    pub hidden_series: usize,
}

impl TimelineModel {
    pub fn compute(household: &Household, viewer: Viewer, filter: TimelineFilter) -> EngineResult<Self> {
        log::info!("computing timeline through {} for viewer {} (scenario {:?})", filter.through, viewer.person, filter.scenario);
        let visible_account = |id: AccountId| !matches!(household.disclosure_for(viewer, ObjectRef::Account(id)), Disclosure::Hidden | Disclosure::Aggregate);
        let visible_entity = |entity: EntityRef| match entity {
            EntityRef::Company(id) => matches!(household.disclosure_for(viewer, ObjectRef::Company(id)), Disclosure::Full | Disclosure::SelectedFields),
            _ => true,
        };
        let series: Vec<SeriesId> = household
            .series
            .iter()
            .filter(|s| visible_account(s.account) && visible_entity(s.entity))
            .filter(|s| s.scenario.is_none() || s.scenario == filter.scenario)
            .map(|s| s.id)
            .collect();
        let hidden_series = household.series.iter().filter(|s| s.scenario.is_none() || s.scenario == filter.scenario).count() - series.len();

        let occurrences: Vec<Occurrence> = household
            .expand_all(household.as_of, filter.through, filter.scenario)
            .into_iter()
            .filter(|o| series.contains(&o.series))
            .filter(|o| filter.entity.is_none_or(|e| o.entity == e))
            .filter(|o| filter.account.is_none_or(|a| o.account == a || o.linked_account == Some(a) || matches!(o.direction, Direction::Transfer { to } if to == a)))
            .filter(|o| filter.certainty.is_none_or(|c| o.certainty == c))
            .filter(|o| filter.status.is_none_or(|s| o.status == s))
            .collect();
        let currency = household.base_currency;
        let total_in = Money::sum(currency, occurrences.iter().filter(|o| o.direction == Direction::Income && o.is_live()).map(|o| o.remaining_expected()))?;
        let total_out = Money::sum(currency, occurrences.iter().filter(|o| o.direction == Direction::Expense && o.is_live()).map(|o| o.remaining_expected()))?;
        Ok(TimelineModel { filter, occurrences, total_in, total_out, series, hidden_series })
    }
}

fn status_tag(status: OccurrenceStatus) -> Tag {
    match status {
        OccurrenceStatus::Overdue => Tag::danger().xsmall().outline().child(status.label()),
        OccurrenceStatus::Due | OccurrenceStatus::PartiallyFulfilled => Tag::warning().xsmall().outline().child(status.label()),
        _ => Tag::secondary().xsmall().outline().child(status.label()),
    }
}

pub fn render(model: &TimelineModel, controls: &TimelineControls, household: &Household, viewer: Viewer, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    let scenario_name = model.filter.scenario.and_then(|id| household.scenario(id)).map(|s| s.name.clone());
    let buy_car_on = model.filter.scenario.is_some();

    v_flex()
        .id("screen-timeline")
        .test_support()
        .gap_6()
        .child(page_header(
            "Timeline",
            format!(
                "{} occurrences from {} through {} — every planned movement with its due, posting, settlement and availability dates (M05) and its reconciliation status (§16)",
                model.occurrences.len(),
                household.as_of.format("%d %b %Y"),
                model.filter.through.format("%d %b %Y")
            ),
            cx,
        ))
        .child(
            h_flex()
                .flex_wrap()
                .gap_3()
                .items_end()
                .child(labelled("Entity", Select::new(&controls.entity).small().w_48(), cx))
                .child(labelled("Account", Select::new(&controls.account).small().w_56(), cx))
                .child(labelled("Certainty", Select::new(&controls.certainty).small().w_40(), cx))
                .child(labelled("Status", Select::new(&controls.status).small().w_40(), cx))
                .child(labelled("Horizon", Select::new(&controls.horizon).small().w_48(), cx))
                .child(
                    Checkbox::new("timeline-buy-car")
                        .label("Overlay scenario “Buy car” (§18)")
                        .checked(buy_car_on)
                        .on_change(cx.listener(|this, checked, _, cx| this.set_timeline_scenario(*checked, cx))),
                ),
        )
        .child(
            GroupBox::new().id("timeline-occurrences").title(match &scenario_name {
                Some(name) => format!("Occurrences — baseline + scenario “{name}”"),
                None => "Occurrences — baseline".to_string(),
            }).child(
                v_flex()
                    .gap_3()
                    .child(div().text_xs().text_color(theme.muted_foreground).child(format!(
                        "Live inflows {} · live outflows {} (expected values, remainders after reconciliation). Skipped, cancelled and fulfilled rows post nothing (§16).",
                        model.total_in.format(),
                        model.total_out.format()
                    )))
                    .child(if model.occurrences.is_empty() {
                        div().text_sm().text_color(theme.muted_foreground).child("No occurrences match the filters in this window.").into_any_element()
                    } else {
                        render_occurrences(model, household, cx).into_any_element()
                    }),
            ),
        )
        .child(render_series(model, household, cx))
        .child(render_actuals(household, viewer, cx))
}

fn labelled(label: &'static str, control: impl IntoElement, cx: &App) -> impl IntoElement {
    v_flex().gap_1().child(div().text_xs().text_color(cx.theme().muted_foreground).child(label)).child(control)
}

fn render_occurrences(model: &TimelineModel, household: &Household, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let date = |d: NaiveDate| d.format("%d %b %y").to_string();
    Table::new()
        .child(
            TableHeader::new().child(
                TableRow::new()
                    .child(TableHead::new().w_20().flex_shrink_0().child("Due"))
                    .child(TableHead::new().w_20().flex_shrink_0().child("Settles"))
                    .child(TableHead::new().w_20().flex_shrink_0().child("Available"))
                    .child(TableHead::new().min_w_0().child("Series · entity"))
                    .child(TableHead::new().w_56().flex_shrink_0().child("Account"))
                    .child(TableHead::new().w_48().flex_shrink_0().text_right().child("Expected (signed)"))
                    .child(TableHead::new().w_32().flex_shrink_0().child("Certainty"))
                    .child(TableHead::new().w_32().flex_shrink_0().child("Status")),
            ),
        )
        .child(TableBody::new().children(model.occurrences.iter().enumerate().map(|(index, o)| {
            let account = household.account(o.account).map(|a| a.name.clone()).unwrap_or_default();
            let account_text = match o.direction {
                Direction::Transfer { to } => format!("{account} → {}", household.account(to).map(|a| a.name.as_str()).unwrap_or("?")),
                _ => match o.linked_account.and_then(|id| household.account(id)) {
                    Some(linked) => format!("{account} ⇄ {}", linked.name),
                    None => account,
                },
            };
            let mut subtitle = household.entity_name(o.entity);
            if o.original_due != o.due {
                subtitle.push_str(&format!(" · moved from {}", date(o.original_due)));
            }
            if let Some(scenario) = o.scenario.and_then(|id| household.scenario(id)) {
                subtitle.push_str(&format!(" · scenario “{}”", scenario.name));
            }
            if o.linked_account.is_some() {
                subtitle.push_str(" · linked movement (§8.4)");
            }
            let remaining = o.remaining_expected();
            let signed = match o.direction {
                Direction::Income => format!("+{}", remaining.format()),
                Direction::Expense => format!("−{}", remaining.format()),
                Direction::Transfer { .. } => format!("→ {}", remaining.format()),
            };
            let detail = if o.fulfilled.is_positive() {
                Some(format!("{} of {} received", o.fulfilled.format(), o.amount.expected().format()))
            } else if o.amount.low() != o.amount.high() {
                Some(format!("{}–{}", o.amount.low().format(), o.amount.high().format()))
            } else {
                None
            };
            let live = o.is_live();
            let amount_color = match o.direction {
                Direction::Income if live => theme.success,
                _ => theme.foreground,
            };
            TableRow::new()
                .when(index % 2 == 1, |row| row.bg(theme.table_even))
                .when(!live, |row| row.text_color(theme.muted_foreground))
                .child(TableCell::new().w_20().flex_shrink_0().child(date(o.due)))
                .child(muted_cell(date(o.settlement), cx).w_20().flex_shrink_0())
                .child(muted_cell(date(o.availability), cx).w_20().flex_shrink_0())
                .child(
                    TableCell::new().min_w_0().overflow_hidden().child(
                        v_flex()
                            .min_w_0()
                            .child(div().overflow_hidden().text_ellipsis().child(o.label.clone()))
                            .child(div().text_xs().text_color(theme.muted_foreground).overflow_hidden().text_ellipsis().child(subtitle)),
                    ),
                )
                .child(muted_cell(account_text, cx).w_56().flex_shrink_0().overflow_hidden().text_ellipsis())
                .child(
                    TableCell::new().w_48().flex_shrink_0().text_right().child(
                        v_flex()
                            .items_end()
                            .child(
                                div()
                                    .font_family(theme.mono_font_family.clone())
                                    .text_color(if live { amount_color } else { theme.muted_foreground })
                                    .when(!live, |c| c.line_through())
                                    .child(signed),
                            )
                            .when_some(detail, |this, text| this.child(div().text_xs().text_color(theme.muted_foreground).child(text))),
                    ),
                )
                .child(TableCell::new().w_32().flex_shrink_0().child(labels::certainty_tag(o.certainty)))
                .child(TableCell::new().w_32().flex_shrink_0().child(status_tag(o.status)))
        })))
}

fn render_series(model: &TimelineModel, household: &Household, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    GroupBox::new().id("timeline-series").title("Event series (§5.6, §9)").child(
        v_flex()
            .gap_3()
            .child(div().text_xs().text_color(theme.muted_foreground).child(format!(
                "{} series generate the occurrences above{}. Edit one occurrence, this-and-future amounts, or end a series without recreating anything (§9.2–9.4).",
                model.series.len(),
                if model.hidden_series > 0 { format!(" ({} not disclosed to this viewer)", model.hidden_series) } else { String::new() }
            )))
            .child(
                Table::new()
                    .child(
                        TableHeader::new().child(
                            TableRow::new()
                                .child(TableHead::new().w_64().flex_shrink_0().child("Series"))
                                .child(TableHead::new().w_20().flex_shrink_0().child("Direction"))
                                .child(TableHead::new().w_40().flex_shrink_0().text_right().child("Amount"))
                                .child(TableHead::new().min_w_0().child("Recurrence · clocks"))
                                .child(TableHead::new().w_64().flex_shrink_0().child("Changes and exceptions"))
                                .child(TableHead::new().w_32().flex_shrink_0().child("Certainty"))
                                .child(TableHead::new().w_20().flex_shrink_0().child("")),
                        ),
                    )
                    .child(TableBody::new().children(model.series.iter().enumerate().filter_map(|(index, id)| {
                        let series = household.series_by_id(*id)?;
                        let id = *id;
                        let mut edits: Vec<String> = series.amount_changes.iter().map(|c| format!("from {}: {}", c.effective_from.format("%d %b %Y"), c.amount.describe())).collect();
                        edits.extend(series.exceptions.iter().map(|e| e.describe()));
                        let clocks = format!(
                            "{} · settles +{}d · available +{}d · order {}",
                            series.recurrence.describe(),
                            series.settlement_lag_days,
                            series.availability_lag_days,
                            series.intraday_order
                        );
                        Some(
                            TableRow::new()
                                .when(index % 2 == 1, |row| row.bg(theme.table_even))
                                .child(
                                    TableCell::new().w_64().flex_shrink_0().child(
                                        v_flex()
                                            .child(series.name.clone())
                                            .child(div().text_xs().text_color(theme.muted_foreground).child(format!(
                                                "{} · {}",
                                                household.entity_name(series.entity),
                                                household.account(series.account).map(|a| a.name.clone()).unwrap_or_default()
                                            ))),
                                    ),
                                )
                                .child(muted_cell(series.direction.label(), cx).w_20().flex_shrink_0())
                                .child(money_cell(series.amount.expected(), cx).w_40().flex_shrink_0())
                                .child(muted_cell(clocks, cx).min_w_0().overflow_hidden())
                                .child(muted_cell(if edits.is_empty() { "—".to_string() } else { edits.join(" · ") }, cx).w_64().flex_shrink_0().overflow_hidden())
                                .child(TableCell::new().w_32().flex_shrink_0().child(labels::certainty_tag(series.certainty)))
                                .child(
                                    TableCell::new().w_20().flex_shrink_0().child(
                                        Button::new(SharedString::from(format!("edit-series-{}", id.raw())))
                                            .xsmall()
                                            .ghost()
                                            .icon(IconName::Pencil)
                                            .tooltip("Edit series…")
                                            .on_click(cx.listener(move |this, _, window, cx| this.open_series_editor(id, window, cx))),
                                    ),
                                ),
                        )
                    }))),
            ),
    )
}

fn render_actuals(household: &Household, viewer: Viewer, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let visible: Vec<_> = household
        .actuals
        .iter()
        .filter(|t| matches!(household.disclosure_for(viewer, ObjectRef::Account(t.account)), Disclosure::Full | Disclosure::SelectedFields))
        .collect();
    GroupBox::new().id("timeline-actuals").title("Actual transactions and reconciliation (§5.7, §16)").child(
        v_flex()
            .gap_3()
            .child(div().text_xs().text_color(theme.muted_foreground).child(
                "Once an actual transaction is linked, the planned occurrence counts only its remainder — never twice (V012).",
            ))
            .child(if visible.is_empty() {
                div().text_sm().text_color(theme.muted_foreground).child("No actual transactions visible to this viewer.").into_any_element()
            } else {
                Table::new()
                    .child(
                        TableHeader::new().child(
                            TableRow::new()
                                .child(TableHead::new().w_24().flex_shrink_0().child("Date"))
                                .child(TableHead::new().w_56().flex_shrink_0().child("Account"))
                                .child(TableHead::new().min_w_0().child("Description"))
                                .child(TableHead::new().w_32().flex_shrink_0().text_right().child("Amount"))
                                .child(TableHead::new().w_96().flex_shrink_0().child("Reconciled to")),
                        ),
                    )
                    .child(TableBody::new().children(visible.iter().enumerate().map(|(index, t)| {
                        let links: Vec<String> = household
                            .links
                            .iter()
                            .filter(|l| l.transaction == t.id)
                            .map(|l| {
                                let name = household.series_by_id(l.series).map(|s| s.name.clone()).unwrap_or_else(|| l.series.to_string());
                                format!("{} due {} — {}", name, l.original_due.format("%d %b %Y"), l.amount.format())
                            })
                            .collect();
                        TableRow::new()
                            .when(index % 2 == 1, |row| row.bg(theme.table_even))
                            .child(TableCell::new().w_24().flex_shrink_0().child(t.date.format("%d %b %y").to_string()))
                            .child(muted_cell(household.account(t.account).map(|a| a.name.clone()).unwrap_or_default(), cx).w_56().flex_shrink_0())
                            .child(TableCell::new().min_w_0().overflow_hidden().text_ellipsis().child(t.description.clone()))
                            .child(money_cell(t.amount, cx).w_32().flex_shrink_0())
                            .child(muted_cell(if links.is_empty() { "unreconciled".to_string() } else { links.join(" · ") }, cx).w_96().flex_shrink_0().overflow_hidden())
                    })))
                    .into_any_element()
            }),
    )
}
