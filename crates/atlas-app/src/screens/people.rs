//! People (§5.2, §7): roles, held accounts with shares, companies, income,
//! and the person's attributed economic share — with its chain.

use atlas_core::ids::PersonId;
use atlas_core::model::Household;
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

use super::entities::EntityModels;
use crate::app::AtlasApp;
use crate::widgets::labels;
use crate::widgets::master::{master_detail, master_item, page_header};
use crate::widgets::table::{money_cell, muted_cell};

pub fn render(models: &EntityModels, household: &Household, selected: Option<PersonId>, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let selected = selected.or_else(|| household.people.first().map(|p| p.id));

    let master = v_flex().gap_1().children(household.people.iter().map(|person| {
        let id = person.id;
        let model = models.person(id);
        master_item(
            SharedString::from(format!("person-{}", id.raw())),
            person.name.clone(),
            person.role.label(),
            model.map(|m| m.attribution.calc.money().format()).unwrap_or_default(),
            selected == Some(id),
            cx.listener(move |this, _, _, cx| this.select_person(id, cx)),
            cx,
        )
    }));

    let detail: AnyElement = match selected.and_then(|id| household.person(id).zip(models.person(id))) {
        Some((person, model)) => render_detail(person, model, household, cx).into_any_element(),
        None => div().text_color(cx.theme().muted_foreground).child("No people in this household.").into_any_element(),
    };

    v_flex()
        .id("screen-people")
        .test_support()
        .w_full()
        .gap_6()
        .child(
            h_flex().justify_between().items_start().gap_4().child(page_header(
                "People",
                format!("{} participants · a person may own accounts, co-own accounts, earn salaries, own companies and owe taxes (§5.2)", household.people.len()),
                cx,
            ))
            .child(
                Button::new("new-person")
                    .flex_shrink_0()
                    .small()
                    .outline()
                    .icon(IconName::Plus)
                    .label("New person…")
                    .on_click(cx.listener(|this, _, window, cx| this.open_entry(crate::entry::Entry::Person, window, cx))),
            ),
        )
        .child(master_detail("people-master-detail", master, detail, cx))
}

fn render_detail(person: &atlas_core::model::Person, model: &super::entities::PersonModel, household: &Household, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let accounts: Vec<String> = household
        .accounts_of(person.id)
        .map(|a| format!("{} — {}%", a.name, a.holder.share_of(person.id) / 100))
        .collect();
    let companies: Vec<String> = household
        .companies_of(person.id)
        .map(|c| {
            let share = c.owners.iter().filter(|o| o.person == person.id).map(|o| o.basis_points).sum::<u32>() / 100;
            let roles: Vec<&str> = c.roles.iter().filter(|r| r.person == person.id).map(|r| r.role.label()).collect();
            if roles.is_empty() { format!("{} — {share}%", c.name) } else { format!("{} — {share}% · {}", c.name, roles.join(", ")) }
        })
        .collect();
    let liabilities: Vec<String> = household
        .accounts_of(person.id)
        .filter(|a| a.kind.is_liability())
        .map(|a| format!("{} {}", a.name, a.settled_balance.format()))
        .collect();
    let income: Vec<_> = model.income.iter().filter_map(|id| household.series.iter().find(|s| s.id == *id)).collect();

    v_flex()
        .id(SharedString::from(format!("person-detail-{}", person.id.raw())))
        .test_support()
        .gap_6()
        .child(
            h_flex()
                .items_center()
                .gap_3()
                .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(person.name.clone()))
                .child(Tag::secondary().xsmall().outline().child(person.role.label())),
        )
        .child(
            GroupBox::new().id("person-facts").title("Ownership and roles (§7)").child(
                DescriptionList::new()
                    .columns(1)
                    .child(DescriptionItem::new("Household role").value(person.role.label()))
                    .child(DescriptionItem::new("Accounts held").value(if accounts.is_empty() { "—".to_string() } else { accounts.join(" · ") }))
                    .child(DescriptionItem::new("Companies").value(if companies.is_empty() { "—".to_string() } else { companies.join(" · ") }))
                    .child(DescriptionItem::new("Liabilities").value(if liabilities.is_empty() { "—".to_string() } else { liabilities.join(" · ") }))
                    .child(DescriptionItem::new("Tax attribution").value(model.tax_text.clone())),
            ),
        )
        .child(
            GroupBox::new().id("person-attribution").title("Economic attribution (§7)").child(
                v_flex()
                    .gap_3()
                    .child(model.attribution.figure(true))
                    .child(div().text_xs().text_color(theme.muted_foreground).child(
                        "The person's share of each held account. The household counts joint accounts once at 100%; attribution never double counts (V066).",
                    )),
            ),
        )
        .child(
            GroupBox::new().id("person-income").title("Income sources (§9)").child(if income.is_empty() {
                div().text_sm().text_color(theme.muted_foreground).child("No income series attributed to this person.").into_any_element()
            } else {
                Table::new()
                    .child(
                        TableHeader::new().child(
                            TableRow::new()
                                .child(TableHead::new().w_64().flex_shrink_0().child("Series"))
                                .child(TableHead::new().w_40().flex_shrink_0().text_right().child("Expected per occurrence"))
                                .child(TableHead::new().min_w_0().child("Recurrence"))
                                .child(TableHead::new().w_32().flex_shrink_0().child("Certainty"))
                                .child(TableHead::new().w_48().flex_shrink_0().child("Posts to")),
                        ),
                    )
                    .child(TableBody::new().children(income.iter().enumerate().map(|(index, series)| {
                        let account = household.account(series.account).map(|a| a.name.clone()).unwrap_or_default();
                        TableRow::new()
                            .when(index % 2 == 1, |row| row.bg(theme.table_even))
                            .child(TableCell::new().w_64().flex_shrink_0().overflow_hidden().text_ellipsis().child(series.name.clone()))
                            .child(money_cell(series.amount.expected(), cx).w_40().flex_shrink_0())
                            .child(muted_cell(series.recurrence.describe(), cx).min_w_0().overflow_hidden().text_ellipsis())
                            .child(TableCell::new().w_32().flex_shrink_0().child(h_flex().child(labels::certainty_tag(series.certainty))))
                            .child(muted_cell(account, cx).w_48().flex_shrink_0().overflow_hidden().text_ellipsis())
                    })))
                    .into_any_element()
            }),
        )
}
