//! Rules & taxes / Rules: the register with its precedence and versions, the
//! rule detail (what it does, the decisions it took, a with-and-without
//! simulation that changes nothing, its history), the create-rule flow, and
//! the rule-activity report.

use atlas_core::ids::RuleId;
use atlas_core::model::Household;
use atlas_core::rules::{Rule, RuleDecision, TieBreak};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _, DropdownButton},
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
    switch::Switch,
    tab::{Tab, TabBar},
    tag::Tag,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::common::{detail_header, workspace_header};
use crate::app::AtlasApp;
use crate::models::rules::RulesModel;
use crate::nav::{Destination, Route};
use crate::rule_builder::{ACTION_KINDS, CONDITION_KINDS, SCOPE_KINDS, STEPS, TRIGGERS};
use crate::widgets::grid;
use crate::widgets::record::{self, Lane};
use crate::widgets::scope;
use crate::widgets::states::{count_line, empty_state, fact, lanes, note, section};

fn date(d: chrono::NaiveDate) -> String {
    d.format("%d %b %Y").to_string()
}

const LANES: [(&str, Lane); 6] = [
    ("Rule", Lane::fixed(260.)),
    ("Enabled", Lane::fixed(90.)),
    ("Priority", Lane::fixed(120.)),
    ("Version", Lane::fixed(80.)),
    ("What it does", Lane::flex()),
    ("", Lane::fixed(60.)),
];

// ----- Register ----------------------------------------------------------------

pub fn render_register(app: &AtlasApp, model: &RulesModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let header = workspace_header(
        Destination::RulesTaxes,
        Route::Rules,
        vec![Button::new("create-rule").small().outline().icon(IconName::Plus).label("Create rule").on_click(cx.listener(|this, _, window, cx| this.start_rule_flow(window, cx))).into_any_element()],
        cx,
    );
    let selected = app.selected_rule;
    let enabled_count = model.rules.iter().filter(|r| r.enabled).count();
    let rows: Vec<_> = model
        .rules
        .iter()
        .map(|rule| {
            let id = rule.id;
            let enabled = rule.enabled;
            let scenario_name = rule.scenario.and_then(|s| household.scenario(s)).map(|s| s.name.clone());
            record::row(
                SharedString::from(format!("rule-row-{}", id.raw())),
                selected == Some(id),
                vec![
                    (LANES[0].1, v_flex().min_w_0().child(div().text_sm().child(rule.name.clone())).child(div().text_xs().text_color(cx.theme().muted_foreground).child(match &scenario_name {
                        Some(name) => format!("Only under scenario “{name}”"),
                        None => format!("{} · specificity {}", rule.scope.describe(household), rule.scope.specificity()),
                    })).into_any_element()),
                    (LANES[1].1, h_flex().child(Switch::new(ElementId::Name(format!("rule-enabled-{}", id.raw()).into())).checked(enabled).on_click(cx.listener(move |this, _, window, cx| this.toggle_rule(id, window, cx)))).into_any_element()),
                    (
                        LANES[2].1,
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(Button::new(SharedString::from(format!("rule-lower-{}", id.raw()))).xsmall().ghost().compact().icon(IconName::ChevronDown).tooltip("Lower priority by one (new version)").on_click(cx.listener(move |this, _, window, cx| this.bump_rule_priority(id, -1, window, cx))))
                            .child(div().font_family(cx.theme().mono_font_family.clone()).text_sm().child(rule.priority.to_string()))
                            .child(Button::new(SharedString::from(format!("rule-raise-{}", id.raw()))).xsmall().ghost().compact().icon(IconName::ChevronUp).tooltip("Raise priority by one (new version)").on_click(cx.listener(move |this, _, window, cx| this.bump_rule_priority(id, 1, window, cx))))
                            .into_any_element(),
                    ),
                    (LANES[3].1, record::muted(format!("v{}", rule.version), cx)),
                    (LANES[4].1, v_flex().min_w_0().child(div().text_sm().whitespace_normal().child(format!("When {} in {}: {}", rule.trigger.label(), rule.scope.describe(household), rule.action.describe(household)))).child(div().text_xs().text_color(cx.theme().muted_foreground).child(format!("Effective {} – {}", date(rule.effective_from), rule.effective_to.map(date).unwrap_or_else(|| "open".into())))).into_any_element()),
                    (LANES[5].1, h_flex().justify_end().child(if enabled { Tag::secondary().xsmall().outline().child("On") } else { Tag::warning().xsmall().outline().child("Off") }).into_any_element()),
                ],
                move |_, _, cx| crate::app::with_app(cx, |app, cx| app.select_rule(id, cx)),
            )
        })
        .collect();
    let theme = cx.theme();
    let footer = selected.and_then(|id| model.rules.iter().find(|r| r.id == id)).map(|rule| {
        let id = rule.id;
        h_flex()
            .w_full()
            .justify_end()
            .items_center()
            .gap_2()
            .child(div().flex_1().text_xs().text_color(theme.muted_foreground).child(format!("Selected: {}", rule.name)))
            .child(Button::new("rule-open").small().outline().label("Open rule").on_click(cx.listener(move |this, _, _, cx| this.navigate(Route::Rule(id), cx))))
            .child(Button::new("rule-simulate").small().ghost().label("Simulate").on_click(cx.listener(move |this, _, _, cx| this.open_rule_simulation(id, cx))))
            .child(
                DropdownButton::new("rule-more").small().button(Button::new("rule-more-button").small().ghost().label("More")).dropdown_menu(move |menu, _, _| {
                    menu.item(PopupMenuItem::new("Delete rule…").on_click(move |_, window, cx| crate::app::with_app(cx, |app, cx| app.confirm_delete_rule(id, window, cx))))
                }),
            )
    });

    v_flex()
        .id("screen-rules")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(scope::bar(
            vec![
                scope::select("Tie-break", &app.tie_break_choice, px(260.), cx).into_any_element(),
                scope::select("Plan", &app.plan_choices.rules, px(200.), cx).into_any_element(),
            ],
            Some(format!("Resolution order: explicit priority, then scope specificity, then the tie-break ({}). Disabling or re-prioritising records a new version.", model.tie_break.label())),
            cx,
        ))
        .child(count_line(model.rules.len(), model.rules.len(), "rules", cx))
        .child(div().text_xs().text_color(theme.muted_foreground).child(format!("{enabled_count} enabled · {} disabled but kept with their history", model.rules.len() - enabled_count)))
        .child(if model.rules.is_empty() {
            empty_state("rules-empty", "No user rules yet", "A rule adds a fee, classifies a category, or steers which account funds a purchase. Without one, no fee is charged by this app — that is not a claim about your bank's charges.", Some(Button::new("rules-add-first").small().outline().icon(IconName::Plus).label("Create rule").on_click(cx.listener(|this, _, window, cx| this.start_rule_flow(window, cx))).into_any_element()), cx)
        } else {
            record::list("rules-list", record::header(&LANES, cx), rows).into_any_element()
        })
        .children(footer)
        .into_any_element()
}

