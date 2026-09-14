//! People & companies / People: who is in the household and their attributed
//! economic share; the person detail with what they hold, earn and owe in
//! tax. Nothing here changes who is looking.

use atlas_core::ids::{EntityRef, ObjectRef, PersonId};
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
use crate::widgets::states::{action_bar, columns, columns_leading, count_line, empty_state, fact, info_card, note, section};

// The register's lanes, run out to the full width. The share is the one number
// this register answers with, so it sits right-aligned at the trailing edge
// with the row's `open` chevron after it, and the role lane — two short words —
// takes the slack between them. Before this the three lanes were all fixed and
// stopped two thirds of the way across the window.
const LANES: [(&str, Lane); 4] = [
    ("Person", Lane::fixed(300.)),
    ("Household role", Lane::flex()),
    ("Attributed economic share", Lane::money(300.)),
    ("", Lane::fixed(32.)),
];

pub fn render_list(app: &AtlasApp, models: &EntityModels, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let header = workspace_header(
        Destination::Household,
        Route::People,
        vec![Button::new("new-person").small().outline().icon(IconName::Plus).label("Add person…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Person, window, cx))).into_any_element()],
        cx,
    );
    let selected = app.selected_person;
    let looking = app.viewer().person;
    let rows: Vec<_> = household
        .people
        .iter()
        .map(|person| {
            let id = person.id;
            // The terms are dropped here for the reason `Figure::terms`
            // records: `Confirmed current · Exact accounting calculation` does
            // not fit a money lane, and the `ⓘ` still opens the chain.
            let share: AnyElement = match models.person(id) {
                Some(model) => model.attribution.compact().terms(false).into_any_element(),
                None => record::muted("Attribution unavailable", cx),
            };
            // Which person the viewer is, is a fact about that person, so it
            // sits beside the name. It is a label, not a control: `Who is
            // looking` stays the title bar's. It goes in the fixed identity
            // lane rather than the flexible one, because a tag beside text in
            // an auto-width cell is re-measured at every pass
            // (`docs/perf.md` §3.3).
            let who: AnyElement = h_flex()
                .gap_2()
                .items_center()
                .child(record::text(person.name.clone()))
                .when(looking == id, |this| this.child(Tag::info().xsmall().outline().child("Who is looking")))
                .into_any_element();
            record::row(
                SharedString::from(format!("person-{}", id.raw())),
                selected == Some(id),
                vec![
                    (LANES[0].1, who),
                    (LANES[1].1, record::muted(person.role.label(), cx)),
                    (LANES[2].1, share),
                    (
                        LANES[3].1,
                        Button::new(SharedString::from(format!("person-open-{}", id.raw())))
                            .xsmall()
                            .ghost()
                            .compact()
                            .icon(IconName::ChevronRight)
                            .tooltip("Open person")
                            .on_click(cx.listener(move |this, _, _, cx| this.navigate(Route::Person(id), cx)))
                            .into_any_element(),
                    ),
                ],
                cx.listener(move |this, _, _, cx| this.select_person(id, cx)),
            )
        })
        .collect();

    // The foot of the register: what is selected on the left, what can be done
    // with it on the right, ruled off from the rows above.
    let footer = selected.and_then(|id| household.person(id)).map(|person| {
        let id = person.id;
        action_bar(
            "people-footer",
            vec![note(format!("{} · {}", person.name, person.role.label()), cx).into_any_element()],
            vec![Button::new("person-open").small().outline().label("Open person").on_click(cx.listener(move |this, _, _, cx| this.navigate(Route::Person(id), cx))).into_any_element()],
            cx,
        )
        .into_any_element()
    });

    let any = !household.people.is_empty();
    v_flex()
        .id("screen-people")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(count_line(household.people.len(), household.people.len(), "people", cx))
        .child(if any {
            record::list("people-list", record::header(&LANES, cx), rows).into_any_element()
        } else {
            empty_state("people-empty", "No people yet", "Add the first person; accounts and companies need an owner.", Some(Button::new("people-add-first").small().outline().icon(IconName::Plus).label("Add person…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Person, window, cx))).into_any_element()), cx)
        })
        // What this register is and is not: a standing fact, so a bordered
        // card rather than a muted sentence that reads as an afterthought.
        .when(any, |this| {
            this.child(info_card(
                "people-ownership",
                IconName::Users,
                "Ownership is not a viewer switch",
                "The share is each person's part of the accounts they hold, and the household counts a joint account once. Opening a person shows that share; who is looking stays the separate control in the title bar.",
                cx,
            ))
        })
        .children(footer)
        .into_any_element()
}

pub fn render_detail(app: &AtlasApp, id: PersonId, models: &EntityModels, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let Some(person) = household.person(id) else {
        return v_flex().id("screen-person").test_support().gap_4().child(detail_header(Destination::Household, Route::People, "People", "Unknown person", None, vec![], cx)).child(note("This person no longer exists.", cx)).into_any_element();
    };
    let viewer = app.viewer();
    let model = models.person(id);
    let is_viewer = viewer.person == id;
    let muted = cx.theme().muted_foreground;
    // One meta line of the facts that identify the person, with the access tag
    // on the line beneath it rather than sharing a wrap row with it
    // (`docs/perf.md` §3.3). The role was a tag beside the word `Household
    // role`; the register column already names it, so the meta line states it.
    let subtitle = v_flex()
        .w_full()
        .gap_1()
        .child(div().w_full().text_sm().text_color(muted).child(person.role.label()))
        .when(is_viewer, |this| this.child(h_flex().w_full().gap_2().items_center().child(Tag::info().xsmall().outline().child("Who is looking"))))
        .into_any_element();
    let header = detail_header(
        Destination::Household,
        Route::People,
        "People",
        person.name.clone(),
        Some(subtitle),
        vec![
            Button::new("person-add-account").small().outline().icon(IconName::Plus).label("Add account…").on_click(cx.listener(move |this, _, window, cx| this.open_entry_for_person(Entry::Account, id, window, cx))).into_any_element(),
            DropdownButton::new("person-more")
                .small()
                .button(Button::new("person-more-button").small().ghost().label("More"))
                .dropdown_menu(move |menu, _, _| {
                    menu.item(PopupMenuItem::new("Add income…").on_click(move |_, window, cx| crate::app::with_app(cx, |app, cx| app.open_entry_for_person(Entry::Series, id, window, cx))))
                        .item(PopupMenuItem::new("Delete person…").on_click(move |_, window, cx| crate::app::with_app(cx, |app, cx| app.confirm_delete(ObjectRef::Person(id), window, cx))))
                })
                .into_any_element(),
        ],
        cx,
    );

    // Holdings, projected for the viewer. The two registers stand side by side
    // — each is three or four short lanes, and stacked full width they left the
    // right half of the window empty and the screen a scroll longer.
    let account_lanes: [(&str, Lane); 4] = [("Account", Lane::flex()), ("Share", Lane::fixed(64.)), ("Settled", Lane::money(130.)), ("Kind", Lane::fixed(150.))];
    let mut hidden_accounts = 0usize;
    let account_rows: Vec<_> = household
        .accounts_of(id)
        .filter_map(|a| {
            let disclosure = household.disclosure_for(viewer, ObjectRef::Account(a.id));
            if matches!(disclosure, Disclosure::Hidden | Disclosure::Aggregate) {
                hidden_accounts += 1;
                return None;
            }
            let account_id = a.id;
            let balance: AnyElement = match disclosure {
                Disclosure::Full | Disclosure::SelectedFields | Disclosure::BalanceOnly => record::money(a.settled_balance, cx),
                _ => record::muted("Not disclosed", cx),
            };
            Some(record::row(
                SharedString::from(format!("person-account-{}", a.id.raw())),
                false,
                vec![
                    (account_lanes[0].1, record::link(format!("person-account-open-{}", a.id.raw()), a.name.clone(), cx.listener(move |this, _, _, cx| this.navigate(Route::Account(account_id), cx)))),
                    (account_lanes[1].1, record::muted(format!("{}%", a.holder.share_of(id) / 100), cx)),
                    (account_lanes[2].1, balance),
                    (account_lanes[3].1, record::muted(format!("{}{}", a.kind.label(), if a.kind.is_liability() { " · liability" } else { "" }), cx)),
                ],
                |_, _, _| {},
            ))
        })
        .collect();
    let company_lanes: [(&str, Lane); 3] = [("Company", Lane::flex()), ("Share", Lane::fixed(64.)), ("Company roles", Lane::fixed(200.))];
    let mut hidden_companies = 0usize;
    let company_rows: Vec<_> = household
        .companies_of(id)
        .filter_map(|c| {
            let disclosure = household.disclosure_for(viewer, ObjectRef::Company(c.id));
            if disclosure == Disclosure::Hidden {
                hidden_companies += 1;
                return None;
            }
            let company_id = c.id;
            let share = c.owners.iter().filter(|o| o.person == id).map(|o| o.basis_points).sum::<u32>() / 100;
            let roles: Vec<&str> = c.roles.iter().filter(|r| r.person == id).map(|r| r.role.label()).collect();
            let roles = if matches!(disclosure, Disclosure::Full | Disclosure::SelectedFields) {
                if roles.is_empty() { "No company role".to_string() } else { roles.join(", ") }
            } else {
                "Not disclosed".to_string()
            };
            Some(record::row(
                SharedString::from(format!("person-company-{}", c.id.raw())),
                false,
                vec![
                    (company_lanes[0].1, record::link(format!("person-company-open-{}", c.id.raw()), c.name.clone(), cx.listener(move |this, _, _, cx| this.navigate(Route::Company(company_id), cx)))),
                    (company_lanes[1].1, record::muted(format!("{share}%"), cx)),
                    (company_lanes[2].1, record::muted(roles, cx)),
                ],
                |_, _, _| {},
            ))
        })
        .collect();
    let liabilities: Vec<AnyElement> = household
        .accounts_of(id)
        .filter(|a| a.kind.is_liability())
        .filter(|a| matches!(household.disclosure_for(viewer, ObjectRef::Account(a.id)), Disclosure::Full | Disclosure::SelectedFields | Disclosure::BalanceOnly))
        .map(|a| fact(a.name.clone(), a.settled_balance.format(), cx).into_any_element())
        .collect();

    // Income series attributed to the person.
    let income_lanes: [(&str, Lane); 5] = [("Series", Lane::fixed(260.)), ("Expected per occurrence", Lane::money(170.)), ("Recurrence", Lane::flex()), ("Certainty", Lane::fixed(130.)), ("Posts to", Lane::fixed(200.))];
    let income_rows: Vec<_> = model
        .map(|m| m.income.clone())
        .unwrap_or_default()
        .iter()
        .filter_map(|sid| household.series_by_id(*sid))
        .filter(|s| !matches!(household.disclosure_for(viewer, ObjectRef::Account(s.account)), Disclosure::Hidden | Disclosure::Aggregate))
        .map(|s| {
            let sid = s.id;
            record::row(
                SharedString::from(format!("person-income-{}", sid.raw())),
                false,
                vec![
                    (income_lanes[0].1, record::link(format!("person-income-open-{}", sid.raw()), s.name.clone(), cx.listener(move |this, _, _, cx| this.navigate(Route::SeriesDetail(sid), cx)))),
                    (income_lanes[1].1, record::money(s.amount.expected(), cx)),
                    (income_lanes[2].1, record::muted(s.recurrence.describe(), cx)),
                    (income_lanes[3].1, h_flex().child(labels::certainty_tag(s.certainty)).into_any_element()),
                    (income_lanes[4].1, record::muted(household.account(s.account).map(|a| a.name.clone()).unwrap_or_default(), cx)),
                ],
                |_, _, _| {},
            )
        })
        .collect();

    let accounts_empty = account_rows.is_empty();
    let companies_empty = company_rows.is_empty();

    // The tax band: the figure and the two facts that qualify it as one even
    // grid, not three 16 rem cards wrapping at the left of the window.
    let tax: AnyElement = match model.and_then(|m| m.tax.as_ref()) {
        Some(tax) => {
            let packs = if tax.packs.is_empty() { String::new() } else { format!(" under {}", tax.packs.join(", ")) };
            match &tax.total {
                Some(figure) => v_flex()
                    .w_full()
                    .gap_3()
                    .child(columns([figure.standard().into_any_element(), fact("Tax events", tax.events.to_string(), cx).into_any_element(), fact("Through", tax.through.format("%d %b %Y").to_string(), cx).into_any_element()]))
                    .child(note(format!("Expected baseline through the forecast end{packs}. A planning estimate, not a filing."), cx))
                    .into_any_element(),
                None => note(format!("No tax event attributed to {} through {}{packs}.", person.name, tax.through.format("%d %b %Y")), cx).into_any_element(),
            }
        }
        None => note("The tax assessment could not be computed; the person's own facts above are unaffected.", cx).into_any_element(),
    };

    v_flex()
        .id("screen-person")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        // The share leads at 38 % of the width with the rule it is computed
        // under beside it, rather than as a 16 rem card with a paragraph of
        // description above it and the rest of the window empty.
        .child(
            section("person-share", "Attributed economic share").child(columns_leading(
                0.38,
                [
                    match model {
                        Some(m) => m.attribution.leading().into_any_element(),
                        None => note("Attribution unavailable: the calculation failed. The person's facts below are unaffected.", cx).into_any_element(),
                    },
                    info_card(
                        "person-share-basis",
                        IconName::Percent,
                        "A person's view uses their share",
                        "This person's part of every account they hold. The household counts a joint account once, so shares never double count.",
                        cx,
                    ),
                ],
            )),
        )
        // What the person holds, in two columns. The three registers were three
        // full-width bands one under the other, each three or four short lanes
        // wide, so the screen was a scroll longer than it needed to be and the
        // right half of every band was empty. Companies and liabilities are the
        // short ones, so they share the trailing column.
        .child(columns([
            section("person-accounts", "Accounts held")
                .child(if account_rows.is_empty() {
                    note(if hidden_accounts > 0 { format!("{hidden_accounts} not disclosed to this viewer.") } else { "No account yet.".to_string() }, cx).into_any_element()
                } else {
                    record::list("person-accounts-list", record::header(&account_lanes, cx), account_rows).into_any_element()
                })
                .when(hidden_accounts > 0 && !accounts_empty, |this| this.child(div().text_xs().text_color(muted).child(format!("{hidden_accounts} more not disclosed to this viewer."))))
                .into_any_element(),
            v_flex()
                .w_full()
                .gap_6()
                .child(
                    section("person-companies", "Companies owned")
                        .child(if company_rows.is_empty() {
                            note(if hidden_companies > 0 { format!("{hidden_companies} not disclosed to this viewer.") } else { "No company.".to_string() }, cx).into_any_element()
                        } else {
                            record::list("person-companies-list", record::header(&company_lanes, cx), company_rows).into_any_element()
                        })
                        .when(hidden_companies > 0 && !companies_empty, |this| this.child(div().text_xs().text_color(muted).child(format!("{hidden_companies} more not disclosed to this viewer.")))),
                )
                .child(section("person-liabilities", "Liabilities").description("Negative balances on the person's accounts.").child(if liabilities.is_empty() { note("No liability on a disclosed account.", cx).into_any_element() } else { columns(liabilities).into_any_element() }))
                .into_any_element(),
        ]))
        .child(
            section("person-income", "Income").divider(true).child(if income_rows.is_empty() {
                empty_state(
                    "person-income-empty",
                    "No income yet",
                    "No income series is attributed to this person.",
                    Some(Button::new("person-income-add-first").small().outline().icon(IconName::Plus).label("Add income…").on_click(cx.listener(move |this, _, window, cx| this.open_entry_for_person(Entry::Series, id, window, cx))).into_any_element()),
                    cx,
                )
            } else {
                record::list("person-income-list", record::header(&income_lanes, cx), income_rows).into_any_element()
            }),
        )
        // Ruled off: what the person earns and what the window attributes to
        // them in tax are different questions from what they hold. The holdings
        // above are not ruled off from the share, because they are what the
        // share is the sum of.
        .child(section("person-tax", "Tax cash attributed").divider(true).child(tax))
        // The screen's own commands, ruled off at its foot: where this person's
        // money can be inspected on the left, what can be recorded against them
        // on the right. They were four peers scattered across four headings.
        .child(action_bar(
            "person-commands",
            vec![
                Button::new("person-view-money").small().ghost().icon(IconName::Landmark).label("View money").on_click(cx.listener(move |this, _, _, cx| this.open_earmarks_for_boundary(Boundary::Person(id), cx))).into_any_element(),
                Button::new("person-view-forecast").small().ghost().icon(IconName::ChartLine).label("View forecast").on_click(cx.listener(move |this, _, _, cx| this.open_forecast_for_boundary(Boundary::Person(id), cx))).into_any_element(),
                Button::new("person-tax-details").small().ghost().icon(IconName::Gavel).label("Tax details").on_click(cx.listener(move |this, _, window, cx| this.open_taxes_for_entity(EntityRef::Person(id), window, cx))).into_any_element(),
            ],
            vec![Button::new("person-add-income").small().outline().icon(IconName::Plus).label("Add income…").on_click(cx.listener(move |this, _, window, cx| this.open_entry_for_person(Entry::Series, id, window, cx))).into_any_element()],
            cx,
        ))
        .into_any_element()
}
