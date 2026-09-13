//! Decisions / Purchase: the four-step builder (what, down payment and its
//! funding, recurring payment, other costs) and the result — verdict and
//! chart first, then Affordability, Funding, Combinations, Goals and Basis.
//! A finite, deterministic search over the person's own inputs; not advice.

use atlas_core::decision::{AffordabilityMetric, Decision, ExtractionMethod, Objective};
use atlas_core::model::Household;
use atlas_core::vocab::{Certainty, MoneyClass};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _,
    alert::Alert,
    breadcrumb::{Breadcrumb, BreadcrumbItem},
    button::{Button, ButtonVariants as _, DropdownButton},
    chart::AreaChart,
    checkbox::Checkbox,
    date_picker::DatePicker,
    description_list::{DescriptionItem, DescriptionList},
    form::{Field, Form},
    h_flex,
    input::Input,
    menu::PopupMenuItem,
    radio::RadioGroup,
    select::Select,
    stepper::{Stepper, StepperItem},
    tag::Tag,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::common::{confirm_danger, detail_header, workspace_header};
use crate::app::AtlasApp;
use crate::decision_entry::DecisionForm;
use crate::entry::Entry;
use crate::models::decisions::DecisionPoint;
use crate::nav::{Destination, Route};
use crate::widgets::chart::{self, Legend, PathCommand};
use crate::widgets::copy::copy_button;
use crate::widgets::explain;
use crate::widgets::grid;
use crate::widgets::labels;
use crate::widgets::record::{self, Lane};
use crate::widgets::statement::{self, Line, Statement};
use crate::widgets::states::{action_bar, columns, empty_state, fact, hairline, info_card, note, section};

const STEP_TITLES: [&str; 4] = ["Purchase", "Down payment", "Recurring payment", "Other costs"];

fn date(d: chrono::NaiveDate) -> String {
    d.format("%d %b %Y").to_string()
}

// ----- Builder -----------------------------------------------------------------

pub fn render_purchase(app: &AtlasApp, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let step = app.decision_step.min(3);
    let form = &app.decision_form;
    let header = workspace_header(
        Destination::Decisions,
        Route::Purchase,
        vec![
            DropdownButton::new("purchase-more")
                .small()
                .button(Button::new("purchase-more-button").small().ghost().label("More"))
                .dropdown_menu(|menu, _, _| menu.item(PopupMenuItem::new("Start over…").on_click(|_, window, cx| crate::app::with_app(cx, |app, cx| app.confirm_reset_decision(window, cx)))))
                .into_any_element(),
        ],
        cx,
    );
    let viewer_name = household.entity_name(atlas_core::ids::EntityRef::Person(app.viewer().person));
    let has_result = app.decision.is_some();
    // Step 2's source and route tables need the whole width, so they render
    // under the two columns rather than inside the left one.
    let (step_body, wide_body): (AnyElement, Option<AnyElement>) = match step {
        0 => (render_step_purchase(form, household, cx), None),
        1 => {
            let (amounts, tables) = render_step_down_payment(app, form, household, cx);
            (amounts, Some(tables))
        }
        2 => (render_step_recurring(app, form, household, cx), None),
        _ => (render_step_other(form, cx), None),
    };
    let summary = render_summary(app, household, cx);
    let theme = cx.theme();
    let stale = has_result && app.decision_result_stale;
    v_flex()
        .id("screen-purchase")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(
            h_flex()
                .w_full()
                .justify_between()
                .items_start()
                .gap_4()
                .child(v_flex().gap_0p5().child(div().text_lg().font_weight(FontWeight::MEDIUM).child("Build a purchase")).child(div().text_xs().text_color(theme.muted_foreground).child(format!("Draft for {viewer_name} · kept for this session only, not saved as a scenario or a transaction"))))
                .when(has_result, |this| {
                    this.child(Button::new("purchase-open-result").small().outline().label(if stale { "Result (out of date)" } else { "Open result" }).on_click(cx.listener(|this, _, _, cx| this.navigate(Route::PurchaseResult, cx))))
                }),
        )
        .child(
            Stepper::new("purchase-stepper")
                .selected_index(step)
                .items(STEP_TITLES.iter().enumerate().map(|(index, title)| StepperItem::new().child(div().text_sm().child(format!("{}. {}", index + 1, title)))))
                .on_click(cx.listener(|this, target: &usize, window, cx| {
                    // Completed steps are navigable; later ones are not skipped.
                    if *target <= this.decision_step {
                        this.go_to_decision_step(*target, window, cx);
                    } else {
                        window.push_notification("Finish the current step first; Next validates it.", cx);
                    }
                })),
        )
        .child(h_flex().w_full().gap_8().items_start().child(v_flex().flex_1().min_w_0().child(step_body)).child(summary))
        .children(wide_body)
        // The step's commands in one bar at the foot: leaving on the left,
        // moving through the flow on the right, with the step that follows
        // named beside the command that commits to it.
        .child(action_bar(
            "purchase-actions",
            vec![Button::new("purchase-cancel").outline().label("Cancel").on_click(cx.listener(|this, _, window, cx| this.cancel_purchase(window, cx))).into_any_element()],
            vec![
                div().text_xs().text_color(theme.muted_foreground).child(if step < 3 { format!("Next: {}", STEP_TITLES[step + 1]) } else { "Next: the result".to_string() }).into_any_element(),
                Button::new("purchase-back").ghost().label("Back").disabled(step == 0).on_click(cx.listener(move |this, _, window, cx| this.go_to_decision_step(step.saturating_sub(1), window, cx))).into_any_element(),
                if step < 3 {
                    Button::new("purchase-next").primary().label("Next").on_click(cx.listener(move |this, _, window, cx| this.go_to_decision_step(step + 1, window, cx))).into_any_element()
                } else {
                    Button::new("purchase-calculate").primary().icon(IconName::Play).label("Calculate purchase").on_click(cx.listener(|this, _, window, cx| this.calculate_purchase(window, cx))).into_any_element()
                },
            ],
            cx,
        ))
        .into_any_element()
}

fn render_summary(app: &AtlasApp, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let plan = &app.decision_plan;
    let theme = cx.theme();
    let mut items: Vec<(String, String)> = Vec::new();
    if !plan.name.is_empty() && app.decision_step >= 1 || app.decision_step == 0 && !plan.name.is_empty() {
        items.push(("What".into(), plan.name.clone()));
    }
    if plan.price.is_positive() {
        items.push(("Price".into(), plan.price.format()));
        items.push(("Purchase date".into(), date(plan.purchase_on)));
        items.push(("Reserve to keep".into(), plan.reserve.format()));
        items.push(("Compare months".into(), format!("{} – {}", plan.window_from.format("%b %Y"), plan.window_to.format("%b %Y"))));
        items.push(("Objective".into(), plan.objective.label().to_string()));
    }
    if app.decision_step >= 2 {
        items.push(("Down payment".into(), plan.down_payment.format()));
        let sources: Vec<String> = plan.sources.iter().filter(|s| s.allowed).filter_map(|s| household.account(s.account)).map(|a| a.name.clone()).collect();
        let routes: Vec<String> = plan.company_routes.iter().filter(|r| r.allowed).filter_map(|r| household.company(r.company)).map(|c| c.name.clone()).collect();
        items.push(("Funding".into(), sources.into_iter().chain(routes).collect::<Vec<_>>().join(", ")));
    }
    if app.decision_step >= 3 {
        items.push((
            "Recurring".into(),
            match &plan.financing {
                Some(f) => format!("{} of {} over {} months at {}.{:02}%", atlas_core::decision::monthly_payment(plan.financed(), f.months, f.annual_rate_basis_points).format(), plan.financed().format(), f.months, f.annual_rate_basis_points / 100, f.annual_rate_basis_points % 100),
                None => "Paid in full".into(),
            },
        ));
    }
    v_flex()
        .w(px(300.))
        .flex_shrink_0()
        .gap_2()
        .p_4()
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Purchase summary"))
        .child(div().text_xs().text_color(theme.muted_foreground).child("Only values that passed their step."))
        .child(if items.is_empty() {
            div().text_xs().text_color(theme.muted_foreground).child("Nothing entered yet.").into_any_element()
        } else {
            DescriptionList::new().columns(1).children(items.into_iter().map(|(k, v)| DescriptionItem::new(k).value(v))).into_any_element()
        })
        // The scope of the draft is a different kind of statement from the
        // values above it, so it is ruled off rather than run on.
        .child(hairline(cx))
        .child(div().text_xs().text_color(theme.muted_foreground).child(format!("Household · {}", household.base_currency.code())))
        .child(div().text_xs().text_color(theme.muted_foreground).child(format!("Evaluated through {}", date(app.horizon()))))
        .child(div().text_xs().text_color(theme.muted_foreground).child("Nothing is paid, borrowed or saved by building this plan."))
        .into_any_element()
}

