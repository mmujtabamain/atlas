//! The Household overview: money definitions with their chains, the
//! conditional projection and its assumptions, and the people, companies and
//! accounts of the household — each as the viewer is authorized to see it.

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
    ActiveTheme as _, Sizable as _, group_box::GroupBox,
    table::{Table, TableBody, TableCell, TableHead, TableHeader, TableRow},
    tag::Tag, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::widgets::figure::ExplainedFigure;
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
    /// The three tables at the bottom, as the viewer may see them — every
    /// string, disclosure and per-account liquidity figure resolved here,
    /// once, rather than by the render on every frame.
    pub people: Vec<PersonRow>,
    pub companies: Vec<CompanyRow>,
    pub accounts: Vec<AccountRow>,
    /// Accounts whose existence is not disclosed to the viewer.
    pub hidden_accounts: usize,
    /// Hard earmarks and bank minimums of the household today.
    pub hard_floor: ExplainedFigure,
    /// Liquid cash minus the floor, signed.
    pub headroom: ExplainedFigure,
    /// Headroom clamped at zero, and the deficit when it is negative.
    pub spendable: Money,
    pub deficit: Money,
    /// The expected baseline path against the hard floor.
    pub runway: atlas_core::breach::BreachReport,
    /// The smallest addition at the start that keeps the path above the floor.
    pub injection: ExplainedFigure,
}

/// One row of the People table.
#[derive(Clone, Debug)]
pub struct PersonRow {
    pub name: SharedString,
    pub role: SharedString,
    pub accounts: SharedString,
    pub companies: SharedString,
}

/// One row of the Companies table; `cash` is `None` when not disclosed.
#[derive(Clone, Debug)]
pub struct CompanyRow {
    pub name: SharedString,
    pub owners: SharedString,
    pub employees: SharedString,
    pub constraints: SharedString,
    pub cash: Option<Money>,
}

/// One row of the Accounts table; the derived figures are `None` when the
/// viewer only sees the balance.
#[derive(Clone, Debug)]
pub struct AccountRow {
    pub name: SharedString,
    pub institution: SharedString,
    pub kind: SharedString,
    pub holder: SharedString,
    pub liquidity: SharedString,
    pub settled: Money,
    pub reserved: Option<Money>,
    pub free: Option<Money>,
    pub disclosure: Disclosure,
    pub access: SharedString,
}

fn person_rows(household: &Household) -> Vec<PersonRow> {
    household
        .people
        .iter()
        .map(|person| {
            let accounts: Vec<String> = household.accounts_of(person.id).map(|a| format!("{} ({}%)", a.name, a.holder.share_of(person.id) / 100)).collect();
            let companies: Vec<String> = household
                .companies_of(person.id)
                .map(|c| {
                    let share = c.owners.iter().filter(|o| o.person == person.id).map(|o| o.basis_points).sum::<u32>() / 100;
                    format!("{} ({share}%)", c.name)
                })
                .collect();
            PersonRow {
                name: person.name.clone().into(),
                role: person.role.label().into(),
                accounts: if accounts.is_empty() { "—".into() } else { accounts.join(" · ").into() },
                companies: if companies.is_empty() { "—".into() } else { companies.join(" · ").into() },
            }
        })
        .collect()
}

fn company_rows(household: &Household, viewer: Viewer) -> Vec<CompanyRow> {
    household
        .companies
        .iter()
        .map(|company| {
            let owners: Vec<String> = company
                .owners
                .iter()
                .map(|o| format!("{} {}%", household.entity_name(atlas_core::ids::EntityRef::Person(o.person)), o.basis_points / 100))
                .collect();
            let cash = Money::sum(household.base_currency, household.company_accounts(company.id).map(|a| a.settled_balance)).unwrap_or(Money::zero(household.base_currency));
            let disclosed = matches!(household.disclosure_for(viewer, ObjectRef::Company(company.id)), Disclosure::Full | Disclosure::SelectedFields | Disclosure::BalanceOnly);
            CompanyRow {
                name: company.name.clone().into(),
                owners: owners.join(" · ").into(),
                employees: company.employees.len().to_string().into(),
                constraints: company.constraints.iter().map(|c| c.describe()).collect::<Vec<_>>().join(" · ").into(),
                cash: disclosed.then_some(cash),
            }
        })
        .collect()
}

