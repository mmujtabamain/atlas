//! Activity: what is due (Upcoming, with the occurrence inspector), the
//! templates behind it (Series, with the canonical series detail) and the
//! actual transactions recorded against them (Actuals, with the inspector
//! that matches an actual to a planned occurrence).

use atlas_core::ids::{ObjectRef, SeriesId};
use atlas_core::model::Household;
use atlas_core::timeline::{Direction, EventSeries, Occurrence};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _, DropdownButton},
    h_flex,
    input::Input,
    menu::PopupMenuItem,
    select::Select,
    tag::Tag,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::common::{detail_header, workspace_header};
use crate::app::AtlasApp;
use crate::entry::Entry;
use crate::models::timeline::TimelineModel;
use crate::nav::{Destination, Route};
use crate::occurrence_entry::actual_unallocated;
use crate::widgets::grid;
use crate::widgets::labels;
use crate::widgets::record::{self, Lane};
use crate::widgets::scope;
use crate::widgets::states::{action_bar, columns, count_line, empty_state, fact, info_card, lanes, none_disclosed, not_disclosed, note, section};

fn date(d: chrono::NaiveDate) -> String {
    d.format("%d %b %Y").to_string()
}

fn signed_remaining(o: &Occurrence) -> String {
    let remaining = o.remaining_expected();
    match o.direction {
        Direction::Income => format!("+{}", remaining.format()),
        Direction::Expense => format!("−{}", remaining.format()),
        Direction::Transfer { .. } => format!("→ {}", remaining.format()),
    }
}

fn plan_tag(series: &EventSeries, household: &Household) -> AnyElement {
    match series.scenario.and_then(|id| household.scenario(id)) {
        Some(s) => Tag::info().xsmall().outline().child(format!("With {}", s.name)).into_any_element(),
        None => Tag::secondary().xsmall().outline().child("Baseline").into_any_element(),
    }
}

/// One control of a register's filter row, its label beside the control
/// rather than above it.
///
/// `widgets::scope::control` stacks each label over its control, which costs a
/// register a whole line before its first row and leaves a row of ragged
/// label-over-control pairs where one line of controls reads as one filter.
/// The scope bar is shared with every analysis screen, so the inline form is
/// composed per screen (the same way `screens::accounts` composes its
/// register's row).
fn inline_control(label: &'static str, control: impl IntoElement, cx: &App) -> impl IntoElement {
    h_flex()
        .flex_shrink_0()
        .gap_2()
        .items_center()
        .child(div().flex_shrink_0().text_xs().text_color(cx.theme().muted_foreground).child(label))
        .child(control)
}

/// The trailing lane of a register row: the chevron that opens the row's
/// canonical detail, so a row reads as a way in and not only as something to
/// select (`screens::accounts`).
const OPEN_LANE: Lane = Lane::fixed(32.);

// ----- Upcoming ----------------------------------------------------------------

