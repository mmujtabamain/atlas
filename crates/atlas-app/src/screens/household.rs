//! The Household overview (§5–§7, §2.1): money definitions with their chains,
//! the conditional projection and its assumptions, and the people, companies
//! and accounts of the boundary — each as the viewer is authorized to see it.

use atlas_core::authz::Viewer;
use atlas_core::forecast::{Case, ForecastOptions, forecast};
use atlas_core::liquidity::Boundary;
use atlas_core::provenance::ProvNode;
use atlas_core::vocab::{MoneyClass, ResultStrength};
use atlas_core::ids::ObjectRef;
use atlas_core::liquidity::{account_liquidity, household_liquidity};
use atlas_core::model::{Assumption, Household};
use atlas_core::{Calc, Disclosure, EngineResult, Money};
use chrono::NaiveDate;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, group_box::GroupBox, h_flex,
    table::{Table, TableBody, TableCell, TableHead, TableHeader, TableRow},
    tag::Tag, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::widgets::explain;
use crate::widgets::figure::{ExplainedFigure, card};
use crate::widgets::labels;
use crate::widgets::table::{money_cell, muted_cell};

/// Everything the overview shows, computed once per state change.
#[derive(Clone, Debug)]
pub struct HouseholdOverview {
    pub viewer_name: String,
    pub horizon: NaiveDate,
    pub money: Vec<ExplainedFigure>,
    pub conditional: ExplainedFigure,
    pub unreserved: ExplainedFigure,
    pub assumptions: Vec<Assumption>,
    pub occurrence_count: usize,
}

impl HouseholdOverview {
    pub fn compute(household: &Household, viewer: Viewer, horizon: NaiveDate) -> EngineResult<Self> {
        log::info!(
            "computing household overview: viewer={} horizon={} accounts={} series={}",
            viewer.person,
            horizon,
            household.accounts.len(),
            household.series.len()
        );
        let liquidity = household_liquidity(household)?;
        // The same chronological forecast the Projections screen runs (expected
        // case, baseline), so every screen quotes one number.
        let projection = forecast(household, Boundary::Household, ForecastOptions { through: horizon, scenario: None, case: Case::Expected })?;
        let unreserved_total = projection.end.money().checked_sub(liquidity.reserved.money())?;
        let unreserved = Calc::new(
            unreserved_total,
            ProvNode::sum(
                "Conditional projected unreserved cash",
                unreserved_total,
                vec![projection.end.node().clone(), liquidity.reserved.node().clone().minus()],
            )
            .money_class(MoneyClass::ConditionalFuture)
            .strength(ResultStrength::ScenarioTested)
            .note("An earmark reduces unreserved cash, not the bank balance; paying the obligation later reduces cash and releases the earmark (§2.1)."),
        );
        let viewer_name = household.entity_name(atlas_core::ids::EntityRef::Person(viewer.person));
        let figure = |id, label, calc: &Calc<Money>| ExplainedFigure::new(id, label, calc, household, viewer);
        let assumptions = projection
            .assumptions
            .iter()
            .filter(|a| a.private_to.is_none_or(|owner| owner == viewer.person))
            .cloned()
            .collect();
        Ok(HouseholdOverview {
            viewer_name,
            horizon,
            money: vec![
                figure("liquid-cash", "Liquid cash", &liquidity.liquid_cash),
                figure("reserved-cash", "Reserved cash", &liquidity.reserved),
                figure("free-cash", "Free current cash", &liquidity.free),
                figure("total-assets", "Total assets", &liquidity.total_assets),
                figure("liabilities", "Liabilities", &liquidity.liabilities),
                figure("net-worth", "Net worth", &liquidity.net_worth),
            ],
            conditional: figure("conditional-cash", "Conditional projected cash", &projection.end),
            unreserved: figure("unreserved-cash", "Conditional projected unreserved cash", &unreserved),
            assumptions,
            occurrence_count: projection.accounts.iter().map(|a| a.postings.len()).sum(),
        })
    }
}

