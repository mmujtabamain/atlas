//! Event series and their expansion into dated occurrences (§5.5–§5.7, §9, M05).
//!
//! A series is a template; [`expand`] turns it into [`Occurrence`]s inside a
//! window. Every occurrence distinguishes the four clocks of M05 — contractual
//! due date, posting, settlement and availability — even though M0 sets them
//! equal; M3 fills in settlement delays and exceptions. Invalid month days are
//! never left to a calendar library's default: the series states its
//! [`InvalidDayPolicy`].

use crate::ids::*;
use crate::money::{Currency, Money};
use crate::vocab::Certainty;
use chrono::{Datelike, Days, Months, NaiveDate};
use serde::{Deserialize, Serialize};

/// §9 — direction of a movement. Transfers are first-class (§15).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Direction {
    Income,
    Expense,
    /// Money moving to another account of the same boundary; net zero for the household.
    Transfer { to: AccountId },
}

impl Direction {
    pub fn label(self) -> &'static str {
        match self {
            Direction::Income => "Income",
            Direction::Expense => "Expense",
            Direction::Transfer { .. } => "Transfer",
        }
    }
}

/// §9 / §10.4 — exact or ranged amount.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum AmountSpec {
    Exact(Money),
    Range { low: Money, expected: Money, high: Money },
}

impl AmountSpec {
    pub fn expected(&self) -> Money {
        match self {
            AmountSpec::Exact(m) => *m,
            AmountSpec::Range { expected, .. } => *expected,
        }
    }

    pub fn low(&self) -> Money {
        match self {
            AmountSpec::Exact(m) => *m,
            AmountSpec::Range { low, .. } => *low,
        }
    }

    pub fn high(&self) -> Money {
        match self {
            AmountSpec::Exact(m) => *m,
            AmountSpec::Range { high, .. } => *high,
        }
    }

    pub fn currency(&self) -> Currency {
        self.expected().currency()
    }

    /// `300,000` or `300,000 (250,000–320,000)`.
    pub fn describe(&self) -> String {
        match self {
            AmountSpec::Exact(m) => m.format(),
            AmountSpec::Range { low, expected, high } => format!("{} ({}–{})", expected.format(), low.format(), high.format()),
        }
    }
}

/// §10.5 — exact or ranged date for a one-time event.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum DateSpec {
    Exact(NaiveDate),
    Range { earliest: NaiveDate, expected: NaiveDate, latest: NaiveDate },
}

impl DateSpec {
    pub fn expected(&self) -> NaiveDate {
        match self {
            DateSpec::Exact(d) => *d,
            DateSpec::Range { expected, .. } => *expected,
        }
    }

    pub fn latest(&self) -> NaiveDate {
        match self {
            DateSpec::Exact(d) => *d,
            DateSpec::Range { latest, .. } => *latest,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            DateSpec::Exact(d) => d.format("%d %b %Y").to_string(),
            DateSpec::Range { earliest, expected, latest } => {
                format!("{} ({} – {})", expected.format("%d %b %Y"), earliest.format("%d %b"), latest.format("%d %b %Y"))
            }
        }
    }
}

/// §9.1 — when a series stops.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Until {
    Date(NaiveDate),
    Count(u32),
    Indefinite,
}

impl Until {
    pub fn describe(self) -> String {
        match self {
            Until::Date(d) => format!("until {}", d.format("%d %b %Y")),
            Until::Count(n) => format!("for {n} occurrences"),
            Until::Indefinite => "indefinitely".into(),
        }
    }
}

/// M05 — what happens when a monthly day does not exist in a month.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum InvalidDayPolicy {
    ClampToMonthEnd,
    Skip,
    MoveToNextMonthStart,
}

impl InvalidDayPolicy {
    pub fn label(self) -> &'static str {
        match self {
            InvalidDayPolicy::ClampToMonthEnd => "clamp to month end",
            InvalidDayPolicy::Skip => "skip the month",
            InvalidDayPolicy::MoveToNextMonthStart => "move to the 1st of the next month",
        }
    }
}