// ----- Detail ------------------------------------------------------------------

pub fn render_detail(app: &AtlasApp, id: RuleId, model: &RulesModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let Some(rule) = model.rules.iter().find(|r| r.id == id) else {
        return v_flex().id("screen-rule").test_support().gap_4().child(detail_header(Destination::RulesTaxes, Route::Rules, "Rules", "Unknown rule", None, vec![], cx)).child(note("This rule no longer exists.", cx)).into_any_element();
    };
    let enabled = rule.enabled;
    let subtitle = h_flex()
        .gap_2()
        .items_center()
        .flex_wrap()
        .child(div().text_sm().text_color(cx.theme().muted_foreground).child(format!("When {} in {}: {}", rule.trigger.label(), rule.scope.describe(household), rule.action.describe(household))))
        .child(Tag::secondary().xsmall().outline().child(format!("v{}", rule.version)))
        .when_some(rule.scenario.and_then(|s| household.scenario(s)).map(|s| s.name.clone()), |this, name| this.child(Tag::info().xsmall().outline().child(format!("Only under “{name}”"))))
        .into_any_element();
    let header = detail_header(
        Destination::RulesTaxes,
        Route::Rules,
        "Rules",
        rule.name.clone(),
        Some(subtitle),
        vec![
            h_flex()
                .gap_2()
                .items_center()
                .child(div().text_xs().text_color(cx.theme().muted_foreground).child("Enabled"))
                .child(Switch::new("rule-detail-enabled").checked(enabled).on_click(cx.listener(move |this, _, window, cx| this.toggle_rule(id, window, cx))))
                .child(Button::new("rule-detail-lower").xsmall().ghost().compact().icon(IconName::ChevronDown).tooltip("Lower priority by one").on_click(cx.listener(move |this, _, window, cx| this.bump_rule_priority(id, -1, window, cx))))
                .child(div().font_family(cx.theme().mono_font_family.clone()).text_sm().child(rule.priority.to_string()))
                .child(Button::new("rule-detail-raise").xsmall().ghost().compact().icon(IconName::ChevronUp).tooltip("Raise priority by one").on_click(cx.listener(move |this, _, window, cx| this.bump_rule_priority(id, 1, window, cx))))
                .into_any_element(),
            DropdownButton::new("rule-detail-more")
                .small()
                .button(Button::new("rule-detail-more-button").small().ghost().label("More"))
                .dropdown_menu(move |menu, _, _| menu.item(PopupMenuItem::new("Delete rule…").on_click(move |_, window, cx| crate::app::with_app(cx, |app, cx| app.confirm_delete_rule(id, window, cx)))))
                .into_any_element(),
        ],
        cx,
    );
    let tab = app.rule_tab;
    let body: AnyElement = match tab {
        1 => render_decisions_for(app, rule, model, household, cx),
        2 => render_simulation(app, rule, model, household, cx),
        3 => render_history(rule, cx),
        _ => render_rule_details(rule, household, cx),
    };
    v_flex()
        .id("screen-rule")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(
            v_flex()
                .w_full()
                .gap_4()
                .child(
                    TabBar::new("rule-tabs")
                        .selected_index(tab)
                        .on_click(cx.listener(|this, index: &usize, _, cx| {
                            this.rule_tab = *index;
                            cx.notify();
                        }))
                        .children([Tab::new().label("Details"), Tab::new().label("Decisions"), Tab::new().label("Simulation"), Tab::new().label("History")]),
                )
                .child(body),
        )
        .into_any_element()
}

