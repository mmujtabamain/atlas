//! Decisions (§13, §19, §20, §26; M9): a step-by-step decision builder — the
//! purchase, the down payment and its funding, the recurring payment, other
//! costs — and then the result: the graph, the §19.1 affordability metrics,
//! the funding strategies in the §13.5 format, the §19.2 grid, goal
//! trade-offs (§20), the conditional statement (§19.3) and the §26
//! recommendation contract. Deterministic search, not advice.

use atlas_core::decision::Decision;
use atlas_core::decision::Objective;
use atlas_core::model::Household;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    alert::Alert,
    button::{Button, ButtonVariants as _},
    chart::AreaChart,
    checkbox::Checkbox,
    date_picker::DatePicker,
    form::{Field, Form},
    group_box::GroupBox, h_flex,
    input::Input,
    radio::RadioGroup,
    select::Select,
    stepper::{Stepper, StepperItem},
    table::{Table, TableBody, TableCell, TableHead, TableHeader, TableRow},
    tag::Tag, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::AtlasApp;
use crate::decision_entry::{DecisionDraft, DecisionForm, STEPS};
use crate::widgets::explain::{ExplainContent, open_sheet};
use crate::widgets::labels;
use crate::widgets::master::page_header;
use crate::widgets::table::{money_cell, muted_cell, signed_money_cell};
use atlas_core::EngineError;

#[derive(Clone, Debug)]
pub struct DecisionPoint {
    pub label: SharedString,
    pub baseline: f64,
    pub decision: f64,
    pub reserve: f64,
}

pub fn render(step: usize, form: &DecisionForm, decision: Option<&Result<Decision, EngineError>>, household: &Household, viewer_name: &str, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    v_flex()
        .id("screen-decisions")
        .test_support()
        .gap_6()
        .child(page_header(
            "Decisions",
            "Build a concrete decision step by step: what you buy, how the down payment is funded, what you pay every month, what else it costs — then the graph, the affordability metrics and the funding strategies (§19, §13). Deterministic search under your inputs, never advice (§26).",
            cx,
        ))
        .child(
            Stepper::new("decision-stepper")
                .selected_index(step)
                .items(STEPS.iter().enumerate().map(|(index, title)| StepperItem::new().child(div().text_sm().child(format!("{}. {}", index + 1, title)))))
                .on_click(cx.listener(|this, step: &usize, window, cx| this.go_to_decision_step(*step, window, cx))),
        )
        .child(match step {
            0 => render_step_purchase(form, cx).into_any_element(),
            1 => render_step_down_payment(form, household, cx).into_any_element(),
            2 => render_step_recurring(form, cx).into_any_element(),
            3 => render_step_other(form, cx).into_any_element(),
            _ => match decision {
                Some(Ok(decision)) => render_result(decision, household, viewer_name, cx).into_any_element(),
                Some(Err(err)) => Alert::error("decision-error", err.to_string()).title("The decision could not be evaluated").into_any_element(),
                None => div().text_sm().text_color(theme.muted_foreground).child("Evaluating…").into_any_element(),
            },
        })
        .child(render_nav(step, cx))
}

fn render_nav(step: usize, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    h_flex()
        .gap_2()
        .justify_between()
        .child(
            h_flex()
                .gap_2()
                .child(Button::new("decision-back").outline().label("Back").when(step == 0, |b| b.disabled(true)).on_click(cx.listener(move |this, _, window, cx| this.go_to_decision_step(step.saturating_sub(1), window, cx))))
                .child(Button::new("decision-reset").ghost().label("Start over").on_click(cx.listener(|this, _, window, cx| this.reset_decision(window, cx)))),
        )
        .child(if step < 4 {
            Button::new("decision-next").primary().label(if step == 3 { "Show the result" } else { "Next" }).on_click(cx.listener(move |this, _, window, cx| this.go_to_decision_step(step + 1, window, cx))).into_any_element()
        } else {
            Button::new("decision-save-scenario").primary().icon(IconName::Save).label("Save as scenario").on_click(cx.listener(|this, _, window, cx| this.save_decision_as_scenario(window, cx))).into_any_element()
        })
}

