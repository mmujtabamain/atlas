//! Rules & taxes / Taxes: when the configured taxes move cash, who owes
//! them, and what falls due after the horizon; and Tax packs: the exact
//! versioned rules, with a clearly unverified user rule addable.

use atlas_core::ids::EntityRef;
use atlas_core::model::{Household, TaxKind};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    accordion::Accordion,
    button::{Button, ButtonVariants as _},
    description_list::{DescriptionItem, DescriptionList},
    h_flex,
    tag::Tag,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::common::workspace_header;
use crate::app::AtlasApp;
use crate::models::taxes::{TaxModel, verification_tag};
use crate::nav::{Destination, Route};
use crate::widgets::figure::card;
use crate::widgets::grid;
use crate::widgets::record::{self, Lane};
use crate::widgets::scope;
use crate::widgets::states::{count_line, empty_state, fact, lanes, note, section};

fn date(d: chrono::NaiveDate) -> String {
    d.format("%d %b %Y").to_string()
}

pub fn render_taxes(app: &AtlasApp, model: &TaxModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    grid::sync(&app.grids.tax_events, &model.event_rows, cx);
    let header = workspace_header(Destination::RulesTaxes, Route::Taxes, vec![], cx);
    let packs: Vec<AnyElement> = model
        .packs
        .iter()
        .map(|p| h_flex().gap_1().items_center().child(div().text_xs().child(p.name.clone())).child(verification_tag(p.verified)).into_any_element())
        .collect();
    let entity_row = scope::selected(&app.tax_entity_choice, cx);
    let entity_filter = if entity_row == 0 { None } else { app.tax_entities.get(entity_row - 1).copied() };
    let rule_row = scope::selected(&app.tax_rule_choice, cx);
    let payable_row = scope::selected(&app.tax_payable_choice, cx);
    let through = model.assessment.through;
    let events: Vec<&atlas_core::tax::TaxEvent> = model
        .assessment
        .events
        .iter()
        .filter(|e| entity_filter.is_none_or(|f| e.entity == f))
        .filter(|e| rule_row == 0 || app.tax_rules.get(rule_row - 1).is_some_and(|r| e.rule == *r))
        .filter(|e| match payable_row {
            1 => e.cash_date <= through,
            2 => e.cash_date > through,
            _ => true,
        })
        .collect();
    let filtered = entity_filter.is_some() || rule_row > 0 || payable_row > 0;
    let lanes_def: [(&str, Lane); 6] = [("Cash date", Lane::fixed(120.)), ("Entity / account", Lane::fixed(230.)), ("Rule / base", Lane::flex()), ("Base amount", Lane::money(140.)), ("Tax", Lane::money(130.)), ("Kind", Lane::fixed(220.))];
    let expanded = app.tax_event_expanded;
    let rows: Vec<AnyElement> = events
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let after = e.cash_date > through;
            let is_open = expanded == Some(i);
            let row = record::row(
                SharedString::from(format!("tax-event-{i}")),
                is_open,
                vec![
                    (lanes_def[0].1, record::text(date(e.cash_date))),
                    (lanes_def[1].1, record::stack(household.entity_name(e.entity), household.account(e.account).map(|a| a.name.clone()).unwrap_or_default(), cx)),
                    (lanes_def[2].1, record::stack(e.rule_name.clone(), e.base_label.clone(), cx)),
                    (lanes_def[3].1, record::money(e.base_amount, cx)),
                    (lanes_def[4].1, record::money(e.amount, cx)),
                    (lanes_def[5].1, h_flex().gap_1().items_center().child(Tag::secondary().xsmall().outline().child(e.kind.label())).when(after, |this| this.child(Tag::warning().xsmall().outline().child("After the horizon"))).into_any_element()),
                ],
                move |_, _, cx| {
                    crate::app::with_app(cx, |app, cx| {
                        app.tax_event_expanded = if app.tax_event_expanded == Some(i) { None } else { Some(i) };
                        cx.notify();
                    })
                },
            );
            if !is_open {
                return row.into_any_element();
            }
            v_flex()
                .w_full()
                .child(row)
                .child(
                    v_flex()
                        .w_full()
                        .gap_2()
                        .px_3()
                        .py_2()
                        .child(
                            DescriptionList::new()
                                .columns(2)
                                .child(DescriptionItem::new("Accrued").value(date(e.accrual_date)))
                                .child(DescriptionItem::new("Cash moves").value(format!("{}{}", date(e.cash_date), if after { " — after the horizon, so it is a reserve requirement, not a posting in this window" } else { "" })))
                                .child(DescriptionItem::new("Rule").value(format!("{} ({})", e.rule_name, e.pack)))
                                .child(DescriptionItem::new("Base").value(format!("{} — {}", e.base_label, e.base_amount.format())))
                                .child(DescriptionItem::new("Tax").value(e.amount.format()))
                                .child(DescriptionItem::new("Kind").value(e.kind.label())),
                        )
                        .child(crate::widgets::explain::render_top_block(&e.chain, cx)),
                )
                .into_any_element()
        })
        .collect();
    let theme = cx.theme();
    let entity_figures: Vec<AnyElement> = model.by_entity.iter().map(|(_, f)| card(f.standard()).into_any_element()).collect();

    v_flex()
        .id("screen-taxes")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(scope::bar(
            vec![
                scope::select("Plan", &app.plan_choices.taxes, px(200.), cx).into_any_element(),
                scope::fixed("Case", "Expected", cx).into_any_element(),
                scope::fixed("Through", date(through), cx).into_any_element(),
            ],
            Some("Every configured pack applies; a user pack applies alongside them, never instead of them.".into()),
            cx,
        ))
        .child(h_flex().gap_2().items_center().flex_wrap().child(div().text_xs().text_color(theme.muted_foreground).child("Packs used:")).children(packs).child(div().text_xs().text_color(theme.muted_foreground).child("DEMO is fictitious; a user rule stays unverified until someone reviews its source.")))
        .child(
            section("tax-by-entity", "Tax cash in the window")
                .description("Who owes what, as cash on its cash date. A company's figures need full disclosure.")
                .child(if entity_figures.is_empty() { note("No tax cash is attributed to anyone in this window.", cx).into_any_element() } else { lanes(entity_figures).into_any_element() })
                .child(lanes([
                    fact("Creditable withholding", model.assessment.creditable_withholding.format(), cx).into_any_element(),
                    fact("Tax events", model.assessment.events.len().to_string(), cx).into_any_element(),
                ])),
        )
        .child(
            section("tax-reserve", format!("Incurred now, payable after {}", date(through)))
                .action(Button::new("tax-add-earmark").small().outline().icon(IconName::Plus).label("Add earmark…").on_click(cx.listener(|this, _, window, cx| this.open_tax_reserve_earmark(window, cx))))
                .child(lanes([card(model.reserve.leading()).into_any_element()]))
                .child(note(if model.reserve.money().is_negative() { "Negative means a potential refund later. It is not cash you can spend and no earmark exists for it." } else { "This is a reserve requirement, not an earmark already created. Add one if you want the money held back." }, cx)),
        )
        .child(
            section("tax-events", "Tax events")
                .description("Every event, including the ones payable after the horizon. Select a row for its dates, its base and its calculation.")
                .child(
                    h_flex()
                        .flex_wrap()
                        .gap_3()
                        .items_end()
                        .child(scope::select("Entity", &app.tax_entity_choice, px(200.), cx))
                        .child(scope::select("Rule", &app.tax_rule_choice, px(240.), cx))
                        .child(scope::select("Payable", &app.tax_payable_choice, px(200.), cx))
                        .child(Button::new("tax-clear-filters").small().ghost().label("Clear filters").disabled(!filtered).on_click(cx.listener(|this, _, window, cx| this.clear_tax_filters(window, cx)))),
                )
                .child(count_line(events.len(), model.assessment.events.len(), "tax events", cx))
                .child(if model.assessment.events.is_empty() {
                    empty_state("tax-events-empty", "No tax events", "No configured rule matched a posting in this window. That is this household's packs only, not a statement about the law.", Some(Button::new("tax-empty-packs").small().outline().label("Tax packs").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::TaxPacks, cx))).into_any_element()), cx)
                } else if rows.is_empty() {
                    empty_state("tax-events-no-match", "No matches", "No tax event matches these display filters; the assessment above is unfiltered.", Some(Button::new("tax-clear-filters-2").small().ghost().label("Clear filters").on_click(cx.listener(|this, _, window, cx| this.clear_tax_filters(window, cx))).into_any_element()), cx)
                } else {
                    v_flex().w_full().gap_0p5().child(record::header(&lanes_def, cx)).children(rows).into_any_element()
                }),
        )
        .child(
            h_flex()
                .gap_2()
                .child(Button::new("taxes-packs").small().ghost().icon(IconName::BookOpen).label("Tax packs").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::TaxPacks, cx))))
                .child(Button::new("taxes-extraction").small().ghost().label("Extraction timing illustration").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Extraction, cx))))
                .child(div().text_xs().text_color(theme.muted_foreground).child("A planning estimate, not a filing and not tax advice.")),
        )
        .into_any_element()
}

