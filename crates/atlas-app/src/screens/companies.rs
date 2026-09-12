//! Companies (§5.3, §8): a separate ledger per company — accounts, employees,
//! constraints, cash — and what a household viewer may see of it (§8.7).

use atlas_core::authz::Viewer;
use atlas_core::ids::{CompanyId, EntityRef};
use atlas_core::model::{Company, Household};
use atlas_core::Disclosure;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _,
    button::Button,
    description_list::{DescriptionItem, DescriptionList},
    group_box::GroupBox, h_flex,
    table::{Table, TableBody, TableCell, TableHead, TableHeader, TableRow},
    tag::Tag, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::entities::{CompanyModel, EntityModels};
use crate::app::AtlasApp;
use crate::widgets::labels;
use crate::widgets::master::{master_detail, master_item, page_header};
use crate::widgets::table::{money_cell, muted_cell};
use crate::widgets::figure::card;

pub fn render(models: &EntityModels, household: &Household, viewer: Viewer, selected: Option<CompanyId>, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let viewer_name = household.entity_name(EntityRef::Person(viewer.person));
    let selected = selected.filter(|id| models.company(*id).is_some()).or_else(|| models.companies.first().map(|c| c.id));

    let master = v_flex().gap_1().children(models.companies.iter().filter_map(|model| {
        let company = household.company(model.id)?;
        let id = model.id;
        let trailing = if matches!(model.disclosure, Disclosure::Full | Disclosure::SelectedFields | Disclosure::BalanceOnly) {
            model.cash.calc.money().format()
        } else {
            "summary".to_string()
        };
        Some(master_item(
            SharedString::from(format!("company-{}", id.raw())),
            company.name.clone(),
            format!("{} employees · {}", company.employees.len(), model.disclosure.label()),
            trailing,
            selected == Some(id),
            cx.listener(move |this, _, _, cx| this.select_company(id, cx)),
            cx,
        ))
    }));

    let detail: AnyElement = match selected.and_then(|id| household.company(id).zip(models.company(id))) {
        Some((company, model)) => render_detail(company, model, household, &viewer_name, cx).into_any_element(),
        None => div().text_color(cx.theme().muted_foreground).child("No company is visible to this viewer.").into_any_element(),
    };

    v_flex()
        .id("screen-companies")
        .test_support()
        .w_full()
        .gap_6()
        .child(
            h_flex().justify_between().items_start().gap_4().child(page_header(
                "Companies",
                format!(
                    "{} of {} companies visible · a company is legally distinct from its owners; its cash is never household cash (§8.5)",
                    models.companies.len(),
                    household.companies.len()
                ),
                cx,
            ))
            .child(
                Button::new("new-company")
                    .flex_shrink_0()
                    .small()
                    .outline()
                    .icon(IconName::Plus)
                    .label("New company…")
                    .on_click(cx.listener(|this, _, window, cx| this.open_entry(crate::entry::Entry::Company, window, cx))),
            ),
        )
        .child(master_detail("companies-master-detail", master, detail, cx))
}

