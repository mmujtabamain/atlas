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
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Recurrence {
    OneTime { on: DateSpec },
    Daily { from: NaiveDate, until: Until },
    Weekly { every_n_weeks: u32, from: NaiveDate, until: Until },
    Monthly { every_n_months: u32, day: u8, from: NaiveDate, until: Until, invalid_day: InvalidDayPolicy },
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

/// §5.6 — a template that generates occurrences.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct EventSeries {
    pub id: SeriesId,
    pub name: String,
    pub direction: Direction,
    pub amount: AmountSpec,
    pub amount_changes: Vec<AmountChange>,
    pub recurrence: Recurrence,
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
    /// M05 clocks.
    pub due: NaiveDate,
    pub posting: NaiveDate,
    pub settlement: NaiveDate,
    pub availability: NaiveDate,
    pub amount: AmountSpec,
    pub direction: Direction,
    pub account: AccountId,
    pub linked_account: Option<AccountId>,
    pub entity: EntityRef,
    pub certainty: Certainty,
    pub scenario: Option<ScenarioId>,
    pub status: OccurrenceStatus,
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("valid month");
    let next = first.checked_add_months(Months::new(1)).expect("date in range");
    (next - first).num_days() as u32
}

fn last_day_of_month(year: i32, month: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, days_in_month(year, month)).expect("valid date")
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
    match *recurrence {
        Recurrence::OneTime { on } => {
            let date = on.expected();
            if date <= through {
                dates.push(date);
            }
        }
        Recurrence::Daily { from, until } => {
            let mut date = from;
            while push(&mut dates, until, date) {
                date = date.checked_add_days(Days::new(1)).expect("date in range");
            }
        }
        Recurrence::Weekly { every_n_weeks, from, until } => {
            let step = Days::new(7 * every_n_weeks.max(1) as u64);
            let mut date = from;
            while push(&mut dates, until, date) {
                date = date.checked_add_days(step).expect("date in range");
            }
        }
        Recurrence::Monthly { every_n_months, day, from, until, invalid_day } => {
            let step = Months::new(every_n_months.max(1));
            let mut month_start = NaiveDate::from_ymd_opt(from.year(), from.month(), 1).expect("valid");
            loop {
                if month_start > through {
                    break;
                }
                let (year, month) = (month_start.year(), month_start.month());
                let candidate = if day as u32 <= days_in_month(year, month) {
                    Some(NaiveDate::from_ymd_opt(year, month, day as u32).expect("valid"))
                } else {
                    match invalid_day {
                        InvalidDayPolicy::ClampToMonthEnd => Some(last_day_of_month(year, month)),
                        InvalidDayPolicy::Skip => None,
                        InvalidDayPolicy::MoveToNextMonthStart => {
                            Some(month_start.checked_add_months(Months::new(1)).expect("date in range"))
                        }
                    }
                };
                if let Some(date) = candidate
                    && date >= from
                    && !push(&mut dates, until, date)
                {
                    break;
                }
                month_start = month_start.checked_add_months(step).expect("date in range");
            }
        }
        Recurrence::LastDayOfMonth { every_n_months, from, until } => {
            let step = Months::new(every_n_months.max(1));
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
/// already reconciled, not forecast (§2.4).
pub fn expand(series: &EventSeries, after: NaiveDate, through: NaiveDate) -> Vec<Occurrence> {
    recurrence_dates(&series.recurrence, through)
        .into_iter()
        .enumerate()
        .filter(|(_, date)| *date > after)
        .map(|(index, due)| Occurrence {
            series: series.id,
            label: series.name.clone(),
            sequence: index as u32 + 1,
            due,
            posting: due,
            settlement: due,
            availability: due,
            amount: series.amount_on(due),
            direction: series.direction,
            account: series.account,
            linked_account: series.linked_account,
            entity: series.entity,
            certainty: series.certainty,
            scenario: series.scenario,
            status: OccurrenceStatus::Planned,
        })
        .collect()
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
            recurrence: Recurrence::LastDayOfMonth { every_n_months: 1, from: d(2026, 9, 1), until: Until::Indefinite },
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
}
