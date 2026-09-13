//! Accounts: the register (find an account, its present balance and access)
//! and the canonical account detail (settled, pending, reserved, free; the
//! earmarks and planned movements that explain them; every property).

use atlas_core::ids::{AccountId, EntityRef, ObjectRef};
use atlas_core::model::{Account, Coverage, Household};
use atlas_core::Disclosure;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _, DropdownButton},
    description_list::{DescriptionItem, DescriptionList},
    h_flex,
    input::Input,
    menu::PopupMenuItem,
    tab::{Tab, TabBar},
    tag::Tag,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::common::{detail_header, workspace_header};
use crate::app::AtlasApp;
use crate::entry::Entry;
use crate::models::entities::{AccountModel, EntityModels, Touch};
use crate::nav::{Destination, Route};
use crate::widgets::figure::card;
use crate::widgets::labels;
use crate::widgets::record::{self, Lane};
use crate::widgets::scope;
use crate::widgets::states::{count_line, empty_state, lanes, none_disclosed, not_disclosed, note, section};

const LANES: [(&str, Lane); 6] = [
    ("Name / institution", Lane::fixed(260.)),
    ("Kind / holder", Lane::flex()),
    ("Settled", Lane::money(140.)),
    ("Reserved", Lane::money(140.)),
    ("Free", Lane::money(140.)),
    ("Disclosure", Lane::fixed(150.)),
];