fn render_step_purchase(form: &DecisionForm, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    let draft = form.draft.clone();
    let objective = draft.read(cx).objective;
    GroupBox::new().id("decision-step-purchase").title("Step 1 — What are you buying?").child(
        v_flex()
            .gap_4()
            .child(div().text_xs().text_color(theme.muted_foreground).child("The price, when, and the reserve the household must keep throughout (§19). The purchase window is the range of months the grid evaluates (§19.2)."))
            .child(
                Form::vertical()
                    .columns(2)
                    .child(Field::new().label("What").required(true).child(Input::new(&form.name).id("decision-name")))
                    .child(Field::new().label("Total price").required(true).child(Input::new(&form.price).id("decision-price")))
                    .child(Field::new().label("Purchase date").required(true).child(DatePicker::new(&form.purchase_on)))
                    .child(Field::new().label("Household reserve to keep").child(Input::new(&form.reserve).id("decision-reserve")))
                    .child(Field::new().label("Purchase window from (grid)").child(DatePicker::new(&form.window_from)))
                    .child(Field::new().label("Purchase window to (grid)").child(DatePicker::new(&form.window_to)))
                    .child(
                        Field::new().label("Objective (§13.2 — never assumed)").child(
                            RadioGroup::vertical("decision-objective")
                                .children(Objective::ALL.iter().map(|o| o.label()))
                                .selected_index(Some(objective))
                                .on_change(move |index, _, cx| draft.update(cx, |d, cx| { d.objective = *index; cx.notify(); })),
                        ),
                    ),
            ),
    )
}

fn render_step_down_payment(form: &DecisionForm, household: &Household, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    let draft = form.draft.read(cx).clone();
    let draft_entity = form.draft.clone();
    let sources: Vec<AnyElement> = form
        .source_floors
        .iter()
        .enumerate()
        .map(|(index, (account, floor))| {
            let name = household.account(*account).map(|a| a.name.clone()).unwrap_or_default();
            let balance = household.account(*account).map(|a| a.settled_balance.format()).unwrap_or_default();
            let allowed = draft.source_allowed.get(index).copied().unwrap_or(true);
            let d = draft_entity.clone();
            h_flex()
                .gap_4()
                .items_center()
                .child(div().w_80().flex_shrink_0().child(Checkbox::new(ElementId::Name(format!("decision-source-{}", account.raw()).into())).label(format!("{name} ({balance} today)")).checked(allowed).on_change(move |v, _, cx| d.update(cx, |d, cx| { if let Some(slot) = d.source_allowed.get_mut(index) { *slot = *v; } cx.notify(); }))))
                .child(div().w_64().flex_shrink_0().child(Input::new(floor).small().id(ElementId::Name(format!("decision-floor-{}", account.raw()).into()))))
                .child(div().text_xs().text_color(theme.muted_foreground).child("never below (optional; the account's hard earmarks always apply)"))
                .into_any_element()
        })
        .collect();
    let routes: Vec<AnyElement> = form
        .routes
        .iter()
        .enumerate()
        .map(|(index, (company, to))| {
            let name = household.company(*company).map(|c| c.name.clone()).unwrap_or_default();
            let allowed = draft.route_allowed.get(index).copied().unwrap_or(false);
            let d = draft_entity.clone();
            h_flex()
                .gap_4()
                .items_center()
                .child(div().w_80().flex_shrink_0().child(Checkbox::new(ElementId::Name(format!("decision-route-{}", company.raw()).into())).label(format!("{name} → owner salary")).checked(allowed).on_change(move |v, _, cx| d.update(cx, |d, cx| { if let Some(slot) = d.route_allowed.get_mut(index) { *slot = *v; } cx.notify(); }))))
                .child(div().w_64().flex_shrink_0().child(Select::new(to).small()))
                .child(div().text_xs().text_color(theme.muted_foreground).child("received on this account; withholding and the E07 ceiling apply, legal capacity is never assumed (M27)"))
                .into_any_element()
        })
        .collect();
    GroupBox::new().id("decision-step-down-payment").title("Step 2 — The down payment and where it comes from").child(
        v_flex()
            .gap_4()
            .child(div().text_xs().text_color(theme.muted_foreground).child("The amount paid on the purchase date, funded from the allowed sources in this order (§13.4). Fees and withholding are recomputed on the gross amounts (M25). The grid range evaluates alternatives (§19.2)."))
            .child(
                Form::vertical()
                    .columns(2)
                    .child(Field::new().label("Down payment").required(true).child(Input::new(&form.down_payment).id("decision-down-payment")))
                    .child(Field::new().label("Maximum tax + fees (optional, §13.3)").child(Input::new(&form.max_tax).id("decision-max-tax")))
                    .child(Field::new().label("Grid: lowest down payment").child(Input::new(&form.down_low).id("decision-down-low")))
                    .child(Field::new().label("Grid: highest down payment").child(Input::new(&form.down_high).id("decision-down-high")))
                    .child(Field::new().label("Grid: step").child(Input::new(&form.down_step).id("decision-down-step"))),
            )
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Personal accounts, in funding order"))
            .children(sources)
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Company routes (E07, M27)"))
            .child(if routes.is_empty() { div().text_xs().text_color(theme.muted_foreground).child("No company in the household.").into_any_element() } else { v_flex().gap_2().children(routes).into_any_element() }),
    )
}