fn render_rule_details(rule: &Rule, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let conditions: Vec<String> = rule.conditions.iter().map(|c| c.describe(household)).collect();
    section("rule-details", "What this rule does")
        .child(div().text_sm().child(format!("When {} in {}: {}", rule.trigger.label(), rule.scope.describe(household), rule.action.describe(household))))
        .child(
            DescriptionList::new()
                .columns(2)
                .child(DescriptionItem::new("Trigger").value(rule.trigger.label()))
                .child(DescriptionItem::new("Scope").value(format!("{} · specificity {}", rule.scope.describe(household), rule.scope.specificity())))
                .child(DescriptionItem::new("Conditions").value(if conditions.is_empty() { "No additional conditions".to_string() } else { conditions.join(" and ") }))
                .child(DescriptionItem::new("Effective").value(format!("{} – {}", date(rule.effective_from), rule.effective_to.map(date).unwrap_or_else(|| "open".into()))))
                .child(DescriptionItem::new("Applies under").value(rule.scenario.and_then(|s| household.scenario(s)).map(|s| format!("Scenario “{}” only", s.name)).unwrap_or_else(|| "The baseline and every scenario".into())))
                .child(DescriptionItem::new("Status").value(if rule.enabled { format!("Enabled, version {}", rule.version) } else { format!("Disabled, version {}", rule.version) })),
        )
        .child(note(rule.explanation.clone(), cx))
        .into_any_element()
}

const DECISION_LANES: [(&str, Lane); 6] = [
    ("Date", Lane::fixed(120.)),
    ("Occurrence / account", Lane::flex()),
    ("Action", Lane::fixed(150.)),
    ("Candidates", Lane::fixed(100.)),
    ("Resolved by", Lane::fixed(240.)),
    ("Winner", Lane::fixed(200.)),
];

fn decision_rows(decisions: &[RuleDecision], household: &Household, expanded: Option<usize>, cx: &mut Context<AtlasApp>) -> Vec<gpui_kit::component::list::ListItem> {
    decisions
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let account = household.account(d.account).map(|a| a.name.clone()).unwrap_or_default();
            let winner = d.chosen.and_then(|r| household.rules.iter().find(|x| x.id == r)).map(|r| r.name.clone()).unwrap_or_else(|| "None applied".into());
            record::row(
                SharedString::from(format!("decision-{i}")),
                expanded == Some(i),
                vec![
                    (DECISION_LANES[0].1, record::text(date(d.date))),
                    (DECISION_LANES[1].1, record::stack(d.occurrence_label.clone(), account, cx)),
                    (DECISION_LANES[2].1, record::muted(d.action_kind, cx)),
                    (DECISION_LANES[3].1, record::muted(d.candidates.len().to_string(), cx)),
                    (DECISION_LANES[4].1, record::muted(d.resolution.clone(), cx)),
                    (DECISION_LANES[5].1, h_flex().gap_2().items_center().child(record::text(winner)).child(Button::new(SharedString::from(format!("decision-open-{i}"))).xsmall().ghost().compact().label("Open…").on_click(cx.listener(move |this, _, _, cx| {
                        this.rule_decision_expanded = if this.rule_decision_expanded == Some(i) { None } else { Some(i) };
                        cx.notify();
                    }))).into_any_element()),
                ],
                |_, _, _| {},
            )
        })
        .collect()
}