/// The account register.
pub fn render_list(app: &AtlasApp, models: &EntityModels, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let theme = cx.theme();
    let viewer = app.viewer();
    let controls = &app.accounts_controls;
    let query = controls.search.read(cx).value().trim().to_lowercase();
    let holder_row = scope::selected(&controls.holder, cx);
    let kind_row = scope::selected(&controls.kind, cx);
    let filtering = !query.is_empty() || holder_row > 0 || kind_row > 0;
    let holder_filter: Option<EntityRef> = if holder_row == 0 { None } else { controls.holders.get(holder_row - 1).copied() };
    let kind_filter = if kind_row == 0 { None } else { crate::entry::ACCOUNT_KINDS.get(kind_row - 1).copied() };
    let selected = app.selected_account();
    let total = household.accounts.len();
    let hidden = total - models.accounts.len();

    let visible: Vec<(&Account, &AccountModel)> = models
        .accounts
        .iter()
        .filter_map(|m| household.account(m.id).map(|a| (a, m)))
        .filter(|(a, _)| query.is_empty() || a.name.to_lowercase().contains(&query) || a.institution.to_lowercase().contains(&query))
        .filter(|(a, _)| match holder_filter {
            None => true,
            Some(EntityRef::Company(c)) => matches!(a.holder, atlas_core::model::Holder::Company(id) if id == c),
            Some(EntityRef::Person(p)) => a.holder.share_of(p) > 0,
            Some(EntityRef::Household) => true,
        })
        .filter(|(a, _)| kind_filter.is_none_or(|k| a.kind == k))
        .collect();

    let mut sorted = visible;
    sorted.sort_by(|(a, _), (b, _)| a.name.cmp(&b.name));

    let rows: Vec<_> = sorted
        .iter()
        .map(|(account, model)| {
            let id = account.id;
            let derived_visible = matches!(model.disclosure, Disclosure::Full | Disclosure::SelectedFields);
            let reserved = if derived_visible { model.reserved.compact_labelled("Reserved").into_any_element() } else { record::muted("Not disclosed", cx) };
            let free = if derived_visible { model.free.compact_labelled("Free").into_any_element() } else { record::muted("Not disclosed", cx) };
            let holder = if account.is_company_account() { format!("{} · Business cash", account.kind.label()) } else { format!("{} · {}", account.kind.label(), household.holder_description(account)) };
            record::row(
                SharedString::from(format!("account-{}", id.raw())),
                selected == Some(id),
                vec![
                    (LANES[0].1, record::stack(account.name.clone(), account.institution.clone(), cx)),
                    (LANES[1].1, record::muted(holder, cx)),
                    (LANES[2].1, record::money(account.settled_balance, cx)),
                    (LANES[3].1, reserved),
                    (LANES[4].1, free),
                    (LANES[5].1, h_flex().child(labels::disclosure_tag(model.disclosure)).into_any_element()),
                ],
                cx.listener(move |this, _, _, cx| this.select_account(id, cx)),
            )
        })
        .collect();

    let body: AnyElement = if total == 0 {
        let action = if household.people.is_empty() {
            Button::new("accounts-add-person").small().outline().label("Add person…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Person, window, cx))).into_any_element()
        } else {
            Button::new("accounts-add-first").small().outline().icon(IconName::Plus).label("Add account…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Account, window, cx))).into_any_element()
        };
        empty_state("accounts-empty", "No accounts yet", if household.people.is_empty() { "A person must exist before an account can have an owner." } else { "Add the accounts that hold the household's money; balances are entered by hand." }, Some(action), cx)
    } else if models.accounts.is_empty() {
        none_disclosed("accounts-none-disclosed", "accounts", hidden, cx)
    } else if rows.is_empty() {
        empty_state("accounts-no-match", "No matches", "No disclosed account matches these filters.", Some(Button::new("accounts-clear-2").small().ghost().label("Clear filters").on_click(cx.listener(|this, _, window, cx| this.clear_account_filters(window, cx))).into_any_element()), cx)
    } else {
        record::list("accounts-list", record::header(&LANES, cx), rows).into_any_element()
    };

    let selected_footer = selected.and_then(|id| household.account(id)).map(|account| {
        let id = account.id;
        let owner = household.policy_for(ObjectRef::Account(id)).is_some_and(|p| p.full_access.contains(&viewer.person));
        h_flex()
            .w_full()
            .justify_end()
            .items_center()
            .gap_2()
            .child(div().flex_1().text_xs().text_color(theme.muted_foreground).child(format!("Selected: {}", account.name)))
            .child(Button::new("accounts-open").small().outline().label("Open account").on_click(cx.listener(move |this, _, _, cx| this.navigate(Route::Account(id), cx))))
            .child(
                DropdownButton::new("accounts-more").small().button(Button::new("accounts-more-button").small().ghost().label("More")).dropdown_menu(move |menu, _, _| {
                    menu.item(PopupMenuItem::new("Reconcile…").on_click(move |_, window, cx| crate::app::with_app(cx, |app, cx| app.open_entry(Entry::Reconcile(id), window, cx))))
                        .item(PopupMenuItem::new("Delete account…").on_click(move |_, window, cx| crate::app::with_app(cx, |app, cx| app.confirm_delete(ObjectRef::Account(id), window, cx))))
                        .item(PopupMenuItem::new("View policy").on_click(move |_, _, cx| crate::app::with_app(cx, |app, cx| app.open_policy_for(ObjectRef::Account(id), cx))))
                        .when(owner, |menu| menu.item(PopupMenuItem::new("Change policy…").on_click(move |_, window, cx| crate::app::with_app(cx, |app, cx| app.open_policy_editor_for(Some(ObjectRef::Account(id)), window, cx)))))
                }),
            )
    });

    v_flex()
        .id("screen-accounts")
        .test_support()
        .w_full()
        .gap_6()
        .child(workspace_header(
            Destination::Accounts,
            Route::Accounts,
            vec![Button::new("new-account").small().outline().icon(IconName::Plus).label("Add account…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Account, window, cx))).into_any_element()],
            cx,
        ))
        .child(count_line(models.accounts.len(), total, "accounts", cx))
        .child(
            h_flex()
                .flex_wrap()
                .gap_4()
                .items_end()
                .child(scope::control("Search", Input::new(&controls.search).small().w_64(), cx))
                .child(scope::select("Holder", &controls.holder, px(200.), cx))
                .child(scope::select("Type", &controls.kind, px(200.), cx))
                .when(filtering, |this| this.child(Button::new("accounts-clear").small().ghost().label("Clear filters").on_click(cx.listener(|this, _, window, cx| this.clear_account_filters(window, cx))))),
        )
        .child(body)
        .children(selected_footer)
        .into_any_element()
}

/// The canonical account detail.
pub fn render_detail(app: &AtlasApp, id: AccountId, models: &EntityModels, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let theme = cx.theme();
    let viewer = app.viewer();
    let Some((account, model)) = household.account(id).zip(models.account(id)) else {
        return v_flex()
            .id("screen-account")
            .test_support()
            .gap_4()
            .child(workspace_header(Destination::Accounts, Route::Accounts, vec![], cx))
            .child(note("This item is not available to this viewer.", cx))
            .child(Button::new("account-back").small().outline().label("Back to Accounts").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Accounts, cx))))
            .into_any_element();
    };
    let derived_visible = matches!(model.disclosure, Disclosure::Full | Disclosure::SelectedFields);
    let owner = household.policy_for(ObjectRef::Account(id)).is_some_and(|p| p.full_access.contains(&viewer.person));
    let can_earmark = !account.is_company_account() && model.disclosure == Disclosure::Full;
    let tab = app.account_tab;
    let subtitle = h_flex()
        .gap_2()
        .items_center()
        .flex_wrap()
        .text_sm()
        .text_color(theme.muted_foreground)
        .child(account.institution.clone())
        .child("·")
        .child(account.kind.label())
        .child("·")
        .child(household.holder_description(account))
        .child(labels::disclosure_tag(model.disclosure))
        .when(account.is_company_account(), |this| this.child(Tag::secondary().xsmall().outline().child("Business cash — not household cash")))
        .into_any_element();

    let more = DropdownButton::new("account-more").small().button(Button::new("account-more-button").small().ghost().label("More")).dropdown_menu(move |menu, _, _| {
        menu.item(PopupMenuItem::new("Delete account…").on_click(move |_, window, cx| crate::app::with_app(cx, |app, cx| app.confirm_delete(ObjectRef::Account(id), window, cx))))
            .item(PopupMenuItem::new("View policy").on_click(move |_, _, cx| crate::app::with_app(cx, |app, cx| app.open_policy_for(ObjectRef::Account(id), cx))))
            .when(owner, |menu| menu.item(PopupMenuItem::new("Change policy…").on_click(move |_, window, cx| crate::app::with_app(cx, |app, cx| app.open_policy_editor_for(Some(ObjectRef::Account(id)), window, cx)))))
    });

    let pending = card(
        v_flex()
            .gap_1()
            .child(div().text_xs().text_color(theme.muted_foreground).child("Pending"))
            .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).font_family(theme.mono_font_family.clone()).child(account.pending_balance.format()))
            .child(div().text_xs().text_color(theme.muted_foreground).child("Posted, not yet settled — not spendable")),
    );

    let mut figures: Vec<AnyElement> = vec![card(model.ledger.compact_labelled("Settled").variant(crate::widgets::figure::Variant::Standard)).into_any_element(), pending.into_any_element()];
    if derived_visible {
        figures.push(card(model.reserved.standard()).into_any_element());
        figures.push(card(model.free.leading()).into_any_element());
    } else {
        figures.push(card(not_disclosed("account-reserved-free-access", "This policy shares the balance, not earmarks or free cash.", cx)).into_any_element());
    }

    let tabs = TabBar::new("account-tabs")
        .selected_index(tab)
        .on_click(cx.listener(|this, index: &usize, _, cx| {
            this.account_tab = *index;
            cx.notify();
        }))
        .children([Tab::new().label("Overview"), Tab::new().label("Earmarks"), Tab::new().label("Planned movements"), Tab::new().label("Properties")]);

    let body: AnyElement = match tab {
        0 => render_overview(app, account, model, household, derived_visible, can_earmark, cx),
        1 => render_earmarks_section(account, household, derived_visible, can_earmark, cx),
        2 => render_series_section(account, model, household, derived_visible, cx),
        _ => render_properties(account, household, cx),
    };

    v_flex()
        .id("screen-account")
        .test_support()
        .w_full()
        .gap_6()
        .child(detail_header(
            Destination::Accounts,
            Route::Accounts,
            "Accounts",
            account.name.clone(),
            Some(subtitle),
            vec![
                Button::new("reconcile-account").small().outline().label("Reconcile…").tooltip("Set the settled balance from a statement").on_click(cx.listener(move |this, _, window, cx| this.open_entry(Entry::Reconcile(id), window, cx))).into_any_element(),
                more.into_any_element(),
            ],
            cx,
        ))
        .child(lanes(figures))
        .child(tabs)
        .child(body)
        .into_any_element()
}