fn render_step_recurring(form: &DecisionForm, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    let draft = form.draft.read(cx).clone();
    let d = form.draft.clone();
    GroupBox::new().id("decision-step-recurring").title("Step 3 — The recurring payment").child(
        v_flex()
            .gap_4()
            .child(div().text_xs().text_color(theme.muted_foreground).child("The remainder (price − down payment) as a monthly annuity: term, nominal annual rate, first instalment and the paying account. Untick to pay the full price up front."))
            .child(Checkbox::new("decision-financing").label("Finance the remainder with monthly instalments").checked(draft.financing).on_change(move |v, _, cx| d.update(cx, |d, cx| { d.financing = *v; cx.notify(); })))
            .when(draft.financing, |this| {
                this.child(
                    Form::vertical()
                        .columns(2)
                        .child(Field::new().label("Months").required(true).child(Input::new(&form.months).id("decision-months")))
                        .child(Field::new().label("Annual rate (%)").required(true).child(Input::new(&form.rate).id("decision-rate")))
                        .child(Field::new().label("First instalment").child(DatePicker::new(&form.first_instalment)))
                        .child(Field::new().label("Paid from").child(Select::new(&form.financing_account))),
                )
            }),
    )
}

fn render_step_other(form: &DecisionForm, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    let draft = form.draft.read(cx).clone();
    let d1 = form.draft.clone();
    let d2 = form.draft.clone();
    GroupBox::new().id("decision-step-other").title("Step 4 — Other costs").child(
        v_flex()
            .gap_4()
            .child(div().text_xs().text_color(theme.muted_foreground).child("One-off costs around the purchase (registration, insurance, delivery) and the running costs that follow it every month. Both become events on the timeline when the decision is saved."))
            .child(Checkbox::new("decision-other-cost").label("A one-off cost").checked(draft.other_cost).on_change(move |v, _, cx| d1.update(cx, |d, cx| { d.other_cost = *v; cx.notify(); })))
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
            .child(Checkbox::new("decision-running-cost").label("Monthly running costs").checked(draft.running_cost).on_change(move |v, _, cx| d2.update(cx, |d, cx| { d.running_cost = *v; cx.notify(); })))
            .when(draft.running_cost, |this| {
                this.child(
                    Form::vertical()
                        .columns(2)
                        .child(Field::new().label("What").child(Input::new(&form.running_label).id("decision-running-label")))
                        .child(Field::new().label("Per month").required(true).child(Input::new(&form.running_monthly).id("decision-running-monthly")))
                        .child(Field::new().label("From").child(DatePicker::new(&form.running_from)))
                        .child(Field::new().label("Paid from").child(Select::new(&form.running_account))),
                )
            }),
    )
}