/// §9.1 — recurrence patterns.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Recurrence {
    OneTime { on: DateSpec },
    Daily { from: NaiveDate, until: Until },
    Weekly { every_n_weeks: u32, from: NaiveDate, until: Until },
    Monthly { every_n_months: u32, day: u8, from: NaiveDate, until: Until, invalid_day: InvalidDayPolicy },
    /// Several specific days of every month (e.g. the 1st and the 15th).
    DaysOfMonth { days: Vec<u8>, from: NaiveDate, until: Until, invalid_day: InvalidDayPolicy },
    LastDayOfMonth { every_n_months: u32, from: NaiveDate, until: Until },
    Yearly { month: u8, day: u8, from: NaiveDate, until: Until },
}

impl Recurrence {
    pub fn describe(&self) -> String {
        match self {
            Recurrence::OneTime { on } => format!("once, {}", on.describe()),
            Recurrence::Daily { until, .. } => format!("daily {}", until.describe()),
            Recurrence::Weekly { every_n_weeks: 1, until, .. } => format!("weekly {}", until.describe()),
            Recurrence::Weekly { every_n_weeks, until, .. } => format!("every {every_n_weeks} weeks {}", until.describe()),
            Recurrence::Monthly { every_n_months: 1, day, until, invalid_day, .. } => {
                format!("monthly on day {day} {} ({})", until.describe(), invalid_day.label())
            }
            Recurrence::Monthly { every_n_months, day, until, invalid_day, .. } => {
                format!("every {every_n_months} months on day {day} {} ({})", until.describe(), invalid_day.label())
            }
            Recurrence::DaysOfMonth { days, until, invalid_day, .. } => format!(
                "on days {} of every month {} ({})",
                days.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(", "),
                until.describe(),
                invalid_day.label()
            ),
            Recurrence::LastDayOfMonth { every_n_months: 1, until, .. } => format!("last day of every month {}", until.describe()),
            Recurrence::LastDayOfMonth { every_n_months, until, .. } => {
                format!("last day of every {every_n_months} months {}", until.describe())
            }
            Recurrence::Yearly { month, day, until, .. } => format!("yearly on {day}/{month} {}", until.describe()),
        }
    }
}

/// §9.3 — an effective-dated amount change ("Jan onward: 550,000").
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AmountChange {
    pub effective_from: NaiveDate,
    pub amount: AmountSpec,
}

/// §9.2 — what happens to one occurrence, identified by its original due date.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ExceptionKind {
    /// Skip this occurrence (it still appears, as skipped, and posts nothing).
    Skip,
    /// Move this occurrence to another date.
    Move { to: NaiveDate },
    /// Change the amount of this occurrence only.
    Amount(AmountSpec),
    /// Cancel: like skip, but recorded as cancelled (§16).
    Cancel,
}

/// One edited occurrence of a series (§9.2 "edit only this occurrence").
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Exception {
    /// The occurrence's due date before the exception.
    pub original_due: NaiveDate,
    pub kind: ExceptionKind,
}

impl Exception {
    pub fn describe(&self) -> String {
        let day = self.original_due.format("%d %b %Y");
        match self.kind {
            ExceptionKind::Skip => format!("{day}: skipped"),
            ExceptionKind::Cancel => format!("{day}: cancelled"),
            ExceptionKind::Move { to } => format!("{day}: moved to {}", to.format("%d %b %Y")),
            ExceptionKind::Amount(amount) => format!("{day}: amount {}", amount.describe()),
        }
    }
}

/// §5.6 — a template that generates occurrences.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct EventSeries {
    pub id: SeriesId,
    pub name: String,
    pub direction: Direction,
    pub amount: AmountSpec,
    pub amount_changes: Vec<AmountChange>,
    pub exceptions: Vec<Exception>,
    pub recurrence: Recurrence,
    /// M05 — days from posting until the cash has settled.
    pub settlement_lag_days: u16,
    /// M05 — days from settlement until the cash may be spent.
    pub availability_lag_days: u16,
    /// §11.1 — explicit same-day ordering: lower posts first. Rent at 09:00
    /// before salary at 17:00 is `10` vs `20`, never left to chance.
    pub intraday_order: u16,
    /// The account the movement posts to.
    pub account: AccountId,
    /// §8.4 — the other side of a linked company↔person movement.
    pub linked_account: Option<AccountId>,
    pub entity: EntityRef,
    pub certainty: Certainty,
    pub category: String,
    pub tax_treatment: String,
    /// Present only inside a scenario overlay (§18).
    pub scenario: Option<ScenarioId>,
    pub notes: String,
}