fn render_step_purchase(form: &DecisionForm, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let draft = form.draft.clone();
    let objective = draft.read(cx).objective;
    // The two-column grid with the qualifications as each field's own
    // description: `What` and the objective take the whole row because their
    // controls do, the money and date pairs share one, and the sentences that
    // used to sit under the whole step now sit under the field they qualify.
    section("purchase-step-1", "What are you buying?")
        .description(format!("The price, when, and the reserve the household must keep throughout. Balances as of {}.", date(household.as_of)))
        .child(
            Form::vertical()
                .columns(2)
                .child(Field::new().label("What").required(true).col_span(2).child(Input::new(&form.name).id("decision-name")))
                .child(Field::new().label("Total price").required(true).child(Input::new(&form.price).id("decision-price")))
                .child(Field::new().label("Purchase date").required(true).child(DatePicker::new(&form.purchase_on)))
                .child(Field::new().label("Household reserve to keep").description("Kept throughout the tested path; what you enter, never inferred as enough.").child(Input::new(&form.reserve).id("decision-reserve")))
                .child(Field::new().label("Earliest purchase month to compare").description("Every month in the window is evaluated as well; at most 24 of them.").col_start(1).child(DatePicker::new(&form.window_from)))
                .child(Field::new().label("Latest purchase month to compare").child(DatePicker::new(&form.window_to)))
                .child(
                    Field::new().label("What matters most").col_span(2).child(
                        // Across the row rather than down it: four objectives
                        // stacked cost the step four lines before its footer.
                        RadioGroup::horizontal("decision-objective")
                            .children(Objective::ALL.iter().map(|o| o.label()))
                            .selected_index(Some(objective))
                            .on_change(move |index, _, cx| {
                                draft.update(cx, |d, cx| {
                                    d.objective = *index;
                                    cx.notify();
                                })
                            }),
                    ),
                ),
        )
        .into_any_element()
}

const SOURCE_LANES: [(&str, Lane); 5] = [("Personal source", Lane::fixed(240.)), ("Free cash / hard floor", Lane::fixed(230.)), ("Allow", Lane::fixed(60.)), ("Never below", Lane::fixed(180.)), ("Order", Lane::fixed(120.))];
const ROUTE_LANES: [(&str, Lane); 5] = [("Company route", Lane::fixed(240.)), ("Ceiling before costs", Lane::fixed(230.)), ("Allow", Lane::fixed(60.)), ("Method", Lane::fixed(200.)), ("Receive in", Lane::fixed(240.))];

fn render_step_down_payment(app: &AtlasApp, form: &DecisionForm, household: &Household, cx: &mut Context<AtlasApp>) -> (AnyElement, AnyElement) {
    let draft = form.draft.read(cx).clone();
    let draft_entity = form.draft.clone();
    let entities = app.entities();
    let count = form.source_floors.len();
    let mut order = 0usize;
    let source_rows: Vec<_> = form
        .source_floors
        .iter()
        .enumerate()
        .map(|(index, (account, floor))| {
            let name = household.account(*account).map(|a| a.name.clone()).unwrap_or_default();
            let allowed = draft.source_allowed.get(index).copied().unwrap_or(true);
            if allowed {
                order += 1;
            }
            let position = order;
            let hard_floor = atlas_core::liquidity::hard_floor_for_account(household, *account).ok();
            let figures = entities
                .and_then(|m| m.account(*account))
                .map(|a| format!("{} free · floor {}", a.free.money().format(), hard_floor.map(|f| f.format()).unwrap_or_else(|| "—".into())))
                .unwrap_or_else(|| "Not disclosed".into());
            let d = draft_entity.clone();
            record::row(
                SharedString::from(format!("source-{}", account.raw())),
                false,
                vec![
                    (SOURCE_LANES[0].1, record::text(name)),
                    (SOURCE_LANES[1].1, record::muted(figures, cx)),
                    (SOURCE_LANES[2].1, h_flex().child(Checkbox::new(ElementId::Name(format!("decision-source-{}", account.raw()).into())).checked(allowed).on_change(move |v, _, cx| {
                        d.update(cx, |d, cx| {
                            if let Some(slot) = d.source_allowed.get_mut(index) {
                                *slot = *v;
                            }
                            cx.notify();
                        })
                    })).into_any_element()),
                    (SOURCE_LANES[3].1, div().w(px(160.)).child(Input::new(floor).small().id(ElementId::Name(format!("decision-floor-{}", account.raw()).into()))).into_any_element()),
                    (
                        SOURCE_LANES[4].1,
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(record::muted(if allowed { format!("{position}.") } else { "—".into() }, cx))
                            .child(Button::new(SharedString::from(format!("source-up-{}", account.raw()))).xsmall().ghost().compact().icon(IconName::ChevronUp).disabled(index == 0).tooltip("Try earlier").on_click(cx.listener(move |this, _, _, cx| this.move_funding_source(index, true, cx))))
                            .child(Button::new(SharedString::from(format!("source-down-{}", account.raw()))).xsmall().ghost().compact().icon(IconName::ChevronDown).disabled(index + 1 >= count).tooltip("Try later").on_click(cx.listener(move |this, _, _, cx| this.move_funding_source(index, false, cx))))
                            .into_any_element(),
                    ),
                ],
                |_, _, _| {},
            )
        })
        .collect();
    let unavailable: Vec<String> = household
        .accounts
        .iter()
        .filter(|a| !a.holder.is_company() && !form.personal_accounts.contains(&a.id))
        .filter(|a| !matches!(household.disclosure_for(app.viewer(), atlas_core::ids::ObjectRef::Account(a.id)), atlas_core::Disclosure::Hidden))
        .map(|a| format!("{} — not eligible as a source", a.name))
        .chain(household.accounts.iter().filter(|a| !a.holder.is_company() && a.kind.is_liability() && form.personal_accounts.contains(&a.id)).map(|a| format!("{} — a liability; it cannot fund a purchase", a.name)))
        .collect();
    let route_rows: Vec<_> = form
        .routes
        .iter()
        .enumerate()
        .map(|(index, (company, to))| {
            let name = household.company(*company).map(|c| c.name.clone()).unwrap_or_default();
            let allowed = draft.route_allowed.get(index).copied().unwrap_or(false);
            let method = draft.route_method.get(index).copied().unwrap_or(0);
            let ceiling = entities.and_then(|m| m.company(*company)).map(|c| c.ceiling.money().format()).unwrap_or_else(|| "Not disclosed".into());
            let d = draft_entity.clone();
            let d2 = draft_entity.clone();
            record::row(
                SharedString::from(format!("route-{}", company.raw())),
                false,
                vec![
                    (ROUTE_LANES[0].1, record::text(name)),
                    (ROUTE_LANES[1].1, record::muted(ceiling, cx)),
                    (ROUTE_LANES[2].1, h_flex().child(Checkbox::new(ElementId::Name(format!("decision-route-{}", company.raw()).into())).checked(allowed).on_change(move |v, _, cx| {
                        d.update(cx, |d, cx| {
                            if let Some(slot) = d.route_allowed.get_mut(index) {
                                *slot = *v;
                            }
                            cx.notify();
                        })
                    })).into_any_element()),
                    (
                        ROUTE_LANES[3].1,
                        RadioGroup::horizontal(ElementId::Name(format!("decision-route-method-{}", company.raw()).into()))
                            .children(["Salary", "Dividend"])
                            .selected_index(Some(method))
                            .on_change(move |i, _, cx| {
                                d2.update(cx, |d, cx| {
                                    if d.route_method.len() <= index {
                                        d.route_method.resize(index + 1, 0);
                                    }
                                    d.route_method[index] = *i;
                                    cx.notify();
                                })
                            })
                            .into_any_element(),
                    ),
                    (ROUTE_LANES[4].1, div().w(px(220.)).child(Select::new(to).small()).into_any_element()),
                ],
                |_, _, _| {},
            )
        })
        .collect();
    let no_sources = form.source_floors.is_empty();
    let amounts = section("purchase-step-2", "The down payment and where it comes from")
        .description("Paid on the purchase date from the allowed sources below. Fees and withholding are worked out on gross amounts.")
        .child(
            Form::vertical()
                .columns(2)
                .child(Field::new().label("Down payment").required(true).child(Input::new(&form.down_payment).id("decision-down-payment")))
                .child(Field::new().label("Maximum tax + fees (optional)").description("A ceiling on what funding may cost now; it never overrides an account or company constraint.").child(Input::new(&form.max_tax).id("decision-max-tax"))),
        )
        // The range is one thought — from, to, by — so it reads across three
        // columns of its own rather than wrapping out of a two-column grid.
        .child(
            Form::vertical()
                .columns(3)
                .child(Field::new().label("Compare down payments from").child(Input::new(&form.down_low).id("decision-down-low")))
                .child(Field::new().label("… up to").child(Input::new(&form.down_high).id("decision-down-high")))
                .child(Field::new().label("… in steps of").description("Every down payment in the range is evaluated too, at most thirty of them.").child(Input::new(&form.down_step).id("decision-down-step"))),
        )
        .into_any_element();
    let tables = section("purchase-sources", "Funding sources")
                .description("Hard earmarks and funding prohibitions always apply; a floor you enter can only be stricter. Moving a row changes the search order in this draft, not a standing rule.")
                .action(Button::new("purchase-funding-rules").small().ghost().icon(IconName::Gavel).label("Funding rules").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Funding, cx))))
                .child(if no_sources {
                    empty_state(
                        "purchase-no-sources",
                        "No personal account can fund a purchase yet",
                        "A source has to be a personal account that is not a liability. Add one and this step can be validated.",
                        Some(Button::new("purchase-add-account").small().outline().icon(IconName::Plus).label("Add account…").on_click(cx.listener(|this, _, window, cx| this.open_entry(Entry::Account, window, cx))).into_any_element()),
                        cx,
                    )
                } else {
                    record::list("purchase-source-list", record::header(&SOURCE_LANES, cx), source_rows).into_any_element()
                })
                .when(!unavailable.is_empty(), |this| this.child(v_flex().w_full().gap_0p5().child(div().text_xs().text_color(cx.theme().muted_foreground).child("Unavailable sources")).children(unavailable.into_iter().map(|u| div().w_full().text_xs().text_color(cx.theme().muted_foreground).child(u)))))
                .child(if route_rows.is_empty() { note("No company in the household; no company route.", cx).into_any_element() } else { record::list("purchase-route-list", record::header(&ROUTE_LANES, cx), route_rows).into_any_element() })
        // What a company route does not establish is a standing fact about
        // this step, true whether or not a route is allowed, so it is stated
        // as one instead of trailing off as a muted afterthought.
        .child(info_card(
            "purchase-legal-capacity",
            IconName::Gavel,
            "Legal capacity is not established by this calculation",
            "A route that would break a company reserve is rejected whatever its tax, and a permitted route here is still not proof that the company may pay it out.",
            cx,
        ))
        .into_any_element();
    (amounts, tables)
}