fn render_result(decision: &Decision, household: &Household, viewer_name: &str, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let strategies = render_strategies(decision, cx).into_any_element();
    let grid = render_grid(decision, cx).into_any_element();
    let theme = cx.theme();
    let plan = &decision.plan;
    let per_major = 10f64.powi(household.base_currency.minor_digits() as i32);
    let reserve = plan.reserve.minor() as f64 / per_major;
    let points: Vec<DecisionPoint> = decision
        .merged_path
        .iter()
        .map(|(date, base, dec)| DecisionPoint { label: SharedString::from(date.format("%d %b %y").to_string()), baseline: base.minor() as f64 / per_major, decision: dec.minor() as f64 / per_major, reserve })
        .collect();
    let tick_margin = (points.len() / 8).max(1);
    let baseline_color = theme.chart_1;
    let decision_color = theme.chart_2;
    let reserve_color = theme.danger;
    let background = theme.background;
    let keeps_reserve = decision.statement.claim.contains("keeps");
    let winner = decision.strategies.preferred.map(|i| &decision.strategies.strategies[i]);
    let immediate = decision.immediate_cash.clone();
    let immediate_content = std::rc::Rc::new(ExplainContent::new("Household cash right after the purchase", immediate.money(), immediate.node().clone(), viewer_name, atlas_core::Disclosure::Full));
    v_flex()
        .gap_6()
        .child(
            GroupBox::new().id("decision-summary").title(format!("Result — {} on {}, {} down, {}", plan.name, plan.purchase_on.format("%d %b %Y"), plan.down_payment.format(), plan.financing.as_ref().map(|f| format!("{} instalments of {}", f.months, decision.monthly_payment.format())).unwrap_or_else(|| "paid in full".into()))).child(
                v_flex()
                    .gap_3()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .flex_wrap()
                            .child(if keeps_reserve { Tag::secondary().xsmall().outline().child("keeps the reserve in the conservative case") } else { Tag::danger().xsmall().outline().child("breaches the reserve in the conservative case") })
                            .child(labels::strength_tag(decision.statement.coverage))
                            .child(labels::money_class_tag(atlas_core::vocab::MoneyClass::ConditionalFuture))
                            .child(div().text_xs().text_color(theme.muted_foreground).child(format!("through {}", decision.through.format("%d %b %Y")))),
                    )
                    .child(
                        h_flex()
                            .gap_4()
                            .items_start()
                            .justify_between()
                            .child(div().id("decision-recommendation").test_support().flex_1().min_w_0().text_sm().child(decision.recommendation.action.clone()))
                            .child(Button::new("decision-save-scenario-top").flex_shrink_0().small().primary().icon(IconName::Save).label("Save as scenario").on_click(cx.listener(|this, _, window, cx| this.save_decision_as_scenario(window, cx)))),
                    )
                    .child(div().text_xs().text_color(theme.muted_foreground).child(format!("Objective: {}", decision.recommendation.objective)))
                    .child(
                        h_flex()
                            .gap_4()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(h_flex().gap_1().items_center().child(div().size_2().rounded_full().bg(baseline_color)).child("Baseline"))
                            .child(h_flex().gap_1().items_center().child(div().size_2().rounded_full().bg(decision_color)).child("With the decision"))
                            .child(h_flex().gap_1().items_center().child(div().size_2().rounded_full().bg(reserve_color)).child("Reserve")),
                    )
                    .child(
                        div().h_64().w_full().child(
                            AreaChart::new(points)
                                .x(|p: &DecisionPoint| p.label.clone())
                                .y(|p: &DecisionPoint| p.baseline)
                                .stroke(baseline_color)
                                .fill(linear_gradient(0., linear_color_stop(baseline_color.opacity(0.2), 1.), linear_color_stop(background.opacity(0.05), 0.)))
                                .name("Baseline")
                                .y(|p: &DecisionPoint| p.decision)
                                .stroke(decision_color)
                                .fill(linear_gradient(0., linear_color_stop(decision_color.opacity(0.2), 1.), linear_color_stop(background.opacity(0.0), 0.)))
                                .name("Decision")
                                .y(|p: &DecisionPoint| p.reserve)
                                .stroke(reserve_color)
                                .fill(linear_gradient(0., linear_color_stop(reserve_color.opacity(0.03), 1.), linear_color_stop(background.opacity(0.0), 0.)))
                                .name("Reserve")
                                .tick_margin(tick_margin)
                                .id("decision-area-chart"),
                        ),
                    ),
            ),
        )
        .child(
            GroupBox::new().id("decision-metrics").title("Affordability metrics (§19.1)").child(
                v_flex()
                    .gap_3()
                    .child(
                        h_flex().gap_2().items_center().child(div().text_sm().child(format!("Immediate cash after purchase: {}", immediate.money().format()))).child(
                            Button::new("why-immediate-cash").xsmall().ghost().icon(IconName::CircleQuestionMark).label("Why?").on_click(move |_, window, cx| open_sheet(window, cx, immediate_content.clone())),
                        ),
                    )
                    .child(
                        Table::new()
                            .child(TableHeader::new().child(TableRow::new().child(TableHead::new().w_80().flex_shrink_0().child("Metric")).child(TableHead::new().min_w_0().child("Value")).child(TableHead::new().w_96().flex_shrink_0().child("How"))))
                            .child(TableBody::new().children(decision.metrics.iter().enumerate().map(|(index, m)| {
                                TableRow::new()
                                    .when(index % 2 == 1, |r| r.bg(theme.table_even))
                                    .child(TableCell::new().w_80().flex_shrink_0().child(m.name.clone()))
                                    .child(TableCell::new().min_w_0().overflow_hidden().text_ellipsis().font_family(theme.mono_font_family.clone()).text_sm().child(m.value.clone()))
                                    .child(muted_cell(m.how.clone(), cx).w_96().flex_shrink_0().text_xs().overflow_hidden().text_ellipsis())
                            }))),
                    ),
            ),
        )
        .child(strategies)
        .child(grid)
        .child(
            GroupBox::new().id("decision-goals").title("Goals and trade-offs (§20)").child(if decision.goals.is_empty() {
                div().text_sm().text_color(theme.muted_foreground).child("No goals defined for this household.").into_any_element()
            } else {
                Table::new()
                    .child(
                        TableHeader::new().child(
                            TableRow::new()
                                .child(TableHead::new().w_64().flex_shrink_0().child("Goal"))
                                .child(TableHead::new().w_40().flex_shrink_0().text_right().child("Amount"))
                                .child(TableHead::new().w_32().flex_shrink_0().child("Target"))
                                .child(TableHead::new().w_32().flex_shrink_0().child("Baseline"))
                                .child(TableHead::new().w_32().flex_shrink_0().child("With decision"))
                                .child(TableHead::new().min_w_0().child("Effect")),
                        ),
                    )
                    .child(TableBody::new().children(decision.goals.iter().enumerate().map(|(index, g)| {
                        let fmt = |d: Option<chrono::NaiveDate>| d.map(|d| d.format("%d %b %Y").to_string()).unwrap_or_else(|| "not in window".into());
                        TableRow::new()
                            .when(index % 2 == 1, |r| r.bg(theme.table_even))
                            .child(TableCell::new().w_64().flex_shrink_0().child(g.name.clone()))
                            .child(money_cell(g.amount, cx).w_40().flex_shrink_0())
                            .child(muted_cell(g.target_on.format("%d %b %Y").to_string(), cx).w_32().flex_shrink_0())
                            .child(muted_cell(fmt(g.baseline_reached), cx).w_32().flex_shrink_0())
                            .child(TableCell::new().w_32().flex_shrink_0().child(fmt(g.decision_reached)))
                            .child(TableCell::new().min_w_0().overflow_hidden().text_ellipsis().when(g.delay_days.is_some_and(|d| d > 0), |c| c.text_color(theme.warning)).child(g.text.clone()))
                    })))
                    .into_any_element()
            }),
        )
        .child(
            GroupBox::new().id("decision-statement").title("Conditional result (§19.3) — what this depends on").child(
                v_flex()
                    .gap_2()
                    .child(div().text_sm().child(decision.statement.claim.clone()))
                    .child(div().text_xs().text_color(theme.muted_foreground).child(format!("Through {} · {}", decision.statement.horizon.format("%d %b %Y"), decision.statement.coverage.permitted_claim())))
                    .children(decision.statement.assumptions.iter().map(|a| div().text_xs().pl_4().child(format!("• {a}"))))
                    .child(div().text_xs().text_color(theme.muted_foreground).child(format!("Excluded shocks: {}", decision.statement.excluded_shocks.join("; ")))),
            ),
        )
        .child(
            GroupBox::new().id("decision-contract").title("Recommendation contract (§26)").child(
                v_flex()
                    .gap_2()
                    .child(div().text_sm().child(decision.recommendation.action.clone()))
                    .child(div().text_xs().text_color(theme.muted_foreground).child(format!("Objective: {}", decision.recommendation.objective)))
                    .child(div().text_xs().text_color(theme.muted_foreground).child(format!("Candidates evaluated: {} · feasible: {} · winning strategy: {}", decision.recommendation.candidates_evaluated, decision.recommendation.feasible, decision.recommendation.winning_strategy)))
                    .child(div().text_xs().child("Why:"))
                    .children(decision.recommendation.metrics.iter().map(|m| div().text_xs().pl_4().child(format!("• {m}"))))
                    .child(div().text_xs().child("Constraints:"))
                    .children(decision.recommendation.constraints.iter().map(|c| div().text_xs().pl_4().child(format!("• {c}"))))
                    .child(div().text_xs().child("Alternatives:"))
                    .children(decision.recommendation.alternatives.iter().map(|a| div().text_xs().pl_4().child(format!("• {a}"))))
                    .child(div().text_xs().child("Applied rules:"))
                    .children(decision.recommendation.applied_rules.iter().map(|r| div().text_xs().pl_4().child(format!("• {r}"))))
                    .child(div().text_xs().text_color(theme.muted_foreground).child(decision.recommendation.explanation.clone()))
                    .when_some(winner, |this, w| this.child(div().text_xs().text_color(theme.muted_foreground).child(format!("Winning strategy caveats: {}", if w.caveats.is_empty() { "none".to_string() } else { w.caveats.join(" ") })))),
            ),
        )
}