pub fn render(overview: &HouseholdOverview, household: &Household, viewer: Viewer, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let viewer_name = overview.viewer_name.as_str();

    v_flex()
        .id("screen-household")
        .test_support()
        .w_full()
        .gap_6()
        .child(
            v_flex()
                .gap_1()
                .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child("Household"))
                .child(div().text_sm().text_color(theme.muted_foreground).child(format!(
                    "{} · balances reconciled {} · {} people · {} companies · {} accounts · viewing as {}",
                    household.name,
                    household.as_of.format("%d %b %Y"),
                    household.people.len(),
                    household.companies.len(),
                    household.accounts.len(),
                    viewer_name
                ))),
        )
        .child(
            GroupBox::new().id("money-definitions").title("Money definitions (§6)").child(
                v_flex()
                    .gap_4()
                    .child(div().text_xs().text_color(theme.muted_foreground).child(
                        "One bank balance is never one number: settled cash, earmarked cash and free cash are kept apart, and every figure opens its chain.",
                    ))
                    .child(h_flex().flex_wrap().gap_8().children(overview.money.iter().map(|f| card(f.figure(f.id.as_ref() == "free-cash"))))),
            ),
        )
        .child(
            GroupBox::new()
                .id("conditional-projection")
                .title(format!("Conditional projection through {} (§2.1)", overview.horizon.format("%d %b %Y")))
                .child(
                    h_flex()
                        .w_full()
                        .items_start()
                        .gap_8()
                        .child(
                            v_flex()
                                .gap_6()
                                .w_80()
                                .flex_shrink_0()
                                .child(overview.conditional.figure(true))
                                .child(overview.unreserved.figure(false))
                                .child(div().text_xs().text_color(theme.muted_foreground).child(format!(
                                    "{} postings entered the chain (expected case, baseline, taxes included once). Only the first term is money already received; the rest is conditional on the assumptions below (§2.4).",
                                    overview.occurrence_count
                                ))),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap_2()
                                .child(div().text_xs().text_color(theme.muted_foreground).child("Chain as the plan lays it out — nested terms open with “Why?”"))
                                .child(explain::render_top_block(overview.unreserved.calc.node(), cx)),
                        ),
                ),
        )
        .child(
            GroupBox::new()
                .id("assumptions")
                .title("Assumptions this projection depends on (§10.2)")
                // Plain full-width rows: a sentence in a flex_1/min_w_0 cell of a
                // wrap row costs taffy ~1,500 measure callbacks per assumption per
                // frame; a column with the tag on its own line costs ~100.
                .child(v_flex().w_full().gap_2().children(overview.assumptions.iter().enumerate().map(|(index, assumption)| {
                    v_flex()
                        .w_full()
                        .gap_1()
                        .child(div().w_full().child(format!("{}. {}", index + 1, assumption.text)))
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(labels::certainty_tag(assumption.certainty))
                                .child(div().text_xs().text_color(theme.muted_foreground).child(match assumption.accepted_on {
                                    Some(date) => format!("{} · accepted {}", assumption.source.describe(), date.format("%d %b %Y")),
                                    None => format!("{} · not yet accepted", assumption.source.describe()),
                                })),
                        )
                }))),
        )
        .child(render_people(household, cx))
        .child(render_companies(household, viewer, cx))
        .child(render_accounts(household, viewer, cx))
}