fn render_overview(app: &AtlasApp, account: &Account, model: &AccountModel, household: &Household, derived_visible: bool, can_earmark: bool, cx: &mut Context<AtlasApp>) -> AnyElement {
    let id = account.id;
    let active: Vec<_> = household.active_reservations_on(id).collect();
    let forecast_possible = account.kind.is_cash() && account.include_in_household && matches!(account.liquidity, atlas_core::model::Liquidity::Immediate) && !account.is_company_account();
    let sensitivity_possible = forecast_possible && app.sensitivity_boundaries().contains(&atlas_core::liquidity::Boundary::Account(id));
    let company = household.companies.iter().find(|c| matches!(account.holder, atlas_core::model::Holder::Company(cid) if cid == c.id)).map(|c| c.id);
    v_flex()
        .w_full()
        .gap_6()
        .child(
            section("account-overview-earmarks", "Active earmarks")
                .when(can_earmark, |s| s.action(Button::new("account-add-earmark").small().outline().icon(IconName::Plus).label("Add earmark…").on_click(cx.listener(move |this, _, window, cx| this.open_new_reservation_for(Some(id), window, cx)))))
                .child(if !derived_visible {
                    note("Earmarks are not disclosed under a balance-only policy.", cx).into_any_element()
                } else if active.is_empty() {
                    note("No earmarks: the whole settled balance is free unless a bank minimum applies.", cx).into_any_element()
                } else {
                    v_flex()
                        .w_full()
                        .gap_1()
                        .children(active.iter().map(|r| {
                            let rid = r.id;
                            h_flex()
                                .w_full()
                                .gap_4()
                                .items_center()
                                .child(div().w_64().flex_shrink_0().text_sm().child(r.name.clone()))
                                .child(div().w_32().flex_shrink_0().child(record::money(r.amount, cx)))
                                .child(div().w_40().flex_shrink_0().child(labels::hardness_tag(r.hardness)))
                                .child(div().flex_1().min_w_0().text_xs().text_color(cx.theme().muted_foreground).child(r.purpose.clone()))
                                .child(Button::new(SharedString::from(format!("release-{}", rid.raw()))).xsmall().ghost().label("Pay and release…").on_click(cx.listener(move |this, _, window, cx| this.open_release_reservation(rid, window, cx))))
                        }))
                        .into_any_element()
                }),
        )
        .child(
            section("account-overview-series", "Planned movements")
                .action(Button::new("account-add-series").small().outline().icon(IconName::Plus).label("Add planned movement…").on_click(cx.listener(move |this, _, window, cx| this.open_entry_with_account(Entry::Series, id, window, cx))))
                .child(render_series_rows(model, household, derived_visible, cx)),
        )
        .child(
            h_flex()
                .flex_wrap()
                .gap_2()
                .child(Button::new("account-record-transaction").small().outline().label("Record transaction…").on_click(cx.listener(move |this, _, window, cx| this.open_entry_with_account(Entry::Actual, id, window, cx))))
                .child(Button::new("account-actuals").small().ghost().label("Actuals").on_click(cx.listener(move |this, _, window, cx| this.open_actuals_for(id, window, cx))))
                .child(
                    Button::new("account-view-forecast")
                        .small()
                        .ghost()
                        .icon(IconName::ChartLine)
                        .label("View forecast")
                        .disabled(!forecast_possible && company.is_none())
                        .tooltip(if forecast_possible || company.is_some() { "The forecast path with this account selected" } else { "This account is not on a forecast path: it is excluded, not immediately liquid, or not a cash account" })
                        .on_click(cx.listener(move |this, _, _, cx| this.open_forecast_for_account(id, company, cx))),
                )
                .child(
                    Button::new("account-sensitivity")
                        .small()
                        .ghost()
                        .label("Sensitivity")
                        .disabled(!sensitivity_possible)
                        .tooltip(if sensitivity_possible { "One-at-a-time limits for this account's path" } else { "Sensitivity is available for the household and personal cash accounts" })
                        .on_click(cx.listener(move |this, _, _, cx| this.open_sensitivity_for_account(id, cx))),
                ),
        )
        .into_any_element()
}

