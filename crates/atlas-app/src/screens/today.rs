//! Today: what is free now, what the expected plan produces by the horizon,
//! and what that forecast depends on. Household only, no scope pickers.

use atlas_core::model::Household;
use atlas_core::Disclosure;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, WindowExt as _,
    alert::Alert,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::AtlasApp;
use crate::entry::Entry;
use crate::models::household::HouseholdOverview;
use crate::nav::Route;
use crate::widgets::explain;
use crate::widgets::figure::{ExplainedFigure, Figure};
use crate::widgets::labels;
use crate::widgets::states::{columns, columns_leading, hairline, info_card, note, page_header, section};

/// One row of the setup checklist.
struct Prerequisite {
    done: bool,
    text: &'static str,
    done_text: String,
    action: AnyElement,
}

fn checklist(app: &AtlasApp, cx: &mut Context<AtlasApp>) -> Option<AnyElement> {
    let household = app.household();
    let viewer = app.viewer();
    let has_person = !household.people.is_empty();
    let usable_account = household.accounts.iter().find(|a| {
        !a.is_company_account() && a.kind.is_cash() && a.include_in_household && household.disclosure_for(viewer, atlas_core::ids::ObjectRef::Account(a.id)) == Disclosure::Full
    });
    let restricted_accounts_exist = usable_account.is_none() && household.accounts.iter().any(|a| !a.is_company_account());
    let has_series = household.series.iter().any(|s| s.scenario.is_none());
    let saved = app.file_path().is_some() && !app.is_dirty();
    if has_person && usable_account.is_some() && has_series {
        return None;
    }
    let theme = cx.theme();
    let rows = vec![
        Prerequisite {
            done: has_person,
            text: "Add a person",
            done_text: household.people.first().map(|p| p.name.clone()).unwrap_or_default(),
            action: if has_person {
                Button::new("setup-people").xsmall().ghost().label("People").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::People, cx))).into_any_element()
            } else {
                Button::new("setup-add-person").xsmall().outline().label("Add person…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Person, window, cx))).into_any_element()
            },
        },
        Prerequisite {
            done: usable_account.is_some(),
            text: if restricted_accounts_exist { "No usable account is disclosed to this viewer" } else { "Add a personal cash account" },
            done_text: usable_account.map(|a| a.name.clone()).unwrap_or_default(),
            action: if usable_account.is_some() {
                Button::new("setup-accounts").xsmall().ghost().label("Accounts").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Accounts, cx))).into_any_element()
            } else {
                Button::new("setup-add-account").xsmall().outline().label("Add account…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Account, window, cx))).into_any_element()
            },
        },
        Prerequisite {
            done: has_series,
            text: "Add planned income or spending",
            done_text: format!("{} planned series", household.series.iter().filter(|s| s.scenario.is_none()).count()),
            action: if has_series {
                Button::new("setup-forecast").xsmall().ghost().label("View forecast").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::ForecastPath, cx))).into_any_element()
            } else {
                Button::new("setup-add-movement").xsmall().outline().label("Add movement…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Series, window, cx))).into_any_element()
            },
        },
        Prerequisite {
            done: saved,
            text: "Save this household",
            done_text: app.file_path().map(|p| p.display().to_string()).unwrap_or_default(),
            action: Button::new("setup-save").xsmall().outline().icon(IconName::Save).label(app.save_command_label()).on_click(cx.listener(|this, _, window, cx| this.save(window, cx))).into_any_element(),
        },
    ];
    let mut first_unmet = true;
    Some(
        v_flex()
            .id("setup-checklist")
            .test_support()
            .w_full()
            .gap_2()
            .p_4()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Start with your balances"))
            .children(rows.into_iter().map(|row| {
                let marker = if row.done {
                    "Done"
                } else if first_unmet {
                    first_unmet = false;
                    "Next"
                } else {
                    ""
                };
                h_flex()
                    .w_full()
                    .gap_3()
                    .items_center()
                    .child(div().w_10().flex_shrink_0().text_xs().text_color(if row.done { theme.success } else { theme.muted_foreground }).child(marker))
                    .child(div().flex_1().min_w_0().text_sm().child(if row.done { row.done_text } else { row.text.to_string() }))
                    .child(row.action)
            }))
            .child(note("Earmarks, assumptions, companies and scenarios are optional.", cx))
            .into_any_element(),
    )
}

/// The same figure under a shorter label. Six columns of one grid have room
/// for `Headroom`, not `Headroom over the floor`, and a label that wraps onto
/// a second line pushes its value out of line with the rest of the row. The
/// calculation sheet still opens under the figure's full name.
fn relabel(figure: &ExplainedFigure, label: &'static str) -> Figure {
    Figure::new(figure.id.clone(), label, &figure.calc, figure.content())
}