fn candidate_table(decision: &RuleDecision, cx: &mut Context<AtlasApp>) -> AnyElement {
    let lanes_def: [(&str, Lane); 4] = [("Candidate", Lane::fixed(260.)), ("Priority", Lane::fixed(100.)), ("Specificity", Lane::fixed(120.)), ("Outcome", Lane::flex())];
    let rows: Vec<_> = decision
        .candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            record::row(
                SharedString::from(format!("candidate-{i}")),
                decision.chosen == Some(c.rule),
                vec![
                    (lanes_def[0].1, record::text(c.name.clone())),
                    (lanes_def[1].1, record::muted(c.priority.to_string(), cx)),
                    (lanes_def[2].1, record::muted(c.specificity.to_string(), cx)),
                    (lanes_def[3].1, record::muted(c.outcome.clone(), cx)),
                ],
                |_, _, _| {},
            )
        })
        .collect();
    v_flex().w_full().gap_1().px_3().py_2().child(record::list("candidates", record::header(&lanes_def, cx), rows)).child(note(decision.resolution.clone(), cx)).into_any_element()
}

fn render_decisions_for(app: &AtlasApp, rule: &Rule, model: &RulesModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let id = rule.id;
    let expanded = app.rule_decision_expanded;
    let decisions: Vec<RuleDecision> = model.evaluation.decisions.iter().filter(|d| d.candidates.iter().any(|c| c.rule == id)).cloned().collect();
    let fees: Vec<_> = model.evaluation.fees.iter().filter(|f| f.rule == id).collect();
    let fee_lanes: [(&str, Lane); 4] = [("Date", Lane::fixed(120.)), ("Account", Lane::fixed(220.)), ("Event", Lane::flex()), ("Amount", Lane::money(150.))];
    let fee_rows: Vec<_> = fees
        .iter()
        .enumerate()
        .map(|(i, f)| {
            record::row(
                SharedString::from(format!("rule-fee-{i}")),
                false,
                vec![
                    (fee_lanes[0].1, record::text(date(f.date))),
                    (fee_lanes[1].1, record::muted(household.account(f.account).map(|a| a.name.clone()).unwrap_or_default(), cx)),
                    (fee_lanes[2].1, record::text(f.label.clone())),
                    (fee_lanes[3].1, record::money(f.amount, cx)),
                ],
                |_, _, _| {},
            )
        })
        .collect();
    v_flex()
        .w_full()
        .gap_6()
        .child(
            section("rule-decisions", "Decisions this rule took part in")
                .description("Every occurrence where the rule was a candidate, including the ones it lost. Single-candidate decisions are listed for completeness.")
                .child(if decisions.is_empty() { note("This rule was not a candidate for any occurrence in the window.", cx).into_any_element() } else { record::list("rule-decisions-list", record::header(&DECISION_LANES, cx), decision_rows(&decisions, household, expanded, cx)).into_any_element() })
                .children(expanded.and_then(|i| decisions.get(i)).map(|d| candidate_table(d, cx))),
        )
        .child(section("rule-fees", "Fee postings from this rule").child(if fee_rows.is_empty() { note("No fee posting came from this rule in the window.", cx).into_any_element() } else { record::list("rule-fees-list", record::header(&fee_lanes, cx), fee_rows).into_any_element() }))
        .into_any_element()
}