fn render_earmarks_section(account: &Account, household: &Household, derived_visible: bool, can_earmark: bool, cx: &mut Context<AtlasApp>) -> AnyElement {
    let id = account.id;
    let theme = cx.theme();
    let reservations: Vec<_> = household.reservations.iter().filter(|r| r.account == id).collect();
    let lanes_def: [(&str, Lane); 7] = [
        ("Earmark", Lane::fixed(200.)),
        ("Amount", Lane::money(120.)),
        ("Relationship", Lane::fixed(220.)),
        ("Constraint", Lane::fixed(150.)),
        ("Purpose", Lane::flex()),
        ("Status", Lane::fixed(150.)),
        ("", Lane::fixed(180.)),
    ];
    section("account-earmarks", "Earmarks on this account")
        .when(can_earmark, |s| s.action(Button::new("account-add-earmark-2").small().outline().icon(IconName::Plus).label("Add earmark…").on_click(cx.listener(move |this, _, window, cx| this.open_new_reservation_for(Some(id), window, cx)))))
        .child(if !derived_visible {
            note("Earmarks are not disclosed under a balance-only policy.", cx).into_any_element()
        } else if reservations.is_empty() {
            note("No earmarks: the whole settled balance is free unless a bank minimum applies.", cx).into_any_element()
        } else {
            let rows = reservations
                .iter()
                .map(|r| {
                    let rid = r.id;
                    let coverage = match r.coverage {
                        Coverage::Disjoint => "Separate amount".to_string(),
                        Coverage::CoversAccountMinimum => "Includes the bank minimum".to_string(),
                        Coverage::NestedIn(outer) => format!("Inside {}", household.reservation(outer).map(|o| o.name.clone()).unwrap_or_else(|| outer.to_string())),
                    };
                    let released = r.released_on;
                    let actions: AnyElement = match released {
                        None => h_flex()
                            .gap_1()
                            .child(Button::new(SharedString::from(format!("release-{}", rid.raw()))).xsmall().ghost().label("Pay and release…").on_click(cx.listener(move |this, _, window, cx| this.open_release_reservation(rid, window, cx))))
                            .child(Button::new(SharedString::from(format!("delete-earmark-{}", rid.raw()))).xsmall().ghost().icon(IconName::Trash).tooltip("Delete earmark…").on_click(cx.listener(move |this, _, window, cx| this.confirm_delete(ObjectRef::Reservation(rid), window, cx))))
                            .into_any_element(),
                        Some(_) => div().into_any_element(),
                    };
                    record::row(
                        SharedString::from(format!("earmark-{}", rid.raw())),
                        false,
                        vec![
                            (lanes_def[0].1, record::text(r.name.clone())),
                            (lanes_def[1].1, record::money(r.amount, cx)),
                            (lanes_def[2].1, record::muted(coverage, cx)),
                            (lanes_def[3].1, h_flex().child(labels::hardness_tag(r.hardness)).into_any_element()),
                            (lanes_def[4].1, record::muted(r.purpose.clone(), cx)),
                            (lanes_def[5].1, record::muted(released.map(|d| format!("Released {}", d.format("%d %b %Y"))).unwrap_or_else(|| "Active".into()), cx)),
                            (lanes_def[6].1, actions),
                        ],
                        |_, _, _| {},
                    )
                })
                .collect();
            v_flex().w_full().gap_2().child(record::list("account-earmark-list", record::header(&lanes_def, cx), rows)).child(div().text_xs().text_color(theme.muted_foreground).child("Paying releases an earmark and lowers the settled balance; deleting removes it without recording a payment.")).into_any_element()
        })
        .into_any_element()
}

