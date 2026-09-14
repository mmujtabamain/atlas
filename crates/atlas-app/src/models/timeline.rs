//! Timeline: every planned occurrence in a window with its four clocks,
//! status and reconciliation; the series that generate them with their
//! exceptions and effective-dated changes; the actual transactions they are
//! reconciled to.

use atlas_core::authz::Viewer;
use atlas_core::ids::{AccountId, EntityRef, ObjectRef, ScenarioId, SeriesId};
use atlas_core::model::Household;
use atlas_core::timeline::{Direction, Occurrence, OccurrenceStatus};
use atlas_core::vocab::Certainty;
use atlas_core::{Disclosure, EngineResult, Money};
use std::sync::Arc;
use chrono::NaiveDate;
use gpui_kit::*;

use crate::widgets::grid::{self, Cell, GridColumn, Row, Tone};

/// What the timeline shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineFilter {
    pub entity: Option<EntityRef>,
    pub account: Option<AccountId>,
    pub certainty: Option<Certainty>,
    pub status: Option<OccurrenceStatus>,
    pub scenario: Option<ScenarioId>,
    pub through: NaiveDate,
    /// Upcoming narrowed to one series (from a series detail).
    pub series: Option<SeriesId>,
    /// The actual-transactions register narrowed to one account. Actuals are
    /// otherwise independent of the window and the plan.
    pub actuals_account: Option<AccountId>,
}

/// Rows and lists for the screen, computed once per state change.
#[derive(Clone, Debug)]
pub struct TimelineModel {
    pub filter: TimelineFilter,
    /// Every visible occurrence in the window under the plan, before the
    /// entity/account/certainty/status filters (series previews use it).
    pub all_occurrences: Vec<Occurrence>,
    pub occurrences: Vec<Occurrence>,
    /// The occurrences as grid rows — every string formatted here, once, so
    /// the virtualised table only paints (see `widgets::grid`).
    pub rows: grid::Rows,
    pub total_in: Money,
    pub total_out: Money,
    /// Series the viewer may see, in fixture order.
    pub series: Vec<SeriesId>,
    pub hidden_series: usize,
    /// The actual transactions the viewer may see, with what each is
    /// reconciled to, as grid rows; `actual_ids` runs parallel to them.
    pub actual_rows: grid::Rows,
    pub actual_ids: Vec<atlas_core::ids::TransactionId>,
    /// Actuals the viewer may not see (their accounts are not disclosed).
    pub hidden_actuals: usize,
    /// The scenario the overlay toggle applies (see [`super::overlay_scenario`]).
    pub overlay_scenario: Option<ScenarioId>,
}

/// Columns of the actual-transactions grid, in display order.
pub const ACTUAL_COLUMNS: [GridColumn; 5] = [
    GridColumn::new("date", "Date", 104.),
    GridColumn::new("account", "Account", 200.),
    GridColumn::new("description", "Description", 300.),
    GridColumn::new("amount", "Signed amount", 140.).right(),
    GridColumn::new("reconciled", "Reconciled to", 360.),
];

fn actual_rows(household: &Household, viewer: Viewer, account: Option<AccountId>) -> (grid::Rows, Vec<atlas_core::ids::TransactionId>) {
    let mut visible: Vec<&atlas_core::model::ActualTransaction> = household
        .actuals
        .iter()
        .filter(|t| matches!(household.disclosure_for(viewer, ObjectRef::Account(t.account)), Disclosure::Full | Disclosure::SelectedFields))
        .filter(|t| account.is_none_or(|a| t.account == a))
        .collect();
    // Newest first: the register is scanned as history.
    visible.sort_by(|a, b| b.date.cmp(&a.date).then(b.id.raw().cmp(&a.id.raw())));
    let ids = visible.iter().map(|t| t.id).collect();
    let rows = Arc::new(
        visible
            .into_iter()
            .map(|t| {
                let links: Vec<String> = household
                    .links
                    .iter()
                    .filter(|l| l.transaction == t.id)
                    .map(|l| {
                        let name = household.series_by_id(l.series).map(|s| s.name.clone()).unwrap_or_else(|| l.series.to_string());
                        format!("{} due {} — {}", name, l.original_due.format("%d %b %Y"), l.amount.format())
                    })
                    .collect();
                Row::new(vec![
                    Cell::text(t.date.format("%d %b %y").to_string()),
                    Cell::muted(household.account(t.account).map(|a| a.name.clone()).unwrap_or_default()),
                    Cell::text(t.description.clone()),
                    Cell::money(t.amount),
                    Cell::muted(if links.is_empty() { "Unreconciled".to_string() } else { links.join(" · ") }),
                ])
            })
            .collect(),
    );
    (rows, ids)
}