fn render_simulation(app: &AtlasApp, rule: &Rule, model: &RulesModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let id = rule.id;
    let ran = model.simulated == Some(id);
    let theme = cx.theme();
    let body: AnyElement = match (&model.simulation, ran) {
        (Some(sim), true) => {
            let lanes_def: [(&str, Lane); 4] = [("Metric", Lane::fixed(240.)), ("With the rule", Lane::money(190.)), ("Without it", Lane::money(190.)), ("Difference", Lane::flex())];
            let rows = vec![
                ("Cash at the end", sim.with_rule.end.money(), sim.without_rule.end.money(), sim.end_delta),
                ("Lowest cash", sim.with_rule.lowest.money(), sim.without_rule.lowest.money(), sim.lowest_delta),
            ];
            let list: Vec<_> = rows
                .into_iter()
                .enumerate()
                .map(|(i, (name, a, b, delta))| {
                    record::row(
                        SharedString::from(format!("simulation-{i}")),
                        false,
                        vec![
                            (lanes_def[0].1, record::text(name)),
                            (lanes_def[1].1, record::money(a, cx)),
                            (lanes_def[2].1, record::money(b, cx)),
                            (lanes_def[3].1, record::muted(delta.format_signed(), cx)),
                        ],
                        |_, _, _| {},
                    )
                })
                .collect();
            v_flex()
                .w_full()
                .gap_3()
                .child(record::list("simulation-list", record::header(&lanes_def, cx), list))
                .child(lanes([
                    fact("Fee difference", sim.fee_delta.format_signed(), cx).into_any_element(),
                    fact("Rule's own status", if sim.currently_enabled { "Enabled in the household".to_string() } else { "Disabled in the household".to_string() }, cx).into_any_element(),
                    fact("Floor with the rule", sim.with_rule.breach.summary(), cx).into_any_element(),
                    fact("Floor without it", sim.without_rule.breach.summary(), cx).into_any_element(),
                ]))
                .child(if sim.end_delta.is_zero() && sim.lowest_delta.is_zero() {
                    note("No difference in this window under this plan. That is a fact about this period, not about the rule in general.", cx).into_any_element()
                } else {
                    div().into_any_element()
                })
                .child(note("This simulation changes nothing: the rule keeps its own enabled state.", cx))
                .into_any_element()
        }
        _ => note("Not run yet. Choose the plan and run the simulation; nothing is applied.", cx).into_any_element(),
    };
    section("rule-simulation", "With and without this rule")
        .description(format!("Household · Expected case · through {}. The rule is evaluated enabled and disabled; the household is untouched.", date(model.through)))
        .action(
            h_flex()
                .gap_2()
                .items_center()
                .child(scope::select("Plan", &app.plan_choices.rules, px(200.), cx))
                .child(Button::new("run-rule-simulation").small().primary().icon(IconName::Play).label("Run simulation").on_click(cx.listener(move |this, _, _, cx| this.simulate_rule(id, cx)))),
        )
        .child(body)
        .child(div().text_xs().text_color(theme.muted_foreground).child(format!("Scope: {} · {}", household.base_currency.code(), if rule.enabled { "the rule is currently enabled" } else { "the rule is currently disabled" })))
        .into_any_element()
}

fn render_history(rule: &Rule, cx: &mut Context<AtlasApp>) -> AnyElement {
    let lanes_def: [(&str, Lane); 3] = [("Version", Lane::fixed(110.)), ("Changed", Lane::fixed(160.)), ("What changed", Lane::flex())];
    let rows: Vec<_> = rule
        .history
        .iter()
        .enumerate()
        .map(|(i, h)| {
            record::row(
                SharedString::from(format!("rule-version-{i}")),
                false,
                vec![(lanes_def[0].1, record::text(format!("v{}", h.version))), (lanes_def[1].1, record::muted(date(h.changed_on), cx)), (lanes_def[2].1, record::text(h.summary.clone()))],
                |_, _, _| {},
            )
        })
        .collect();
    section("rule-history", "History")
        .description("Every version, so an old forecast can still be explained. Read-only.")
        .child(if rows.is_empty() { note(format!("Version {} is the first; nothing has changed it yet.", rule.version), cx).into_any_element() } else { record::list("rule-history-list", record::header(&lanes_def, cx), rows).into_any_element() })
        .into_any_element()
}

// ----- Rule activity -------------------------------------------------------------

