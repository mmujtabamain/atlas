//! People & companies / Companies: business entities with their planning-safe
//! cash limits, always separate from household cash; the company detail in
//! its full form or the deliberately reduced summary.

use atlas_core::ids::{CompanyId, EntityRef, ObjectRef};
use atlas_core::liquidity::Boundary;
use atlas_core::model::Household;
use atlas_core::Disclosure;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _,
    button::{Button, ButtonVariants as _, DropdownButton},
    h_flex,
    menu::PopupMenuItem,
    tag::Tag,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::common::{detail_header, workspace_header};
use crate::app::AtlasApp;
use crate::entry::Entry;
use crate::models::entities::EntityModels;
use crate::nav::{Destination, Route};
use crate::widgets::figure::card;
use crate::widgets::labels;
use crate::widgets::record::{self, Lane};
use crate::widgets::states::{about_access_button, count_line, empty_state, fact, lanes, none_disclosed, note, section};

const LANES: [(&str, Lane); 4] = [("Company / jurisdiction", Lane::fixed(300.)), ("Disclosure", Lane::fixed(150.)), ("Business cash", Lane::money(180.)), ("Cash ceiling before extraction costs", Lane::flex())];

pub fn render_list(app: &AtlasApp, models: &EntityModels, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let header = workspace_header(
        Destination::Household,
        Route::Companies,
        vec![Button::new("new-company").small().outline().icon(IconName::Plus).label("Add company…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Company, window, cx))).into_any_element()],
        cx,
    );
    let selected = app.selected_company.filter(|id| models.company(*id).is_some());
    let rows: Vec<_> = models
        .companies
        .iter()
        .filter_map(|model| {
            let company = household.company(model.id)?;
            let id = model.id;
            let full = matches!(model.disclosure, Disclosure::Full | Disclosure::SelectedFields);
            let cash: AnyElement = if full { model.cash.compact().into_any_element() } else { record::muted("Not disclosed", cx) };
            Some(record::row(
                SharedString::from(format!("company-{}", id.raw())),
                selected == Some(id),
                vec![
                    (LANES[0].1, record::stack(company.name.clone(), company.jurisdiction.clone(), cx)),
                    (LANES[1].1, h_flex().child(labels::disclosure_tag(model.disclosure)).into_any_element()),
                    (LANES[2].1, cash),
                    (LANES[3].1, h_flex().child(model.ceiling.compact()).into_any_element()),
                ],
                move |_, _, cx| crate::app::with_app(cx, |app, cx| app.select_company(id, cx)),
            ))
        })
        .collect();
    let visible = models.companies.len();
    let total = household.companies.len();
    let theme = cx.theme();
    let footer = selected.and_then(|id| household.company(id)).map(|company| {
        let id = company.id;
        h_flex()
            .w_full()
            .justify_end()
            .items_center()
            .gap_2()
            .child(div().flex_1().text_xs().text_color(theme.muted_foreground).child(format!("Selected: {}", company.name)))
            .child(Button::new("company-open").small().outline().label("Open company").on_click(cx.listener(move |this, _, _, cx| this.navigate(Route::Company(id), cx))))
    });
    let body: AnyElement = if total == 0 {
        let action = if household.people.is_empty() {
            Button::new("companies-add-person").small().outline().label("Add person…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Person, window, cx))).into_any_element()
        } else {
            Button::new("companies-add-first").small().outline().icon(IconName::Plus).label("Add company…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Company, window, cx))).into_any_element()
        };
        empty_state("companies-empty", "No companies yet", if household.people.is_empty() { "A company needs an owner, so add a person first." } else { "A company keeps its own ledger; its cash is never household cash." }, Some(action), cx)
    } else if visible == 0 {
        none_disclosed("companies-none-disclosed", "companies", total, cx)
    } else {
        record::list("companies-list", record::header(&LANES, cx), rows).into_any_element()
    };

    v_flex()
        .id("screen-companies")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(count_line(visible, total, "companies", cx))
        .child(body)
        .children(footer)
        .child(note("Cash ceilings are before extraction costs and are never money available to the household. A summary viewer sees the ceiling only.", cx))
        .into_any_element()
}

pub fn render_detail(app: &AtlasApp, id: CompanyId, models: &EntityModels, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let (Some(company), Some(model)) = (household.company(id), models.company(id)) else {
        return v_flex()
            .id("screen-company")
            .test_support()
            .gap_4()
            .child(detail_header(Destination::Household, Route::Companies, "Companies", "Company", None, vec![], cx))
            .child(crate::widgets::states::not_disclosed("company-not-disclosed", "This company is not disclosed to this viewer, or it no longer exists.", cx))
            .into_any_element();
    };
    let viewer = app.viewer();
    let full = matches!(model.disclosure, Disclosure::Full | Disclosure::SelectedFields);
    let owner = household.policy_for(ObjectRef::Company(id)).is_some_and(|p| p.full_access.contains(&viewer.person));
    let subtitle = h_flex()
        .gap_2()
        .items_center()
        .child(div().text_sm().text_color(cx.theme().muted_foreground).child(format!("Jurisdiction {}", company.jurisdiction)))
        .child(labels::disclosure_tag(model.disclosure))
        .child(Tag::secondary().xsmall().outline().child("Separate legal entity"))
        .into_any_element();
    let mut actions: Vec<AnyElement> = Vec::new();
    if full {
        actions.push(Button::new("company-add-account").small().outline().icon(IconName::Plus).label("Add company account…").on_click(cx.listener(move |this, _, window, cx| this.open_account_for_company(id, window, cx))).into_any_element());
        actions.push(
            DropdownButton::new("company-more")
                .small()
                .button(Button::new("company-more-button").small().ghost().label("More"))
                .dropdown_menu(move |menu, _, _| {
                    menu.item(PopupMenuItem::new("Model new employment…").on_click(move |_, _, cx| crate::app::with_app(cx, |app, cx| app.navigate(Route::Scenarios, cx))))
                        .item(PopupMenuItem::new("View policy").on_click(move |_, _, cx| crate::app::with_app(cx, |app, cx| app.open_policy_for(ObjectRef::Company(id), cx))))
                        .when(owner, |menu| menu.item(PopupMenuItem::new("Change policy…").on_click(move |_, window, cx| crate::app::with_app(cx, |app, cx| app.open_policy_editor_for(Some(ObjectRef::Company(id)), window, cx)))))
                })
                .into_any_element(),
        );
    }
    let header = detail_header(Destination::Household, Route::Companies, "Companies", company.name.clone(), Some(subtitle), actions, cx);
    let theme = cx.theme();

    if !full {
        // The planning-safe summary: identity and the ceiling, nothing else.
        return v_flex()
            .id("screen-company")
            .test_support()
            .w_full()
            .gap_6()
            .child(header)
            .child(
                section("company-summary", "What this viewer may see")
                    .child(card(model.ceiling.leading()))
                    .child(note("Bank balances, revenue, employee pay, payroll and tax records are not disclosed to this viewer. The owner sees the complete company ledger.", cx))
                    .child(h_flex().child(about_access_button("company-about-access"))),
            )
            .into_any_element();
    }

    let owners: Vec<AnyElement> = company.owners.iter().map(|o| fact(household.entity_name(EntityRef::Person(o.person)), format!("{}% owner", o.basis_points / 100), cx).into_any_element()).collect();
    let roles: Vec<String> = company.roles.iter().map(|r| format!("{} — {}", household.entity_name(EntityRef::Person(r.person)), r.role.label())).collect();
    let constraints: Vec<String> = company.constraints.iter().map(|c| c.describe()).collect();

    let account_lanes: [(&str, Lane); 4] = [("Account", Lane::fixed(260.)), ("Kind / institution", Lane::fixed(220.)), ("Settled", Lane::money(140.)), ("Active earmarks", Lane::flex())];
    let account_rows: Vec<_> = household
        .company_accounts(id)
        .map(|a| {
            let account_id = a.id;
            let earmarks: Vec<String> = household.active_reservations_on(a.id).map(|r| format!("{} {}", r.name, r.amount.format())).collect();
            record::row(
                SharedString::from(format!("company-account-{}", a.id.raw())),
                false,
                vec![
                    (account_lanes[0].1, record::link(format!("company-account-open-{}", a.id.raw()), a.name.clone(), cx.listener(move |this, _, _, cx| this.navigate(Route::Account(account_id), cx)))),
                    (account_lanes[1].1, record::muted(format!("{} · {}", a.kind.label(), a.institution), cx)),
                    (account_lanes[2].1, record::money(a.settled_balance, cx)),
                    (account_lanes[3].1, record::muted(if earmarks.is_empty() { "None".to_string() } else { earmarks.join(" · ") }, cx)),
                ],
                |_, _, _| {},
            )
        })
        .collect();
    let employee_lanes: [(&str, Lane); 4] = [("Employee", Lane::fixed(220.)), ("Monthly gross", Lane::money(140.)), ("Start / end", Lane::fixed(220.)), ("Household link", Lane::flex())];
    let employee_rows: Vec<_> = company
        .employees
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let link: AnyElement = match e.person {
                Some(person) => h_flex()
                    .gap_1()
                    .items_center()
                    .child(record::muted("Salary of", cx))
                    .child(record::link(format!("employee-person-{i}"), household.entity_name(EntityRef::Person(person)), cx.listener(move |this, _, _, cx| this.navigate(Route::Person(person), cx))))
                    .child(record::muted("— one linked movement: expense here, income there", cx))
                    .into_any_element(),
                None => record::muted("External employee: company outflow only", cx),
            };
            record::row(
                SharedString::from(format!("employee-{i}")),
                false,
                vec![
                    (employee_lanes[0].1, record::text(e.name.clone())),
                    (employee_lanes[1].1, record::money(e.monthly_gross, cx)),
                    (employee_lanes[2].1, record::muted(format!("{} – {}", e.start.format("%d %b %Y"), e.end.map(|d| d.format("%d %b %Y").to_string()).unwrap_or_else(|| "open".into())), cx)),
                    (employee_lanes[3].1, link),
                ],
                |_, _, _| {},
            )
        })
        .collect();

    v_flex()
        .id("screen-company")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(
            section("company-cash", "Cash position")
                .description("Business cash, what it is already committed to, and the ceiling the cash could bear before extraction costs. None of it is household cash.")
                .action(
                    h_flex()
                        .gap_2()
                        .child(Button::new("company-view-money").small().ghost().icon(IconName::Landmark).label("View money").on_click(cx.listener(move |this, _, _, cx| this.open_earmarks_for_boundary(Boundary::Company(id), cx))))
                        .child(Button::new("company-view-forecast").small().ghost().icon(IconName::ChartLine).label("View forecast").on_click(cx.listener(move |this, _, _, cx| this.open_forecast_for_boundary(Boundary::Company(id), cx)))),
                )
                .child(lanes([card(model.cash.standard()).into_any_element(), card(model.committed.standard()).into_any_element(), card(model.ceiling.leading()).into_any_element()])),
        )
        .child(
            section("company-extractable", "Lawfully extractable cash")
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(div().id("company-extractable-value").test_support().text_lg().font_weight(FontWeight::SEMIBOLD).child("Not yet determinable"))
                        .child(labels::strength_tag(model.extractable.calc.node().result_strength())),
                )
                .child(note("This needs route-specific rules and legal capacity, not cash alone. Each route — salary, permitted dividend, documented reimbursement, shareholder-loan repayment — is evaluated separately when a purchase names it; the ceiling above is never an amount that may be taken out.", cx))
                .child(h_flex().child(Button::new("company-test-purchase").small().outline().icon(IconName::Target).label("Test a purchase with this company").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Purchase, cx))))),
        )
        .child(
            h_flex()
                .w_full()
                .gap_8()
                .items_start()
                .child(
                    v_flex().flex_1().min_w_0().child(
                        section("company-owners", "Owners, shares and roles")
                            .child(lanes(owners))
                            .child(if roles.is_empty() { note("No company role assigned.", cx).into_any_element() } else { v_flex().gap_0p5().text_sm().children(roles.into_iter().map(|r| div().child(r))).into_any_element() })
                            .child(note("A company role gives no household access, and household membership gives no company access.", cx)),
                    ),
                )
                .child(
                    v_flex().flex_1().min_w_0().child(
                        section("company-constraints", "Constraints")
                            .description("Each one limits what the cash may be used for; the ceiling honours all of them.")
                            .child(if constraints.is_empty() { note("No constraint recorded.", cx).into_any_element() } else { v_flex().gap_1().text_sm().children(constraints.into_iter().map(|c| h_flex().gap_2().child("•").child(c))).into_any_element() }),
                    ),
                ),
        )
        .child(section("company-accounts", "Accounts and active earmarks").child(if account_rows.is_empty() { note("No company account yet.", cx).into_any_element() } else { record::list("company-accounts-list", record::header(&account_lanes, cx), account_rows).into_any_element() }))
        .child(
            section("company-employees", "Employees and payroll")
                .description("Payroll is planned through the company's series; employment changes are modelled in a scenario, not edited here.")
                .child(if employee_rows.is_empty() { note("No employees.", cx).into_any_element() } else { record::list("company-employees-list", record::header(&employee_lanes, cx), employee_rows).into_any_element() }),
        )
        .child(
            h_flex()
                .gap_2()
                .flex_wrap()
                .child(Button::new("company-tax-details").small().ghost().icon(IconName::Gavel).label("Tax details").on_click(cx.listener(move |this, _, window, cx| this.open_taxes_for_entity(EntityRef::Company(id), window, cx))))
                .child(Button::new("company-extraction").small().ghost().label("Extraction timing illustration").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Extraction, cx))))
                .child(Button::new("company-policy").small().ghost().label("View policy").on_click(cx.listener(move |this, _, _, cx| this.open_policy_for(ObjectRef::Company(id), cx))))
                .child(div().text_xs().text_color(theme.muted_foreground).child("The illustration is a bounded tax comparison, not a legal or cash evaluation.")),
        )
        .into_any_element()
}
