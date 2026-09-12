//! Single-occurrence commands (skip, cancel, move, change this amount, record
//! its payment) and the focused sheet that matches an existing actual
//! transaction to a planned occurrence. The original due date is an
//! occurrence's identity throughout: exceptions and reconciliation links are
//! keyed on it even after the occurrence moves.

use atlas_core::ids::{SeriesId, TransactionId};
use atlas_core::timeline::{AmountSpec, Direction, Exception, ExceptionKind, Occurrence};
use atlas_core::Money;
use chrono::NaiveDate;
use gpui_kit::component::{
    ActiveTheme as _, IndexPath, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    date_picker::{DatePicker, DatePickerState},
    dialog::DialogFooter,
    form::{Field, Form},
    h_flex,
    input::{Input, InputState},
    select::Select,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::alerting::{self, Level};
use crate::app::AtlasApp;
use crate::entry::Entry;
use crate::screens::common::confirm_primary;

/// Retained inputs of the occurrence dialogs and the match sheet.
pub struct OccurrenceForm {
    pub move_to: Entity<DatePickerState>,
    pub amount: Entity<InputState>,
    pub match_amount: Entity<InputState>,
}

impl OccurrenceForm {
    pub fn new(window: &mut Window, cx: &mut App) -> Self {
        OccurrenceForm {
            move_to: cx.new(|cx| DatePickerState::new(window, cx)),
            amount: cx.new(|cx| InputState::new(window, cx).placeholder("new amount for this occurrence only")),
            match_amount: cx.new(|cx| InputState::new(window, cx).placeholder("amount to match")),
        }
    }
}

/// What a link may still fulfil: the occurrence's expected amount on its
/// original due date less what is already linked to it.
pub fn occurrence_remaining(household: &atlas_core::model::Household, series: SeriesId, original_due: NaiveDate) -> Option<Money> {
    let series_ref = household.series_by_id(series)?;
    let expected = series_ref.amount_on(original_due).expected();
    let linked = Money::sum(expected.currency(), household.links_for(series, original_due).map(|l| l.amount)).ok()?;
    Some(expected.checked_sub(linked).ok()?.clamped_at_zero())
}

/// What an actual transaction has not yet been linked to any occurrence.
pub fn actual_unallocated(household: &atlas_core::model::Household, transaction: TransactionId) -> Option<(Money, Money)> {
    let actual = household.actual(transaction)?;
    let allocated = Money::sum(actual.amount.currency(), household.links.iter().filter(|l| l.transaction == transaction).map(|l| l.amount)).ok()?;
    let unallocated = actual.amount.abs().checked_sub(allocated).ok()?.clamped_at_zero();
    Some((allocated, unallocated))
}

impl AtlasApp {
    /// The occurrence behind a selection, from the current timeline model.
    pub fn occurrence(&self, series: SeriesId, original_due: NaiveDate) -> Option<&Occurrence> {
        self.timeline().and_then(|m| m.all_occurrences.iter().find(|o| o.series == series && o.original_due == original_due))
    }

    /// Applies one exception to one occurrence through the series.
    fn apply_exception(&mut self, series: SeriesId, original_due: NaiveDate, kind: ExceptionKind, cx: &mut Context<Self>) -> Result<String, String> {
        let entry = self.household.series.iter_mut().find(|s| s.id == series).ok_or("Unknown series.")?;
        let name = entry.name.clone();
        entry.set_exception(Exception { original_due, kind });
        let sentence = match kind {
            ExceptionKind::Skip => format!("Skipped “{name}” due {} — it posts nothing; the series continues.", original_due.format("%d %b %Y")),
            ExceptionKind::Cancel => format!("Cancelled “{name}” due {} — later occurrences are unaffected.", original_due.format("%d %b %Y")),
            ExceptionKind::Move { to } => format!("Moved “{name}” due {} to {}.", original_due.format("%d %b %Y"), to.format("%d %b %Y")),
            ExceptionKind::Amount(amount) => format!("“{name}” due {} now expects {} — only this occurrence.", original_due.format("%d %b %Y"), amount.expected().format()),
        };
        log::info!("occurrence exception: {sentence}");
        self.mark_dirty();
        self.refresh_derived();
        self.note_result(sentence.clone());
        cx.notify();
        Ok(sentence)
    }

    fn describe_occurrence(&self, o: &Occurrence) -> Vec<String> {
        let account = self.household.account(o.account).map(|a| a.name.clone()).unwrap_or_default();
        let mut lines = vec![
            format!("{} · {}", o.label, account),
            format!("Due {}{}", o.due.format("%d %b %Y"), if o.due != o.original_due { format!(" (originally {})", o.original_due.format("%d %b %Y")) } else { String::new() }),
            format!("Remaining expected {}", o.remaining_expected().format()),
        ];
        if o.fulfilled.is_positive() {
            lines.push(format!("Already fulfilled {}", o.fulfilled.format()));
        }
        lines
    }

    /// Skip: the occurrence still appears, posts nothing.
    pub fn confirm_skip_occurrence(&mut self, series: SeriesId, original_due: NaiveDate, window: &mut Window, cx: &mut Context<Self>) {
        let Some(o) = self.occurrence(series, original_due).cloned() else { return };
        let mut lines = self.describe_occurrence(&o);
        lines.push("Only this occurrence is skipped. It cannot be undone here.".to_string());
        confirm_primary(window, cx, "Skip this occurrence?", lines, "Skip occurrence", move |window, cx| {
            crate::app::with_app(cx, |app, cx| Self::report(app.apply_exception(series, original_due, ExceptionKind::Skip, cx), window, cx));
        });
    }

    /// Cancel: distinct from skip, still only this occurrence.
    pub fn confirm_cancel_occurrence(&mut self, series: SeriesId, original_due: NaiveDate, window: &mut Window, cx: &mut Context<Self>) {
        let Some(o) = self.occurrence(series, original_due).cloned() else { return };
        let mut lines = self.describe_occurrence(&o);
        lines.push("Cancelling does not end the series. It cannot be undone here.".to_string());
        confirm_primary(window, cx, "Cancel this occurrence?", lines, "Cancel occurrence", move |window, cx| {
            crate::app::with_app(cx, |app, cx| Self::report(app.apply_exception(series, original_due, ExceptionKind::Cancel, cx), window, cx));
        });
    }

    /// Move: a new due date; lags follow from it.
    pub fn open_move_occurrence(&mut self, series: SeriesId, original_due: NaiveDate, window: &mut Window, cx: &mut Context<Self>) {
        let Some(o) = self.occurrence(series, original_due).cloned() else { return };
        let due = o.due;
        self.occurrence_form.move_to.update(cx, |s, cx| s.set_date(due, window, cx));
        let picker = self.occurrence_form.move_to.clone();
        let lines = self.describe_occurrence(&o);
        let this = cx.entity().downgrade();
        let commit = std::rc::Rc::new(move |window: &mut Window, cx: &mut App| -> bool {
            let to = picker.read(cx).date().start();
            let Some(to) = to else {
                window.push_notification("Pick the new due date.", cx);
                return false;
            };
            let result = this.update(cx, |app, cx| app.apply_exception(series, original_due, ExceptionKind::Move { to }, cx)).unwrap_or(Err("Closed.".into()));
            match result {
                Ok(_) => {
                    window.close_dialog(cx);
                    true
                }
                Err(message) => {
                    window.push_notification(message, cx);
                    false
                }
            }
        });
        let picker = self.occurrence_form.move_to.clone();
        window.open_dialog(cx, move |dialog, _, cx| {
            let muted = cx.theme().muted_foreground;
            let commit = commit.clone();
            let commit_ok = commit.clone();
            dialog
                .title("Move this occurrence")
                .w_96()
                .child(
                    v_flex()
                        .gap_3()
                        .child(v_flex().gap_0p5().text_xs().text_color(muted).children(lines.iter().map(|l| div().child(l.clone()))))
                        .child(Form::vertical().child(Field::new().label("New due date").required(true).child(DatePicker::new(&picker))))
                        .child(div().text_xs().text_color(muted).child(format!("The occurrence keeps its original identity of {}; settlement and availability follow the new date.", original_due.format("%d %b %Y")))),
                )
                .footer(
                    DialogFooter::new()
                        .child(Button::new("move-cancel").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                        .child(Button::new("move-commit").primary().label("Move occurrence").on_click(move |_, window, cx| {
                            commit(window, cx);
                        })),
                )
                .on_ok(move |_, window, cx| commit_ok(window, cx))
        });
    }

    /// Change this amount: the occurrence's expected amount, nothing else.
    pub fn open_change_occurrence_amount(&mut self, series: SeriesId, original_due: NaiveDate, window: &mut Window, cx: &mut Context<Self>) {
        let Some(o) = self.occurrence(series, original_due).cloned() else { return };
        let expected = o.amount.expected();
        self.occurrence_form.amount.update(cx, |s, cx| s.set_value(expected.format(), window, cx));
        let input = self.occurrence_form.amount.clone();
        let lines = self.describe_occurrence(&o);
        let currency = expected.currency();
        let this = cx.entity().downgrade();
        let commit = std::rc::Rc::new(move |window: &mut Window, cx: &mut App| -> bool {
            let text = input.read(cx).value().trim().to_string();
            let amount = match Money::parse(&text, currency) {
                Ok(a) if !a.is_negative() => a,
                Ok(_) => {
                    window.push_notification("The amount cannot be negative; the direction supplies the sign.", cx);
                    return false;
                }
                Err(e) => {
                    window.push_notification(format!("Amount: {e}"), cx);
                    return false;
                }
            };
            let result = this.update(cx, |app, cx| app.apply_exception(series, original_due, ExceptionKind::Amount(AmountSpec::Exact(amount)), cx)).unwrap_or(Err("Closed.".into()));
            match result {
                Ok(_) => {
                    window.close_dialog(cx);
                    true
                }
                Err(message) => {
                    window.push_notification(message, cx);
                    false
                }
            }
        });
        let input = self.occurrence_form.amount.clone();
        window.open_dialog(cx, move |dialog, _, cx| {
            let muted = cx.theme().muted_foreground;
            let commit = commit.clone();
            let commit_ok = commit.clone();
            dialog
                .title("Change this amount")
                .w_96()
                .child(
                    v_flex()
                        .gap_3()
                        .child(v_flex().gap_0p5().text_xs().text_color(muted).children(lines.iter().map(|l| div().child(l.clone()))))
                        .child(Form::vertical().child(Field::new().label("New expected amount").required(true).child(Input::new(&input).id("occurrence-amount"))))
                        .child(div().text_xs().text_color(muted).child("Only this occurrence changes; the series keeps its amount and any range.")),
                )
                .footer(
                    DialogFooter::new()
                        .child(Button::new("amount-cancel").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                        .child(Button::new("amount-commit").primary().label("Change amount").on_click(move |_, window, cx| {
                            commit(window, cx);
                        })),
                )
                .on_ok(move |_, window, cx| commit_ok(window, cx))
        });
    }

    /// The transaction form prefilled for one live occurrence: its account,
    /// the signed remaining amount, and the link to its original due date.
    pub fn open_record_for_occurrence(&mut self, series: SeriesId, original_due: NaiveDate, window: &mut Window, cx: &mut Context<Self>) {
        let Some(o) = self.occurrence(series, original_due).cloned() else { return };
        let remaining = o.remaining_expected();
        let signed = match o.direction {
            Direction::Income => remaining,
            Direction::Expense | Direction::Transfer { .. } => remaining.negated(),
        };
        self.open_entry_with_account(Entry::Actual, o.account, window, cx);
        let f = &self.entry_forms;
        f.actual_date.update(cx, |s, cx| s.set_date(o.due, window, cx));
        f.actual_amount.update(cx, |s, cx| s.set_value(signed.format(), window, cx));
        f.actual_description.update(cx, |s, cx| s.set_value(o.label.clone(), window, cx));
        if let Some(row) = f.series_row(series) {
            f.actual_link_series.update(cx, |s, cx| s.set_selected_index(Some(IndexPath::new(row + 1)), window, cx));
        }
        f.actual_link_due.update(cx, |s, cx| s.set_date(original_due, window, cx));
    }

    /// The transaction form with a series preselected (from its detail); the
    /// person chooses the original due date.
    pub fn open_record_for_series(&mut self, series: SeriesId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.household.series_by_id(series).cloned() else { return };
        self.open_entry_with_account(Entry::Actual, s.account, window, cx);
        let f = &self.entry_forms;
        if let Some(row) = f.series_row(series) {
            f.actual_link_series.update(cx, |s, cx| s.set_selected_index(Some(IndexPath::new(row + 1)), window, cx));
        }
    }

    /// Matches an existing transaction to a planned occurrence: a new link,
    /// never a second transaction.
    pub fn open_match_sheet(&mut self, transaction: TransactionId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(actual) = self.household.actual(transaction).cloned() else { return };
        let Some((allocated, unallocated)) = actual_unallocated(&self.household, transaction) else { return };
        if !unallocated.is_positive() {
            window.push_notification("Nothing left to match on this transaction.", cx);
            return;
        }
        let account_name = self.household.account(actual.account).map(|a| a.name.clone()).unwrap_or_default();
        let f = &self.entry_forms;
        f.actual_link_series.update(cx, |s, cx| s.set_selected_index(Some(IndexPath::new(0)), window, cx));
        f.actual_link_due.update(cx, |s, cx| s.set_date(actual.date, window, cx));
        self.occurrence_form.match_amount.update(cx, |s, cx| s.set_value(unallocated.format(), window, cx));
        let series_choice = f.actual_link_series.clone();
        let due_picker = f.actual_link_due.clone();
        let amount_input = self.occurrence_form.match_amount.clone();
        let header = vec![
            format!("{} · {} · {}", actual.date.format("%d %b %Y"), account_name, actual.amount.format()),
            if actual.description.is_empty() { String::new() } else { actual.description.clone() },
            format!("Already matched {} · unallocated {}", allocated.format(), unallocated.format()),
        ];
        let this = cx.entity().downgrade();
        let commit = std::rc::Rc::new(move |window: &mut Window, cx: &mut App| {
            let result = this.update(cx, |app, cx| app.submit_match(transaction, cx)).unwrap_or(Err("Closed.".into()));
            match result {
                Ok(_) => window.close_sheet(cx),
                Err(message) => window.push_notification(message, cx),
            }
        });
        window.open_sheet(cx, move |sheet, _, cx| {
            let muted = cx.theme().muted_foreground;
            let commit = commit.clone();
            sheet
                .title("Match to a planned occurrence")
                .child(
                    v_flex()
                        .gap_4()
                        .child(v_flex().gap_0p5().text_sm().children(header.iter().filter(|l| !l.is_empty()).enumerate().map(|(i, l)| div().when(i > 0, |d| d.text_xs().text_color(muted)).child(l.clone()))))
                        .child(
                            Form::vertical()
                                .child(Field::new().label("Series").required(true).child(Select::new(&series_choice)))
                                .child(Field::new().label("Original due date").required(true).child(DatePicker::new(&due_picker)))
                                .child(Field::new().label("Amount to match").required(true).child(Input::new(&amount_input).id("match-amount"))),
                        )
                        .child(div().text_xs().text_color(muted).child("The link may not exceed the transaction's unallocated amount or the occurrence's remaining expected amount. Existing links stay as they are. Matching cannot be undone here; the account balance does not change.")),
                )
                .footer(
                    h_flex()
                        .w_full()
                        .justify_end()
                        .gap_2()
                        .child(Button::new("match-cancel").outline().small().label("Cancel").on_click(|_, window, cx| window.close_sheet(cx)))
                        .child(Button::new("match-commit").primary().small().label("Match transaction").on_click(move |_, window, cx| commit(window, cx))),
                )
        });
    }

    /// Validates the match sheet and links.
    pub fn submit_match(&mut self, transaction: TransactionId, cx: &mut Context<Self>) -> Result<String, String> {
        let f = &self.entry_forms;
        let row = f.actual_link_series.read(cx).selected_index(cx).map(|p| p.row).unwrap_or(0);
        if row == 0 {
            return Err("Pick the series this transaction fulfils.".into());
        }
        let series = f.series_id_at(row - 1).ok_or("Unknown series.")?;
        let original_due = f.actual_link_due.read(cx).date().start().ok_or("Pick the occurrence's original due date.")?;
        let currency = self.household.base_currency;
        let text = self.occurrence_form.match_amount.read(cx).value().trim().to_string();
        let amount = Money::parse(&text, currency).map_err(|e| format!("Amount to match: {e}"))?;
        if !amount.is_positive() {
            return Err("The amount to match must be positive.".into());
        }
        let (_, unallocated) = actual_unallocated(&self.household, transaction).ok_or("Unknown transaction.")?;
        if amount.minor() > unallocated.minor() {
            return Err(format!("Only {} of this transaction is unallocated.", unallocated.format()));
        }
        let remaining = occurrence_remaining(&self.household, series, original_due).ok_or("Unknown series.")?;
        if amount.minor() > remaining.minor() {
            return Err(format!("The occurrence due {} has only {} left to fulfil.", original_due.format("%d %b %Y"), remaining.format()));
        }
        let series_ref = self.household.series_by_id(series).ok_or("Unknown series.")?;
        let actual = self.household.actual(transaction).ok_or("Unknown transaction.")?;
        if series_ref.account != actual.account && series_ref.linked_account != Some(actual.account) && !matches!(series_ref.direction, Direction::Transfer { to } if to == actual.account) {
            return Err("The transaction's account is not one this series posts to.".into());
        }
        let sign_ok = match series_ref.direction {
            Direction::Income => actual.amount.is_positive() || series_ref.linked_account == Some(actual.account),
            Direction::Expense => actual.amount.is_negative() || series_ref.linked_account == Some(actual.account),
            Direction::Transfer { to } => (actual.amount.is_negative() && actual.account == series_ref.account) || (actual.amount.is_positive() && actual.account == to),
        };
        if !sign_ok {
            return Err("The transaction's sign does not fit this series' direction on that account.".into());
        }
        let name = series_ref.name.clone();
        self.household
            .link_actual(atlas_core::model::ReconciliationLink { series, original_due, transaction, amount })
            .map_err(|e| {
                alerting::report(Level::Warning, format!("link refused for {transaction}: {e}"));
                e.to_string()
            })?;
        let left = occurrence_remaining(&self.household, series, original_due).unwrap_or(Money::zero(currency));
        let sentence = format!("Matched {} to “{name}” due {}; {} of that occurrence remains expected.", amount.format(), original_due.format("%d %b %Y"), left.format());
        log::info!("match: {sentence}");
        self.mark_dirty();
        self.refresh_derived();
        self.note_result(sentence.clone());
        cx.notify();
        Ok(sentence)
    }

    fn report(result: Result<String, String>, window: &mut Window, cx: &mut App) {
        if let Err(message) = result {
            window.push_notification(message, cx);
        }
    }
}