pub fn render_activity(app: &AtlasApp, model: &RulesModel, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    grid::sync(&app.grids.rule_fees, &model.fee_rows, cx);
    let header = workspace_header(Destination::RulesTaxes, Route::RuleActivity, vec![], cx);
    let tab = app.activity_report_tab;
    let conflicts_only = app.rule_conflicts_only;
    let rule_filter_row = scope::selected(&app.rule_filter_choice, cx);
    let rule_filter = if rule_filter_row == 0 { None } else { model.rules.get(rule_filter_row - 1).map(|r| r.id) };
    let decisions: Vec<RuleDecision> = model
        .evaluation
        .decisions
        .iter()
        .filter(|d| !conflicts_only || d.candidates.len() > 1)
        .filter(|d| rule_filter.is_none_or(|id| d.candidates.iter().any(|c| c.rule == id)))
        .cloned()
        .collect();
    let expanded = app.rule_decision_expanded.and_then(|i| decisions.get(i)).map(|d| candidate_table(d, cx));
    let theme = cx.theme();
    let body: AnyElement = if tab == 1 {
        v_flex()
            .w_full()
            .gap_2()
            .child(if model.evaluation.fees.is_empty() {
                note("No fee posting in this window. That is the app's own rules only; it is not a claim about your bank's charges.", cx).into_any_element()
            } else {
                grid::render("rule-fees-grid", &app.grids.rule_fees, cx).into_any_element()
            })
            .child(div().text_xs().text_color(theme.muted_foreground).child(format!("Total fees {} · through {} · expected case", model.fee_total.format(), date(model.through))))
            .into_any_element()
    } else {
        v_flex()
            .w_full()
            .gap_1()
            .child(if decisions.is_empty() {
                note(if conflicts_only { "No decision in this window had competing candidates." } else { "No rule applied to any occurrence in this window." }, cx).into_any_element()
            } else {
                record::list("activity-decisions", record::header(&DECISION_LANES, cx), decision_rows(&decisions, household, app.rule_decision_expanded, cx)).into_any_element()
            })
            .children(expanded)
            .into_any_element()
    };

    v_flex()
        .id("screen-rule-activity")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(scope::bar(
            vec![scope::select("Plan", &app.plan_choices.rules, px(200.), cx).into_any_element(), scope::fixed("Case", "Expected", cx).into_any_element(), scope::fixed("Through", date(model.through), cx).into_any_element()],
            Some("Scenario-scoped rules take part only under their own plan.".into()),
            cx,
        ))
        .child(lanes([
            fact("Decisions", model.evaluation.decisions.len().to_string(), cx).into_any_element(),
            fact("With competing candidates", model.conflicts.len().to_string(), cx).into_any_element(),
            fact("Fees added", model.fee_total.format(), cx).into_any_element(),
        ]))
        .child(
            h_flex()
                .w_full()
                .gap_4()
                .items_center()
                .flex_wrap()
                .child(
                    TabBar::new("activity-tabs")
                        .selected_index(tab)
                        .on_click(cx.listener(|this, index: &usize, _, cx| {
                            this.activity_report_tab = *index;
                            this.rule_decision_expanded = None;
                            cx.notify();
                        }))
                        .children([Tab::new().label("Decisions"), Tab::new().label("Fee postings")]),
                )
                .child(scope::select("Rule", &app.rule_filter_choice, px(240.), cx))
                .child(Checkbox::new("conflicts-only").label("Competing candidates only").checked(conflicts_only).on_change(cx.listener(|this, v, _, cx| {
                    this.rule_conflicts_only = *v;
                    this.rule_decision_expanded = None;
                    cx.notify();
                })))
                .child(Button::new("activity-clear").small().ghost().label("Clear").disabled(!conflicts_only && rule_filter.is_none()).on_click(cx.listener(|this, _, window, cx| {
                    this.rule_conflicts_only = false;
                    AtlasApp::set_choice(&this.rule_filter_choice, 0, window, cx);
                    this.rule_decision_expanded = None;
                    cx.notify();
                }))),
        )
        .child(body)
        .child(
            h_flex()
                .gap_2()
                .child(Button::new("activity-funding-order").small().ghost().icon(IconName::Landmark).label("Funding order").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Funding, cx))))
                .child(Button::new("activity-expense-selection").small().ghost().label("Expense account selection").on_click(cx.listener(|this, _, _, cx| this.navigate(Route::Funding, cx)))),
        )
        .into_any_element()
}

// ----- Create rule ---------------------------------------------------------------