impl EventSeries {
    /// The exception recorded for an original due date, if any.
    pub fn exception_on(&self, original_due: NaiveDate) -> Option<&Exception> {
        self.exceptions.iter().find(|e| e.original_due == original_due)
    }

    /// §9.2 — records an exception, replacing any earlier one for the date.
    pub fn set_exception(&mut self, exception: Exception) {
        self.exceptions.retain(|e| e.original_due != exception.original_due);
        self.exceptions.push(exception);
    }

    /// §9.3 — "this and future occurrences": an effective-dated amount change.
    pub fn change_amount_from(&mut self, effective_from: NaiveDate, amount: AmountSpec) {
        self.amount_changes.retain(|c| c.effective_from != effective_from);
        self.amount_changes.push(AmountChange { effective_from, amount });
        self.amount_changes.sort_by_key(|c| c.effective_from);
    }

    /// §9.4 — ends the series after `last` (resignation, last salary date …).
    pub fn end_after(&mut self, last: NaiveDate) {
        match &mut self.recurrence {
            Recurrence::OneTime { .. } => {}
            Recurrence::Daily { until, .. }
            | Recurrence::Weekly { until, .. }
            | Recurrence::Monthly { until, .. }
            | Recurrence::DaysOfMonth { until, .. }
            | Recurrence::LastDayOfMonth { until, .. }
            | Recurrence::Yearly { until, .. } => *until = Until::Date(last),
        }
    }

    /// The amount in force on `date`, honouring effective-dated changes.
    pub fn amount_on(&self, date: NaiveDate) -> AmountSpec {
        self.amount_changes
            .iter()
            .filter(|c| c.effective_from <= date)
            .max_by_key(|c| c.effective_from)
            .map(|c| c.amount)
            .unwrap_or(self.amount)
    }
}

/// §16 — lifecycle of one occurrence.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum OccurrenceStatus {
    Planned,
    Due,
    PartiallyFulfilled,
    Fulfilled,
    Skipped,
    Cancelled,
    Overdue,
}

impl OccurrenceStatus {
    pub fn label(self) -> &'static str {
        match self {
            OccurrenceStatus::Planned => "Planned",
            OccurrenceStatus::Due => "Due",
            OccurrenceStatus::PartiallyFulfilled => "Partially fulfilled",
            OccurrenceStatus::Fulfilled => "Fulfilled",
            OccurrenceStatus::Skipped => "Skipped",
            OccurrenceStatus::Cancelled => "Cancelled",
            OccurrenceStatus::Overdue => "Overdue",
        }
    }
}

/// §5.5 — one dated financial event generated from a series.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Occurrence {
    pub series: SeriesId,
    pub label: String,
    pub sequence: u32,
    /// The due date the recurrence produced, before any exception (§9.2).
    pub original_due: NaiveDate,
    /// M05 clocks.
    pub due: NaiveDate,
    pub posting: NaiveDate,
    pub settlement: NaiveDate,
    pub availability: NaiveDate,
    pub intraday_order: u16,
    pub amount: AmountSpec,
    /// Amount already matched to actual transactions (§16).
    pub fulfilled: Money,
    pub direction: Direction,
    pub account: AccountId,
    pub linked_account: Option<AccountId>,
    pub entity: EntityRef,
    pub certainty: Certainty,
    pub scenario: Option<ScenarioId>,
    pub status: OccurrenceStatus,
}

impl Occurrence {
    /// What the forecast still expects: expected minus what actually arrived
    /// (§16, V012). Zero for skipped, cancelled and fulfilled occurrences.
    pub fn remaining_expected(&self) -> Money {
        match self.status {
            OccurrenceStatus::Skipped | OccurrenceStatus::Cancelled | OccurrenceStatus::Fulfilled => Money::zero(self.amount.currency()),
            _ => self.amount.expected().checked_sub(self.fulfilled).unwrap_or(Money::zero(self.amount.currency())).clamped_at_zero(),
        }
    }

    /// Whether the forecast should post this occurrence at all.
    pub fn is_live(&self) -> bool {
        !matches!(self.status, OccurrenceStatus::Skipped | OccurrenceStatus::Cancelled | OccurrenceStatus::Fulfilled)
    }

