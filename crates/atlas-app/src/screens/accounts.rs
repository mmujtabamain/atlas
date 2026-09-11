//! Accounts (§5.4, §7): every property the plan lists, the balance-definition
//! strip with chains, the reservations on the account and the series that
//! touch it — as far as the viewer's disclosure allows.

use atlas_core::authz::Viewer;
use atlas_core::ids::{AccountId, EntityRef, ObjectRef};
use atlas_core::model::{Account, Coverage, Household};
use atlas_core::Disclosure;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    description_list::{DescriptionItem, DescriptionList},
    group_box::GroupBox, h_flex,
    table::{Table, TableBody, TableCell, TableHead, TableHeader, TableRow},
    tag::Tag, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::entities::{AccountModel, EntityModels};
use crate::app::AtlasApp;
use crate::widgets::labels;
use crate::widgets::master::{master_detail, master_item, page_header};
use crate::widgets::table::{money_cell, muted_cell};

pub fn render(models: &EntityModels, household: &Household, viewer: Viewer, selected: Option<AccountId>, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let viewer_name = household.entity_name(EntityRef::Person(viewer.person));
    let selected = selected.filter(|id| models.account(*id).is_some()).or_else(|| models.accounts.first().map(|a| a.id));
    let hidden = household.accounts.len() - models.accounts.len();

    let master = v_flex().gap_1().children(models.accounts.iter().filter_map(|model| {
        let account = household.account(model.id)?;
        let id = model.id;
        Some(master_item(
            SharedString::from(format!("account-{}", id.raw())),
            account.name.clone(),
            format!("{} · {}", account.kind.label(), household.holder_description(account)),
            account.settled_balance.format(),
            selected == Some(id),
            cx.listener(move |this, _, _, cx| this.select_account(id, cx)),
            cx,
        ))
    }));

    let detail: AnyElement = match selected.and_then(|id| household.account(id).zip(models.account(id))) {
        Some((account, model)) => {
            let id = account.id;
            v_flex()
                .gap_4()
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("reconcile-account")
                                .small()
                                .outline()
                                .label("Reconcile…")
                                .tooltip("Set the settled balance from a statement (§7 last reconciliation)")
                                .on_click(cx.listener(move |this, _, window, cx| this.open_entry(crate::entry::Entry::Reconcile(id), window, cx))),
                        )
                        .child(
                            Button::new("delete-account")
                                .small()
                                .ghost()
                                .label("Delete")
                                .tooltip("Refused while series, earmarks or actuals still reference the account")
                                .on_click(cx.listener(move |this, _, window, cx| this.delete_object(ObjectRef::Account(id), window, cx))),
                        ),
                )
                .child(render_detail(account, model, household, &viewer_name, cx))
                .into_any_element()
        }
        None => div().text_color(cx.theme().muted_foreground).child("No account is visible to this viewer.").into_any_element(),
    };

    v_flex()
        .id("screen-accounts")
        .test_support()
        .gap_6()
        .child(
            h_flex().justify_between().items_start().gap_4().child(page_header(
                "Accounts",
                format!(
                    "{} of {} accounts visible to {}{} · every property of §7, every balance with its chain",
                    models.accounts.len(),
                    household.accounts.len(),
                    viewer_name,
                    if hidden > 0 { format!(" ({hidden} not disclosed, V062)") } else { String::new() }
                ),
                cx,
            ))
            .child(
                Button::new("new-account")
                    .flex_shrink_0()
                    .small()
                    .outline()
                    .icon(IconName::Plus)
                    .label("New account…")
                    .on_click(cx.listener(|this, _, window, cx| this.open_entry(crate::entry::Entry::Account, window, cx))),
            ),
        )
        .child(master_detail("accounts-master-detail", master, detail, cx))
}

fn yes_no(value: bool) -> &'static str {
    if value { "Yes" } else { "No" }
}