pub fn render_create(app: &AtlasApp, household: &Household, cx: &mut Context<AtlasApp>) -> AnyElement {
    let b = &app.rule_builder;
    let step = b.step.min(3);
    let preview = b.preview(household, cx);
    let header = detail_header(Destination::RulesTaxes, Route::Rules, "Rules", "Create rule", None, vec![], cx);
    let theme = cx.theme();
    let body: AnyElement = match step {
        0 => section("create-rule-scope", "Scope")
            .description("What the rule applies to, and how it ranks against others. A higher priority wins; specificity breaks the rest.")
            .child(
                Form::vertical()
                    .columns(2)
                    .child(Field::new().label("Name").required(true).child(Input::new(&b.name).id("rule-name")))
                    .child(Field::new().label("Priority (higher wins)").child(Input::new(&b.priority).id("rule-priority")))
                    .child(
                        Field::new().label("Applies to").child(
                            RadioGroup::vertical("rule-scope-kind")
                                .children(SCOPE_KINDS)
                                .selected_index(Some(b.scope_kind))
                                .on_change(cx.listener(|this, index: &usize, _, cx| {
                                    this.rule_builder.scope_kind = *index;
                                    cx.notify();
                                })),
                        ),
                    )
                    .children(match b.scope_kind {
                        1 => vec![Field::new().label("Category").child(Select::new(&b.category))],
                        2 => vec![Field::new().label("Institution").child(Select::new(&b.institution))],
                        3 => vec![Field::new().label("Person").child(Select::new(&b.person))],
                        4 => vec![Field::new().label("Company").child(Select::new(&b.company))],
                        5 => vec![Field::new().label("Account").child(Select::new(&b.account))],
                        6 => vec![Field::new().label("Scenario").child(Select::new(&b.scenario))],
                        _ => vec![],
                    }),
            )
            .when(b.scope_kind == 6, |this| this.child(note("A scenario scope also means the rule applies only under that scenario's plan; it does not switch any scenario on.", cx)))
            .into_any_element(),
        1 => {
            let condition_rows: Vec<AnyElement> = b
                .conditions
                .iter()
                .enumerate()
                .map(|(i, row)| {
                    let field: AnyElement = match row.kind {
                        0 | 1 => div().w(px(220.)).child(Input::new(&row.money).small().id(ElementId::Name(format!("condition-money-{i}").into()))).into_any_element(),
                        2 => div().w(px(220.)).child(Input::new(&row.text).small().id(ElementId::Name(format!("condition-text-{i}").into()))).into_any_element(),
                        3 => div().w(px(220.)).child(Select::new(&row.account).small()).into_any_element(),
                        4 => div().w(px(220.)).text_xs().text_color(theme.muted_foreground).child("No value needed").into_any_element(),
                        _ => div().w(px(220.)).child(DatePicker::new(&row.date).small()).into_any_element(),
                    };
                    h_flex()
                        .w_full()
                        .gap_3()
                        .items_center()
                        .child(
                            div().w(px(360.)).child(
                                RadioGroup::vertical(ElementId::Name(format!("condition-kind-{i}").into()))
                                    .children(CONDITION_KINDS)
                                    .selected_index(Some(row.kind))
                                    .on_change(cx.listener(move |this, index: &usize, _, cx| this.set_rule_condition_kind(i, *index, cx))),
                            ),
                        )
                        .child(field)
                        .child(Button::new(SharedString::from(format!("condition-remove-{i}"))).xsmall().ghost().icon(IconName::Trash).label("Remove condition").on_click(cx.listener(move |this, _, _, cx| this.remove_rule_condition(i, cx))))
                        .into_any_element()
                })
                .collect();
            section("create-rule-trigger", "Trigger and conditions")
                .description("The trigger says which postings the rule looks at. Conditions all have to hold together; with none, it applies to every matching posting.")
                .child(
                    Form::vertical().child(
                        Field::new().label("Trigger").child(
                            RadioGroup::vertical("rule-trigger")
                                .children(TRIGGERS.iter().map(|t| t.label()))
                                .selected_index(Some(b.trigger))
                                .on_change(cx.listener(|this, index: &usize, _, cx| {
                                    this.rule_builder.trigger = *index;
                                    cx.notify();
                                })),
                        ),
                    ),
                )
                .child(if condition_rows.is_empty() { note("No additional conditions.", cx).into_any_element() } else { v_flex().w_full().gap_4().children(condition_rows).into_any_element() })
                .child(h_flex().child(Button::new("add-condition").small().outline().icon(IconName::Plus).label("Add condition").on_click(cx.listener(|this, _, window, cx| this.add_rule_condition(0, window, cx)))))
                .into_any_element()
        }
        2 => section("create-rule-action", "Action")
            .description("Exactly one action. The trigger above is shown with it so a mismatch is visible before you commit.")
            .child(
                Form::vertical()
                    .child(
                        Field::new().label("Action").child(
                            RadioGroup::vertical("rule-action-kind")
                                .children(ACTION_KINDS)
                                .selected_index(Some(b.action_kind))
                                .on_change(cx.listener(|this, index: &usize, _, cx| {
                                    this.rule_builder.action_kind = *index;
                                    cx.notify();
                                })),
                        ),
                    )
                    .children(match b.action_kind {
                        0 => vec![Field::new().label("Rate (%)").required(true).child(Input::new(&b.percent).id("rule-percent")), Field::new().label("Fee label").child(Input::new(&b.fee_label).id("rule-fee-label"))],
                        1 => vec![Field::new().label("Amount").required(true).child(Input::new(&b.fixed).id("rule-fixed")), Field::new().label("Fee label").child(Input::new(&b.fee_label).id("rule-fee-label"))],
                        2 => vec![Field::new().label("Category").required(true).child(Input::new(&b.new_category).id("rule-new-category"))],
                        3 => vec![Field::new().label("Account").child(Select::new(&b.target_account)), Field::new().label("Never below (optional)").child(Input::new(&b.floor).id("rule-floor"))],
                        4 => vec![Field::new().label("Account").child(Select::new(&b.target_account)), Field::new().label("Prohibition lifts after (optional)").child(DatePicker::new(&b.unless_after))],
                        _ => vec![
                            Field::new().label("Preferred account").child(Select::new(&b.target_account)),
                            Field::new().label("Fallback account").child(Select::new(&b.fallback_account)),
                            Field::new().label("Use the fallback below").required(true).child(Input::new(&b.floor).id("rule-threshold")),
                        ],
                    }),
            )
            .child(div().text_sm().child(format!("Trigger: {}", TRIGGERS.get(b.trigger).copied().unwrap_or(atlas_core::rules::Trigger::AnyPosting).label())))
            .into_any_element(),
        _ => section("create-rule-review", "Review and dates")
            .description("Nothing is added until you commit. The rule starts as enabled version 1.")
            .child(
                Form::vertical()
                    .columns(2)
                    .child(Field::new().label("Effective from").required(true).child(DatePicker::new(&b.effective_from)))
                    .child(Field::new().label("Effective to (optional)").child(DatePicker::new(&b.effective_to)))
                    .child(Field::new().label("Explanation").child(Input::new(&b.explanation).id("rule-explanation"))),
            )
            .child(
                DescriptionList::new()
                    .columns(1)
                    .child(DescriptionItem::new("Name").value(b.name.read(cx).value().to_string()))
                    .child(DescriptionItem::new("Priority").value(b.priority.read(cx).value().to_string()))
                    .child(DescriptionItem::new("Rule").value(preview.clone()))
                    .child(DescriptionItem::new("Conditions").value(if b.condition_texts(household, cx).is_empty() { "No additional conditions".to_string() } else { b.condition_texts(household, cx).join(" and ") }))
                    .child(DescriptionItem::new("Applies under").value(if b.scope_kind == 6 { "That scenario's plan only".to_string() } else { "The baseline and every scenario".to_string() })),
            )
            .into_any_element(),
    };

    v_flex()
        .id("screen-create-rule")
        .test_support()
        .w_full()
        .gap_6()
        .child(header)
        .child(
            Stepper::new("create-rule-stepper")
                .selected_index(step)
                .items(STEPS.iter().enumerate().map(|(i, t)| StepperItem::new().child(div().text_sm().child(format!("{}. {t}", i + 1)))))
                .on_click(cx.listener(|this, target: &usize, _, cx| {
                    if *target <= this.rule_builder.step {
                        this.set_rule_step(*target, cx);
                    }
                })),
        )
        .child(body)
        .child(v_flex().gap_1().child(div().text_xs().text_color(theme.muted_foreground).child("Rule preview")).child(div().id("rule-preview").test_support().text_sm().child(preview)))
        .child(
            h_flex()
                .w_full()
                .justify_between()
                .items_center()
                .pt_3()
                .border_t_1()
                .border_color(theme.border)
                .child(
                    h_flex()
                        .gap_2()
                        .child(Button::new("create-rule-cancel").outline().label("Cancel").on_click(cx.listener(|this, _, window, cx| this.cancel_rule_flow(window, cx))))
                        .child(Button::new("create-rule-back").ghost().label("Back").disabled(step == 0).on_click(cx.listener(move |this, _, _, cx| this.set_rule_step(step.saturating_sub(1), cx)))),
                )
                .child(if step < 3 {
                    Button::new("create-rule-next").primary().label("Next").on_click(cx.listener(|this, _, window, cx| this.advance_rule_step(window, cx))).into_any_element()
                } else {
                    Button::new("create-rule-commit").primary().label("Create rule").on_click(cx.listener(|this, _, window, cx| this.submit_rule_flow(window, cx))).into_any_element()
                }),
        )
        .into_any_element()
}

