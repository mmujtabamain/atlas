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
    select::Select,
    tag::Tag,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::common::workspace_header;
use crate::app::AtlasApp;
use crate::models::taxes::{TaxModel, verification_tag};
use crate::nav::{Destination, Route};
use crate::widgets::grid;
use crate::widgets::record::{self, Lane};
use crate::widgets::scope;
use crate::widgets::states::{action_bar, columns, columns_leading, count_line, empty_state, fact, info_card, note, section};

fn date(d: chrono::NaiveDate) -> String {
    d.format("%d %b %Y").to_string()
}

/// One control of a filter or scope row, its label beside the control rather
/// than above it — the register form of `widgets::scope::control`, which
/// stacks them and costs the screen a line before the first row.
fn inline_control(label: &'static str, control: impl IntoElement, cx: &App) -> impl IntoElement {
    h_flex()
        .flex_shrink_0()
        .gap_2()
        .items_center()
        .child(div().flex_shrink_0().text_xs().text_color(cx.theme().muted_foreground).child(label))
        .child(control)
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
    // The per-entity figures and the two facts that qualify them as one even
    // grid across the width. They were 16 rem cards in a wrap row with the
    // facts in a second wrap row beneath, which left the right of the window
    // empty and read as two unrelated bands.
    let mut entity_cells: Vec<AnyElement> = model.by_entity.iter().map(|(_, f)| f.standard().into_any_element()).collect();
    let no_entity_figures = entity_cells.is_empty();
    entity_cells.push(fact("Creditable withholding", model.assessment.creditable_withholding.format(), cx).into_any_element());
    entity_cells.push(fact("Tax events", model.assessment.events.len().to_string(), cx).into_any_element());
    let refund = model.reserve.money().is_negative();

    v_flex()
        .id("screen-taxes")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        // The scope on one row — `Plan [Baseline]` — with the case and the
        // horizon it cannot change at the trailing edge.
        .child(
            h_flex()
                .w_full()
                .justify_between()
                .items_center()
                .gap_4()
                .child(inline_control("Plan", Select::new(&app.plan_choices.taxes).small().w(px(180.)), cx))
                .child(div().flex_shrink_0().text_xs().text_color(theme.muted_foreground).child(format!("Expected · Through {}", date(through)))),
        )
        // Which packs produced these figures, and what they are worth, stays
        // directly under the scope: it qualifies every number below it. The
        // names and their verification tags are the wrap row; the sentence
        // that qualifies them has its own full-width line beneath, because
        // long text beside a tag in a wrap row is re-measured per wrap line
        // (`docs/perf.md` §3.3).
        .child(
            v_flex()
                .w_full()
                .gap_1()
                .child(h_flex().w_full().gap_2().items_center().flex_wrap().child(div().text_xs().text_color(theme.muted_foreground).child("Packs used:")).children(packs))
                .child(div().w_full().text_xs().text_color(theme.muted_foreground).child(
                    "DEMO is fictitious; a user rule stays unverified until someone reviews its source. Every configured pack applies, and a user pack applies alongside them, never instead of them.",
                )),
        )
        .child(
            section("tax-by-entity", "Tax cash in the window")
                .description("Who owes what, as cash on its cash date. A company's figures need full disclosure.")
                // The withholding and the event count are stated whether or not
                // any entity figure is disclosed: an empty attribution is not a
                // reason to drop the two facts that qualify the window.
                .when(no_entity_figures, |this| this.child(note("No tax cash is attributed to anyone in this window.", cx)))
                .child(columns(entity_cells)),
        )
        // The reserve leads at 38 % of the width with what it is — and is not —
        // beside it, rather than as a 16 rem card with the qualification as a
        // muted sentence underneath that reads as an afterthought.
        .child(
            section("tax-reserve", format!("Incurred now, payable after {}", date(through)))
                .action(Button::new("tax-add-earmark").small().outline().icon(IconName::Plus).label("Add earmark…").on_click(cx.listener(|this, _, window, cx| this.open_tax_reserve_earmark(window, cx))))
                .child(columns_leading(
                    0.38,
                    [
                        model.reserve.leading().into_any_element(),
                        info_card(
                            "tax-reserve-basis",
                            IconName::Info,
                            if refund { "A refund to come, not spendable cash" } else { "A reserve requirement, not an earmark" },
                            if refund {
                                "Negative means a potential refund later. It is not cash you can spend and no earmark exists for it."
                            } else {
                                "This is a reserve requirement, not an earmark already created. Add one if you want the money held back."
                            },
                            cx,
                        ),
                    ],
                )),
        )
        .child(
            // Ruled off from the two figure bands: the register answers a
            // different question from the totals above it — which events, on
            // which dates, rather than how much and who owes it.
            section("tax-events", "Tax events")
                .divider(true)
                .description("Every event, including the ones payable after the horizon. Select a row for its dates, its base and its calculation.")
                // One inline row of display filters with `Clear filters` at its
                // trailing edge, and the count of what they left underneath —
                // the count answers the filters, so it reads after them.
                .child(
                    h_flex()
                        .w_full()
                        .justify_between()
                        .items_center()
                        .gap_4()
                        .child(
                            h_flex()
                                .flex_wrap()
                                .gap_5()
                                .items_center()
                                .child(inline_control("Entity", Select::new(&app.tax_entity_choice).small().w(px(180.)), cx))
                                .child(inline_control("Rule", Select::new(&app.tax_rule_choice).small().w(px(220.)), cx))
                                .child(inline_control("Payable", Select::new(&app.tax_payable_choice).small().w(px(180.)), cx)),
                        )
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
        // The screen's own commands, ruled off at its foot, with the one thing
        // every figure above it is qualified by held on the same line.
        .child(action_bar(
            "taxes-footer",
            vec![
                Button::new("taxes-packs").small().ghost().icon(IconName::BookOpen).label("Tax packs").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::TaxPacks, cx))).into_any_element(),
                Button::new("taxes-extraction").small().ghost().label("Extraction timing illustration").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Extraction, cx))).into_any_element(),
            ],
            vec![note("A planning estimate, not a filing and not tax advice.", cx).into_any_element()],
            cx,
        ))
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
            .enumerate()
            .map(|(index, rule)| {
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
                    .py_3()
                    .when(index > 0, |this| this.border_t_1().border_color(theme.border).pt_4())
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
        // The pack's name is the heading and its identity is the line under it,
        // rather than one run-on `name · version · jurisdiction` string with
        // two tags trailing it. The verification tag stays beside the name,
        // where it cannot be read as belonging to anything else.
        let name = pack.name.clone();
        let meta = format!("Version {} · {} · {} rule{}", pack.version, pack.jurisdiction, pack.rules.len(), if pack.rules.len() == 1 { "" } else { "s" });
        let verified = pack.verified;
        let muted = theme.muted_foreground;
        accordion = accordion.item(move |item| {
            item.title(
                v_flex()
                    .w_full()
                    .gap_1()
                    .child(h_flex().w_full().gap_2().items_center().child(div().text_sm().font_weight(FontWeight::MEDIUM).child(name.clone())).child(verification_tag(verified)))
                    .child(div().w_full().text_xs().text_color(muted).child(meta.clone())),
            )
            .open(is_open)
            .child(
                v_flex()
                    .w_full()
                    .gap_3()
                    .child(div().w_full().text_xs().text_color(muted).child(if verified {
                        "A configured pack; its figures are a planning estimate, not a filing."
                    } else {
                        "Unverified: nobody has reviewed its source. It applies alongside the configured packs."
                    }))
                    .children(rules),
            )
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
        // What this screen is and is not: a standing fact about every pack on
        // it, so a bordered card rather than a muted trailing sentence.
        .child(info_card(
            "packs-readonly",
            IconName::BookOpen,
            "Packs are read-only reference",
            "There is no edit, disable or delete here: a pack is the versioned text the assessment was computed from, and an old forecast still has to be explainable. A rule you add goes into a separate pack that stays marked unverified, and a scenario can add or disable a rule inside itself.",
            cx,
        ))
        .child(action_bar(
            "packs-footer",
            vec![
                Button::new("packs-events").small().ghost().icon(IconName::Gavel).label("Tax events").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Taxes, cx))).into_any_element(),
                Button::new("packs-extraction").small().ghost().label("Extraction timing illustration").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Extraction, cx))).into_any_element(),
            ],
            vec![note("The illustration uses whichever schedule you choose in it.", cx).into_any_element()],
            cx,
        ))
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