fn account_rows(household: &Household, viewer: Viewer) -> (Vec<AccountRow>, usize) {
    let mut hidden = 0;
    let rows = household
        .accounts
        .iter()
        .filter_map(|account| {
            let disclosure = household.disclosure_for(viewer, ObjectRef::Account(account.id));
            if matches!(disclosure, Disclosure::Hidden | Disclosure::Aggregate) {
                hidden += 1;
                return None;
            }
            let liquidity = account_liquidity(household, account.id).ok();
            let derived_visible = matches!(disclosure, Disclosure::Full | Disclosure::SelectedFields);
            let derived = liquidity.filter(|_| derived_visible);
            Some(AccountRow {
                name: account.name.clone().into(),
                institution: account.institution.clone().into(),
                kind: account.kind.label().into(),
                holder: household.holder_description(account).into(),
                liquidity: account.liquidity.describe().into(),
                settled: account.settled_balance,
                reserved: derived.as_ref().map(|l| l.reserved.money()),
                free: derived.as_ref().map(|l| l.free.money()),
                disclosure,
                access: household.policy_for(ObjectRef::Account(account.id)).map(|p| p.calculation_access.label()).unwrap_or("no policy — excluded").into(),
            })
        })
        .collect();
    (rows, hidden)
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
            .note("An earmark reduces unreserved cash, not the bank balance; paying the obligation later reduces cash and releases the earmark."),
        );
        let viewer_name = household.entity_name(atlas_core::ids::EntityRef::Person(viewer.person));
        let figure = |id, label, calc: &Calc<Money>| ExplainedFigure::new(id, label, calc, household, viewer);
        let assumptions = projection
            .assumptions
            .iter()
            .filter(|a| a.private_to.is_none_or(|owner| owner == viewer.person))
            .cloned()
            .collect();
        let (accounts, hidden_accounts) = account_rows(household, viewer);
        let boundary = atlas_core::liquidity::boundary_liquidity(household, Boundary::Household)?;
        let hard_floor = ExplainedFigure::new("hard-floor", "Hard floor", &boundary.hard_floor, household, viewer);
        let headroom = ExplainedFigure::new("headroom", "Headroom over the floor", &boundary.headroom, household, viewer);
        let spendable = boundary.headroom.money().clamped_at_zero();
        let deficit = boundary.headroom.money().negated().clamped_at_zero();
        let runway = atlas_core::breach::analyse(&projection.path, boundary.hard_floor.money(), horizon)?;
        let injection = ExplainedFigure::new("injection", "Extra cash needed at the start to never breach", &runway.minimum_injection, household, viewer);
        Ok(HouseholdOverview {
            hard_floor,
            headroom,
            spendable,
            deficit,
            runway,
            injection,
            people: person_rows(household),
            companies: company_rows(household, viewer),
            accounts,
            hidden_accounts,
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

pub(crate) fn render_people(overview: &HouseholdOverview, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    GroupBox::new().id("people").title("People").child(
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
            .child(TableBody::new().children(overview.people.iter().enumerate().map(|(index, person)| {
                TableRow::new()
                    .when(index % 2 == 1, |row| row.bg(theme.table_even))
                    .child(TableCell::new().w_48().flex_shrink_0().overflow_hidden().text_ellipsis().child(person.name.clone()))
                    .child(TableCell::new().w_40().flex_shrink_0().overflow_hidden().text_ellipsis().child(Tag::secondary().xsmall().outline().child(person.role.clone())))
                    .child(muted_cell(person.accounts.clone(), cx).min_w_0().overflow_hidden().text_ellipsis())
                    .child(muted_cell(person.companies.clone(), cx).w_56().flex_shrink_0().overflow_hidden().text_ellipsis())
            }))),
    )
}

pub(crate) fn render_companies(overview: &HouseholdOverview, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    GroupBox::new().id("companies").title("Companies").child(
        v_flex()
            .gap_3()
            .child(div().text_xs().text_color(theme.muted_foreground).child(
                "A company is legally distinct from its owners. Its cash is business cash and never counts as household free cash.",
            ))
            .child(
                Table::new()
                    .child(
                        TableHeader::new().child(
                            TableRow::new()
                                .child(TableHead::new().w_48().flex_shrink_0().child("Company"))
                                .child(TableHead::new().w_40().flex_shrink_0().child("Owners"))
                                .child(TableHead::new().w_24().flex_shrink_0().text_right().child("Employees"))
                                .child(TableHead::new().min_w_0().child("Constraints"))
                                .child(TableHead::new().w_40().flex_shrink_0().text_right().child("Business cash")),
                        ),
                    )
                    .child(TableBody::new().children(overview.companies.iter().enumerate().map(|(index, company)| {
                        let cash_cell = match company.cash {
                            Some(cash) => money_cell(cash, cx).w_40().flex_shrink_0().text_color(theme.muted_foreground),
                            None => muted_cell("not disclosed", cx).w_40().flex_shrink_0().overflow_hidden().text_ellipsis().text_right(),
                        };
                        TableRow::new()
                            .when(index % 2 == 1, |row| row.bg(theme.table_even))
                            .child(TableCell::new().w_48().flex_shrink_0().overflow_hidden().text_ellipsis().child(company.name.clone()))
                            .child(muted_cell(company.owners.clone(), cx).w_40().flex_shrink_0().overflow_hidden().text_ellipsis())
                            .child(TableCell::new().w_24().flex_shrink_0().overflow_hidden().text_ellipsis().text_right().child(company.employees.clone()))
                            .child(muted_cell(company.constraints.clone(), cx).min_w_0().overflow_hidden().text_ellipsis())
                            .child(cash_cell)
                    }))),
            ),
    )
}

pub(crate) fn render_accounts(overview: &HouseholdOverview, household: &Household, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let visible = overview.accounts.len();
    let hidden_count = overview.hidden_accounts;
    GroupBox::new().id("accounts").title("Accounts").child(
        v_flex()
            .gap_3()
            .child(div().text_xs().text_color(theme.muted_foreground).child(if hidden_count == 0 {
                format!("All {} accounts are visible to {}.", visible, overview.viewer_name)
            } else {
                format!(
                    "{} of {} accounts are visible to {} under the current access policies; the others can still contribute to household figures as authorized totals.",
                    visible,
                    household.accounts.len(),
                    overview.viewer_name
                )
            }))
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
                    .child(TableBody::new().children(overview.accounts.iter().enumerate().map(|(index, account)| {
                        let derived = |money: Option<Money>| match money {
                            Some(money) => money_cell(money, cx).w_32().flex_shrink_0(),
                            None => muted_cell("not disclosed", cx).w_32().flex_shrink_0().overflow_hidden().text_ellipsis().text_right(),
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
                            .child(muted_cell(account.kind.clone(), cx).w_32().flex_shrink_0().overflow_hidden().text_ellipsis())
                            .child(muted_cell(account.holder.clone(), cx).w_48().flex_shrink_0().overflow_hidden().text_ellipsis())
                            .child(muted_cell(account.liquidity.clone(), cx).w_40().flex_shrink_0().overflow_hidden().text_ellipsis())
                            .child(money_cell(account.settled, cx).w_32().flex_shrink_0())
                            .child(derived(account.reserved))
                            .child(derived(account.free))
                            .child(TableCell::new().w_32().flex_shrink_0().overflow_hidden().text_ellipsis().child(labels::disclosure_tag(account.disclosure)))
                            .child(muted_cell(account.access.clone(), cx).min_w_0().overflow_hidden().text_ellipsis())
                    }))),
            )
            .when(hidden_count > 0, |this| {
                this.child(div().text_xs().text_color(theme.muted_foreground).child(format!(
                    "{hidden_count} account(s) not listed: their existence is not disclosed to this viewer."
                )))
            }),
    )
}