impl AtlasApp {
    pub fn select_rule(&mut self, id: RuleId, cx: &mut Context<Self>) {
        self.selected_rule = Some(id);
        cx.notify();
    }

    /// Opens a rule's detail on its Simulation tab.
    pub fn open_rule_simulation(&mut self, id: RuleId, cx: &mut Context<Self>) {
        self.selected_rule = Some(id);
        self.rule_tab = 2;
        self.navigate(Route::Rule(id), cx);
    }

    /// Leaves the create-rule flow, warning when something was entered.
    pub fn cancel_rule_flow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dirty = !self.rule_builder.name.read(cx).value().trim().is_empty() || !self.rule_builder.conditions.is_empty();
        if !dirty {
            self.navigate(Route::Rules, cx);
            return;
        }
        super::common::confirm_danger(window, cx, "Discard this rule?", "Nothing has been added yet; the entered values are lost.", "Discard", |window, cx| {
            crate::app::with_app(cx, |app, cx| {
                app.rule_builder = crate::rule_builder::RuleBuilder::new(&app.household, app.viewer(), window, cx);
                app.navigate(Route::Rules, cx);
            })
        });
    }

    /// The tie-break policy, applied at once with a result sentence.
    pub fn apply_tie_break(&mut self, tie_break: TieBreak, cx: &mut Context<Self>) {
        if self.household.rule_tie_break != tie_break {
            self.household.rule_tie_break = tie_break;
            self.mark_dirty();
            self.refresh_derived();
            self.note_result(format!("Tie-break is now “{}”; rule decisions recomputed.", tie_break.label()));
            cx.notify();
        }
    }
}