fn render_step_recurring(app: &AtlasApp, form: &DecisionForm, _household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let draft = form.draft.read(cx).clone();
    let d = form.draft.clone();
    let plan = &app.decision_plan;
    let principal = plan.financed();
    let currency = plan.price.currency();
    // A preview only when every input parses: not a guessed loan quote.
    let preview = if draft.financing {
        let months: Option<u32> = form.months.read(cx).value().trim().parse().ok().filter(|m| (1..=480).contains(m));
        let rate: Option<f64> = form.rate.read(cx).value().trim().replace('%', "").parse().ok().filter(|r: &f64| (0.0..=100.0).contains(r));
        match (months, rate) {
            (Some(m), Some(r)) if principal.is_positive() => Some((
                format!("{} a month for {m} months", atlas_core::decision::monthly_payment(principal, m, (r * 100.0).round() as u32).format()),
                format!("The supplied annuity model on {} financed at {r}% nominal. It is shown only because every input above parses; it is not a loan quote from any lender.", principal.format()),
            )),
            _ => None,
        }
    } else {
        None
    };
    section("purchase-step-3", "The recurring payment")
        .description(format!("The remainder ({}) as a monthly annuity: term, nominal annual rate, first instalment and the paying account. Untick to pay the full price at once.", if principal.is_positive() { principal.format() } else { atlas_core::Money::zero(currency).format() }))
        .child(Checkbox::new("decision-financing").label("Finance the remainder with monthly instalments").checked(draft.financing).on_change(move |v, _, cx| {
            d.update(cx, |d, cx| {
                d.financing = *v;
                cx.notify();
            })
        }))
        .when(draft.financing, |this| {
            this.child(
                Form::vertical()
                    .columns(2)
                    .child(Field::new().label("Months").required(true).child(Input::new(&form.months).id("decision-months")))
                    .child(Field::new().label("Nominal annual rate (%)").required(true).description("Zero is a valid rate.").child(Input::new(&form.rate).id("decision-rate")))
                    .child(Field::new().label("First instalment").child(DatePicker::new(&form.first_instalment)))
                    .child(Field::new().label("Paying account").child(Select::new(&form.financing_account))),
            )
        })
        // The instalment the entered term and rate imply is what this step is
        // for, so it is stated as a fact of the step rather than as one more
        // sentence under the form.
        .when_some(preview, |this, (payment, how)| this.child(info_card("purchase-instalment-preview", IconName::Percent, payment, how, cx)))
        .when(!draft.financing, |this| this.child(note("Without financing the down payment must cover the price.", cx)))
        .into_any_element()
}

fn render_step_other(form: &DecisionForm, cx: &mut Context<AtlasApp>) -> AnyElement {
    let draft = form.draft.read(cx).clone();
    let d1 = form.draft.clone();
    let d2 = form.draft.clone();
    section("purchase-step-4", "Other costs")
        .description("One-off costs around the purchase and the running costs that follow it. Only the groups you enable become events.")
        .child(Checkbox::new("decision-other-cost").label("A one-off cost").checked(draft.other_cost).on_change(move |v, _, cx| {
            d1.update(cx, |d, cx| {
                d.other_cost = *v;
                cx.notify();
            })
        }))
        .when(draft.other_cost, |this| {
            this.child(
                Form::vertical()
                    .columns(2)
                    .child(Field::new().label("What").child(Input::new(&form.other_label).id("decision-other-label")))
                    .child(Field::new().label("Amount").required(true).child(Input::new(&form.other_amount).id("decision-other-amount")))
                    .child(Field::new().label("On").child(DatePicker::new(&form.other_on)))
                    .child(Field::new().label("Paid from").child(Select::new(&form.other_account))),
            )
        })
        .child(Checkbox::new("decision-running-cost").label("Monthly running costs").checked(draft.running_cost).on_change(move |v, _, cx| {
            d2.update(cx, |d, cx| {
                d.running_cost = *v;
                cx.notify();
            })
        }))
        .when(draft.running_cost, |this| {
            this.child(
                Form::vertical()
                    .columns(2)
                    .child(Field::new().label("What").child(Input::new(&form.running_label).id("decision-running-label")))
                    .child(Field::new().label("Per month").required(true).child(Input::new(&form.running_monthly).id("decision-running-monthly")))
                    .child(Field::new().label("From").child(DatePicker::new(&form.running_from)))
                    .child(Field::new().label("Paid from").child(Select::new(&form.running_account))),
            )
        })
        .into_any_element()
}

// ----- Result --------------------------------------------------------------------

/// The result's header: the trail that reaches it, the purchase's own name as
/// the title, and one meta line of what was entered.
///
/// `common::detail_header` spells the last crumb with the title, which here
/// would read `Decisions / Purchase / Family car — result` and then repeat the
/// same words as the heading. The crumb is the place — `Result` — and the
/// title is the thing the result is about.
fn result_header(name: SharedString, meta: SharedString, actions: Vec<AnyElement>, cx: &mut Context<AtlasApp>) -> AnyElement {
    let theme = cx.theme();
    v_flex()
        .w_full()
        .gap_2()
        .child(
            Breadcrumb::new()
                .child(BreadcrumbItem::new("Decisions").on_click(cx.listener(|this, _, _, cx| this.navigate(Destination::Decisions.home(), cx))))
                .child(BreadcrumbItem::new("Purchase").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Purchase, cx))))
                .child(BreadcrumbItem::new("Result")),
        )
        .child(
            h_flex()
                .w_full()
                .justify_between()
                .items_start()
                .gap_4()
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_1()
                        .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(name))
                        .child(div().text_sm().text_color(theme.muted_foreground).child(meta)),
                )
                .when(!actions.is_empty(), |this| this.child(h_flex().flex_shrink_0().gap_2().children(actions))),
        )
        .into_any_element()
}