/// `Spendable now`: headroom clamped at zero, with the deficit stated when the
/// floor is breached.
///
/// A raw restatement rather than a derived figure — it has no chain of its
/// own, so it carries no `ⓘ`. The empty cell where the other five columns
/// carry one keeps all six values on the same line.
fn spendable_cell(overview: &HouseholdOverview, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let deficit = overview.deficit.is_positive();
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .child(
            h_flex()
                .w_full()
                .justify_between()
                .items_baseline()
                .gap_2()
                .child(div().flex_1().min_w_0().text_xs().text_color(theme.muted_foreground).child("Spendable now"))
                .child(div().flex_shrink_0().size_5()),
        )
        .child(div().id("today-spendable").test_support().font_family(theme.mono_font_family.clone()).font_weight(FontWeight::SEMIBOLD).text_xl().child(overview.spendable.format()))
        .child(div().w_full().text_xs().text_color(if deficit { theme.danger } else { theme.muted_foreground }).child(if deficit {
            format!("Shown as 0: the floor is breached by {}", overview.deficit.format())
        } else {
            "No deficit against the hard floor".to_string()
        }))
        .into_any_element()
}

/// `2 people · 1 company · 7 accounts visible` — the membership band as the
/// counts it can state on one line; the names are one click away in the
/// summaries sheet beside it.
fn membership_summary(overview: &HouseholdOverview) -> String {
    fn count(n: usize, one: &str, many: &str) -> String {
        if n == 1 { format!("1 {one}") } else { format!("{n} {many}") }
    }
    let mut text = format!(
        "{} · {} · {} visible",
        count(overview.people.len(), "person", "people"),
        count(overview.companies.len(), "company", "companies"),
        count(overview.accounts.len(), "account", "accounts")
    );
    if overview.hidden_accounts > 0 {
        text.push_str(&format!(" · {} not disclosed", overview.hidden_accounts));
    }
    text
}