fn render_people(household: &Household, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    GroupBox::new().id("people").title("People (§5.2)").child(
        Table::new()
            .child(
                TableHeader::new().child(
                    TableRow::new()
                        .child(TableHead::new().w_48().flex_shrink_0().child("Person"))
                        .child(TableHead::new().w_40().flex_shrink_0().child("Household role"))
                        .child(TableHead::new().min_w_0().child("Accounts held (share)"))
                        .child(TableHead::new().w_56().flex_shrink_0().child("Companies owned")),
                ),
            )
            .child(TableBody::new().children(household.people.iter().enumerate().map(|(index, person)| {
                let accounts: Vec<String> = household
                    .accounts_of(person.id)
                    .map(|a| format!("{} ({}%)", a.name, a.holder.share_of(person.id) / 100))
                    .collect();
                let companies: Vec<String> = household
                    .companies_of(person.id)
                    .map(|c| {
                        let share = c.owners.iter().filter(|o| o.person == person.id).map(|o| o.basis_points).sum::<u32>() / 100;
                        format!("{} ({share}%)", c.name)
                    })
                    .collect();
                TableRow::new()
                    .when(index % 2 == 1, |row| row.bg(theme.table_even))
                    .child(TableCell::new().w_48().flex_shrink_0().overflow_hidden().text_ellipsis().child(person.name.clone()))
                    .child(TableCell::new().w_40().flex_shrink_0().overflow_hidden().text_ellipsis().child(Tag::secondary().xsmall().outline().child(person.role.label())))
                    .child(muted_cell(if accounts.is_empty() { "—".to_string() } else { accounts.join(" · ") }, cx).min_w_0().overflow_hidden().text_ellipsis())
                    .child(muted_cell(if companies.is_empty() { "—".to_string() } else { companies.join(" · ") }, cx).w_56().flex_shrink_0().overflow_hidden().text_ellipsis())
            }))),
    )
}

fn render_companies(household: &Household, viewer: Viewer, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    GroupBox::new().id("companies").title("Companies (§5.3, §8)").child(
        v_flex()
            .gap_3()
            .child(div().text_xs().text_color(theme.muted_foreground).child(
                "A company is legally distinct from its owners. Its cash is shown here as business cash and never enters household free cash (§8.5).",
            ))
            .child(
                Table::new()
                    .child(
                        TableHeader::new().child(
                            TableRow::new()
                                .child(TableHead::new().w_48().flex_shrink_0().child("Company"))
                                .child(TableHead::new().w_40().flex_shrink_0().child("Owners"))
                                .child(TableHead::new().w_24().flex_shrink_0().text_right().child("Employees"))
                                .child(TableHead::new().min_w_0().child("Constraints (§8.6)"))
                                .child(TableHead::new().w_40().flex_shrink_0().text_right().child("Business cash")),
                        ),
                    )
                    .child(TableBody::new().children(household.companies.iter().enumerate().map(|(index, company)| {
                        let owners: Vec<String> = company
                            .owners
                            .iter()
                            .map(|o| format!("{} {}%", household.entity_name(atlas_core::ids::EntityRef::Person(o.person)), o.basis_points / 100))
                            .collect();
                        let cash = Money::sum(household.base_currency, household.company_accounts(company.id).map(|a| a.settled_balance))
                            .unwrap_or(Money::zero(household.base_currency));
                        let disclosure = household.disclosure_for(viewer, ObjectRef::Company(company.id));
                        let cash_cell = match disclosure {
                            Disclosure::Full | Disclosure::SelectedFields | Disclosure::BalanceOnly => {
                                money_cell(cash, cx).w_40().flex_shrink_0().text_color(theme.muted_foreground)
                            }
                            _ => muted_cell("not disclosed (§8.7)", cx).w_40().flex_shrink_0().overflow_hidden().text_ellipsis().text_right(),
                        };
                        TableRow::new()
                            .when(index % 2 == 1, |row| row.bg(theme.table_even))
                            .child(TableCell::new().w_48().flex_shrink_0().overflow_hidden().text_ellipsis().child(company.name.clone()))
                            .child(muted_cell(owners.join(" · "), cx).w_40().flex_shrink_0().overflow_hidden().text_ellipsis())
                            .child(TableCell::new().w_24().flex_shrink_0().overflow_hidden().text_ellipsis().text_right().child(company.employees.len().to_string()))
                            .child(muted_cell(company.constraints.iter().map(|c| c.describe()).collect::<Vec<_>>().join(" · "), cx).min_w_0().overflow_hidden().text_ellipsis())
                            .child(cash_cell)
                    }))),
            ),
    )
}