    /// The chronological sort key (§11.1: date, then explicit intraday order).
    pub fn sort_key(&self) -> (NaiveDate, u16, SeriesId, u32) {
        (self.due, self.intraday_order, self.series, self.sequence)
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("valid month");
    let next = first.checked_add_months(Months::new(1)).expect("date in range");
    (next - first).num_days() as u32
}

fn last_day_of_month(year: i32, month: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, days_in_month(year, month)).expect("valid date")
}

/// The date for `day` in the month of `month_start`, under the invalid-day policy.
fn day_in_month(month_start: NaiveDate, day: u8, invalid_day: InvalidDayPolicy) -> Option<NaiveDate> {
    let (year, month) = (month_start.year(), month_start.month());
    if day as u32 <= days_in_month(year, month) {
        return Some(NaiveDate::from_ymd_opt(year, month, day as u32).expect("valid"));
    }
    match invalid_day {
        InvalidDayPolicy::ClampToMonthEnd => Some(last_day_of_month(year, month)),
        InvalidDayPolicy::Skip => None,
        InvalidDayPolicy::MoveToNextMonthStart => Some(month_start.checked_add_months(Months::new(1)).expect("date in range")),
    }
}

/// The dates a recurrence produces, starting at its own `from` (so
/// `Until::Count` counts from the series start, not the window), stopping at
/// `until` or at `through`, whichever comes first.
pub fn recurrence_dates(recurrence: &Recurrence, through: NaiveDate) -> Vec<NaiveDate> {
    let mut dates = Vec::new();
    let push = |dates: &mut Vec<NaiveDate>, until: Until, date: NaiveDate| -> bool {
        if date > through {
            return false;
        }
        match until {
            Until::Date(end) if date > end => return false,
            Until::Count(n) if dates.len() as u32 >= n => return false,
            _ => {}
        }
        dates.push(date);
        true
    };
    match recurrence {
        Recurrence::OneTime { on } => {
            let date = on.expected();
            if date <= through {
                dates.push(date);
            }
        }
        Recurrence::Daily { from, until } => {
            let (from, until) = (*from, *until);
            let mut date = from;
            while push(&mut dates, until, date) {
                date = date.checked_add_days(Days::new(1)).expect("date in range");
            }
        }
        Recurrence::Weekly { every_n_weeks, from, until } => {
            let (from, until) = (*from, *until);
            let step = Days::new(7 * (*every_n_weeks).max(1) as u64);
            let mut date = from;
            while push(&mut dates, until, date) {
                date = date.checked_add_days(step).expect("date in range");
            }
        }
        Recurrence::Monthly { every_n_months, day, from, until, invalid_day } => {
            let (from, until, invalid_day) = (*from, *until, *invalid_day);
            let step = Months::new((*every_n_months).max(1));
            let mut month_start = NaiveDate::from_ymd_opt(from.year(), from.month(), 1).expect("valid");
            loop {
                if month_start > through {
                    break;
                }
                let candidate = day_in_month(month_start, *day, invalid_day);
                if let Some(date) = candidate
                    && date >= from
                    && !push(&mut dates, until, date)
                {
                    break;
                }
                month_start = month_start.checked_add_months(step).expect("date in range");
            }
        }
        Recurrence::DaysOfMonth { days, from, until, invalid_day } => {
            let (from, until, invalid_day) = (*from, *until, *invalid_day);
            let mut sorted = days.clone();
            sorted.sort_unstable();
            sorted.dedup();
            let mut month_start = NaiveDate::from_ymd_opt(from.year(), from.month(), 1).expect("valid");
            'months: loop {
                if month_start > through {
                    break;
                }
                for day in &sorted {
                    if let Some(date) = day_in_month(month_start, *day, invalid_day)
                        && date >= from
                        && !push(&mut dates, until, date)
                    {
                        break 'months;
                    }
                }
                month_start = month_start.checked_add_months(Months::new(1)).expect("date in range");
            }
        }
        Recurrence::LastDayOfMonth { every_n_months, from, until } => {
            let (from, until) = (*from, *until);
            let step = Months::new((*every_n_months).max(1));
            let mut month_start = NaiveDate::from_ymd_opt(from.year(), from.month(), 1).expect("valid");
            loop {
                if month_start > through {
                    break;
                }
                let date = last_day_of_month(month_start.year(), month_start.month());
                if date >= from && !push(&mut dates, until, date) {
                    break;
                }
                month_start = month_start.checked_add_months(step).expect("date in range");
            }
        }
        Recurrence::Yearly { month, day, from, until } => {
            let (month, day, from, until) = (*month, *day, *from, *until);
            let mut year = from.year();
            loop {
                let candidate = NaiveDate::from_ymd_opt(year, month as u32, day as u32)
                    .unwrap_or_else(|| last_day_of_month(year, month as u32));
                if candidate > through {
                    break;
                }
                if candidate >= from && !push(&mut dates, until, candidate) {
                    break;
                }
                year += 1;
            }
        }
    }
    dates
}