pub fn render_packs(app: &AtlasApp, model: &TaxModel, cx: &mut Context<AtlasApp>) -> AnyElement {
    let header = workspace_header(
        Destination::RulesTaxes,
        Route::TaxPacks,
        vec![Button::new("add-tax-rule").small().outline().icon(IconName::Plus).label("Add tax rule…").on_click(cx.listener(|this, _, window, cx| this.open_new_tax_rule(window, cx))).into_any_element()],
        cx,
    );
    let theme = cx.theme();
    let open = app.tax_pack_open.clone();
    let mut accordion = Accordion::new("tax-packs").bordered(true).multiple(true).on_toggle_click(cx.listener(|this, open: &[usize], _, cx| {
        this.tax_pack_open = open.to_vec();
        cx.notify();
    }));
    for (i, pack) in model.packs.iter().enumerate() {
        let is_open = if open.is_empty() { i == 0 } else { open.contains(&i) };
        let rules: Vec<AnyElement> = pack
            .rules
            .iter()
            .map(|rule| {
                let brackets: Option<AnyElement> = match &rule.kind {
                    TaxKind::AnnualBrackets { brackets } => {
                        let lanes_def: [(&str, Lane); 3] = [("From", Lane::money(160.)), ("Up to", Lane::money(160.)), ("Rate", Lane::flex())];
                        let rows: Vec<_> = brackets
                            .iter()
                            .enumerate()
                            .map(|(bi, b)| {
                                record::row(
                                    SharedString::from(format!("bracket-{}-{bi}", rule.id.raw())),
                                    false,
                                    vec![
                                        (lanes_def[0].1, record::money(b.lower, cx)),
                                        (lanes_def[1].1, match b.upper {
                                            Some(u) => record::money(u, cx),
                                            None => record::muted("no upper limit", cx),
                                        }),
                                        (lanes_def[2].1, record::text(format!("{}.{:02}%", b.rate_basis_points / 100, b.rate_basis_points % 100))),
                                    ],
                                    |_, _, _| {},
                                )
                            })
                            .collect();
                        Some(record::list(SharedString::from(format!("brackets-{}", rule.id.raw())), record::header(&lanes_def, cx), rows).into_any_element())
                    }
                    _ => None,
                };
                v_flex()
                    .w_full()
                    .gap_2()
                    .py_2()
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(rule.name.clone()))
                    .child(
                        DescriptionList::new()
                            .columns(2)
                            .child(DescriptionItem::new("Tax type").value(rule.tax_type.clone()))
                            .child(DescriptionItem::new("Categories").value(if rule.categories.is_empty() { "Every category".to_string() } else { rule.categories.join(", ") }))
                            .child(DescriptionItem::new("Charged").value(rule.describe_kind()))
                            .child(DescriptionItem::new("Timing").value(rule.timing.label()))
                            .child(DescriptionItem::new("Effective").value(format!("{} – {}", date(rule.effective_from), rule.effective_to.map(date).unwrap_or_else(|| "open".into()))))
                            .child(DescriptionItem::new("Scope").value(rule.scope.clone()))
                            .child(DescriptionItem::new("Source").value(rule.source.clone()))
                            .child(DescriptionItem::new("Explanation").value(rule.explanation.clone())),
                    )
                    .children(brackets)
                    .into_any_element()
            })
            .collect();
        let title = format!("{} · version {} · {}", pack.name, pack.version, pack.jurisdiction);
        let verified = pack.verified;
        let count = pack.rules.len();
        accordion = accordion.item(move |item| {
            item.title(h_flex().gap_2().items_center().child(div().text_sm().child(title.clone())).child(verification_tag(verified)).child(div().text_xs().child(format!("{count} rule{}", if count == 1 { "" } else { "s" }))))
                .open(is_open)
                .child(v_flex().w_full().gap_3().child(div().text_xs().child(if verified { "A configured pack; its figures are a planning estimate, not a filing." } else { "Unverified: nobody has reviewed its source. It applies alongside the configured packs." })).children(rules))
        });
    }

    v_flex()
        .id("screen-tax-packs")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(count_line(model.packs.len(), model.packs.len(), "tax packs", cx))
        .child(if model.packs.is_empty() {
            empty_state("packs-empty", "No tax packs configured", "Without a pack no tax is computed. Add your own rule; nothing is selected for you from a currency or a country.", Some(Button::new("packs-add-first").small().outline().icon(IconName::Plus).label("Add tax rule…").on_click(cx.listener(|this, _, window, cx| this.open_new_tax_rule(window, cx))).into_any_element()), cx)
        } else {
            accordion.into_any_element()
        })
        .child(note("Rules are read-only reference: there is no edit, disable or delete here. A scenario can add or disable a rule inside itself.", cx))
        .child(
            h_flex()
                .gap_2()
                .child(Button::new("packs-events").small().ghost().icon(IconName::Gavel).label("Tax events").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Taxes, cx))))
                .child(Button::new("packs-extraction").small().ghost().label("Extraction timing illustration").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Extraction, cx))))
                .child(div().text_xs().text_color(theme.muted_foreground).child("The illustration uses whichever schedule you choose in it.")),
        )
        .into_any_element()
}

impl AtlasApp {
    pub fn clear_tax_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for choice in [&self.tax_entity_choice, &self.tax_rule_choice, &self.tax_payable_choice] {
            choice.update(cx, |s, cx| s.set_selected_index(Some(gpui_kit::component::IndexPath::default()), window, cx));
        }
        self.tax_event_expanded = None;
        cx.notify();
    }

    /// The earmark form for the post-horizon tax reserve: a personal account
    /// and the amount proposed, for the person to review.
    pub fn open_tax_reserve_earmark(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let amount = self.taxes().map(|m| m.reserve.money());
        self.open_new_reservation_for(None, window, cx);
        if let Some(amount) = amount
            && amount.is_positive()
        {
            self.reservation_form.amount.update(cx, |s, cx| s.set_value(amount.format(), window, cx));
            self.reservation_form.name.update(cx, |s, cx| s.set_value("Tax payable after the horizon", window, cx));
        }
    }

    /// Taxes with the entity display filter set from another screen.
    pub fn apply_tax_entity_filter(&mut self, entity: EntityRef, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(row) = self.tax_entities.iter().position(|e| *e == entity) {
            Self::set_choice(&self.tax_entity_choice, row + 1, window, cx);
        }
        cx.notify();
    }
}