pub fn render_result(app: &AtlasApp, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let decision = match &app.decision {
        Some(Ok(d)) => d,
        Some(Err(err)) => {
            return v_flex()
                .id("screen-purchase-result")
                .test_support()
                .gap_4()
                .child(detail_header(Destination::Decisions, Route::Purchase, "Purchase", "Result", None, vec![], cx))
                .child(Alert::error("decision-error", format!("The purchase could not be evaluated: {err}")).title("No result"))
                .child(h_flex().gap_2().child(Button::new("decision-retry").small().outline().label("Retry calculation").on_click(cx.listener(|this, _, window, cx| this.calculate_purchase(window, cx)))).child(Button::new("decision-edit-after-error").small().ghost().label("Edit purchase").on_click(cx.listener(|this, _, window, cx| this.go_to_decision_step(3, window, cx)))))
                .into_any_element();
        }
        None => {
            return v_flex()
                .id("screen-purchase-result")
                .test_support()
                .gap_4()
                .child(detail_header(Destination::Decisions, Route::Purchase, "Purchase", "Result", None, vec![], cx))
                .child(note("No result yet: the purchase has not been calculated.", cx))
                .child(
                    h_flex()
                        .gap_2()
                        .child(Button::new("decision-go-build").small().outline().label("Build a purchase").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Purchase, cx))))
                        .child(Button::new("decision-calculate-now").small().primary().icon(IconName::Play).label("Calculate purchase").on_click(cx.listener(|this, _, window, cx| this.calculate_purchase(window, cx)))),
                )
                .into_any_element();
        }
    };
    let plan = &decision.plan;
    let keeps = decision.statement.claim.contains("keeps");
    let saved = app.decision_saved_scenario;
    let stale = app.decision_result_stale;
    // The meta line reads left to right the way the purchase was entered:
    // when, for how much, how much of it now, and what follows monthly.
    let meta = format!(
        "{} · Price {} · Down payment {} · {} · through {}",
        date(plan.purchase_on),
        plan.price.format(),
        plan.down_payment.format(),
        plan.financing.as_ref().map(|f| format!("{} payments of {}", f.months, decision.monthly_payment.format())).unwrap_or_else(|| "paid in full".into()),
        date(decision.through)
    );
    let save: AnyElement = match saved {
        Some(id) => Button::new("decision-open-scenario").small().outline().label("Open saved scenario").on_click(cx.listener(move |this, _, _, cx| this.open_scenario_detail(id, cx))).into_any_element(),
        None => Button::new("decision-save-scenario").small().primary().icon(IconName::Save).label("Save as scenario").disabled(stale).tooltip(if stale { "Recalculate first; the inputs changed" } else { "Creates “Decision: <purchase>” with its planned movements" }).on_click(cx.listener(|this, _, window, cx| this.save_decision_as_scenario(window, cx))).into_any_element(),
    };
    let header = result_header(
        plan.name.clone().into(),
        meta.into(),
        vec![Button::new("decision-edit").small().outline().label("Edit purchase").on_click(cx.listener(|this, _, window, cx| this.go_to_decision_step(0, window, cx))).into_any_element(), save],
        cx,
    );
    let tab = app.decision_report_tab;
    let report: AnyElement = match tab {
        1 => render_funding(app, decision, cx),
        2 => render_combinations(app, decision, cx),
        3 => render_goals(decision, cx),
        4 => render_basis(app, decision, household, cx),
        _ => render_affordability(app, decision, cx),
    };
    let chart_block = render_decision_chart(app, decision, household, cx);
    // Whether the reserve survives the conservative test is the one thing
    // that can be wrong here, so it is an alert when it is; when it holds it
    // is a standing fact and reads as a card, not as a warning that passed.
    let verdict_fact: AnyElement = if keeps {
        info_card("decision-verdict-fact", IconName::ShieldCheck, "Reserve kept in the Conservative case", decision.statement.claim.clone(), cx)
    } else {
        div()
            .id("decision-verdict-fact")
            .test_support()
            .w_full()
            .child(Alert::warning("decision-verdict-alert", decision.statement.claim.clone()).title("Reserve breached in the Conservative case"))
            .into_any_element()
    };
    let theme = cx.theme();
    v_flex()
        .id("screen-purchase-result")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .when(stale, |this| this.child(Alert::warning("decision-stale", "The inputs changed after this result was calculated. Recalculate to refresh it; saving is disabled until then.").title("Out of date")))
        .child(
            // No heading over the verdict: the card or the alert *is* the
            // verdict, and the sentence under it is what the search chose.
            v_flex()
                .id("decision-verdict")
                .test_support()
                .w_full()
                .gap_3()
                .child(verdict_fact)
                .child(
                    v_flex()
                        .w_full()
                        .gap_2()
                        .child(
                            h_flex()
                                .w_full()
                                .gap_1()
                                .flex_wrap()
                                .items_center()
                                .child(labels::money_class_tag(MoneyClass::ConditionalFuture))
                                .child(labels::certainty_tag(Certainty::ScenarioOnly))
                                .child(labels::strength_tag(decision.statement.coverage)),
                        )
                        .child(div().id("decision-recommendation").test_support().w_full().text_base().child(decision.recommendation.action.clone()))
                        .child(div().w_full().text_xs().text_color(theme.muted_foreground).child(format!(
                            "Objective: {} · best among {} tested strategies, {} feasible — a finite search, not a global optimum. The verdict is the conservative test through {}; the chart below is the expected path.",
                            decision.recommendation.objective, decision.recommendation.candidates_evaluated, decision.recommendation.feasible, date(decision.through)
                        ))),
                ),
        )
        .child(chart_block)
        .child(
            v_flex()
                .w_full()
                .gap_4()
                .child(
                    TabBar::new("decision-report-tabs")
                        .selected_index(tab)
                        .on_click(cx.listener(|this, index: &usize, _, cx| {
                            this.decision_report_tab = *index;
                            cx.notify();
                        }))
                        .children([Tab::new().label("Affordability"), Tab::new().label("Funding"), Tab::new().label("Combinations"), Tab::new().label("Goals"), Tab::new().label("Basis")]),
                )
                .child(report),
        )
        .into_any_element()
}

use gpui_kit::component::tab::{Tab, TabBar};

fn render_decision_chart(app: &AtlasApp, decision: &Decision, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    grid::sync(&app.grids.decision_values, &app.decision_values_rows, cx);
    let plan = &decision.plan;
    let per_major = household.base_currency.minor_per_major() as f64;
    let reserve = plan.reserve.minor() as f64 / per_major;
    let points: Vec<DecisionPoint> = decision
        .merged_path
        .iter()
        .map(|(d, base, dec)| DecisionPoint { label: SharedString::from(d.format("%d %b").to_string()), baseline: base.minor() as f64 / per_major, decision: dec.minor() as f64 / per_major, reserve })
        .collect();
    let lowest_index = decision.merged_path.iter().enumerate().min_by_key(|(_, (_, _, dec))| dec.minor()).map(|(i, _)| i);
    let commands = lowest_index.map(|i| PathCommand { id: "decision-show-lowest", label: "Show lowest point", select: i }).into_iter().collect();
    let selected = app.decision_path_state.read(cx).selected.and_then(|i| decision.merged_path.get(i).map(|p| (i, p)));
    let readout = selected.map(|(i, (d, base, dec))| {
        chart::readout(
            format!("Selected: {} · point {} of {}", date(*d), i + 1, decision.merged_path.len()),
            vec![fact("Baseline", base.format(), cx).into_any_element(), fact("With the purchase", dec.format(), cx).into_any_element(), fact("Difference", (*dec - *base).format_signed(), cx).into_any_element(), fact("Reserve", plan.reserve.format(), cx).into_any_element()],
            cx,
        )
    });
    let theme = cx.theme();
    let (c1, c2, c3) = (theme.chart_1, theme.chart_2, theme.danger);
    let legend = vec![Legend { name: "Baseline".into(), color: c1 }, Legend { name: "With the purchase".into(), color: c2 }, Legend { name: "Your reserve".into(), color: c3 }];
    let tick_margin = (points.len() / 8).max(1);
    let chart_el = div()
        .h_64()
        .w_full()
        .child(
            AreaChart::new(points)
                .id("decision-area-chart")
                .x(|p: &DecisionPoint| p.label.clone())
                .y(|p: &DecisionPoint| p.baseline)
                .stroke(c1)
                .fill(c1.opacity(0.0))
                .name("Baseline")
                .step_after()
                .y(|p: &DecisionPoint| p.decision)
                .stroke(c2)
                .fill(c2.opacity(0.08))
                .name("With the purchase")
                .step_after()
                .y(|p: &DecisionPoint| p.reserve)
                .stroke(c3)
                .fill(c3.opacity(0.0))
                .name("Reserve")
                .step_after()
                .tick_margin(tick_margin),
        )
        .into_any_element();
    let values = grid::render("decision-values-grid", &app.grids.decision_values, cx).into_any_element();
    let showing_values = app.decision_path_state.read(cx).values;
    let cash_path = chart::cash_path(
        "decision-path",
        &app.decision_path_state,
        format!("Expected graph · the Conservative reserve test is above · through {}", date(decision.through)),
        chart_el,
        values,
        legend,
        commands,
        readout,
        cx,
    );
    // The equation behind the immediate cash rather than its chain as a
    // table: the terms of the terms are what `Full calculation…` opens, and
    // the figure itself leads the Affordability grid below.
    let equation = app.decision_immediate.as_ref().map(|f| explain::render_equation("decision-immediate-equation", f.calc.node(), f.content(), cx));
    let theme = cx.theme();
    let (border, radius) = (theme.border, theme.radius);
    v_flex()
        .id("decision-chart")
        .test_support()
        .w_full()
        .gap_4()
        // The plot, its title, its Chart/Values toggle and its legend are one
        // bordered object; a borderless plot under a floating title reads as
        // three unrelated things stacked.
        .child(
            v_flex()
                .w_full()
                .gap_2()
                .p_4()
                .rounded(radius)
                .border_1()
                .border_color(border)
                .child(div().w_full().text_sm().font_weight(FontWeight::MEDIUM).child(if showing_values { "Exact cash-path values" } else { "Expected cash paths" }))
                .child(cash_path),
        )
        .children(equation)
        .into_any_element()
}