fn render_strategies(decision: &Decision, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    let report = &decision.strategies;
    GroupBox::new().id("decision-strategies").title("Funding strategies for the down payment (§13.5)").child(
        v_flex()
            .gap_4()
            .child(div().id("decision-strategy-status").test_support().text_sm().child(report.status.clone()))
            .child(div().text_xs().text_color(theme.muted_foreground).child(format!("Search space: {}.", report.search_space)))
            .children(report.strategies.iter().enumerate().map(|(index, s)| {
                let preferred = report.preferred == Some(index);
                v_flex()
                    .gap_1()
                    .px_3()
                    .py_2()
                    .rounded(theme.radius)
                    .when(index % 2 == 1, |c| c.bg(theme.table_even))
                    .when(preferred, |c| c.border_1().border_color(theme.primary))
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .flex_wrap()
                            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(format!("Strategy {} — {}", index + 1, s.name)))
                            .child(if s.feasible { Tag::secondary().xsmall().outline().child("feasible") } else { Tag::danger().xsmall().outline().child("infeasible") })
                            .when(preferred, |row| row.child(Tag::secondary().xsmall().child(format!("preferred under “{}”", report.objective.label())))),
                    )
                    .children(s.steps.iter().map(|step| {
                        div().text_xs().pl_4().child(format!(
                            "• {} {} gross from {} → {} net{}{} · ending balance {} (floor {}) · {}",
                            if step.withholding.is_positive() { "Extract" } else { "Withdraw" },
                            step.gross.format(),
                            step.source,
                            step.net.format(),
                            if step.withholding.is_positive() { format!(" · withholding {}", step.withholding.format()) } else { String::new() },
                            if step.fees.is_positive() { format!(" · fees {}", step.fees.format()) } else { String::new() },
                            step.ending_balance.format(),
                            step.floor.format(),
                            step.note
                        ))
                    }))
                    .child(div().text_xs().pl_4().child(format!(
                        "• Estimated immediate tax: {} · bank fees: {} · net household cash: {} · future incremental tax: {} ({}) · transfers: {}",
                        s.immediate_tax.format(),
                        s.fees.format(),
                        s.net.format(),
                        s.future_tax.format(),
                        s.future_tax_note,
                        s.transfers
                    )))
                    .child(div().text_xs().pl_4().child(format!("• Reserve constraints: {}", if s.violations.is_empty() { "satisfied".to_string() } else { s.violations.join("; ") })))
                    .children(s.caveats.iter().map(|c| div().text_xs().pl_4().text_color(theme.muted_foreground).child(format!("• {c}"))))
            })),
    )
}