pub fn render_upcoming(app: &AtlasApp, model: &TimelineModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    grid::sync(&app.grids.timeline_occurrences, &model.rows, cx);
    let header = workspace_header(
        Destination::Activity,
        Route::Upcoming,
        vec![
            Button::new("new-series").small().outline().icon(IconName::Plus).label("Add movement…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Series, window, cx))).into_any_element(),
            Button::new("new-actual").small().outline().label("Record transaction…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Actual, window, cx))).into_any_element(),
        ],
        cx,
    );
    let controls = &app.timeline_controls;
    // `Clear filters` restores the four filters and the window; the Plan choice
    // is deliberately left explicit, so it does not count as a filter here.
    let filtering = [&controls.entity, &controls.account, &controls.certainty, &controls.status, &controls.horizon].into_iter().any(|choice| scope::selected(choice, cx) > 0) || model.filter.series.is_some();
    let series_chip: Option<AnyElement> = model.filter.series.and_then(|id| household.series_by_id(id)).map(|s| {
        inline_control(
            "Series",
            h_flex()
                .gap_1()
                .items_center()
                .child(Tag::info().small().outline().child(s.name.clone()))
                .child(Button::new("clear-series-filter").xsmall().ghost().compact().icon(IconName::Close).tooltip("Show every series").on_click(cx.listener(|this, _, _, cx| this.set_timeline_series(None, cx)))),
            cx,
        )
        .into_any_element()
    });
    let inspector = app.selected_occurrence.and_then(|(series, due)| model.all_occurrences.iter().find(|o| o.series == series && o.original_due == due)).map(|o| render_occurrence_inspector(o, household, cx));
    let scenario_name = model.filter.scenario.and_then(|id| household.scenario(id)).map(|s| s.name.clone());
    let shown = model.occurrences.len();
    let in_window = model.all_occurrences.len();
    let live = model.occurrences.iter().filter(|o| o.is_live()).count();
    // The count answers the filters, so it says what they left and how much of
    // it still posts — not a bare total that hides how much of the window is
    // out of view.
    let counted = if shown == in_window { format!("{shown} movements · {live} live") } else { format!("{shown} of {in_window} movements match the filters · {live} live") };
    let window_line = format!(
        "· Expected values from {} through {}, {} · the window is local to Upcoming and recorded transactions are unaffected",
        date(household.as_of),
        date(model.filter.through),
        match &scenario_name {
            Some(name) => format!("baseline plus scenario “{name}”"),
            None => "baseline only".to_string(),
        }
    );
    let theme = cx.theme();

    let body: AnyElement = if model.occurrences.is_empty() {
        if model.all_occurrences.is_empty() && household.series.is_empty() {
            empty_state("upcoming-empty", "No planned movements yet", "Add a movement to see what is due and when planned money becomes usable.", Some(Button::new("upcoming-add-first").small().outline().icon(IconName::Plus).label("Add movement…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Series, window, cx))).into_any_element()), cx)
        } else if model.all_occurrences.is_empty() && model.hidden_series > 0 && model.series.is_empty() {
            none_disclosed("upcoming-none-disclosed", "planned movements", model.hidden_series, cx)
        } else {
            empty_state("upcoming-no-match", "No matches", "No movement in this window matches the filters.", Some(Button::new("clear-timeline-filters-2").small().ghost().label("Clear filters").on_click(cx.listener(|this, _, window, cx| this.clear_timeline_filters(window, cx))).into_any_element()), cx)
        }
    } else {
        grid::render("upcoming-grid", &app.grids.timeline_occurrences, cx).into_any_element()
    };

    v_flex()
        .id("screen-upcoming")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        // One inline row of filters with `Clear filters` at its trailing edge
        // and what they left underneath. Stacked label-over-control pairs cost
        // this screen a line before the first figure and read as six separate
        // questions rather than one filter (`screens::accounts`).
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
                                .child(inline_control("Entity", Select::new(&controls.entity).small().w(px(160.)), cx))
                                .child(inline_control("Account", Select::new(&controls.account).small().w(px(200.)), cx))
                                .child(inline_control("Certainty", Select::new(&controls.certainty).small().w(px(170.)), cx))
                                .child(inline_control("Status", Select::new(&controls.status).small().w(px(160.)), cx))
                                .child(inline_control("Show upcoming through", Select::new(&controls.horizon).small().w(px(200.)), cx))
                                .child(inline_control("Plan", Select::new(&app.plan_choices.upcoming).small().w(px(180.)), cx))
                                .children(series_chip),
                        )
                        .when(filtering, |this| this.child(Button::new("clear-timeline-filters").small().ghost().label("Clear filters").on_click(cx.listener(|this, _, window, cx| this.clear_timeline_filters(window, cx))))),
                )
                .child(
                    h_flex()
                        .w_full()
                        .flex_wrap()
                        .gap_1()
                        .items_center()
                        .child(div().id("count-movements").test_support().text_xs().text_color(theme.muted_foreground).child(counted))
                        .child(note(window_line, cx)),
                ),
        )
        // The two totals beside the rule they are read under, as one even grid.
        // The reading (`Expected · Baseline`) is the heading's badge instead of
        // a clause repeated in the description.
        .child(
            section("upcoming-totals", "Still to come")
                .badge(match &scenario_name {
                    Some(name) => format!("Expected · With {name}"),
                    None => "Expected · Baseline".to_string(),
                })
                .description("Expected values, less what has already been received or paid.")
                // Both totals are figures now, not plain text: each carries
                // the movements it is the sum of, so `ⓘ` opens the same kind
                // of calculation sheet every other headline number does.
                .child(columns([
                    model.total_in.standard().into_any_element(),
                    model.total_out.standard().into_any_element(),
                    info_card(
                        "upcoming-basis",
                        IconName::Info,
                        "What these totals leave out",
                        "Before tax and fees. Transfers are not counted, and skipped, cancelled or fulfilled movements post nothing.",
                        cx,
                    ),
                ])),
        )
        .child(
            section("upcoming-list", "Movements")
                .divider(true)
                .description("Due date first, then the explicit same-day order. Settlement and availability are in the row; the inspector has all four dates. Select a row to open it.")
                .child(body)
                .when(model.hidden_series > 0, |this| this.child(note(format!("{} series not disclosed to this viewer; their movements are not listed.", model.hidden_series), cx))),
        )
        .children(inspector)
        .into_any_element()
}

/// The selected occurrence: all four dates, identity, amounts, the source
/// series and its account, and the single-occurrence commands.
fn render_occurrence_inspector(o: &Occurrence, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let series = o.series;
    let original_due = o.original_due;
    let account = o.account;
    let live = o.is_live();
    let account_name = household.account(o.account).map(|a| a.name.clone()).unwrap_or_default();
    let direction = match o.direction {
        Direction::Transfer { to } => format!("Transfer to {}", household.account(to).map(|a| a.name.clone()).unwrap_or_default()),
        d => d.label().to_string(),
    };
    let linked = o.linked_account.and_then(|id| household.account(id)).map(|a| format!("Linked movement on {}", a.name));
    let record_label = if o.direction == Direction::Income { "Record receipt…" } else { "Record payment…" };
    let same_day = o.due == o.settlement && o.settlement == o.availability;
    // The declared range qualifies the expected amount rather than standing as
    // a lane of its own: it is the same figure read two ways.
    let expected = match o.amount.low() != o.amount.high() {
        true => format!("{} · declared {} – {}", o.amount.expected().format(), o.amount.low().format(), o.amount.high().format()),
        false => o.amount.expected().format(),
    };
    let scenario = o.scenario.and_then(|id| household.scenario(id)).map(|s| s.name.clone());

    // Where this occurrence can take you on the left of the bar, what can be
    // done to it on the right.
    let leading: Vec<AnyElement> = vec![
        Button::new("occurrence-open-series").small().ghost().icon(IconName::ListTree).label("Open series").on_click(cx.listener(move |this, _, _, cx| this.navigate(Route::SeriesDetail(series), cx))).into_any_element(),
        Button::new("occurrence-open-account").small().ghost().icon(IconName::Landmark).label("Open account").on_click(cx.listener(move |this, _, _, cx| this.navigate(Route::Account(account), cx))).into_any_element(),
    ];
    let mut trailing: Vec<AnyElement> = Vec::new();
    if live {
        trailing.push(Button::new("occurrence-record").small().outline().label(record_label).on_click(cx.listener(move |this, _, window, cx| this.open_record_for_occurrence(series, original_due, window, cx))).into_any_element());
        trailing.push(
            DropdownButton::new("occurrence-more")
                .small()
                .button(Button::new("occurrence-more-button").small().ghost().label("More"))
                .dropdown_menu(move |menu, _, _| {
                    menu.item(PopupMenuItem::new("Skip occurrence…").on_click(move |_, window, cx| crate::app::with_app(cx, |app, cx| app.confirm_skip_occurrence(series, original_due, window, cx))))
                        .item(PopupMenuItem::new("Cancel occurrence…").on_click(move |_, window, cx| crate::app::with_app(cx, |app, cx| app.confirm_cancel_occurrence(series, original_due, window, cx))))
                        .item(PopupMenuItem::new("Move occurrence…").on_click(move |_, window, cx| crate::app::with_app(cx, |app, cx| app.open_move_occurrence(series, original_due, window, cx))))
                        .item(PopupMenuItem::new("Change this amount…").on_click(move |_, window, cx| crate::app::with_app(cx, |app, cx| app.open_change_occurrence_amount(series, original_due, window, cx))))
                })
                .into_any_element(),
        );
    }

    let theme = cx.theme();
    // Identity varies in length (a link and a plan are not always there), so it
    // wraps; the four clocks and the three amounts are fixed sets and read as
    // even grids.
    let mut identity: Vec<AnyElement> = vec![
        fact("Direction", direction, cx).into_any_element(),
        fact("Account", account_name, cx).into_any_element(),
        fact("Whose", household.entity_name(o.entity), cx).into_any_element(),
    ];
    if let Some(linked) = linked {
        identity.push(fact("Link", linked, cx).into_any_element());
    }
    if let Some(scenario) = scenario {
        identity.push(fact("Plan", format!("Scenario “{scenario}”"), cx).into_any_element());
    }

    section("occurrence-inspector", format!("Selected: {}", o.label))
        .divider(true)
        .action(Button::new("occurrence-close").small().ghost().icon(IconName::Close).tooltip("Close").on_click(cx.listener(|this, _, _, cx| this.close_occurrence_inspector(cx))))
        .child(
            h_flex()
                .w_full()
                .gap_2()
                .items_center()
                .child(labels::certainty_tag(o.certainty))
                .child(labels::status_tag(o.status))
                .when(!live, |this| this.child(div().text_xs().text_color(theme.muted_foreground).child("Not live: it posts nothing and cannot be paid from here."))),
        )
        .child(columns([
            fact("Due", format!("{}{}", date(o.due), if o.due != o.original_due { format!(" · originally {}", date(o.original_due)) } else { String::new() }), cx).into_any_element(),
            fact("Posts", date(o.posting), cx).into_any_element(),
            fact("Settles", date(o.settlement), cx).into_any_element(),
            fact("Available", if same_day { format!("{} · same day", date(o.availability)) } else { date(o.availability) }, cx).into_any_element(),
        ]))
        .child(columns([
            fact("Expected", expected, cx).into_any_element(),
            fact("Fulfilled", o.fulfilled.format(), cx).into_any_element(),
            fact("Remaining", signed_remaining(o), cx).into_any_element(),
        ]))
        .child(lanes(identity))
        .child(info_card(
            "occurrence-identity",
            IconName::Calendar,
            "The original due date is this occurrence's identity",
            format!("Exceptions and reconciliation are keyed on {}, even after a move. An expected receipt is not usable before its availability date.", date(o.original_due)),
            cx,
        ))
        .child(action_bar("occurrence-commands", leading, trailing, cx))
        .into_any_element()
}

// ----- Series ----------------------------------------------------------------

const SERIES_LANES: [(&str, Lane); 7] = [
    ("Series / whose", Lane::fixed(260.)),
    ("Account / direction", Lane::fixed(210.)),
    ("Amount / range", Lane::money(150.)),
    ("Recurrence", Lane::flex()),
    ("Certainty", Lane::fixed(130.)),
    ("Plan", Lane::fixed(110.)),
    ("", OPEN_LANE),
];

pub fn render_series(app: &AtlasApp, model: &TimelineModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let header = workspace_header(
        Destination::Activity,
        Route::Series,
        vec![Button::new("new-series").small().outline().icon(IconName::Plus).label("Add movement…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Series, window, cx))).into_any_element()],
        cx,
    );
    let query = app.activity_controls.series_search.read(cx).value().trim().to_lowercase();
    let searching = !query.is_empty();
    let selected = app.selected_series;
    let rows: Vec<_> = model
        .series
        .iter()
        .filter_map(|id| household.series_by_id(*id))
        .filter(|s| query.is_empty() || s.name.to_lowercase().contains(&query))
        .map(|s| {
            let id = s.id;
            let account_name = household.account(s.account).map(|a| a.name.clone()).unwrap_or_default();
            let amount = s.amount.expected();
            let range = if s.amount.low() != s.amount.high() { format!("{} – {}", s.amount.low().format(), s.amount.high().format()) } else { String::new() };
            record::row(
                SharedString::from(format!("series-{}", id.raw())),
                selected == Some(id),
                vec![
                    (SERIES_LANES[0].1, record::stack(s.name.clone(), household.entity_name(s.entity), cx)),
                    (SERIES_LANES[1].1, record::stack(account_name, s.direction.label(), cx)),
                    (SERIES_LANES[2].1, v_flex().items_end().child(record::money(amount, cx)).when(!range.is_empty(), |v| v.child(record::muted(range, cx))).into_any_element()),
                    (SERIES_LANES[3].1, record::muted(s.recurrence.describe(), cx)),
                    (SERIES_LANES[4].1, h_flex().child(labels::certainty_tag(s.certainty)).into_any_element()),
                    (SERIES_LANES[5].1, h_flex().child(plan_tag(s, household)).into_any_element()),
                    (
                        SERIES_LANES[6].1,
                        Button::new(SharedString::from(format!("series-open-{}", id.raw())))
                            .xsmall()
                            .ghost()
                            .compact()
                            .icon(IconName::ChevronRight)
                            .tooltip("Open series")
                            .on_click(cx.listener(move |this, _, _, cx| this.navigate(Route::SeriesDetail(id), cx)))
                            .into_any_element(),
                    ),
                ],
                move |_, _, cx| crate::app::with_app(cx, |app, cx| app.select_series(id, cx)),
            )
        })
        .collect();
    let visible = model.series.len();
    let total = visible + model.hidden_series;
    // The foot of the register: what is selected on the left, what can be done
    // with it on the right, ruled off from the rows above.
    let selected_footer = selected.and_then(|id| household.series_by_id(id)).map(|s| {
        let id = s.id;
        let account_name = household.account(s.account).map(|a| a.name.clone()).unwrap_or_default();
        action_bar(
            "series-footer",
            vec![note(format!("{} · {} · {}", s.name, account_name, s.direction.label()), cx).into_any_element()],
            vec![
                Button::new("series-open").small().outline().label("Open series").on_click(cx.listener(move |this, _, _, cx| this.navigate(Route::SeriesDetail(id), cx))).into_any_element(),
                DropdownButton::new("series-more")
                    .small()
                    .button(Button::new("series-more-button").small().ghost().label("More"))
                    .dropdown_menu(move |menu, _, _| {
                        menu.item(PopupMenuItem::new("Change series…").on_click(move |_, window, cx| crate::app::with_app(cx, |app, cx| app.open_series_editor(id, window, cx))))
                            .item(PopupMenuItem::new("View upcoming").on_click(move |_, _, cx| crate::app::with_app(cx, |app, cx| app.open_upcoming_for_series(id, cx))))
                            .item(PopupMenuItem::new("Delete series…").on_click(move |_, window, cx| crate::app::with_app(cx, |app, cx| app.confirm_delete(ObjectRef::Series(id), window, cx))))
                    })
                    .into_any_element(),
            ],
            cx,
        )
        .into_any_element()
    });
    let body: AnyElement = if household.series.is_empty() {
        let action = if household.accounts.is_empty() {
            Button::new("series-add-account").small().outline().label("Add account…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Account, window, cx))).into_any_element()
        } else {
            Button::new("series-add-first").small().outline().icon(IconName::Plus).label("Add movement…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Series, window, cx))).into_any_element()
        };
        empty_state("series-empty", "No planned movements yet", if household.accounts.is_empty() { "A movement needs an account to post to." } else { "Salary, rent, a loan repayment — every recurring or one-time plan is a series." }, Some(action), cx)
    } else if model.series.is_empty() {
        none_disclosed("series-none-disclosed", "series", model.hidden_series, cx)
    } else if rows.is_empty() {
        empty_state("series-no-match", "No matches", "No disclosed series has that name.", Some(Button::new("series-clear-search").small().ghost().label("Clear search").on_click(cx.listener(|this, _, window, cx| {
            this.activity_controls.series_search.update(cx, |s, cx| s.set_value("", window, cx));
            cx.notify();
        })).into_any_element()), cx)
    } else {
        record::list("series-list", record::header(&SERIES_LANES, cx), rows).into_any_element()
    };

    v_flex()
        .id("screen-series")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
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
                                .child(inline_control("Plan", Select::new(&app.plan_choices.series).small().w(px(200.)), cx))
                                .child(inline_control("Search", Input::new(&app.activity_controls.series_search).small().w(px(240.)), cx)),
                        )
                        .when(searching, |this| {
                            this.child(Button::new("series-clear").small().ghost().label("Clear search").on_click(cx.listener(|this, _, window, cx| {
                                this.activity_controls.series_search.update(cx, |s, cx| s.set_value("", window, cx));
                                cx.notify();
                            })))
                        }),
                )
                .child(
                    h_flex()
                        .w_full()
                        .flex_wrap()
                        .gap_1()
                        .items_center()
                        .child(count_line(visible, total, "series", cx))
                        // The rule, not a repeat of the register: the Plan
                        // choice above already says which plan is on show.
                        .child(note("· Lags, same-day order, changes and exceptions are in each series' detail", cx)),
                ),
        )
        .child(body)
        .children(selected_footer)
        .into_any_element()
}

// ----- Series detail ---------------------------------------------------------

pub fn render_series_detail(app: &AtlasApp, id: SeriesId, model: &TimelineModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let Some(series) = household.series_by_id(id) else {
        return v_flex().id("screen-series-detail").test_support().gap_4().child(detail_header(Destination::Activity, Route::Series, "Series", "Unknown series", None, vec![], cx)).child(note("This series no longer exists.", cx)).into_any_element();
    };
    if !model.series.contains(&id) {
        return v_flex()
            .id("screen-series-detail")
            .test_support()
            .gap_4()
            .child(detail_header(Destination::Activity, Route::Series, "Series", "Series", None, vec![], cx))
            .child(not_disclosed("series-detail-not-disclosed", "Its account or company is not disclosed to this viewer, or it belongs to a plan not on show.", cx))
            .into_any_element()
    }
    let viewer = app.viewer();
    let account_name = household.account(series.account).map(|a| a.name.clone()).unwrap_or_default();
    let account = series.account;
    let muted = cx.theme().muted_foreground;
    // One meta line of the facts that identify the series, with the certainty
    // and plan tags on the line beneath rather than sharing a wrap row with it
    // (`docs/perf.md` §3.3).
    let subtitle = v_flex()
        .w_full()
        .gap_1()
        .child(div().w_full().text_sm().text_color(muted).child(format!("{} · {} · {}", household.entity_name(series.entity), account_name, series.direction.label())))
        .child(h_flex().w_full().gap_2().items_center().child(labels::certainty_tag(series.certainty)).child(plan_tag(series, household)))
        .into_any_element();
    let header = detail_header(
        Destination::Activity,
        Route::Series,
        "Series",
        series.name.clone(),
        Some(subtitle),
        vec![
            Button::new("series-change").small().outline().label("Change series…").on_click(cx.listener(move |this, _, window, cx| this.open_series_editor(id, window, cx))).into_any_element(),
            DropdownButton::new("series-detail-more")
                .small()
                .button(Button::new("series-detail-more-button").small().ghost().label("More"))
                .dropdown_menu(move |menu, _, _| {
                    // `Record transaction…` left this menu for the command bar
                    // at the foot, where the screen's other commands are.
                    menu.item(PopupMenuItem::new("View policy").on_click(move |_, _, cx| crate::app::with_app(cx, |app, cx| app.open_policy_for(ObjectRef::Account(account), cx))))
                        .item(PopupMenuItem::new("Delete series…").on_click(move |_, window, cx| crate::app::with_app(cx, |app, cx| app.confirm_delete(ObjectRef::Series(id), window, cx))))
                })
                .into_any_element(),
        ],
        cx,
    );

    // Scheduling facts, as two even rows: what one occurrence is worth, then
    // when each of its four clocks fires. The optional facts (a transfer
    // target, a linked movement, a tax treatment) vary in number, so they wrap
    // beneath instead of leaving holes in the grid.
    let amount = if series.amount.low() != series.amount.high() {
        format!("{} expected · {} – {}", series.amount.expected().format(), series.amount.low().format(), series.amount.high().format())
    } else {
        series.amount.expected().format()
    };
    let mut extra: Vec<AnyElement> = Vec::new();
    if let Direction::Transfer { to } = series.direction {
        extra.push(fact("Transfer to", household.account(to).map(|a| a.name.clone()).unwrap_or_default(), cx).into_any_element());
    }
    if let Some(linked) = series.linked_account.and_then(|a| household.account(a)) {
        extra.push(fact("Linked movement", format!("Also posts on {}", linked.name), cx).into_any_element());
    }
    if !series.tax_treatment.is_empty() {
        extra.push(fact("Tax treatment", series.tax_treatment.clone(), cx).into_any_element());
    }
    let notes = (!series.notes.is_empty()).then(|| series.notes.clone());

    // Changes and exceptions.
    let change_lanes: [(&str, Lane); 2] = [("From", Lane::fixed(160.)), ("Amount", Lane::flex())];
    let change_rows: Vec<_> = series
        .amount_changes
        .iter()
        .enumerate()
        .map(|(i, c)| record::row(SharedString::from(format!("change-{i}")), false, vec![(change_lanes[0].1, record::text(date(c.effective_from))), (change_lanes[1].1, record::text(c.amount.describe()))], |_, _, _| {}))
        .collect();
    let exception_lanes: [(&str, Lane); 2] = [("Original due", Lane::fixed(160.)), ("What happens", Lane::flex())];
    let exception_rows: Vec<_> = series
        .exceptions
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let what = match e.kind {
                atlas_core::timeline::ExceptionKind::Skip => "Skipped — posts nothing".to_string(),
                atlas_core::timeline::ExceptionKind::Cancel => "Cancelled — posts nothing".to_string(),
                atlas_core::timeline::ExceptionKind::Move { to } => format!("Moved to {}", date(to)),
                atlas_core::timeline::ExceptionKind::Amount(a) => format!("Amount {} for this occurrence only", a.describe()),
            };
            record::row(SharedString::from(format!("exception-{i}")), false, vec![(exception_lanes[0].1, record::text(date(e.original_due))), (exception_lanes[1].1, record::text(what))], |_, _, _| {})
        })
        .collect();

    // Upcoming preview from the window on show.
    const PREVIEW: usize = 8;
    let preview_lanes: [(&str, Lane); 5] = [("Due", Lane::fixed(120.)), ("Remaining", Lane::money(140.)), ("Settles / available", Lane::fixed(220.)), ("Status", Lane::flex()), ("", OPEN_LANE)];
    let upcoming: Vec<&Occurrence> = model.all_occurrences.iter().filter(|o| o.series == id).collect();
    let preview_rows: Vec<_> = upcoming
        .iter()
        .take(PREVIEW)
        .map(|o| {
            let original_due = o.original_due;
            record::row(
                SharedString::from(format!("preview-{}", o.sequence)),
                false,
                vec![
                    (preview_lanes[0].1, record::text(format!("{}{}", date(o.due), if o.due != o.original_due { " · moved" } else { "" }))),
                    (preview_lanes[1].1, record::money(o.remaining_expected(), cx)),
                    (preview_lanes[2].1, record::muted(format!("{} / {}", date(o.settlement), date(o.availability)), cx)),
                    (preview_lanes[3].1, h_flex().child(labels::status_tag(o.status)).into_any_element()),
                    (
                        preview_lanes[4].1,
                        Button::new(SharedString::from(format!("preview-open-{}", o.sequence)))
                            .xsmall()
                            .ghost()
                            .compact()
                            .icon(IconName::ChevronRight)
                            .tooltip("Open this occurrence in Upcoming")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.select_occurrence(id, original_due, cx);
                                this.open_upcoming_for_series(id, cx);
                            }))
                            .into_any_element(),
                    ),
                ],
                |_, _, _| {},
            )
        })
        .collect();
    let no_future_reason = if upcoming.is_empty() {
        Some(match &series.recurrence {
            atlas_core::timeline::Recurrence::OneTime { on } if on.latest() < household.as_of => format!("Its one occurrence on {} is before the reconciliation date.", on.describe()),
            r if r.describe().contains("until") => format!("The series ends before the window: {}.", r.describe()),
            _ => format!("No occurrence falls between {} and {}.", date(household.as_of), date(model.filter.through)),
        })
    } else {
        None
    };

    // Reconciliation links and assumptions.
    let links: Vec<String> = household
        .links
        .iter()
        .filter(|l| l.series == id)
        .map(|l| {
            let actual = household.actual(l.transaction);
            format!(
                "Due {} — {} matched to the transaction of {}{}",
                date(l.original_due),
                l.amount.format(),
                actual.map(|t| date(t.date)).unwrap_or_default(),
                actual.map(|t| if t.description.is_empty() { String::new() } else { format!(" “{}”", t.description) }).unwrap_or_default()
            )
        })
        .collect();
    let assumptions: Vec<&atlas_core::model::Assumption> = household.assumptions.iter().filter(|a| a.applies_to.contains(&id)).filter(|a| a.private_to.is_none_or(|p| p == viewer.person)).collect();
    let theme = cx.theme();

    v_flex()
        .id("screen-series-detail")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(
            section("series-facts", "Amount and schedule")
                .description("Everything the engine reads when it expands this series. Lags shift settlement and availability after the due date; the same-day order decides what posts first when dates coincide.")
                .child(columns([
                    fact("Amount per occurrence", amount, cx).into_any_element(),
                    fact("Category", if series.category.is_empty() { "Uncategorised".to_string() } else { series.category.clone() }, cx).into_any_element(),
                    fact("Recurrence", series.recurrence.describe(), cx).into_any_element(),
                ]))
                .child(columns([
                    fact("Settlement lag", format!("{} days", series.settlement_lag_days), cx).into_any_element(),
                    fact("Availability lag", format!("{} days", series.availability_lag_days), cx).into_any_element(),
                    fact("Same-day order", series.intraday_order.to_string(), cx).into_any_element(),
                ]))
                .when(!extra.is_empty(), |this| this.child(lanes(extra)))
                // A note can be a paragraph, so it gets its own full-width row
                // rather than a 16 rem lane (`docs/perf.md` §3.3).
                .when_some(notes, |this, notes| {
                    this.child(v_flex().w_full().gap_1().child(div().w_full().text_xs().text_color(muted).child("Notes")).child(div().w_full().text_sm().child(notes)))
                }),
        )
        // The two exception registers side by side: each is two short lanes,
        // and stacked full width they left the right half of the window empty.
        .child(columns([
            section("series-changes", "Amount changes")
                .description("Effective-dated: each applies from its date until the next.")
                .child(if change_rows.is_empty() { note("None — the amount above applies throughout.", cx).into_any_element() } else { record::list("series-changes-list", record::header(&change_lanes, cx), change_rows).into_any_element() })
                .into_any_element(),
            section("series-exceptions", "Exceptions")
                .description("Single occurrences, identified by their original due date.")
                .child(if exception_rows.is_empty() { note("None — every occurrence follows the schedule.", cx).into_any_element() } else { record::list("series-exceptions-list", record::header(&exception_lanes, cx), exception_rows).into_any_element() })
                .into_any_element(),
        ]))
        .child(
            section("series-upcoming", "Upcoming movements")
                .divider(true)
                .badge(format!("Expected · through {}", date(model.filter.through)))
                .description("What this series expands to under the plan on show. Commands on one occurrence are in Upcoming.")
                .action(Button::new("series-view-upcoming").small().ghost().icon(IconName::Calendar).label("View upcoming").on_click(cx.listener(move |this, _, _, cx| this.open_upcoming_for_series(id, cx))))
                .child(match no_future_reason {
                    Some(reason) => note(format!("No upcoming movement. {reason}"), cx).into_any_element(),
                    None => record::list("series-upcoming-list", record::header(&preview_lanes, cx), preview_rows).into_any_element(),
                })
                .when(upcoming.len() > PREVIEW, |this| this.child(div().text_xs().text_color(theme.muted_foreground).child(format!("{} more in Upcoming.", upcoming.len() - PREVIEW)))),
        )
        .child(
            section("series-records", "Recorded transactions")
                .description("What has been matched to this series. A link only reduces the occurrence's remaining amount; it never changes a balance or extends history.")
                .child(if links.is_empty() { note("Nothing matched yet.", cx).into_any_element() } else { v_flex().w_full().gap_1().text_sm().children(links.into_iter().map(|l| div().w_full().child(l))).into_any_element() }),
        )
        .child(
            section("series-assumptions", "Assumptions")
                .divider(true)
                .description("What must hold for this series' amounts to be right.")
                .child(if assumptions.is_empty() {
                    note("No assumption applies to this series.", cx).into_any_element()
                } else {
                    // The sentence takes its own full-width row and the tag and
                    // the link sit on the line beneath it: long text beside a
                    // tag is re-measured at every sizing pass
                    // (`docs/perf.md` §3.3).
                    v_flex()
                        .w_full()
                        .gap_3()
                        .children(assumptions.iter().map(|a| {
                            v_flex()
                                .w_full()
                                .gap_1()
                                .child(div().w_full().text_sm().child(a.text.clone()))
                                .child(
                                    h_flex()
                                        .w_full()
                                        .gap_2()
                                        .items_center()
                                        .child(labels::certainty_tag(a.certainty))
                                        .child(record::link(format!("series-assumption-{}", a.id.raw()), "Open", cx.listener(|this, _, _, cx| this.navigate(Route::Assumptions, cx)))),
                                )
                        }))
                        .into_any_element()
                }),
        )
        .child(info_card(
            "series-access",
            IconName::ShieldCheck,
            "Access is inherited from the account",
            format!("Whoever can see {account_name} can see this series and its occurrences. A series carries no policy of its own."),
            cx,
        ))
        // The screen's own commands, ruled off at its foot: where this series
        // can be inspected on the left, what can be recorded against it on the
        // right. They were scattered across four section headings.
        .child(action_bar(
            "series-commands",
            vec![
                Button::new("series-open-account").small().ghost().icon(IconName::Landmark).label(format!("Open {account_name}")).on_click(cx.listener(move |this, _, _, cx| this.navigate(Route::Account(account), cx))).into_any_element(),
                Button::new("series-policy").small().ghost().icon(IconName::Eye).label("Access policy").on_click(cx.listener(move |this, _, _, cx| this.open_policy_for(ObjectRef::Account(account), cx))).into_any_element(),
            ],
            vec![
                Button::new("series-derive").small().ghost().label("Derive from history").on_click(cx.listener(move |this, _, _, cx| this.open_derive_for_series(id, cx))).into_any_element(),
                Button::new("series-add-assumption").small().outline().icon(IconName::Plus).label("Add assumption…").on_click(cx.listener(move |this, _, window, cx| this.open_assumption_for_series(id, window, cx))).into_any_element(),
                Button::new("series-record").small().outline().label("Record transaction…").on_click(cx.listener(move |this, _, window, cx| this.open_record_for_series(id, window, cx))).into_any_element(),
            ],
            cx,
        ))
        .into_any_element()
}