/// The four metrics the result answers with, lifted out of the table into the
/// grid under the report tabs; every other metric continues in the table.
///
/// They are matched by the engine's own names rather than by position, so a
/// metric that is renamed or dropped falls back into the table instead of
/// putting the wrong number under a heading. Each of these is a single figure:
/// a metric whose value states two cases (the lowest cash, its dates) reads as
/// a row, not as a grid cell that wraps while its neighbours do not.
const LEAD_METRICS: [&str; 4] = ["Immediate cash after purchase", "Emergency reserve remaining", "Monthly repayment", "Total financing cost"];

/// One cell of that grid for a metric the engine states as a value and a
/// method: it has no chain of its own to open, so it carries no `ⓘ` and no
/// vocabulary terms — the engine's `How` is what it can say for itself.
fn metric_cell(m: &AffordabilityMetric, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let negative = m.money.is_some_and(|v| v.is_negative());
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .child(div().w_full().text_xs().text_color(theme.muted_foreground).whitespace_normal().child(m.name.clone()))
        .child(
            div()
                .w_full()
                .font_family(theme.mono_font_family.clone())
                .text_xl()
                .font_weight(FontWeight::SEMIBOLD)
                .when(negative, |d| d.text_color(theme.danger))
                .child(m.value.clone()),
        )
        .child(div().w_full().text_xs().text_color(theme.muted_foreground).whitespace_normal().child(m.how.clone()))
        .into_any_element()
}