fn render_series_rows(model: &AccountModel, household: &Household, derived_visible: bool, cx: &mut Context<AtlasApp>) -> AnyElement {
    if !derived_visible {
        return note("Planned movements are not disclosed under a balance-only policy.", cx).into_any_element();
    }
    if model.series.is_empty() {
        return note("No planned movements post to or from this account.", cx).into_any_element();
    }
    let lanes_def: [(&str, Lane); 5] = [("Series", Lane::fixed(260.)), ("Direction", Lane::fixed(90.)), ("Expected", Lane::money(140.)), ("Recurrence", Lane::flex()), ("Role here", Lane::fixed(220.))];
    let rows = model
        .series
        .iter()
        .filter_map(|(sid, touch)| household.series_by_id(*sid).map(|s| (s, *touch)))
        .map(|(series, touch)| {
            let sid = series.id;
            let role = match touch {
                Touch::PostsHere => "Posts here",
                Touch::LinkedSide => "Linked other side",
                Touch::TransferTarget => "Transfer target",
            };
            record::row(
                SharedString::from(format!("account-series-{}", sid.raw())),
                false,
                vec![
                    (lanes_def[0].1, record::stack(series.name.clone(), format!("{} · {}", household.entity_name(series.entity), series.certainty.label()), cx)),
                    (lanes_def[1].1, record::muted(series.direction.label(), cx)),
                    (lanes_def[2].1, record::money(series.amount.expected(), cx)),
                    (lanes_def[3].1, record::muted(series.recurrence.describe(), cx)),
                    (lanes_def[4].1, record::muted(role, cx)),
                ],
                cx.listener(move |this, _, _, cx| this.navigate(Route::SeriesDetail(sid), cx)),
            )
        })
        .collect();
    record::list("account-series-list", record::header(&lanes_def, cx), rows).into_any_element()
}