pub fn render(app: &AtlasApp, overview: &HouseholdOverview, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let setup = checklist(app, cx);
    let theme = cx.theme();
    let free = overview.money.iter().find(|f| f.id.as_ref() == "free-cash");
    let liquid = overview.money.iter().find(|f| f.id.as_ref() == "liquid-cash");
    let reserved = overview.money.iter().find(|f| f.id.as_ref() == "reserved-cash");
    let assets = overview.money.iter().find(|f| f.id.as_ref() == "total-assets");
    let liabilities = overview.money.iter().find(|f| f.id.as_ref() == "liabilities");
    let net_worth = overview.money.iter().find(|f| f.id.as_ref() == "net-worth");
    let breach = overview.runway.first_breach.is_some();
    let company_only = household.accounts.iter().all(|a| a.is_company_account()) && !household.accounts.is_empty();
    let visible_assumptions = overview.assumptions.len();
    let show_all = app.today_show_all_assumptions;
    let assumption_rows: Vec<_> = overview.assumptions.iter().take(if show_all { usize::MAX } else { 4 }).collect();

    v_flex()
        .id("screen-today")
        .test_support()
        .w_full()
        .gap_8()
        // Accounts and Earmarks are where this screen can take you, not
        // commands of the current band: they belong in the page header.
        .child(page_header(
            "Today",
            Some(format!("Household · Balances as of {}", household.as_of.format("%d %b %Y")).into()),
            vec![
                Button::new("today-accounts").small().ghost().label("Accounts").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Accounts, cx))).into_any_element(),
                Button::new("today-earmarks").small().ghost().label("Earmarks").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Earmarks, cx))).into_any_element(),
            ],
            cx,
        ))
        .children(setup)
        // The current band carries no heading: free current cash is the
        // heading, at three times the size of the two figures it is made of.
        .child(
            v_flex()
                .id("today-current")
                .test_support()
                .w_full()
                .gap_6()
                .child(columns_leading(
                    0.42,
                    [free.map(|f| f.leading().into_any_element()), liquid.map(|f| f.standard().into_any_element()), reserved.map(|f| f.standard().into_any_element())].into_iter().flatten(),
                ))
                .when(company_only, |this| this.child(Alert::info("today-company-only", "Business cash is separate from household cash. Add a personal account to see household money here.")))
                .child(hairline(cx))
                // The equation, not the chain as a table: the terms of the
                // terms are what `Full calculation…` opens.
                .children(free.map(|f| explain::render_equation("today-free-equation", f.calc.node(), f.content(), cx)))
                // Six supporting figures as one grid across the full width.
                .child(columns(
                    [
                        Some(overview.hard_floor.standard().into_any_element()),
                        Some(relabel(&overview.headroom, "Headroom").into_any_element()),
                        Some(spendable_cell(overview, cx)),
                        assets.map(|f| relabel(f, "Assets").into_any_element()),
                        liabilities.map(|f| relabel(f, "Amount owed").into_any_element()),
                        net_worth.map(|f| f.standard().into_any_element()),
                    ]
                    .into_iter()
                    .flatten(),
                )),
        )
        .child(
            section("today-outlook", format!("Outlook through {}", overview.horizon.format("%d %b %Y")))
                .divider(true)
                .badge("Expected · Baseline")
                .description(format!("{} planned postings, taxes and fees included once", overview.occurrence_count))
                .action(Button::new("today-view-forecast").small().outline().icon(IconName::ChartLine).label("View forecast").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::ForecastPath, cx))))
                .child(columns([
                    relabel(&overview.conditional, "Projected cash").into_any_element(),
                    relabel(&overview.unreserved, "Less today's reserves").into_any_element(),
                    // The runway is a fact about this outlook, so it is a card
                    // and not a muted afterthought — until the floor is
                    // actually breached, which is an alert across the band and
                    // puts the injection figure in this column instead.
                    if breach {
                        relabel(&overview.injection, "Extra needed at the start").into_any_element()
                    } else {
                        info_card("today-runway", IconName::ChartLine, "No hard-floor breach", overview.runway.summary(), cx)
                    },
                ]))
                .when(breach, |this| this.child(Alert::warning("today-runway", overview.runway.summary()).title("The expected path breaches the hard floor")))
                .child(explain::render_equation("today-unreserved-equation", overview.unreserved.calc.node(), overview.unreserved.content(), cx)),
        )
        .child(
            section("today-assumptions", "Assumptions this outlook depends on")
                .action(Button::new("today-review-assumptions").small().ghost().label("Review assumptions").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Assumptions, cx))))
                .child(if overview.assumptions.is_empty() {
                    note("No assumptions recorded.", cx).into_any_element()
                } else {
                    v_flex()
                        .w_full()
                        .child(hairline(cx))
                        // Text, certainty, acceptance, freshness in columns:
                        // every cell has a definite width, which is what keeps
                        // a sentence beside a tag out of taffy's re-measuring
                        // (`docs/perf.md` §3.3). The source describes the text,
                        // so it sits under it rather than in a narrow column of
                        // its own where it would wrap to five lines.
                        .children(assumption_rows.iter().enumerate().map(|(index, assumption)| {
                            let freshness = assumption.freshness(household.as_of);
                            div().w_full().py_3().border_b_1().border_color(theme.border).child(columns_leading(
                                0.47,
                                [
                                    v_flex()
                                        .w_full()
                                        .gap_1()
                                        .child(div().w_full().text_sm().child(format!("{}. {}", index + 1, assumption.text)))
                                        .child(div().w_full().text_xs().text_color(theme.muted_foreground).child(assumption.source.describe()))
                                        .into_any_element(),
                                    h_flex().w_full().items_center().child(labels::certainty_tag(assumption.certainty)).into_any_element(),
                                    div()
                                        .w_full()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child(match assumption.accepted_on {
                                            Some(date) => format!("Accepted {}", date.format("%d %b %Y")),
                                            None => "Not accepted".to_string(),
                                        })
                                        .into_any_element(),
                                    h_flex().w_full().items_center().justify_end().child(labels::freshness_tag(freshness)).into_any_element(),
                                ],
                            ))
                        }))
                        .when(visible_assumptions > 4, |this| {
                            this.child(
                                h_flex().w_full().pt_2().child(
                                    Button::new("today-show-all-assumptions").xsmall().ghost().label(if show_all { "Show fewer".to_string() } else { format!("Show all {visible_assumptions}") }).on_click(cx.listener(|this, _, _, cx| {
                                        this.today_show_all_assumptions = !this.today_show_all_assumptions;
                                        cx.notify();
                                    })),
                                ),
                            )
                        })
                        .into_any_element()
                }),
        )
        // One row: what the household is made of, and the sheet that reads it
        // out in full. The three registers stay reachable from here because
        // they are this band's own commands, not the launcher's.
        .child(
            section("today-members", "Household members and accounts")
                .badge(membership_summary(overview))
                .action(Button::new("today-people").xsmall().ghost().label("People").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::People, cx))))
                .action(Button::new("today-companies").xsmall().ghost().label("Companies").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Companies, cx))))
                .action(Button::new("today-accounts-2").xsmall().ghost().label("Accounts").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Accounts, cx))))
                .action(Button::new("today-all-summaries").small().ghost().label("All summaries…").on_click(cx.listener(|this, _, window, cx| this.open_household_summaries(window, cx)))),
        )
        .into_any_element()
}

impl AtlasApp {
    /// The read-only sheet with the three fully projected summary tables.
    pub fn open_household_summaries(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(overview) = self.overview().cloned() else { return };
        let household = self.household.clone();
        let overview = std::sync::Arc::new(overview);
        window.open_sheet(cx, move |sheet, _, cx| {
            let overview = overview.clone();
            sheet.title("Household summaries").size(relative(0.6)).child(
                v_flex()
                    .py_4()
                    .gap_6()
                    .child(crate::models::household::render_people(&overview, cx))
                    .child(crate::models::household::render_companies(&overview, cx))
                    .child(crate::models::household::render_accounts(&overview, &household, cx)),
            )
        });
    }
}