fn render_affordability(app: &AtlasApp, decision: &Decision, cx: &mut Context<AtlasApp>) -> AnyElement {
    let lanes_def: [(&str, Lane); 3] = [("Metric", Lane::fixed(300.)), ("Value", Lane::fixed(260.)), ("How", Lane::flex())];
    let expanded = app.decision_metric_expanded;
    // The headline grid, in the order the four are named — the immediate cash
    // through its own explained figure (it is the one with a chain), the rest
    // as the engine states them.
    let mut cells: Vec<AnyElement> = Vec::with_capacity(LEAD_METRICS.len());
    for name in LEAD_METRICS {
        let Some(m) = decision.metrics.iter().find(|m| m.name == name) else { continue };
        match &app.decision_immediate {
            Some(f) if name == LEAD_METRICS[0] => cells.push(f.leading().into_any_element()),
            _ => cells.push(metric_cell(m, cx)),
        }
    }
    let rows: Vec<AnyElement> = decision
        .metrics
        .iter()
        .enumerate()
        .filter(|(_, m)| !LEAD_METRICS.contains(&m.name.as_str()))
        .map(|(i, m)| {
            let is_open = expanded == Some(i);
            let row = record::row(
                SharedString::from(format!("metric-{i}")),
                is_open,
                vec![
                    (lanes_def[0].1, record::text(m.name.clone())),
                    (lanes_def[1].1, div().font_family(cx.theme().mono_font_family.clone()).text_sm().child(m.value.clone()).into_any_element()),
                    (lanes_def[2].1, record::muted(m.how.clone(), cx)),
                ],
                move |_, _, cx| {
                    crate::app::with_app(cx, |app, cx| {
                        app.decision_metric_expanded = if app.decision_metric_expanded == Some(i) { None } else { Some(i) };
                        cx.notify();
                    })
                },
            );
            if is_open {
                v_flex().w_full().child(row).child(div().px_3().py_2().text_sm().whitespace_normal().child(m.how.clone())).into_any_element()
            } else {
                row.into_any_element()
            }
        })
        .collect();
    let theme = cx.theme();
    section("decision-affordability", "Affordability")
        .description("What the purchase does to the household's cash: the four figures it answers with, then every other metric the engine reports with its own account of how it was worked out.")
        .child(div().id("decision-affordability-figures").test_support().w_full().child(columns(cells)))
        .child(hairline(cx))
        .child(v_flex().w_full().gap_0p5().child(record::header(&lanes_def, cx)).children(rows))
        .when(!decision.company_consequences.is_empty(), |this| {
            this.child(v_flex().w_full().gap_1().child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Company consequences")).children(decision.company_consequences.iter().map(|c| div().w_full().text_sm().child(c.clone()))))
                .child(div().w_full().text_xs().text_color(theme.muted_foreground).child("Business cash is never counted as household cash."))
        })
        .into_any_element()
}

/// The lanes the funding strategies are compared down. The last is the
/// caveat or the reason a strategy is infeasible, so it takes what is left.
const STRATEGY_LANES: [(&str, Lane); 8] = [
    ("Strategy", Lane::fixed(220.)),
    ("Status", Lane::fixed(150.)),
    ("Immediate tax", Lane::money(120.)),
    ("Fees", Lane::money(100.)),
    ("Net delivered", Lane::money(130.)),
    ("Future incremental tax", Lane::money(150.)),
    ("Transfers", Lane::fixed(80.)),
    ("Caveat / why not", Lane::flex()),
];

/// A right-aligned money cell of a fixed lane, outside a `record::row`.
fn lane_money(lane: Lane, money: atlas_core::Money, cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .w(px(lane.width.unwrap_or(120.)))
        .flex_shrink_0()
        .min_w_0()
        .overflow_hidden()
        .flex()
        .justify_end()
        .text_right()
        .font_family(theme.mono_font_family.clone())
        .text_sm()
        .when(money.is_negative(), |d| d.text_color(theme.danger))
        .child(money.format())
        .into_any_element()
}

fn render_funding(app: &AtlasApp, decision: &Decision, cx: &mut Context<AtlasApp>) -> AnyElement {
    let report = &decision.strategies;
    let expanded = app.decision_strategy_expanded.or(report.preferred).or(if report.strategies.is_empty() { None } else { Some(0) });
    let show_basis = app.decision_show_basis;
    let theme = cx.theme();
    let step_lanes: [(&str, Lane); 7] = [("Step", Lane::fixed(40.)), ("Source", Lane::fixed(220.)), ("Gross", Lane::money(120.)), ("Withheld", Lane::money(110.)), ("Fees", Lane::money(100.)), ("Net", Lane::money(120.)), ("Ending cash / floor", Lane::flex())];
    let strategies: Vec<AnyElement> = report
        .strategies
        .iter()
        .enumerate()
        .map(|(index, s)| {
            let preferred = report.preferred == Some(index);
            let is_open = expanded == Some(index);
            // The strategies compare across the same five figures, so they
            // read down lanes; the sentence of metadata they used to carry
            // put five numbers on one wrapping line nobody could compare.
            let header = h_flex()
                .id(SharedString::from(format!("strategy-{index}")))
                .w_full()
                .gap_4()
                .items_center()
                .px_3()
                .py_2()
                .rounded(theme.radius)
                .cursor_pointer()
                .when(preferred, |c| c.border_1().border_color(theme.primary))
                .when(!preferred, |c| c.border_1().border_color(theme.border))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.decision_strategy_expanded = Some(index);
                    cx.notify();
                }))
                .child(div().w(px(STRATEGY_LANES[0].1.width.unwrap_or(220.))).flex_shrink_0().min_w_0().overflow_hidden().text_ellipsis().text_sm().font_weight(FontWeight::MEDIUM).child(format!("{}. {}", index + 1, s.name)))
                .child(
                    h_flex()
                        .w(px(STRATEGY_LANES[1].1.width.unwrap_or(150.)))
                        .flex_shrink_0()
                        .gap_1()
                        .items_center()
                        .child(if s.feasible { Tag::secondary().xsmall().outline().child("Feasible") } else { Tag::danger().xsmall().outline().child("Infeasible") })
                        .when(preferred, |row| row.child(Tag::info().xsmall().outline().child("Preferred"))),
                )
                .child(lane_money(STRATEGY_LANES[2].1, s.immediate_tax, cx))
                .child(lane_money(STRATEGY_LANES[3].1, s.fees, cx))
                .child(lane_money(STRATEGY_LANES[4].1, s.net, cx))
                .child(lane_money(STRATEGY_LANES[5].1, s.future_tax, cx))
                .child(div().w(px(STRATEGY_LANES[6].1.width.unwrap_or(80.))).flex_shrink_0().text_sm().child(s.transfers.to_string()))
                .child(div().flex_1().min_w_0().overflow_hidden().text_ellipsis().text_xs().when(!s.feasible, |d| d.text_color(theme.danger)).when(s.feasible, |d| d.text_color(theme.muted_foreground)).child(if s.feasible {
                    s.caveats.first().cloned().unwrap_or_else(|| if preferred { "Preferred under the stated objective".to_string() } else { String::new() })
                } else {
                    s.violations.first().cloned().unwrap_or_default()
                }));
            if !is_open {
                return header.into_any_element();
            }
            let steps: Vec<_> = s
                .steps
                .iter()
                .enumerate()
                .map(|(i, step)| {
                    record::row(
                        SharedString::from(format!("strategy-{index}-step-{i}")),
                        false,
                        vec![
                            (step_lanes[0].1, record::muted(format!("{}.", i + 1), cx)),
                            (step_lanes[1].1, record::text(step.source.clone())),
                            (step_lanes[2].1, record::money(step.gross, cx)),
                            (step_lanes[3].1, record::money(step.withholding, cx)),
                            (step_lanes[4].1, record::money(step.fees, cx)),
                            (step_lanes[5].1, record::money(step.net, cx)),
                            (step_lanes[6].1, record::muted(format!("{} / floor {}{}", step.ending_balance.format(), step.floor.format(), if step.note.is_empty() { String::new() } else { format!(" · {}", step.note) }), cx)),
                        ],
                        |_, _, _| {},
                    )
                })
                .collect();
            v_flex()
                .w_full()
                .child(header)
                .child(
                    v_flex()
                        .w_full()
                        .gap_2()
                        .px_3()
                        .py_2()
                        .child(if steps.is_empty() { note("No funding step.", cx).into_any_element() } else { record::list(SharedString::from(format!("strategy-{index}-steps")), record::header(&step_lanes, cx), steps).into_any_element() })
                        .child(div().text_xs().child(format!("Future incremental tax {} — {}", s.future_tax.format(), s.future_tax_note)))
                        .child(div().text_xs().child(format!("Reserve constraints: {}", if s.violations.is_empty() { "satisfied".to_string() } else { s.violations.join("; ") })))
                        .when(!s.caveats.is_empty(), |this| this.child(div().text_xs().text_color(theme.muted_foreground).child(format!("Caveats: {}", s.caveats.join(" "))))),
                )
                .into_any_element()
        })
        .collect();
    let recommendation_text = decision.recommendation.render();
    section("decision-funding", "Ways to fund the down payment")
        .action(
            h_flex()
                .gap_2()
                .child(Button::new("decision-how-chosen").small().ghost().label("How this was chosen…").on_click(cx.listener(|this, _, _, cx| {
                    this.decision_show_basis = !this.decision_show_basis;
                    cx.notify();
                })))
                .child(Button::new("decision-edit-funding").small().outline().label("Edit funding").on_click(cx.listener(|this, _, window, cx| this.go_to_decision_step(1, window, cx)))),
        )
        .child(div().id("decision-strategy-status").test_support().w_full().text_sm().child(report.status.clone()))
        .child(div().w_full().text_xs().text_color(theme.muted_foreground).child(format!("Search space: {}. Objective: {}. Best among the tested strategies, not a global optimum.", report.search_space, report.objective.label())))
        .child(if strategies.is_empty() {
            note("No strategy could be enumerated: no allowed source or route.", cx).into_any_element()
        } else {
            v_flex().w_full().gap_2().child(record::header(&STRATEGY_LANES, cx)).children(strategies).into_any_element()
        })
        .when(show_basis, |this| {
            this.child(
                v_flex()
                    .w_full()
                    .gap_2()
                    .p_3()
                    .rounded_md()
                    .bg(theme.secondary)
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("How this was chosen"))
                    .child(div().text_sm().child(decision.recommendation.explanation.clone()))
                    .child(div().text_xs().child("Why:"))
                    .children(decision.recommendation.metrics.iter().map(|m| div().text_xs().pl_4().child(format!("• {m}"))))
                    .child(div().text_xs().child("Constraints:"))
                    .children(decision.recommendation.constraints.iter().map(|c| div().text_xs().pl_4().child(format!("• {c}"))))
                    .child(div().text_xs().child("Alternatives:"))
                    .children(decision.recommendation.alternatives.iter().map(|a| div().text_xs().pl_4().child(format!("• {a}"))))
                    .child(h_flex().justify_end().child(copy_button("copy-recommendation", "Copy recommendation", recommendation_text))),
            )
        })
        .into_any_element()
}

