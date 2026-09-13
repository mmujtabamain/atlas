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
use crate::widgets::figure::card;
use crate::widgets::labels;
use crate::widgets::record::{self, Lane};
use crate::widgets::states::{count_line, empty_state, fact, lanes, note, section};

const LANES: [(&str, Lane); 3] = [("Person", Lane::fixed(280.)), ("Household role", Lane::fixed(200.)), ("Attributed economic share", Lane::flex())];

pub fn render_list(app: &AtlasApp, models: &EntityModels, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let header = workspace_header(
        Destination::Household,
        Route::People,
        vec![Button::new("new-person").small().outline().icon(IconName::Plus).label("Add person…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Person, window, cx))).into_any_element()],
        cx,
    );
    let selected = app.selected_person;
    let rows: Vec<_> = household
        .people
        .iter()
        .map(|person| {
            let id = person.id;
            let share: AnyElement = match models.person(id) {
                Some(model) => model.attribution.compact().into_any_element(),
                None => record::muted("Attribution unavailable", cx),
            };
            record::row(
                SharedString::from(format!("person-{}", id.raw())),
                selected == Some(id),
                vec![
                    (LANES[0].1, record::text(person.name.clone())),
                    (LANES[1].1, record::muted(person.role.label(), cx)),
                    (LANES[2].1, h_flex().justify_start().child(share).into_any_element()),
                ],
                move |_, _, cx| crate::app::with_app(cx, |app, cx| app.select_person(id, cx)),
            )
        })
        .collect();
    let theme = cx.theme();
    let footer = selected.and_then(|id| household.person(id)).map(|person| {
        let id = person.id;
        h_flex()
            .w_full()
            .justify_end()
            .items_center()
            .gap_2()
            .child(div().flex_1().text_xs().text_color(theme.muted_foreground).child(format!("Selected: {}", person.name)))
            .child(Button::new("person-open").small().outline().label("Open person").on_click(cx.listener(move |this, _, _, cx| this.navigate(Route::Person(id), cx))))
    });

    v_flex()
        .id("screen-people")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(count_line(household.people.len(), household.people.len(), "people", cx))
        .child(if household.people.is_empty() {
            empty_state("people-empty", "No people yet", "Add the first person; accounts and companies need an owner.", Some(Button::new("people-add-first").small().outline().icon(IconName::Plus).label("Add person…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Person, window, cx))).into_any_element()), cx)
        } else {
            record::list("people-list", record::header(&LANES, cx), rows).into_any_element()
        })
        .children(footer)
        .child(note("The share is each person's part of the accounts they hold; the household counts a joint account once. Who is looking is the separate control in the title bar.", cx))
        .into_any_element()
}

pub fn render_detail(app: &AtlasApp, id: PersonId, models: &EntityModels, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let Some(person) = household.person(id) else {
        return v_flex().id("screen-person").test_support().gap_4().child(detail_header(Destination::Household, Route::People, "People", "Unknown person", None, vec![], cx)).child(note("This person no longer exists.", cx)).into_any_element();
    };
    let viewer = app.viewer();
    let model = models.person(id);
    let is_viewer = viewer.person == id;
    let header = detail_header(
        Destination::Household,
        Route::People,
        "People",
        person.name.clone(),
        Some(h_flex().gap_2().items_center().child(div().text_sm().text_color(cx.theme().muted_foreground).child("Household role")).child(Tag::secondary().xsmall().outline().child(person.role.label())).when(is_viewer, |this| this.child(Tag::info().xsmall().outline().child("Who is looking"))).into_any_element()),
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

    // Holdings, projected for the viewer.
    let account_lanes: [(&str, Lane); 4] = [("Account", Lane::fixed(260.)), ("Share", Lane::fixed(80.)), ("Settled", Lane::money(140.)), ("Kind", Lane::flex())];
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
    let company_lanes: [(&str, Lane); 3] = [("Company", Lane::fixed(260.)), ("Share", Lane::fixed(80.)), ("Company roles", Lane::flex())];
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

    let theme = cx.theme();
    let tax: AnyElement = match model.and_then(|m| m.tax.as_ref()) {
        Some(tax) => {
            let packs = if tax.packs.is_empty() { String::new() } else { format!(" under {}", tax.packs.join(", ")) };
            match &tax.total {
                Some(figure) => v_flex()
                    .gap_2()
                    .child(lanes([card(figure.standard()).into_any_element(), fact("Tax events", tax.events.to_string(), cx).into_any_element(), fact("Through", tax.through.format("%d %b %Y").to_string(), cx).into_any_element()]))
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
        .child(
            section("person-share", "Attributed economic share")
                .description("This person's part of every account they hold. The household counts a joint account once; a person's view uses their share, so shares never double count.")
                .action(
                    h_flex()
                        .gap_2()
                        .child(Button::new("person-view-money").small().ghost().icon(IconName::Landmark).label("View money").on_click(cx.listener(move |this, _, _, cx| this.open_earmarks_for_boundary(Boundary::Person(id), cx))))
                        .child(Button::new("person-view-forecast").small().ghost().icon(IconName::ChartLine).label("View forecast").on_click(cx.listener(move |this, _, _, cx| this.open_forecast_for_boundary(Boundary::Person(id), cx)))),
                )
                .child(match model {
                    Some(m) => card(m.attribution.leading()).into_any_element(),
                    None => note("Attribution unavailable: the calculation failed. The person's facts below are unaffected.", cx).into_any_element(),
                }),
        )
        .child(
            section("person-accounts", "Accounts held")
                .child(if account_rows.is_empty() {
                    note(if hidden_accounts > 0 { format!("{hidden_accounts} not disclosed to this viewer.") } else { "No account yet.".to_string() }, cx).into_any_element()
                } else {
                    record::list("person-accounts-list", record::header(&account_lanes, cx), account_rows).into_any_element()
                })
                .when(hidden_accounts > 0, |this| this.child(div().text_xs().text_color(theme.muted_foreground).child(format!("{hidden_accounts} more not disclosed to this viewer.")))),
        )
        .child(
            section("person-companies", "Companies owned")
                .child(if company_rows.is_empty() {
                    note(if hidden_companies > 0 { format!("{hidden_companies} not disclosed to this viewer.") } else { "No company.".to_string() }, cx).into_any_element()
                } else {
                    record::list("person-companies-list", record::header(&company_lanes, cx), company_rows).into_any_element()
                })
                .when(hidden_companies > 0, |this| this.child(div().text_xs().text_color(theme.muted_foreground).child(format!("{hidden_companies} more not disclosed to this viewer.")))),
        )
        .child(section("person-liabilities", "Liabilities").description("Negative balances on the person's accounts.").child(if liabilities.is_empty() { note("No liability on a disclosed account.", cx).into_any_element() } else { lanes(liabilities).into_any_element() }))
        .child(
            section("person-income", "Income")
                .action(Button::new("person-add-income").small().outline().icon(IconName::Plus).label("Add income…").on_click(cx.listener(move |this, _, window, cx| this.open_entry_for_person(Entry::Series, id, window, cx))))
                .child(if income_rows.is_empty() { note("No income series attributed to this person.", cx).into_any_element() } else { record::list("person-income-list", record::header(&income_lanes, cx), income_rows).into_any_element() }),
        )
        .child(
            section("person-tax", "Tax cash attributed")
                .action(Button::new("person-tax-details").small().ghost().icon(IconName::Gavel).label("Tax details").on_click(cx.listener(move |this, _, window, cx| this.open_taxes_for_entity(EntityRef::Person(id), window, cx))))
                .child(tax),
        )
        .into_any_element()
}