/// Expands a series into the occurrences falling in `(after, through]`.
/// `after` is normally the household's `as_of` date: money on or before it is
/// already reconciled, not forecast (§2.4). Exceptions (§9.2) are applied:
/// skipped and cancelled occurrences are kept with their status and post
/// nothing; moved occurrences take their new date. Reconciliation is applied
/// separately by [`crate::model::Household::expand_series`].
pub fn expand(series: &EventSeries, after: NaiveDate, through: NaiveDate) -> Vec<Occurrence> {
    let mut occurrences: Vec<Occurrence> = recurrence_dates(&series.recurrence, through)
        .into_iter()
        .enumerate()
        .filter_map(|(index, original_due)| {
            let exception = series.exception_on(original_due);
            let (due, status, amount) = match exception.map(|e| e.kind) {
                Some(ExceptionKind::Skip) => (original_due, OccurrenceStatus::Skipped, series.amount_on(original_due)),
                Some(ExceptionKind::Cancel) => (original_due, OccurrenceStatus::Cancelled, series.amount_on(original_due)),
                Some(ExceptionKind::Move { to }) => (to, OccurrenceStatus::Planned, series.amount_on(to)),
                Some(ExceptionKind::Amount(amount)) => (original_due, OccurrenceStatus::Planned, amount),
                None => (original_due, OccurrenceStatus::Planned, series.amount_on(original_due)),
            };
            if due <= after {
                return None;
            }
            let settlement = due.checked_add_days(Days::new(series.settlement_lag_days as u64)).unwrap_or(due);
            let availability = settlement.checked_add_days(Days::new(series.availability_lag_days as u64)).unwrap_or(settlement);
            Some(Occurrence {
                series: series.id,
                label: series.name.clone(),
                sequence: index as u32 + 1,
                original_due,
                due,
                posting: due,
                settlement,
                availability,
                intraday_order: series.intraday_order,
                amount,
                fulfilled: Money::zero(amount.currency()),
                direction: series.direction,
                account: series.account,
                linked_account: series.linked_account,
                entity: series.entity,
                certainty: series.certainty,
                scenario: series.scenario,
                status,
            })
        })
        .collect();
    occurrences.sort_by_key(|o| o.sort_key());
    occurrences
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Currency;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn pkr(major: i64) -> Money {
        Money::from_major(major, Currency::PKR)
    }

    #[test]
    fn monthly_clamps_or_skips_invalid_days_explicitly() {
        let clamp = Recurrence::Monthly {
            every_n_months: 1,
            day: 31,
            from: d(2026, 1, 31),
            until: Until::Count(4),
            invalid_day: InvalidDayPolicy::ClampToMonthEnd,
        };
        assert_eq!(recurrence_dates(&clamp, d(2026, 12, 31)), vec![d(2026, 1, 31), d(2026, 2, 28), d(2026, 3, 31), d(2026, 4, 30)]);

        let skip = Recurrence::Monthly {
            every_n_months: 1,
            day: 31,
            from: d(2026, 1, 31),
            until: Until::Count(3),
            invalid_day: InvalidDayPolicy::Skip,
        };
        assert_eq!(recurrence_dates(&skip, d(2026, 12, 31)), vec![d(2026, 1, 31), d(2026, 3, 31), d(2026, 5, 31)]);

        let moved = Recurrence::Monthly {
            every_n_months: 1,
            day: 30,
            from: d(2028, 1, 30),
            until: Until::Count(3),
            invalid_day: InvalidDayPolicy::MoveToNextMonthStart,
        };
        // 2028 is a leap year: Feb 30 does not exist, moved to Mar 1.
        assert_eq!(recurrence_dates(&moved, d(2028, 12, 31)), vec![d(2028, 1, 30), d(2028, 3, 1), d(2028, 3, 30)]);
    }

    #[test]
    fn last_day_of_month_and_until_date() {
        let salary = Recurrence::LastDayOfMonth { every_n_months: 1, from: d(2026, 9, 1), until: Until::Date(d(2026, 11, 30)) };
        assert_eq!(recurrence_dates(&salary, d(2027, 3, 1)), vec![d(2026, 9, 30), d(2026, 10, 31), d(2026, 11, 30)]);
        // The window can end earlier than the series.
        assert_eq!(recurrence_dates(&salary, d(2026, 10, 15)), vec![d(2026, 9, 30)]);
    }

    #[test]
    fn weekly_quarterly_and_yearly() {
        let fortnightly = Recurrence::Weekly { every_n_weeks: 2, from: d(2026, 9, 4), until: Until::Count(3) };
        assert_eq!(recurrence_dates(&fortnightly, d(2027, 1, 1)), vec![d(2026, 9, 4), d(2026, 9, 18), d(2026, 10, 2)]);
        let quarterly = Recurrence::Monthly {
            every_n_months: 3,
            day: 15,
            from: d(2026, 9, 15),
            until: Until::Indefinite,
            invalid_day: InvalidDayPolicy::ClampToMonthEnd,
        };
        assert_eq!(recurrence_dates(&quarterly, d(2027, 4, 1)), vec![d(2026, 9, 15), d(2026, 12, 15), d(2027, 3, 15)]);
        let yearly = Recurrence::Yearly { month: 2, day: 29, from: d(2027, 1, 1), until: Until::Count(2) };
        // Feb 29 falls back to Feb 28 in non-leap years.
        assert_eq!(recurrence_dates(&yearly, d(2029, 1, 1)), vec![d(2027, 2, 28), d(2028, 2, 29)]);
    }

    #[test]
    fn expansion_excludes_reconciled_dates_and_applies_amount_changes() {
        let series = EventSeries {
            id: SeriesId::new(1),
            name: "Salary".into(),
            direction: Direction::Income,
            amount: AmountSpec::Exact(pkr(500_000)),
            amount_changes: vec![AmountChange { effective_from: d(2027, 1, 1), amount: AmountSpec::Exact(pkr(550_000)) }],
            exceptions: Vec::new(),
            recurrence: Recurrence::LastDayOfMonth { every_n_months: 1, from: d(2026, 9, 1), until: Until::Indefinite },
            settlement_lag_days: 0,
            availability_lag_days: 0,
            intraday_order: 20,
            account: AccountId::new(1),
            linked_account: None,
            entity: EntityRef::Person(PersonId::new(1)),
            certainty: Certainty::Contractual,
            category: "Salary".into(),
            tax_treatment: String::new(),
            scenario: None,
            notes: String::new(),
        };
        let occurrences = expand(&series, d(2026, 9, 30), d(2027, 1, 31));
        let dues: Vec<_> = occurrences.iter().map(|o| o.due).collect();
        assert_eq!(dues, vec![d(2026, 10, 31), d(2026, 11, 30), d(2026, 12, 31), d(2027, 1, 31)]);
        assert_eq!(occurrences[0].amount.expected(), pkr(500_000));
        assert_eq!(occurrences[3].amount.expected(), pkr(550_000));
        assert_eq!(occurrences[0].sequence, 2, "sequence counts from the series start");
        assert_eq!(occurrences[0].status, OccurrenceStatus::Planned);
    }

    fn salary_series() -> EventSeries {
        EventSeries {
            id: SeriesId::new(1),
            name: "Salary".into(),
            direction: Direction::Income,
            amount: AmountSpec::Exact(pkr(500_000)),
            amount_changes: Vec::new(),
            exceptions: Vec::new(),
            recurrence: Recurrence::LastDayOfMonth { every_n_months: 1, from: d(2026, 9, 1), until: Until::Indefinite },
            settlement_lag_days: 2,
            availability_lag_days: 1,
            intraday_order: 20,
            account: AccountId::new(1),
            linked_account: None,
            entity: EntityRef::Person(PersonId::new(1)),
            certainty: Certainty::Contractual,
            category: "Salary".into(),
            tax_treatment: String::new(),
            scenario: None,
            notes: String::new(),
        }
    }

    #[test]
    fn exceptions_skip_move_and_override_single_occurrences() {
        let mut series = salary_series();
        series.set_exception(Exception { original_due: d(2026, 10, 31), kind: ExceptionKind::Skip });
        series.set_exception(Exception { original_due: d(2026, 11, 30), kind: ExceptionKind::Move { to: d(2026, 11, 27) } });
        series.set_exception(Exception { original_due: d(2026, 12, 31), kind: ExceptionKind::Amount(AmountSpec::Exact(pkr(650_000))) });
        let occurrences = expand(&series, d(2026, 9, 11), d(2026, 12, 31));
        assert_eq!(occurrences.len(), 4);
        assert_eq!(occurrences[1].status, OccurrenceStatus::Skipped);
        assert_eq!(occurrences[1].remaining_expected(), pkr(0), "a skipped occurrence posts nothing");
        assert_eq!(occurrences[2].due, d(2026, 11, 27));
        assert_eq!(occurrences[2].original_due, d(2026, 11, 30));
        assert_eq!(occurrences[3].amount.expected(), pkr(650_000));
        assert_eq!(occurrences[0].amount.expected(), pkr(500_000), "other occurrences are untouched");
        // M05 clocks: settlement two days after posting, availability one day later.
        assert_eq!(occurrences[0].posting, d(2026, 9, 30));
        assert_eq!(occurrences[0].settlement, d(2026, 10, 2));
        assert_eq!(occurrences[0].availability, d(2026, 10, 3));
    }

    #[test]
    fn employment_transition_ends_a_series_and_a_new_one_starts() {
        // §9.4: resignation with a final, different paycheck; new job starts before the old ends.
        let mut old_job = salary_series();
        old_job.end_after(d(2026, 11, 30));
        old_job.set_exception(Exception { original_due: d(2026, 11, 30), kind: ExceptionKind::Amount(AmountSpec::Exact(pkr(380_000))) });
        let mut new_job = salary_series();
        new_job.id = SeriesId::new(2);
        new_job.recurrence = Recurrence::Monthly { every_n_months: 1, day: 25, from: d(2026, 11, 25), until: Until::Indefinite, invalid_day: InvalidDayPolicy::ClampToMonthEnd };
        new_job.amount = AmountSpec::Exact(pkr(600_000));
        let through = d(2027, 1, 31);
        let old: Vec<_> = expand(&old_job, d(2026, 9, 11), through);
        let new: Vec<_> = expand(&new_job, d(2026, 9, 11), through);
        assert_eq!(old.iter().map(|o| o.due).collect::<Vec<_>>(), vec![d(2026, 9, 30), d(2026, 10, 31), d(2026, 11, 30)]);
        assert_eq!(old.last().unwrap().amount.expected(), pkr(380_000), "final paycheck differs");
        assert_eq!(new.iter().map(|o| o.due).collect::<Vec<_>>(), vec![d(2026, 11, 25), d(2026, 12, 25), d(2027, 1, 25)]);
    }

    #[test]
    fn days_of_month_and_intraday_ordering() {
        let twice = Recurrence::DaysOfMonth { days: vec![15, 1], from: d(2026, 9, 1), until: Until::Count(4), invalid_day: InvalidDayPolicy::ClampToMonthEnd };
        assert_eq!(recurrence_dates(&twice, d(2027, 1, 1)), vec![d(2026, 9, 1), d(2026, 9, 15), d(2026, 10, 1), d(2026, 10, 15)]);
        let mut rent = salary_series();
        rent.id = SeriesId::new(9);
        rent.intraday_order = 10;
        let salary = salary_series();
        let mut all = expand(&rent, d(2026, 9, 11), d(2026, 9, 30));
        all.extend(expand(&salary, d(2026, 9, 11), d(2026, 9, 30)));
        all.sort_by_key(|o| o.sort_key());
        assert_eq!(all[0].series, SeriesId::new(9), "the 09:00 rent debit sorts before the 17:00 salary");
    }
}