fn render_combinations(app: &AtlasApp, decision: &Decision, cx: &mut Context<AtlasApp>) -> AnyElement {
    let long = app.decision_combo_long;
    let only_ok = app.decision_combo_filter;
    let mut dates: Vec<chrono::NaiveDate> = decision.grid.iter().map(|c| c.purchase_on).collect();
    dates.dedup();
    let mut amounts: Vec<atlas_core::Money> = decision.grid.iter().map(|c| c.down_payment).collect();
    amounts.sort_by_key(|m| m.minor());
    amounts.dedup();
    let total = decision.grid.len();
    let hidden_by_filter = if only_ok { decision.grid.iter().filter(|c| !c.reserve_ok).count() } else { 0 };
    let outside = |c: &atlas_core::decision::GridCell| c.purchase_on > decision.through;
    let selected = app.decision_combo_selected.and_then(|i| decision.grid.get(i).map(|c| (i, c)));
    // One outcome control per cell, built before the theme borrow.
    let mut cell_buttons: Vec<AnyElement> = Vec::with_capacity(decision.grid.len());
    for (index, c) in decision.grid.iter().enumerate() {
        let label = if outside(c) { "Outside window" } else if c.reserve_ok { "Keeps reserve" } else { "Breaches reserve" };
        let is_selected = app.decision_combo_selected == Some(index);
        let ok = c.reserve_ok && !outside(c);
        let best = c.best;
        cell_buttons.push(
            h_flex()
                .gap_1()
                .items_center()
                .child(
                    Button::new(SharedString::from(format!("combo-{index}")))
                        .xsmall()
                        .map(|b| if is_selected { b.primary() } else if ok { b.outline() } else { b.danger().outline() })
                        .label(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.decision_combo_selected = Some(index);
                            cx.notify();
                        })),
                )
                .when(best, |this| this.child(Tag::info().xsmall().outline().child("Best")))
                .into_any_element(),
        );
    }
    let mut cell_buttons: Vec<Option<AnyElement>> = cell_buttons.into_iter().map(Some).collect();
    let theme = cx.theme();
    let body: AnyElement = if decision.grid.is_empty() {
        note("Calculate the purchase to compare combinations.", cx).into_any_element()
    } else if !long && amounts.len() <= 5 {
        let mut lanes_def: Vec<(&str, Lane)> = vec![("Purchase on", Lane::fixed(130.))];
        let amount_labels: Vec<String> = amounts.iter().map(|a| a.format()).collect();
        for label in &amount_labels {
            lanes_def.push((label.as_str(), Lane::fixed(155.)));
        }
        let header: Vec<AnyElement> = lanes_def.iter().map(|(label, lane)| div().min_w_0().when_some(lane.width, |d, w| d.w(px(w)).flex_shrink_0()).child(div().text_xs().text_color(theme.muted_foreground).child(label.to_string())).into_any_element()).collect();
        let rows: Vec<AnyElement> = dates
            .iter()
            .map(|d| {
                let cells: Vec<AnyElement> = amounts
                    .iter()
                    .map(|a| {
                        let found = decision.grid.iter().position(|c| c.purchase_on == *d && c.down_payment == *a);
                        let content = match found {
                            Some(i) if !only_ok || decision.grid[i].reserve_ok => cell_buttons[i].take().unwrap_or_else(|| div().into_any_element()),
                            Some(_) => div().text_xs().text_color(theme.muted_foreground).child("Hidden by filter").into_any_element(),
                            None => div().text_xs().text_color(theme.muted_foreground).child("—").into_any_element(),
                        };
                        div().w(px(155.)).flex_shrink_0().child(content).into_any_element()
                    })
                    .collect();
                h_flex().w_full().gap_4().px_3().py_1().items_center().child(div().w(px(130.)).flex_shrink_0().text_sm().child(d.format("%b %Y").to_string())).children(cells).into_any_element()
            })
            .collect();
        v_flex().w_full().gap_0p5().child(h_flex().w_full().gap_4().px_3().py_1().children(header)).children(rows).into_any_element()
    } else {
        let lanes_def: [(&str, Lane); 7] = [("Purchase on", Lane::fixed(120.)), ("Down payment", Lane::money(130.)), ("Lowest cash", Lane::money(130.)), ("Lowest on", Lane::fixed(110.)), ("Shortfall", Lane::money(120.)), ("Financing cost", Lane::money(130.)), ("Outcome", Lane::flex())];
        let mono = theme.mono_font_family.clone();
        let muted = theme.muted_foreground;
        let danger = theme.danger;
        let money = |m: atlas_core::Money| div().font_family(mono.clone()).text_sm().when(m.is_negative(), |d| d.text_color(danger)).child(m.format()).into_any_element();
        let rows: Vec<_> = decision
            .grid
            .iter()
            .enumerate()
            .filter(|(_, c)| !only_ok || c.reserve_ok)
            .map(|(i, c)| {
                let outcome = cell_buttons[i].take().unwrap_or_else(|| div().into_any_element());
                record::row(
                    SharedString::from(format!("combo-row-{i}")),
                    app.decision_combo_selected == Some(i),
                    vec![
                        (lanes_def[0].1, record::text(date(c.purchase_on))),
                        (lanes_def[1].1, money(c.down_payment)),
                        (lanes_def[2].1, money(c.lowest)),
                        (lanes_def[3].1, div().text_xs().text_color(muted).child(c.lowest_on.map(date).unwrap_or_else(|| "—".into())).into_any_element()),
                        (lanes_def[4].1, money(c.shortfall)),
                        (lanes_def[5].1, money(c.financing_cost)),
                        (lanes_def[6].1, outcome),
                    ],
                    move |_, _, cx| {
                        crate::app::with_app(cx, |app, cx| {
                            app.decision_combo_selected = Some(i);
                            cx.notify();
                        })
                    },
                )
            })
            .collect();
        record::list("combo-list", record::header(&lanes_def, cx), rows).into_any_element()
    };
    let inspector = selected.map(|(i, c)| {
        let purchase_on = c.purchase_on;
        let down = c.down_payment;
        v_flex()
            .w_full()
            .gap_2()
            .pt_2()
            .border_t_1()
            .border_color(theme.border)
            .child(div().w_full().text_sm().font_weight(FontWeight::MEDIUM).child(format!("Selected: {} · {} down", date(c.purchase_on), c.down_payment.format())))
            // An even grid, not a wrap row: four readings of one cell divide
            // the width between them instead of leaving the right third empty.
            .child(columns([
                fact("Conservative lowest cash", format!("{}{}", c.lowest.format(), c.lowest_on.map(|d| format!(" on {}", date(d))).unwrap_or_default()), cx).into_any_element(),
                fact("Reserve shortfall", if c.shortfall.is_positive() { c.shortfall.format() } else { "None".into() }, cx).into_any_element(),
                fact("Financing cost", c.financing_cost.format(), cx).into_any_element(),
                fact("Reserve condition", if outside(c) { "Outside the evaluated window — not proven affordable".into() } else if c.reserve_ok { "Kept under the conservative case".into() } else { format!("Breached by {}", c.shortfall.format()) }, cx).into_any_element(),
            ]))
            .when(c.best, |this| this.child(div().text_xs().text_color(theme.muted_foreground).child(format!("Best on the specified grid under “{}”.", decision.strategies.objective.label()))))
            .child(h_flex().child(Button::new(SharedString::from(format!("combo-use-{i}"))).small().outline().label("Use these inputs").on_click(cx.listener(move |this, _, window, cx| this.use_combination_inputs(purchase_on, down, window, cx)))))
            .into_any_element()
    });
    section("decision-combinations", "Purchase month × down payment")
        .badge("Conservative case")
        .description(format!(
            "{total} combinations · {} keep the reserve · every cell is a full evaluation. {}",
            decision.grid.iter().filter(|c| c.reserve_ok && !outside(c)).count(),
            decision.grid_status
        ))
        .action(
            h_flex()
                .gap_3()
                .items_center()
                .child(Checkbox::new("combo-filter").label("Keeps reserve only").checked(only_ok).on_change(cx.listener(|this, v, _, cx| {
                    this.decision_combo_filter = *v;
                    cx.notify();
                })))
                .child(
                    TabBar::new("combo-view-tabs")
                        .selected_index(if long { 1 } else { 0 })
                        .on_click(cx.listener(|this, index: &usize, _, cx| {
                            this.decision_combo_long = *index == 1;
                            cx.notify();
                        }))
                        .children([Tab::new().label("Matrix"), Tab::new().label("All combinations")]),
                ),
        )
        .when(only_ok && hidden_by_filter > 0, |this| this.child(div().text_xs().text_color(theme.muted_foreground).child(format!("{hidden_by_filter} combinations hidden by the filter."))))
        .child(body)
        .children(inspector)
        .into_any_element()
}

fn render_goals(decision: &Decision, cx: &mut Context<AtlasApp>) -> AnyElement {
    let lanes_def: [(&str, Lane); 6] = [("Goal", Lane::fixed(220.)), ("Amount", Lane::money(130.)), ("Target", Lane::fixed(120.)), ("Baseline reach", Lane::fixed(130.)), ("With purchase", Lane::fixed(130.)), ("Effect", Lane::flex())];
    let fmt = |d: Option<chrono::NaiveDate>| d.map(date).unwrap_or_else(|| "Not reached in window".into());
    let rows: Vec<_> = decision
        .goals
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let delayed = g.delay_days.is_some_and(|d| d > 0);
            record::row(
                SharedString::from(format!("goal-{i}")),
                false,
                vec![
                    (lanes_def[0].1, record::text(g.name.clone())),
                    (lanes_def[1].1, record::money(g.amount, cx)),
                    (lanes_def[2].1, record::muted(date(g.target_on), cx)),
                    (lanes_def[3].1, record::muted(fmt(g.baseline_reached), cx)),
                    (lanes_def[4].1, record::muted(fmt(g.decision_reached), cx)),
                    (lanes_def[5].1, h_flex().gap_2().items_center().child(div().text_xs().when(delayed, |d| d.text_color(cx.theme().warning)).child(g.text.clone())).child(Button::new(SharedString::from(format!("goal-open-{i}"))).xsmall().ghost().compact().label("Open goal…").on_click(cx.listener(move |this, _, window, cx| this.open_goal_sheet(i, window, cx)))).into_any_element()),
                ],
                |_, _, _| {},
            )
        })
        .collect();
    section("decision-goals", "Effect on goals")
        .child(if rows.is_empty() { note("No goals recorded for this household.", cx).into_any_element() } else { record::list("goals-list", record::header(&lanes_def, cx), rows).into_any_element() })
        .into_any_element()
}