fn render_series_section(account: &Account, model: &AccountModel, household: &Household, derived_visible: bool, cx: &mut Context<AtlasApp>) -> AnyElement {
    let id = account.id;
    section("account-series", "Planned movements touching this account")
        .action(Button::new("account-add-series-2").small().outline().icon(IconName::Plus).label("Add planned movement…").on_click(cx.listener(move |this, _, window, cx| this.open_entry_with_account(Entry::Series, id, window, cx))))
        .child(render_series_rows(model, household, derived_visible, cx))
        .into_any_element()
}

fn yes_no(value: bool) -> &'static str {
    if value { "Yes" } else { "No" }
}

fn render_properties(account: &Account, household: &Household, _cx: &mut Context<AtlasApp>) -> AnyElement {
    let policy = household.policy_for(ObjectRef::Account(account.id));
    let fees = if account.fees.is_empty() {
        "None".to_string()
    } else {
        account
            .fees
            .iter()
            .map(|f| {
                let mut parts = Vec::new();
                if let Some(fixed) = f.fixed {
                    parts.push(fixed.format());
                }
                if f.basis_points > 0 {
                    parts.push(format!("{}.{:02}%", f.basis_points / 100, f.basis_points % 100));
                }
                format!("{} ({})", f.description, parts.join(" + "))
            })
            .collect::<Vec<_>>()
            .join(" · ")
    };
    let source = match account.source_of_truth {
        atlas_core::model::SourceOfTruth::Manual => "Manual entry".to_string(),
        other => format!("{} (recorded history; not an import or connection)", other.label()),
    };
    v_flex()
        .w_full()
        .gap_6()
        .child(
            section("account-identity", "Identity and ownership").child(
                DescriptionList::new()
                    .columns(2)
                    .child(DescriptionItem::new("Institution").value(account.institution.clone()))
                    .child(DescriptionItem::new("Type").value(account.kind.label()))
                    .child(DescriptionItem::new("Economic owners and shares").value(household.holder_description(account)))
                    .child(DescriptionItem::new("Currency").value(account.currency.code().to_string()))
                    .child(DescriptionItem::new("Included in household calculations").value(yes_no(account.include_in_household)))
                    .child(DescriptionItem::new("Source of balances").value(source))
                    .child(DescriptionItem::new("Last reconciliation").value(account.last_reconciled.map(|d| d.format("%d %b %Y").to_string()).unwrap_or_else(|| "Never".into()))),
            ),
        )
        .child(
            section("account-availability", "Availability and constraints").child(
                DescriptionList::new()
                    .columns(2)
                    .child(DescriptionItem::new("Liquidity").value(account.liquidity.describe()))
                    .child(DescriptionItem::new("Bank minimum balance").value(account.minimum_balance.map(|m| m.format()).unwrap_or_else(|| "None".into())))
                    .child(DescriptionItem::new("Transfer delay").value(format!("{} day(s)", account.transfer_delay_days)))
                    .child(DescriptionItem::new("Fees").value(fees))
                    .child(DescriptionItem::new("Withdrawals permitted").value(yes_no(account.withdrawals_permitted)))
                    .child(DescriptionItem::new("Can fund").value(if account.funds_categories.is_empty() { "Any expense category".to_string() } else { account.funds_categories.join(", ") }))
                    .child(DescriptionItem::new("Tax treatment").value(if account.tax_treatment.is_empty() { "—".to_string() } else { account.tax_treatment.clone() })),
            ),
        )
        .child(
            section("account-sharing", "Sharing").child(
                DescriptionList::new()
                    .columns(2)
                    .child(DescriptionItem::new("Visibility policy").value(policy.map(|p| format!("{} (version {}, effective {})", p.preset_label(), p.version, p.effective_from.format("%d %b %Y"))).unwrap_or_else(|| "None — excluded until an owner sets one".into())))
                    .child(DescriptionItem::new("Use in calculations").value(policy.map(|p| p.calculation_access.label().to_string()).unwrap_or_else(|| "Excluded (no policy)".into()))),
            ),
        )
        .into_any_element()
}