fn render_accounts(household: &Household, viewer: Viewer, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let visible: Vec<_> = household
        .accounts
        .iter()
        .filter(|a| !matches!(household.disclosure_for(viewer, ObjectRef::Account(a.id)), Disclosure::Hidden | Disclosure::Aggregate))
        .collect();
    let hidden_count = household.accounts.len() - visible.len();
    GroupBox::new().id("accounts").title("Accounts (§7)").child(
        v_flex()
            .gap_3()
            .child(div().text_xs().text_color(theme.muted_foreground).child(format!(
                "{} of {} accounts are visible to {} under the current access policies; the others may still contribute to household figures as authorized aggregates (§7.2).",
                visible.len(),
                household.accounts.len(),
                household.entity_name(atlas_core::ids::EntityRef::Person(viewer.person))
            )))
            .child(
                Table::new()
                    .child(
                        TableHeader::new().child(
                            TableRow::new()
                                .child(TableHead::new().w_56().flex_shrink_0().child("Account"))
                                .child(TableHead::new().w_32().flex_shrink_0().child("Kind"))
                                .child(TableHead::new().w_48().flex_shrink_0().child("Holder (shares)"))
                                .child(TableHead::new().w_40().flex_shrink_0().child("Liquidity"))
                                .child(TableHead::new().w_32().flex_shrink_0().text_right().child("Settled"))
                                .child(TableHead::new().w_32().flex_shrink_0().text_right().child("Reserved"))
                                .child(TableHead::new().w_32().flex_shrink_0().text_right().child("Free"))
                                .child(TableHead::new().w_32().flex_shrink_0().child("Visibility"))
                                .child(TableHead::new().min_w_0().child("Calculation access")),
                        ),
                    )
                    .child(TableBody::new().children(visible.iter().enumerate().map(|(index, account)| {
                        let disclosure = household.disclosure_for(viewer, ObjectRef::Account(account.id));
                        let policy = household.policy_for(ObjectRef::Account(account.id));
                        let liquidity = account_liquidity(household, account.id).ok();
                        let derived_visible = matches!(disclosure, Disclosure::Full | Disclosure::SelectedFields);
                        let reserved_cell = match (&liquidity, derived_visible) {
                            (Some(l), true) => money_cell(l.reserved.money(), cx).w_32().flex_shrink_0(),
                            _ => muted_cell("not disclosed", cx).w_32().flex_shrink_0().overflow_hidden().text_ellipsis().text_right(),
                        };
                        let free_cell = match (&liquidity, derived_visible) {
                            (Some(l), true) => money_cell(l.free.money(), cx).w_32().flex_shrink_0(),
                            _ => muted_cell("not disclosed", cx).w_32().flex_shrink_0().overflow_hidden().text_ellipsis().text_right(),
                        };
                        TableRow::new()
                            .when(index % 2 == 1, |row| row.bg(theme.table_even))
                            .child(
                                TableCell::new().w_56().flex_shrink_0().overflow_hidden().text_ellipsis().child(
                                    v_flex()
                                        .child(account.name.clone())
                                        .child(div().text_xs().text_color(theme.muted_foreground).child(account.institution.clone())),
                                ),
                            )
                            .child(muted_cell(account.kind.label(), cx).w_32().flex_shrink_0().overflow_hidden().text_ellipsis())
                            .child(muted_cell(household.holder_description(account), cx).w_48().flex_shrink_0().overflow_hidden().text_ellipsis())
                            .child(muted_cell(account.liquidity.describe(), cx).w_40().flex_shrink_0().overflow_hidden().text_ellipsis())
                            .child(money_cell(account.settled_balance, cx).w_32().flex_shrink_0())
                            .child(reserved_cell)
                            .child(free_cell)
                            .child(TableCell::new().w_32().flex_shrink_0().overflow_hidden().text_ellipsis().child(labels::disclosure_tag(disclosure)))
                            .child(muted_cell(policy.map(|p| p.calculation_access.label()).unwrap_or("no policy — excluded (F162)"), cx).min_w_0().overflow_hidden().text_ellipsis())
                    }))),
            )
            .when(hidden_count > 0, |this| {
                this.child(div().text_xs().text_color(theme.muted_foreground).child(format!(
                    "{hidden_count} account(s) not listed: their existence is not disclosed to this viewer (V062)."
                )))
            }),
    )
}