fn render_detail(account: &Account, model: &AccountModel, household: &Household, viewer_name: &str, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let derived_visible = matches!(model.disclosure, Disclosure::Full | Disclosure::SelectedFields);
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
    let reservations: Vec<_> = household.reservations.iter().filter(|r| r.account == account.id).collect();

    let mut detail = v_flex()
        .id(SharedString::from(format!("account-detail-{}", account.id.raw())))
        .test_support()
        .gap_6()
        .child(
            h_flex()
                .items_center()
                .gap_3()
                .flex_wrap()
                .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(account.name.clone()))
                .child(Tag::secondary().xsmall().outline().child(account.kind.label()))
                .child(labels::disclosure_tag(model.disclosure))
                .when(account.is_company_account(), |this| this.child(Tag::secondary().xsmall().outline().child("business cash — not household cash (§8.5)"))),
        )
        .child(
            GroupBox::new().id("account-balances").title("Balance definitions (§6)").child(
                v_flex()
                    .gap_4()
                    .child(
                        h_flex()
                            .flex_wrap()
                            .gap_8()
                            .child(div().min_w_48().child(model.ledger.figure(viewer_name, false)))
                            .child(
                                div().min_w_48().child(
                                    v_flex()
                                        .gap_1()
                                        .child(div().text_xs().text_color(theme.muted_foreground).child("Pending, unsettled"))
                                        .child(
                                            div()
                                                .text_xl()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .font_family(theme.mono_font_family.clone())
                                                .child(account.pending_balance.format()),
                                        )
                                        .child(div().text_xs().text_color(theme.muted_foreground).child("posted, not yet settled — never spendable (M01)")),
                                ),
                            )
                            .when(derived_visible, |this| {
                                this.child(div().min_w_48().child(model.reserved.figure(viewer_name, false)))
                                    .child(div().min_w_48().child(model.free.figure(viewer_name, true)))
                            })
                            .when(!derived_visible, |this| {
                                this.child(
                                    div().min_w_48().text_sm().text_color(theme.muted_foreground).child("Reserved and free cash are not disclosed under a balance-only policy (§7.2)."),
                                )
                            }),
                    ),
            ),
        )
        .child(
            GroupBox::new().id("account-properties").title("Properties (§7)").child(
                DescriptionList::new()
                    .columns(2)
                    .child(DescriptionItem::new("Institution").value(account.institution.clone()))
                    .child(DescriptionItem::new("Account type").value(account.kind.label()))
                    .child(DescriptionItem::new("Economic owner(s) and shares").value(household.holder_description(account)))
                    .child(DescriptionItem::new("Household inclusion").value(yes_no(account.include_in_household)))
                    .child(DescriptionItem::new("Currency").value(account.currency.code().to_string()))
                    .child(DescriptionItem::new("Liquidity classification").value(account.liquidity.describe()))
                    .child(DescriptionItem::new("Minimum desired/required balance").value(account.minimum_balance.map(|m| m.format()).unwrap_or_else(|| "None".into())))
                    .child(DescriptionItem::new("Transfer delay").value(format!("{} day(s)", account.transfer_delay_days)))
                    .child(DescriptionItem::new("Fees").value(fees))
                    .child(DescriptionItem::new("Tax treatment").value(account.tax_treatment.clone()))
                    .child(DescriptionItem::new("Source of truth").value(account.source_of_truth.label()))
                    .child(DescriptionItem::new("Last reconciliation").value(account.last_reconciled.map(|d| d.format("%d %b %Y").to_string()).unwrap_or_else(|| "never".into())))
                    .child(DescriptionItem::new("Withdrawals permitted").value(yes_no(account.withdrawals_permitted)))
                    .child(DescriptionItem::new("Can fund").value(if account.funds_categories.is_empty() { "Any expense category".to_string() } else { account.funds_categories.join(", ") }))
                    .child(DescriptionItem::new("Visibility policy").value(policy.map(|p| format!("{} (v{}, effective {})", p.preset_label(), p.version, p.effective_from.format("%d %b %Y"))).unwrap_or_else(|| "none — fails closed (F162)".into())))
                    .child(DescriptionItem::new("Calculation access (§7.3)").value(policy.map(|p| p.calculation_access.label().to_string()).unwrap_or_else(|| "Excluded (no policy)".into()))),
            ),
        );

    if derived_visible {
        detail = detail
            .child(
                GroupBox::new().id("account-reservations").title("Reservations on this account (§17)").child(if reservations.is_empty() {
                    div().text_sm().text_color(theme.muted_foreground).child("No earmarks; the whole settled balance is free unless a bank minimum applies.").into_any_element()
                } else {
                    Table::new()
                        .child(
                            TableHeader::new().child(
                                TableRow::new()
                                    .child(TableHead::new().w_48().child("Reservation"))
                                    .child(TableHead::new().w_32().text_right().child("Amount"))
                                    .child(TableHead::new().w_56().child("Coverage"))
                                    .child(TableHead::new().w_48().child("Hardness"))
                                    .child(TableHead::new().child("Purpose"))
                                    .child(TableHead::new().w_32().child("Status")),
                            ),
                        )
                        .child(TableBody::new().children(reservations.iter().enumerate().map(|(index, r)| {
                            let coverage = match r.coverage {
                                Coverage::Disjoint => "Disjoint — additive".to_string(),
                                Coverage::CoversAccountMinimum => "Includes the bank minimum — not deducted twice".to_string(),
                                Coverage::NestedIn(outer) => format!("Nested in {}", household.reservation(outer).map(|o| o.name.clone()).unwrap_or_else(|| outer.to_string())),
                            };
                            TableRow::new()
                                .when(index % 2 == 1, |row| row.bg(theme.table_even))
                                .child(TableCell::new().w_48().child(r.name.clone()))
                                .child(money_cell(r.amount, cx).w_32())
                                .child(muted_cell(coverage, cx).w_56())
                                .child(muted_cell(r.hardness.label(), cx).w_48())
                                .child(muted_cell(r.purpose.clone(), cx))
                                .child(muted_cell(r.released_on.map(|d| format!("released {}", d.format("%d %b %Y"))).unwrap_or_else(|| "active".into()), cx).w_32())
                        })))
                        .into_any_element()
                }),
            )
            .child(
                GroupBox::new().id("account-series").title("Series touching this account (§9)").child(if model.series.is_empty() {
                    div().text_sm().text_color(theme.muted_foreground).child("No planned series post to or from this account.").into_any_element()
                } else {
                    Table::new()
                        .child(
                            TableHeader::new().child(
                                TableRow::new()
                                    .child(TableHead::new().w_64().child("Series"))
                                    .child(TableHead::new().w_24().child("Direction"))
                                    .child(TableHead::new().w_40().text_right().child("Expected"))
                                    .child(TableHead::new().child("Recurrence"))
                                    .child(TableHead::new().w_40().child("Certainty"))
                                    .child(TableHead::new().w_56().child("Role for this account")),
                            ),
                        )
                        .child(TableBody::new().children(model.series.iter().enumerate().filter_map(|(index, (id, touch))| {
                            let series = household.series.iter().find(|s| s.id == *id)?;
                            Some(
                                TableRow::new()
                                    .when(index % 2 == 1, |row| row.bg(theme.table_even))
                                    .child(TableCell::new().w_64().child(series.name.clone()))
                                    .child(muted_cell(series.direction.label(), cx).w_24())
                                    .child(money_cell(series.amount.expected(), cx).w_40())
                                    .child(muted_cell(series.recurrence.describe(), cx))
                                    .child(TableCell::new().w_40().child(labels::certainty_tag(series.certainty)))
                                    .child(muted_cell(touch.label(), cx).w_56()),
                            )
                        })))
                        .into_any_element()
                }),
            );
    }
    detail
}