fn render_detail(company: &Company, model: &CompanyModel, household: &Household, viewer_name: &str, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let full = matches!(model.disclosure, Disclosure::Full | Disclosure::SelectedFields);
    let owners: Vec<String> = company
        .owners
        .iter()
        .map(|o| format!("{} {}%", household.entity_name(EntityRef::Person(o.person)), o.basis_points / 100))
        .collect();
    let roles: Vec<String> = company
        .roles
        .iter()
        .map(|r| format!("{} — {}", household.entity_name(EntityRef::Person(r.person)), r.role.label()))
        .collect();

    let mut detail = v_flex()
        .id(SharedString::from(format!("company-detail-{}", company.id.raw())))
        .test_support()
        .gap_6()
        .child(
            h_flex()
                .items_center()
                .gap_3()
                .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(company.name.clone()))
                .child(Tag::secondary().xsmall().outline().child("Separate legal entity"))
                .child(labels::disclosure_tag(model.disclosure)),
        );

    if !full {
        // §8.7: a household participant sees only a planning-safe output.
        return detail
            .child(
                GroupBox::new().id("company-summary").title("Planning-safe output (§8.7)").child(
                    v_flex()
                        .gap_3()
                        .child(model.ceiling.figure(viewer_name, true))
                        .child(div().text_xs().text_color(theme.muted_foreground).child(
                            "Bank balances, client revenue, employee salaries, payroll and tax records are not disclosed to this viewer. The owner sees the complete company ledger.",
                        )),
                ),
            )
            .into_any_element();
    }

    detail = detail
        .child(
            GroupBox::new().id("company-facts").title("Entity (§5.3)").child(
                DescriptionList::new()
                    .columns(1)
                    .child(DescriptionItem::new("Jurisdiction").value(company.jurisdiction.clone()))
                    .child(DescriptionItem::new("Owners").value(owners.join(" · ")))
                    .child(DescriptionItem::new("Entity roles (§5.16)").value(if roles.is_empty() { "—".to_string() } else { roles.join(" · ") }))
                    .child(DescriptionItem::new("Household access").value("Company roles never confer household access, and household membership never grants company access (§8.7)")),
            ),
        )
        .child(
            GroupBox::new().id("company-cash").title("Cash position (§6.8, §6.9, E07)").child(
                v_flex()
                    .gap_4()
                    .child(
                        h_flex()
                            .flex_wrap()
                            .gap_8()
                            .child(card(model.cash.figure(viewer_name, false)))
                            .child(card(model.committed.figure(viewer_name, false)))
                            .child(card(model.ceiling.figure(viewer_name, true))),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(div().text_xs().text_color(theme.muted_foreground).child(model.extractable.label.clone()))
                                    .child(div().font_weight(FontWeight::SEMIBOLD).child("not yet determinable"))
                                    .child(labels::strength_tag(model.extractable.calc.node().result_strength())),
                            )
                            .child(div().text_xs().text_color(theme.muted_foreground).child(
                                "The ceiling is a cash constraint before extraction costs, not lawfully distributable cash. Each route — salary, permitted dividend, documented reimbursement, genuine shareholder-loan repayment — is a separate route on the Decisions screen, each with the legal-capacity caveat (M27).",
                            )),
                    ),
            ),
        )
        .child(
            GroupBox::new().id("company-constraints").title("Constraints (§8.6)").child(
                v_flex().gap_1().text_sm().children(company.constraints.iter().map(|c| h_flex().gap_2().child("•").child(c.describe()))),
            ),
        )
        .child(
            GroupBox::new().id("company-accounts").title("Accounts").child(
                Table::new()
                    .child(
                        TableHeader::new().child(
                            TableRow::new()
                                .child(TableHead::new().w_64().flex_shrink_0().child("Account"))
                                .child(TableHead::new().w_40().flex_shrink_0().child("Kind"))
                                .child(TableHead::new().w_40().flex_shrink_0().child("Institution"))
                                .child(TableHead::new().w_32().flex_shrink_0().text_right().child("Settled"))
                                .child(TableHead::new().min_w_0().child("Earmarks")),
                        ),
                    )
                    .child(TableBody::new().children(household.company_accounts(company.id).enumerate().map(|(index, account)| {
                        let earmarks: Vec<String> = household.active_reservations_on(account.id).map(|r| format!("{} {}", r.name, r.amount.format())).collect();
                        TableRow::new()
                            .when(index % 2 == 1, |row| row.bg(theme.table_even))
                            .child(TableCell::new().w_64().flex_shrink_0().overflow_hidden().text_ellipsis().child(account.name.clone()))
                            .child(muted_cell(account.kind.label(), cx).w_40().flex_shrink_0().overflow_hidden().text_ellipsis())
                            .child(muted_cell(account.institution.clone(), cx).w_40().flex_shrink_0().overflow_hidden().text_ellipsis())
                            .child(money_cell(account.settled_balance, cx).w_32().flex_shrink_0())
                            .child(muted_cell(if earmarks.is_empty() { "—".to_string() } else { earmarks.join(" · ") }, cx).min_w_0().overflow_hidden().text_ellipsis())
                    }))),
            ),
        )
        .child(
            GroupBox::new().id("company-employees").title("Employees and payroll (§8.3)").child(if company.employees.is_empty() {
                div().text_sm().text_color(theme.muted_foreground).child("No employees.").into_any_element()
            } else {
                Table::new()
                    .child(
                        TableHeader::new().child(
                            TableRow::new()
                                .child(TableHead::new().w_64().flex_shrink_0().child("Employee"))
                                .child(TableHead::new().w_32().flex_shrink_0().text_right().child("Monthly gross"))
                                .child(TableHead::new().w_32().flex_shrink_0().child("Start"))
                                .child(TableHead::new().w_32().flex_shrink_0().child("End"))
                                .child(TableHead::new().min_w_0().child("Household link (§8.4)")),
                        ),
                    )
                    .child(TableBody::new().children(company.employees.iter().enumerate().map(|(index, employee)| {
                        let link = match employee.person {
                            Some(person) => format!("Owner salary → {}: one linked movement, expense here and income there", household.entity_name(EntityRef::Person(person))),
                            None => "External employee: company outflow only".to_string(),
                        };
                        TableRow::new()
                            .when(index % 2 == 1, |row| row.bg(theme.table_even))
                            .child(TableCell::new().w_64().flex_shrink_0().overflow_hidden().text_ellipsis().child(employee.name.clone()))
                            .child(money_cell(employee.monthly_gross, cx).w_32().flex_shrink_0())
                            .child(muted_cell(employee.start.format("%d %b %Y").to_string(), cx).w_32().flex_shrink_0().overflow_hidden().text_ellipsis())
                            .child(muted_cell(employee.end.map(|d| d.format("%d %b %Y").to_string()).unwrap_or_else(|| "open".into()), cx).w_32().flex_shrink_0().overflow_hidden().text_ellipsis())
                            .child(muted_cell(link, cx).min_w_0().overflow_hidden().text_ellipsis())
                    })))
                    .into_any_element()
            }),
        );
    detail.into_any_element()
}