// ----- Actuals -----------------------------------------------------------------

pub fn render_actuals(app: &AtlasApp, model: &TimelineModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    grid::sync(&app.grids.timeline_actuals, &model.actual_rows, cx);
    let header = workspace_header(
        Destination::Activity,
        Route::Actuals,
        vec![Button::new("new-actual").small().outline().icon(IconName::Plus).label("Record transaction…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Actual, window, cx))).into_any_element()],
        cx,
    );
    let filtered = model.filter.actuals_account.is_some();
    let inspector = app.selected_actual.and_then(|id| household.actual(id)).map(|t| render_actual_inspector(t, household, cx));
    let visible_total = household.actuals.len() - model.hidden_actuals;

    let body: AnyElement = if household.actuals.is_empty() {
        empty_state("actuals-empty", "No recorded transactions", "A planned payment is not a transaction; record one when money actually moved.", Some(Button::new("actuals-add-first").small().outline().icon(IconName::Plus).label("Record transaction…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Actual, window, cx))).into_any_element()), cx)
    } else if visible_total == 0 {
        none_disclosed("actuals-none-disclosed", "transactions", model.hidden_actuals, cx)
    } else if model.actual_ids.is_empty() {
        empty_state("actuals-no-match", "No transactions on this account", "Every disclosed transaction is on another account.", Some(Button::new("clear-actuals-filter-2").small().ghost().label("Clear filter").on_click(cx.listener(|this, _, window, cx| {
            AtlasApp::set_choice(&this.activity_controls.actuals_account, 0, window, cx);
            this.set_actuals_account(None, cx);
        })).into_any_element()), cx)
    } else {
        grid::render("actuals-grid", &app.grids.timeline_actuals, cx).into_any_element()
    };

    v_flex()
        .id("screen-actuals")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
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
                        .child(h_flex().flex_wrap().gap_5().items_center().child(inline_control("Account", Select::new(&app.activity_controls.actuals_account).small().w(px(240.)), cx)))
                        .when(filtered, |this| {
                            this.child(Button::new("clear-actuals-filter").small().ghost().label("Clear filter").on_click(cx.listener(|this, _, window, cx| {
                                AtlasApp::set_choice(&this.activity_controls.actuals_account, 0, window, cx);
                                this.set_actuals_account(None, cx);
                            })))
                        }),
                )
                .child(
                    h_flex()
                        .w_full()
                        .flex_wrap()
                        .gap_1()
                        .items_center()
                        .child(count_line(model.actual_ids.len(), if filtered { model.actual_ids.len() } else { household.actuals.len() }, "transactions", cx))
                        // The distinction the register exists to protect, said
                        // once and always, rather than as an alert about
                        // something being wrong.
                        .child(note("· Recording a transaction does not update the statement balance", cx)),
                ),
        )
        .child(
            section("actuals-list", "Transactions")
                .description("Newest first, whatever the Upcoming window or the plan on show. Matching is explicit: a transaction is unreconciled until someone matches it to a planned occurrence. Select a row to open it.")
                .child(body)
                .when(model.hidden_actuals > 0 && visible_total > 0, |this| this.child(note(format!("{} transactions on accounts not disclosed to this viewer are not listed.", model.hidden_actuals), cx)))
                .child(info_card(
                    "actuals-basis",
                    IconName::Landmark,
                    "Matching and reconciliation are separate",
                    "Matching an actual reduces a planned occurrence's remainder. Reconciling an account is the entry that updates its statement balance.",
                    cx,
                )),
        )
        .children(inspector)
        .into_any_element()
}

/// The selected transaction: the full record, every reconciliation link, the
/// allocated and unallocated amounts, and the match command.
fn render_actual_inspector(t: &atlas_core::model::ActualTransaction, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let id = t.id;
    let account = t.account;
    let account_name = household.account(t.account).map(|a| a.name.clone()).unwrap_or_default();
    let (allocated, unallocated) = actual_unallocated(household, id).unwrap_or((t.amount.abs(), t.amount.abs()));
    let fully = !unallocated.is_positive();
    let links: Vec<AnyElement> = household
        .links
        .iter()
        .filter(|l| l.transaction == id)
        .enumerate()
        .map(|(i, l)| {
            let series = l.series;
            let name = household.series_by_id(series).map(|s| s.name.clone()).unwrap_or_else(|| series.to_string());
            let moved = household.series_by_id(series).and_then(|s| s.exception_on(l.original_due)).and_then(|e| match e.kind {
                atlas_core::timeline::ExceptionKind::Move { to } => Some(to),
                _ => None,
            });
            h_flex()
                .gap_2()
                .items_center()
                .child(div().text_sm().child(format!("{} matched to", l.amount.format())))
                .child(record::link(format!("link-series-{i}"), name, cx.listener(move |this, _, _, cx| this.navigate(Route::SeriesDetail(series), cx))))
                .child(div().text_sm().child(format!("due {}{}", date(l.original_due), moved.map(|d| format!(" (now {})", date(d))).unwrap_or_default())))
                .into_any_element()
        })
        .collect();
    let theme = cx.theme();
    section("actual-inspector", format!("Selected: {} on {}", t.amount.format(), date(t.date)))
        .divider(true)
        .action(Button::new("actual-close").small().ghost().icon(IconName::Close).tooltip("Close").on_click(cx.listener(|this, _, _, cx| this.close_actual_inspector(cx))))
        .child(columns([
            fact("Date", date(t.date), cx).into_any_element(),
            fact("Account", account_name, cx).into_any_element(),
            fact("Description", if t.description.is_empty() { "Manual entry".to_string() } else { t.description.clone() }, cx).into_any_element(),
        ]))
        .child(columns([
            fact("Signed amount", t.amount.format(), cx).into_any_element(),
            fact("Matched", allocated.format(), cx).into_any_element(),
            fact("Unallocated", if fully { "Nothing left to match".to_string() } else { unallocated.format() }, cx).into_any_element(),
        ]))
        .child(if links.is_empty() {
            div().w_full().text_sm().text_color(theme.muted_foreground).child("Unreconciled — not matched to any planned occurrence.").into_any_element()
        } else {
            v_flex().w_full().gap_1().children(links).into_any_element()
        })
        .child(note("Links are read-only here — there is no unlink command.", cx))
        .child(action_bar(
            "actual-commands",
            vec![Button::new("actual-open-account").small().ghost().icon(IconName::Landmark).label("Open account").on_click(cx.listener(move |this, _, _, cx| this.navigate(Route::Account(account), cx))).into_any_element()],
            vec![
                Button::new("actual-match")
                    .small()
                    .outline()
                    .label("Match to planned occurrence…")
                    .disabled(fully)
                    .tooltip(if fully { "Nothing left to match" } else { "Link part or all of this transaction to a planned occurrence" })
                    .on_click(cx.listener(move |this, _, window, cx| this.open_match_sheet(id, window, cx)))
                    .into_any_element(),
                Button::new("actual-reconcile").small().ghost().label("Reconcile account…").on_click(cx.listener(move |this, _, window, cx| this.open_entry(Entry::Reconcile(account), window, cx))).into_any_element(),
            ],
            cx,
        ))
        .into_any_element()
}
