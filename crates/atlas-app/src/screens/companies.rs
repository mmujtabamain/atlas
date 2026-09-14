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
use crate::widgets::labels;
use crate::widgets::record::{self, Lane};
use crate::widgets::states::{about_access_button, action_bar, columns, columns_leading, count_line, empty_state, info_card, none_disclosed, note, section};

// The register's lanes, run out to the full width. The two money lanes are
// right-aligned at the trailing edge with the row's `open` chevron after them,
// and the identity lane — the only one with long text — takes the slack. Before
// this every lane was fixed and the last one flexible, so the ceiling, which is
// what this register is read for, floated in the middle of the window.
const LANES: [(&str, Lane); 5] = [
    ("Company / jurisdiction", Lane::flex()),
    ("Disclosure", Lane::fixed(130.)),
    ("Business cash", Lane::money(240.)),
    ("Cash ceiling before extraction costs", Lane::money(300.)),
    ("", Lane::fixed(32.)),
];

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
            // The terms are dropped in the register for the reason
            // `Figure::terms` records: they do not fit a money lane. The lane
            // headers carry the qualifier that matters here — a ceiling is
            // before extraction costs — and the `ⓘ` still opens each chain.
            let cash: AnyElement = if full { model.cash.compact().terms(false).into_any_element() } else { record::muted("Not disclosed", cx) };
            Some(record::row(
                SharedString::from(format!("company-{}", id.raw())),
                selected == Some(id),
                vec![
                    (LANES[0].1, record::stack(company.name.clone(), company.jurisdiction.clone(), cx)),
                    (LANES[1].1, h_flex().child(labels::disclosure_tag(model.disclosure)).into_any_element()),
                    (LANES[2].1, cash),
                    (LANES[3].1, model.ceiling.compact().terms(false).into_any_element()),
                    (
                        LANES[4].1,
                        Button::new(SharedString::from(format!("company-open-{}", id.raw())))
                            .xsmall()
                            .ghost()
                            .compact()
                            .icon(IconName::ChevronRight)
                            .tooltip("Open company")
                            .on_click(cx.listener(move |this, _, _, cx| this.navigate(Route::Company(id), cx)))
                            .into_any_element(),
                    ),
                ],
                cx.listener(move |this, _, _, cx| this.select_company(id, cx)),
            ))
        })
        .collect();
    let visible = models.companies.len();
    let total = household.companies.len();

    // The foot of the register: what is selected on the left, what can be done
    // with it on the right, ruled off from the rows above.
    let footer = selected.and_then(|id| household.company(id)).map(|company| {
        let id = company.id;
        action_bar(
            "companies-footer",
            vec![note(format!("{} · {}", company.name, company.jurisdiction), cx).into_any_element()],
            vec![Button::new("company-open").small().outline().label("Open company").on_click(cx.listener(move |this, _, _, cx| this.navigate(Route::Company(id), cx))).into_any_element()],
            cx,
        )
        .into_any_element()
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
        // The honest visible / total / not-disclosed count, with the rule that
        // governs every figure under it on the same line. The mockup reduced
        // this to `1 visible company` and dropped the not-disclosed count,
        // which the privacy contract requires; the count line stays as it is.
        .child(h_flex().w_full().gap_1().items_center().child(count_line(visible, total, "companies", cx)).child(note("· Business cash is separate from household cash", cx)))
        .child(body)
        // What a ceiling is, stated once for the whole register: a standing
        // fact, so a bordered card rather than a muted trailing sentence.
        .when(visible > 0, |this| {
            this.child(info_card(
                "companies-ceiling",
                IconName::Gavel,
                "A cash ceiling is not permission to withdraw",
                "The ceiling is what the company's cash could bear before extraction costs, and it is never money available to the household. A lawful withdrawal also depends on route-specific rules and legal capacity. A summary viewer sees the ceiling only.",
                cx,
            ))
        })
        .children(footer)
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
    let muted = cx.theme().muted_foreground;
    // One meta line of the facts that identify the company, with the access
    // tags on the line beneath it rather than sharing a wrap row with it
    // (`docs/perf.md` §3.3) — the same shape the account detail uses.
    let subtitle = v_flex()
        .w_full()
        .gap_1()
        .child(div().w_full().text_sm().text_color(muted).child(format!("Jurisdiction {}", company.jurisdiction)))
        .child(h_flex().w_full().gap_2().items_center().child(labels::disclosure_tag(model.disclosure)).child(Tag::secondary().xsmall().outline().child("Separate legal entity")))
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

    if !full {
        // The planning-safe summary: identity and the ceiling, nothing else.
        // The ceiling leads because it is the only figure there is; the card
        // beside it names the *categories* withheld, never a withheld value,
        // and nothing on this screen calls the ceiling business cash.
        return v_flex()
            .id("screen-company")
            .test_support()
            .w_full()
            .gap_6()
            .child(header)
            .child(
                section("company-summary", "What this viewer may see")
                    .child(columns_leading(
                        0.4,
                        [
                            model.ceiling.leading().into_any_element(),
                            info_card(
                                "company-summary-access",
                                IconName::ShieldCheck,
                                "Only this planning-safe total is disclosed",
                                "Bank balances, revenue, employee pay, payroll and tax records are not disclosed to this viewer. The owner sees the complete company ledger.",
                                cx,
                            ),
                        ],
                    ))
                    .child(note("A ceiling before extraction costs is not household cash, and it does not establish what may lawfully be withdrawn.", cx)),
            )
            .child(action_bar("company-summary-commands", vec![about_access_button("company-about-access").into_any_element()], vec![], cx))
            .into_any_element();
    }

    let owners: Vec<String> = company.owners.iter().map(|o| format!("{} · {}% owner", household.entity_name(EntityRef::Person(o.person)), o.basis_points / 100)).collect();
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
        // Three readings of one company's cash, as one even grid across the
        // width. None of the three is emphasised: the ceiling is the figure the
        // screen is read for, but an emphasised ceiling reads as an amount that
        // may be taken out, which is exactly what it is not.
        .child(
            section("company-cash", "Cash position")
                .description("Business cash, what it is already committed to, and the ceiling the cash could bear before extraction costs. None of it is household cash.")
                .child(columns([model.cash.standard().into_any_element(), model.committed.standard().into_any_element(), model.ceiling.standard().into_any_element()])),
        )
        // The statement leads; the caveat that makes it a statement and not a
        // number sits beside it, and the one command that follows from it is at
        // the trailing edge of the heading.
        .child(
            section("company-extractable", "Lawfully extractable cash")
                .divider(true)
                .action(Button::new("company-test-purchase").small().outline().icon(IconName::Target).label("Test a purchase with this company").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Purchase, cx))))
                .child(columns_leading(
                    0.35,
                    [
                        v_flex()
                            .w_full()
                            .gap_2()
                            .child(div().id("company-extractable-value").test_support().text_lg().font_weight(FontWeight::SEMIBOLD).child("Not yet determinable"))
                            .child(h_flex().child(labels::strength_tag(model.extractable.calc.node().result_strength())))
                            .into_any_element(),
                        div()
                            .w_full()
                            .text_xs()
                            .text_color(muted)
                            .child("This needs route-specific rules and legal capacity, not cash alone. Each route — salary, permitted dividend, documented reimbursement, shareholder-loan repayment — is evaluated separately when a purchase names it; the ceiling above is never an amount that may be taken out.")
                            .into_any_element(),
                    ],
                )),
        )
        .child(columns([
            section("company-owners", "Owners, shares and roles")
                .child(if owners.is_empty() { note("No owner recorded.", cx).into_any_element() } else { v_flex().w_full().gap_0p5().text_sm().children(owners.into_iter().map(|o| div().w_full().child(o))).into_any_element() })
                .child(if roles.is_empty() { note("No company role assigned.", cx).into_any_element() } else { v_flex().w_full().gap_0p5().text_sm().children(roles.into_iter().map(|r| div().w_full().child(r))).into_any_element() })
                .child(note("A company role gives no household access, and household membership gives no company access.", cx))
                .into_any_element(),
            section("company-constraints", "Constraints")
                .description("Each one limits what the cash may be used for; the ceiling honours all of them.")
                .child(if constraints.is_empty() { note("No constraint recorded.", cx).into_any_element() } else { v_flex().w_full().gap_1().text_sm().children(constraints.into_iter().map(|c| h_flex().w_full().gap_2().items_start().child("•").child(div().flex_1().min_w_0().child(c)))).into_any_element() })
                .into_any_element(),
        ]))
        // Ruled off: what the company holds is a different question from who
        // owns it and what limits its cash.
        .child(section("company-accounts", "Accounts and active earmarks").divider(true).child(if account_rows.is_empty() { note("No company account yet.", cx).into_any_element() } else { record::list("company-accounts-list", record::header(&account_lanes, cx), account_rows).into_any_element() }))
        .child(
            section("company-employees", "Employees and payroll")
                .description("Payroll is planned through the company's series; employment changes are modelled in a scenario, not edited here.")
                .child(if employee_rows.is_empty() { note("No employees.", cx).into_any_element() } else { record::list("company-employees-list", record::header(&employee_lanes, cx), employee_rows).into_any_element() }),
        )
        .child(div().w_full().text_xs().text_color(muted).child("The extraction timing illustration is a bounded tax comparison, not a legal or cash evaluation."))
        // The screen's own commands, ruled off at its foot: where this company's
        // money can be inspected on the left, what governs access to it on the
        // right. They were a wrapping row of three peers with a sentence caught
        // between them.
        .child(action_bar(
            "company-commands",
            vec![
                Button::new("company-view-money").small().ghost().icon(IconName::Landmark).label("View money").on_click(cx.listener(move |this, _, _, cx| this.open_earmarks_for_boundary(Boundary::Company(id), cx))).into_any_element(),
                Button::new("company-view-forecast").small().ghost().icon(IconName::ChartLine).label("View forecast").on_click(cx.listener(move |this, _, _, cx| this.open_forecast_for_boundary(Boundary::Company(id), cx))).into_any_element(),
                Button::new("company-tax-details").small().ghost().icon(IconName::Gavel).label("Tax details").on_click(cx.listener(move |this, _, window, cx| this.open_taxes_for_entity(EntityRef::Company(id), window, cx))).into_any_element(),
                Button::new("company-extraction").small().ghost().label("Extraction timing illustration").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Extraction, cx))).into_any_element(),
            ],
            vec![Button::new("company-policy").small().outline().label("View policy").on_click(cx.listener(move |this, _, _, cx| this.open_policy_for(ObjectRef::Company(id), cx))).into_any_element()],
            cx,
        ))
        .into_any_element()
}
