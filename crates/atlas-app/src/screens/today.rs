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
use crate::widgets::figure::card;
use crate::widgets::labels;
use crate::widgets::states::{fact, lanes, note, page_header, section};

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
        .child(page_header("Today", Some(format!("Balances as of {}", household.as_of.format("%d %b %Y")).into()), vec![], cx))
        .children(setup)
        .child(
            section("today-current", "Free now")
                .action(Button::new("today-accounts").small().ghost().label("Accounts").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Accounts, cx))))
                .action(Button::new("today-earmarks").small().ghost().label("Earmarks").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Earmarks, cx))))
                .child(
                    h_flex()
                        .w_full()
                        .items_start()
                        .gap_8()
                        .child(div().w_80().flex_shrink_0().children(free.map(|f| f.leading())))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap_2()
                                .child(lanes([liquid.map(|f| card(f.standard()).into_any_element()), reserved.map(|f| card(f.standard()).into_any_element())].into_iter().flatten()))
                                .children(free.map(|f| explain::render_preview(f.calc.node(), f.content(), cx))),
                        ),
                )
                .when(company_only, |this| this.child(Alert::info("today-company-only", "Business cash is separate from household cash. Add a personal account to see household money here.")))
                .child(lanes([
                    card(overview.hard_floor.standard()).into_any_element(),
                    card(overview.headroom.standard()).into_any_element(),
                    card(
                        v_flex()
                            .gap_1()
                            .child(div().text_xs().text_color(theme.muted_foreground).child("Spendable now"))
                            .child(div().id("today-spendable").test_support().text_xl().font_weight(FontWeight::SEMIBOLD).font_family(theme.mono_font_family.clone()).child(overview.spendable.format()))
                            .child(div().text_xs().text_color(if overview.deficit.is_positive() { theme.danger } else { theme.muted_foreground }).child(if overview.deficit.is_positive() {
                                format!("Shown as 0: the floor is breached by {}", overview.deficit.format())
                            } else {
                                "No deficit against the hard floor".to_string()
                            })),
                    )
                    .into_any_element(),
                ]))
                .child(lanes([assets.map(|f| card(f.standard()).into_any_element()), liabilities.map(|f| card(f.compact_labelled("Amount owed").variant(crate::widgets::figure::Variant::Standard)).into_any_element()), net_worth.map(|f| card(f.standard()).into_any_element())].into_iter().flatten())),
        )
        .child(
            section("today-outlook", format!("Outlook through {}", overview.horizon.format("%d %b %Y")))
                .description(format!("Expected case · Baseline · {} planned postings, taxes and fees included once", overview.occurrence_count))
                .action(Button::new("today-view-forecast").small().outline().icon(IconName::ChartLine).label("View forecast").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::ForecastPath, cx))))
                .child(
                    h_flex()
                        .w_full()
                        .items_start()
                        .gap_8()
                        .child(v_flex().w_80().flex_shrink_0().gap_6().child(overview.conditional.leading()).child(overview.unreserved.standard()))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap_2()
                                .child(note("Projected cash less today's reserves = projected unreserved cash. Only the starting cash is money already received; everything after it depends on the assumptions below.", cx))
                                .child(explain::render_preview(overview.unreserved.calc.node(), overview.unreserved.content(), cx)),
                        ),
                )
                .child(if breach {
                    Alert::warning("today-runway", overview.runway.summary()).title("The expected path breaches the hard floor").into_any_element()
                } else {
                    div().id("today-runway").test_support().text_sm().child(overview.runway.summary()).into_any_element()
                })
                .when(breach, |this| this.child(lanes([card(overview.injection.standard()).into_any_element()]))),
        )
        .child(
            section("today-assumptions", "Assumptions this outlook depends on")
                .action(Button::new("today-review-assumptions").small().ghost().label("Review assumptions").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Assumptions, cx))))
                .child(if overview.assumptions.is_empty() {
                    note("No assumptions recorded.", cx).into_any_element()
                } else {
                    v_flex()
                        .w_full()
                        .gap_2()
                        .children(assumption_rows.iter().enumerate().map(|(index, assumption)| {
                            let freshness = assumption.freshness(household.as_of);
                            v_flex()
                                .w_full()
                                .gap_1()
                                .child(div().w_full().text_sm().child(format!("{}. {}", index + 1, assumption.text)))
                                .child(
                                    h_flex()
                                        .gap_2()
                                        .items_center()
                                        .child(labels::certainty_tag(assumption.certainty))
                                        .child(labels::freshness_tag(freshness))
                                        .child(div().text_xs().text_color(theme.muted_foreground).child(match assumption.accepted_on {
                                            Some(date) => format!("{} · accepted {}", assumption.source.describe(), date.format("%d %b %Y")),
                                            None => format!("{} · not accepted", assumption.source.describe()),
                                        })),
                                )
                        }))
                        .when(visible_assumptions > 4, |this| {
                            this.child(Button::new("today-show-all-assumptions").xsmall().ghost().label(if show_all { "Show fewer".to_string() } else { format!("Show all {visible_assumptions}") }).on_click(cx.listener(|this, _, _, cx| {
                                this.today_show_all_assumptions = !this.today_show_all_assumptions;
                                cx.notify();
                            })))
                        })
                        .into_any_element()
                }),
        )
        .child(
            section("today-members", "Household members and accounts")
                .action(Button::new("today-all-summaries").small().ghost().label("All summaries…").on_click(cx.listener(|this, _, window, cx| this.open_household_summaries(window, cx))))
                .child(lanes([
                    fact("People", overview.people.iter().map(|p| format!("{} — {}", p.name, p.role)).collect::<Vec<_>>().join(" · "), cx).into_any_element(),
                    fact("Companies", if overview.companies.is_empty() { "None".to_string() } else { overview.companies.iter().map(|c| c.name.to_string()).collect::<Vec<_>>().join(" · ") }, cx).into_any_element(),
                    fact(
                        "Accounts",
                        if overview.hidden_accounts == 0 { format!("{} accounts", overview.accounts.len()) } else { format!("{} of {} accounts · {} not disclosed", overview.accounts.len(), household.accounts.len(), overview.hidden_accounts) },
                        cx,
                    )
                    .into_any_element(),
                ]))
                .child(
                    h_flex()
                        .gap_2()
                        .child(Button::new("today-people").xsmall().ghost().label("People").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::People, cx))))
                        .child(Button::new("today-companies").xsmall().ghost().label("Companies").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Companies, cx))))
                        .child(Button::new("today-accounts-2").xsmall().ghost().label("Accounts").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Accounts, cx)))),
                ),
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