fn render_basis(app: &AtlasApp, decision: &Decision, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let viewer = app.viewer();
    let statement = Statement {
        claim: decision.statement.claim.clone(),
        through: decision.statement.horizon,
        scope: format!("Household · Conservative verdict, Expected chart · baseline plus this purchase"),
        assumptions: household.assumptions.iter().filter(|a| a.private_to.is_none_or(|p| p == viewer.person)).filter(|a| decision.statement.assumptions.iter().any(|t| t.contains(&a.text))).map(|a| Line::from_assumption(a, household.as_of)).collect(),
        some_hidden: household.assumptions.iter().any(|a| a.private_to.is_some_and(|p| p != viewer.person)),
        strength: decision.statement.coverage,
        coverage: decision.statement.coverage.permitted_claim().to_string(),
        does_not_establish: decision.statement.coverage.does_not_establish().to_string(),
        excluded_shocks: decision.statement.excluded_shocks.join("; "),
    };
    let text = statement.as_text();
    let recommendation = decision.recommendation.render();
    let theme = cx.theme();
    section("decision-basis", "Basis")
        .child(statement::render(
            "decision-statement",
            &statement,
            app.decision_show_all_assumptions,
            cx.listener(|this, _, _, cx| {
                this.decision_show_all_assumptions = !this.decision_show_all_assumptions;
                cx.notify();
            }),
            vec![copy_button("copy-decision-statement", "Copy statement", text).into_any_element(), copy_button("copy-decision-recommendation", "Copy recommendation", recommendation).into_any_element()],
            cx,
        ))
        .child(
            v_flex()
                .gap_1()
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Recommendation record"))
                .child(div().text_xs().text_color(theme.muted_foreground).child(format!("Objective {} · {} candidates evaluated · {} feasible · winning strategy: {}", decision.recommendation.objective, decision.recommendation.candidates_evaluated, decision.recommendation.feasible, decision.recommendation.winning_strategy)))
                .child(div().text_xs().child("Applied rules:"))
                .children(decision.recommendation.applied_rules.iter().map(|r| div().text_xs().pl_4().child(format!("• {r}"))))
                .child(div().text_xs().child("Constraints:"))
                .children(decision.recommendation.constraints.iter().map(|c| div().text_xs().pl_4().child(format!("• {c}"))))
                .child(div().text_xs().text_color(theme.muted_foreground).child(decision.recommendation.explanation.clone())),
        )
        // The equation, not the chain as a table: the chain is what
        // `Full calculation…` opens, and as a table here it cost the tab
        // twenty rows of what the sheet already lists exactly.
        .child(explain::render_equation(
            "decision-basis-equation",
            decision.immediate_cash.node(),
            app.decision_immediate.as_ref().map(|f| f.content()).unwrap_or_else(|| std::sync::Arc::new(explain::ExplainContent::new("Immediate cash after the purchase", decision.immediate_cash.money(), decision.immediate_cash.shared_node(), household.entity_name(atlas_core::ids::EntityRef::Person(viewer.person)), atlas_core::Disclosure::Full))),
            cx,
        ))
        .into_any_element()
}

impl AtlasApp {
    /// Reorders the personal funding sources of the draft.
    pub fn move_funding_source(&mut self, index: usize, up: bool, cx: &mut Context<Self>) {
        let len = self.decision_form.source_floors.len();
        let target = if up { index.checked_sub(1) } else { (index + 1 < len).then_some(index + 1) };
        let Some(target) = target else { return };
        self.decision_form.source_floors.swap(index, target);
        self.decision_form.draft.update(cx, |d, cx| {
            if d.source_allowed.len() > target.max(index) {
                d.source_allowed.swap(index, target);
            }
            cx.notify();
        });
        cx.notify();
    }

    /// Validates every step and evaluates; the result screen opens on success.
    pub fn calculate_purchase(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for step in 0..4 {
            if let Err(message) = self.apply_decision_step(step, cx) {
                log::warn!("purchase step {step} refused: {message}");
                window.push_notification(message, cx);
                self.decision_step = step;
                cx.notify();
                return;
            }
        }
        self.decision_step = 3;
        self.evaluate_decision();
        self.decision_result_stale = false;
        self.decision_saved_scenario = None;
        self.decision_combo_selected = None;
        self.decision_strategy_expanded = None;
        self.decision_metric_expanded = None;
        self.decision_report_tab = 0;
        self.decision_path_state.update(cx, |s, cx| {
            s.selected = None;
            cx.notify();
        });
        self.prepare_decision_result();
        match &self.decision {
            Some(Ok(d)) => self.note_result(format!("Calculated “{}”: {}", d.plan.name, if d.statement.claim.contains("keeps") { "keeps the reserve in the conservative case." } else { "breaches the reserve in the conservative case." })),
            Some(Err(err)) => self.note_result(format!("The purchase could not be evaluated: {err}")),
            None => {}
        }
        self.navigate(Route::PurchaseResult, cx);
    }

    /// The per-result pieces the screen reads every frame: the explained
    /// immediate-cash figure and the exact rows of the merged path.
    pub(crate) fn prepare_decision_result(&mut self) {
        use crate::widgets::grid::{Cell, Row};
        let Some(Ok(d)) = &self.decision else {
            self.decision_immediate = None;
            self.decision_values_rows = std::sync::Arc::new(Vec::new());
            return;
        };
        self.decision_immediate = Some(crate::widgets::figure::ExplainedFigure::new("decision-immediate-cash", "Immediate cash after the purchase", &d.immediate_cash, &self.household, self.viewer));
        self.decision_values_rows = std::sync::Arc::new(
            d.merged_path
                .iter()
                .map(|(date, base, dec)| Row::new(vec![Cell::text(date.format("%d %b %Y").to_string()), Cell::money(*base), Cell::money(*dec), Cell::muted((*dec - *base).format_signed())]))
                .collect(),
        );
    }

    /// Confirms before discarding the draft and its result.
    pub fn confirm_reset_decision(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        confirm_danger(window, cx, "Start over?", "The purchase draft and its result are discarded. Saved scenarios are not affected.", "Start over", |window, cx| {
            crate::app::with_app(cx, |app, cx| {
                app.reset_decision(window, cx);
                app.decision_result_stale = false;
                app.decision_saved_scenario = None;
                app.navigate(Route::Purchase, cx);
            })
        });
    }

    /// Leaves the builder for Today; the draft stays unless discarded.
    pub fn cancel_purchase(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let has_draft = !self.decision_form.name.read(cx).value().trim().is_empty() || self.decision.is_some();
        if !has_draft {
            self.navigate(Route::Today, cx);
            return;
        }
        super::common::confirm_primary(window, cx, "Leave the purchase?", vec!["The draft is kept for this session unless you discard it.".into()], "Keep draft and leave", |_, cx| {
            crate::app::with_app(cx, |app, cx| app.navigate(Route::Today, cx));
        });
    }

    /// Returns to step 1 with a grid cell's inputs; the rest of the draft is
    /// kept and must be validated and recalculated.
    pub fn use_combination_inputs(&mut self, purchase_on: chrono::NaiveDate, down_payment: atlas_core::Money, window: &mut Window, cx: &mut Context<Self>) {
        self.decision_form.purchase_on.update(cx, |s, cx| s.set_date(purchase_on, window, cx));
        self.decision_form.down_payment.update(cx, |s, cx| s.set_value(down_payment.format(), window, cx));
        self.decision_result_stale = true;
        self.decision_step = 0;
        self.navigate(Route::Purchase, cx);
        window.push_notification(format!("Purchase date {} and down payment {} filled in; validate the steps and recalculate.", date(purchase_on), down_payment.format()), cx);
    }

    /// A read-only sheet of one goal and this result's effect on it.
    pub fn open_goal_sheet(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Ok(d)) = &self.decision else { return };
        let Some(g) = d.goals.get(index).cloned() else { return };
        let plan_name = d.plan.name.clone();
        let fmt = |d: Option<chrono::NaiveDate>| d.map(date).unwrap_or_else(|| "Not reached within the window".into());
        window.open_sheet(cx, move |sheet, _, _| {
            sheet
                .title(format!("Goal: {}", g.name))
                .child(
                    DescriptionList::new()
                        .columns(1)
                        .child(DescriptionItem::new("Amount").value(g.amount.format()))
                        .child(DescriptionItem::new("Target date").value(date(g.target_on)))
                        .child(DescriptionItem::new("Reached on the baseline").value(fmt(g.baseline_reached)))
                        .child(DescriptionItem::new(format!("Reached with “{plan_name}”")).value(fmt(g.decision_reached)))
                        .child(DescriptionItem::new("Delay").value(g.delay_days.map(|d| format!("{d} days")).unwrap_or_else(|| "None".into())))
                        .child(DescriptionItem::new("Effect").value(g.text.clone())),
                )
                .footer(h_flex().w_full().justify_end().child(Button::new("close-goal").outline().small().label("Close").on_click(|_, window, cx| window.close_sheet(cx))))
        });
    }

    /// Opens a scenario's detail on the Scenarios screen.
    pub fn open_scenario_detail(&mut self, id: atlas_core::ids::ScenarioId, cx: &mut Context<Self>) {
        self.scenario_detail = Some(id);
        self.navigate(Route::Scenarios, cx);
    }
}

/// The route's method from a draft index.
pub fn route_method(index: usize) -> ExtractionMethod {
    if index == 1 { ExtractionMethod::Dividend } else { ExtractionMethod::Salary }
}