/// Columns of the occurrences grid, in display order.
pub const OCCURRENCE_COLUMNS: [GridColumn; 6] = [
    GridColumn::new("due", "Due · settles / available", 190.),
    GridColumn::new("series", "Series · whose", 300.),
    GridColumn::new("account", "Account", 200.),
    GridColumn::new("expected", "Remaining (signed)", 160.).right(),
    GridColumn::new("certainty", "Certainty", 130.),
    GridColumn::new("status", "Status", 130.),
];

/// One occurrence as a grid row: the strings the table paints.
fn occurrence_row(o: &Occurrence, household: &Household) -> Row {
    let date = |d: NaiveDate| d.format("%d %b %y").to_string();
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
        subtitle.push_str(" · linked movement");
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
    let tone = match o.direction {
        _ if !live => Tone::Muted,
        Direction::Income => Tone::Success,
        _ => Tone::Foreground,
    };
    let clocks = if o.settlement == o.due && o.availability == o.due {
        "Same day".to_string()
    } else if o.settlement == o.availability {
        format!("Settles and available {}", date(o.settlement))
    } else {
        format!("Settles {} · available {}", date(o.settlement), date(o.availability))
    };
    Row::new(vec![
        Cell::stack(date(o.due), clocks),
        Cell::stack(o.label.clone(), subtitle),
        Cell::muted(account_text),
        Cell::Money { text: signed.into(), tone, line_through: !live, detail: detail.map(SharedString::from) },
        Cell::Certainty(o.certainty),
        Cell::Status(o.status),
    ])
    .muted(!live)
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

        let all_occurrences: Vec<Occurrence> = household.expand_all(household.as_of, filter.through, filter.scenario).into_iter().filter(|o| series.contains(&o.series)).collect();
        let occurrences: Vec<Occurrence> = all_occurrences
            .iter()
            .cloned()
            .filter(|o| filter.series.is_none_or(|s| o.series == s))
            .filter(|o| filter.entity.is_none_or(|e| o.entity == e))
            .filter(|o| filter.account.is_none_or(|a| o.account == a || o.linked_account == Some(a) || matches!(o.direction, Direction::Transfer { to } if to == a)))
            .filter(|o| filter.certainty.is_none_or(|c| o.certainty == c))
            .filter(|o| filter.status.is_none_or(|s| o.status == s))
            .collect();
        let currency = household.base_currency;
        let total_in = Money::sum(currency, occurrences.iter().filter(|o| o.direction == Direction::Income && o.is_live()).map(|o| o.remaining_expected()))?;
        let total_out = Money::sum(currency, occurrences.iter().filter(|o| o.direction == Direction::Expense && o.is_live()).map(|o| o.remaining_expected()))?;
        let rows = Arc::new(occurrences.iter().map(|o| occurrence_row(o, household)).collect());
        let (actual_rows, actual_ids) = actual_rows(household, viewer, filter.actuals_account);
        let hidden_actuals = household.actuals.iter().filter(|t| !matches!(household.disclosure_for(viewer, ObjectRef::Account(t.account)), Disclosure::Full | Disclosure::SelectedFields)).count();
        let overlay_scenario = super::overlay_scenario(household, viewer);
        Ok(TimelineModel { filter, all_occurrences, occurrences, rows, total_in, total_out, series, hidden_series, actual_rows, actual_ids, hidden_actuals, overlay_scenario })
    }
}