fn render_grid(decision: &Decision, cx: &mut Context<AtlasApp>) -> impl IntoElement {
    let theme = cx.theme();
    let mut dates: Vec<chrono::NaiveDate> = decision.grid.iter().map(|c| c.purchase_on).collect();
    dates.dedup();
    let mut amounts: Vec<atlas_core::Money> = decision.grid.iter().map(|c| c.down_payment).collect();
    amounts.sort_by_key(|m| m.minor());
    amounts.dedup();
    GroupBox::new().id("decision-grid").title("Purchase month × down payment (§19.2, E03) — lowest household cash after purchase, conservative case").child(
        v_flex()
            .gap_3()
            .child(div().id("decision-grid-status").test_support().text_sm().child(decision.grid_status.clone()))
            .child(div().text_xs().text_color(theme.muted_foreground).child("Each cell is a full timeline evaluation of that combination under the conservative case (lowest inflows, highest outflows, latest receipts). Red cells fall below the reserve (the shortfall is in the list below); the best cell under the objective is marked ★."))
            .child(
                Table::new()
                    .child(TableHeader::new().child(TableRow::new().child(TableHead::new().w_32().flex_shrink_0().child("Purchase on")).children(amounts.iter().map(|a| TableHead::new().flex_1().min_w_0().text_right().child(a.format())))))
                    .child(TableBody::new().children(dates.iter().enumerate().map(|(index, date)| {
                        TableRow::new().when(index % 2 == 1, |r| r.bg(theme.table_even)).child(TableCell::new().w_32().flex_shrink_0().child(date.format("%d %b %Y").to_string())).children(amounts.iter().map(|amount| {
                            match decision.grid.iter().find(|c| c.purchase_on == *date && c.down_payment == *amount) {
                                Some(cell) => {
                                    let text = cell.lowest.format();
                                    TableCell::new()
                                        .flex_1()
                                        .min_w_0()
                                        .text_right()
                                        .font_family(theme.mono_font_family.clone())
                                        .when(!cell.reserve_ok, |c| c.text_color(theme.danger))
                                        .when(cell.best, |c| c.font_weight(FontWeight::BOLD).text_color(theme.primary))
                                        .child(if cell.best { format!("★ {text}") } else { text })
                                }
                                None => muted_cell("–", cx).flex_1().min_w_0().text_right(),
                            }
                        }))
                    }))),
            )
            .child(
                Table::new()
                    .child(
                        TableHeader::new().child(
                            TableRow::new()
                                .child(TableHead::new().w_32().flex_shrink_0().child("Purchase on"))
                                .child(TableHead::new().w_40().flex_shrink_0().text_right().child("Down payment"))
                                .child(TableHead::new().w_40().flex_shrink_0().text_right().child("Lowest cash"))
                                .child(TableHead::new().w_32().flex_shrink_0().child("Lowest on"))
                                .child(TableHead::new().w_40().flex_shrink_0().text_right().child("Reserve shortfall"))
                                .child(TableHead::new().w_40().flex_shrink_0().text_right().child("Financing cost"))
                                .child(TableHead::new().min_w_0().child("Reserve condition")),
                        ),
                    )
                    .child(TableBody::new().children(decision.grid.iter().enumerate().map(|(index, cell)| {
                        TableRow::new()
                            .when(index % 2 == 1, |r| r.bg(theme.table_even))
                            .child(TableCell::new().w_32().flex_shrink_0().child(cell.purchase_on.format("%d %b %Y").to_string()))
                            .child(money_cell(cell.down_payment, cx).w_40().flex_shrink_0())
                            .child(money_cell(cell.lowest, cx).w_40().flex_shrink_0())
                            .child(muted_cell(cell.lowest_on.map(|d| d.format("%d %b %Y").to_string()).unwrap_or_else(|| "–".into()), cx).w_32().flex_shrink_0())
                            .child(signed_money_cell(cell.shortfall.negated(), cx).w_40().flex_shrink_0())
                            .child(money_cell(cell.financing_cost, cx).w_40().flex_shrink_0())
                            .child(TableCell::new().min_w_0().child(h_flex().gap_2().child(if cell.reserve_ok { Tag::secondary().xsmall().outline().child("maintained under these assumptions") } else { Tag::danger().xsmall().outline().child(format!("breached by {}", cell.shortfall.format())) }).when(cell.best, |c| c.child(Tag::secondary().xsmall().child("best on the grid")))))
                    }))),
            ),
    )
}

#[allow(dead_code)]
fn _draft_type_check(_: DecisionDraft) {}
