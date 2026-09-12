//! Guard: nothing a person can read in the app refers to the internal plan.
//!
//! The specification the engine was built from numbers its sections (§7.6),
//! milestones (M13), worked examples (E05) and validation items (V012). Those
//! codes mean nothing to someone using the app, so they must not appear in
//! anything the app can show: labels, titles, notes, notifications, the sample
//! household, provenance chains, engine errors.
//!
//! Two checks: a scan of every string literal in the app and engine sources
//! (comments excluded — they are for developers), and a walk over what the
//! sample household actually renders for both fixture viewers.

use std::path::{Path, PathBuf};

use atlas_app::models::Section;
use atlas_core::authz::Viewer;
use atlas_core::forecast::{Case, ForecastOptions, forecast};
use atlas_core::ids::EntityRef;
use atlas_core::liquidity::{Boundary, boundary_liquidity, household_liquidity};
use atlas_core::model::Household;
use atlas_core::vocab::{Certainty, MoneyClass, ResultStrength};
use atlas_core::{Disclosure, fixtures};

/// A plan reference: `§` anywhere, or a milestone/example/validation/feature
/// code — one of the letters M, E, V, F followed by one to three digits, as a
/// whole word (`M9`, `M13`, `E05`, `V012`, `F162`; not `EUR2026`, not `M1000`).
fn plan_reference(text: &str) -> Option<String> {
    if let Some(pos) = text.find('§') {
        return Some(text[pos..].chars().take(12).collect());
    }
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if !matches!(c, 'M' | 'E' | 'V' | 'F') {
            continue;
        }
        if i > 0 && chars[i - 1].is_alphanumeric() {
            continue;
        }
        let digits = chars[i + 1..].iter().take_while(|d| d.is_ascii_digit()).count();
        if !(1..=3).contains(&digits) {
            continue;
        }
        if chars.get(i + 1 + digits).is_some_and(|d| d.is_alphanumeric()) {
            continue;
        }
        return Some(chars[i..i + 1 + digits].iter().collect());
    }
    None
}

/// The contents of every string literal on a line that is not a comment.
fn string_literals(line: &str) -> Vec<String> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '"' {
            continue;
        }
        let mut literal = String::new();
        let mut closed = false;
        while let Some(c) = chars.next() {
            match c {
                '\\' => {
                    if let Some(next) = chars.next() {
                        literal.push(next);
                    }
                }
                '"' => {
                    closed = true;
                    break;
                }
                other => literal.push(other),
            }
        }
        if closed {
            out.push(literal);
        }
    }
    out
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_plan_references_in_source_strings() {
    let app = Path::new(env!("CARGO_MANIFEST_DIR"));
    let crates = app.parent().unwrap();
    let mut files = Vec::new();
    for dir in ["atlas-app/src", "atlas-core/src", "atlas-store/src"] {
        rust_files(&crates.join(dir), &mut files);
    }
    assert!(files.len() > 30, "found only {} source files", files.len());
    let mut offending = Vec::new();
    for file in &files {
        let source = std::fs::read_to_string(file).unwrap();
        for (number, line) in source.lines().enumerate() {
            for literal in string_literals(line) {
                if let Some(code) = plan_reference(&literal) {
                    offending.push(format!("{}:{}: {code:?} in {literal:?}", file.strip_prefix(crates).unwrap().display(), number + 1));
                }
            }
        }
    }
    assert!(offending.is_empty(), "plan references in user-facing strings:\n{}", offending.join("\n"));
}

#[test]
fn plan_reference_detector_matches_codes_only() {
    assert_eq!(plan_reference("counted once (§7)").as_deref(), Some("§7)"));
    assert_eq!(plan_reference("first passage (M13)").as_deref(), Some("M13"));
    assert_eq!(plan_reference("never counted twice (V012)").as_deref(), Some("V012"));
    assert_eq!(plan_reference("fails closed, F162").as_deref(), Some("F162"));
    assert_eq!(plan_reference("E05 example").as_deref(), Some("E05"));
    assert_eq!(plan_reference("arrives with the decision builder (M9)").as_deref(), Some("M9"));
    assert_eq!(plan_reference("Person A current account"), None);
    assert_eq!(plan_reference("EUR2026 v2 100,000 M"), None);
    assert_eq!(plan_reference("Series M100 is fine? no: 3 digits"), Some("M100".into()));
    assert_eq!(plan_reference("M1000 is not a code"), None);
}

/// Everything the sample household renders for one viewer, as text.
fn rendered_texts(household: &Household, viewer: Viewer) -> Vec<(String, String)> {
    let mut texts: Vec<(String, String)> = Vec::new();
    let mut push = |what: &str, text: String| texts.push((what.to_string(), text));

    for section in Section::ALL {
        push("section label", section.label().to_string());
    }
    for (group, _) in Section::GROUPS {
        push("sidebar group", group.to_string());
    }
    for class in MoneyClass::ALL {
        push("money class", format!("{} — {}", class.label(), class.description()));
    }
    for certainty in Certainty::ALL {
        push("certainty", format!("{} — {}", certainty.label(), certainty.description()));
    }
    for strength in ResultStrength::ALL {
        push("result strength", format!("{} — {} — {}", strength.label(), strength.permitted_claim(), strength.does_not_establish()));
    }

    let project = |node: &atlas_core::provenance::ProvNode| node.project(&household.disclosure_fn(viewer)).render_chain();
    let liquidity = household_liquidity(household).unwrap();
    for (name, calc) in [
        ("liquid cash", &liquidity.liquid_cash),
        ("reserved", &liquidity.reserved),
        ("free", &liquidity.free),
        ("total assets", &liquidity.total_assets),
        ("liabilities", &liquidity.liabilities),
        ("net worth", &liquidity.net_worth),
    ] {
        push(&format!("household {name} chain"), project(calc.node()));
    }
    let mut boundaries = vec![Boundary::Household];
    boundaries.extend(household.people.iter().map(|p| Boundary::Person(p.id)));
    boundaries.extend(household.companies.iter().map(|c| Boundary::Company(c.id)));
    for boundary in &boundaries {
        let report = boundary_liquidity(household, *boundary).unwrap();
        for (label, calc) in &report.figures {
            push(&format!("{} figure {label}", boundary.slug()), project(calc.node()));
        }
        push(&format!("{} hard floor", boundary.slug()), project(report.hard_floor.node()));
        push(&format!("{} headroom", boundary.slug()), project(report.headroom.node()));
        let through = fixtures::default_horizon();
        for scenario in [None, Some(fixtures::ids::BUY_CAR)] {
            for case in Case::ALL {
                let result = forecast(household, *boundary, ForecastOptions { through, scenario, case }).unwrap();
                push(&format!("{} forecast end", boundary.slug()), project(result.end.node()));
                push(&format!("{} forecast lowest", boundary.slug()), project(result.lowest.node()));
                push(&format!("{} forecast injection", boundary.slug()), project(result.breach.minimum_injection.node()));
                push(&format!("{} forecast summary", boundary.slug()), result.breach.summary());
                for assumption in &result.assumptions {
                    push("forecast assumption", format!("{} — {}", assumption.text, assumption.source.describe()));
                }
                for (_, reason) in &result.record.excluded_accounts {
                    push("excluded account reason", reason.clone());
                }
                push("forecast record algorithm", result.record.algorithm.to_string());
                for rule in &result.record.rules_applied {
                    push("forecast record rule", rule.clone());
                }
            }
        }
    }

    for account in &household.accounts {
        push("account tax treatment", account.tax_treatment.clone());
        push("account liquidity", account.liquidity.describe());
        push("account holder", household.holder_description(account));
        for fee in &account.fees {
            push("account fee", fee.description.clone());
        }
    }
    for series in &household.series {
        push("series notes", series.notes.clone());
        push("series recurrence", series.recurrence.describe());
        push("series tax treatment", series.tax_treatment.clone());
        for change in &series.amount_changes {
            push("series change", change.amount.describe());
        }
        for exception in &series.exceptions {
            push("series exception", exception.describe());
        }
    }
    for reservation in &household.reservations {
        push("reservation purpose", reservation.purpose.clone());
    }
    for assumption in &household.assumptions {
        push("assumption", format!("{} — {}", assumption.text, assumption.source.describe()));
    }
    for scenario in &household.scenarios {
        push("scenario description", scenario.description.clone());
        for entry in household.scenario_overlay(scenario.id) {
            push("scenario overlay entry", format!("{} — {}", entry.kind, entry.text));
        }
    }
    for company in &household.companies {
        for constraint in &company.constraints {
            push("company constraint", constraint.describe());
        }
    }
    for rule in &household.rules {
        push("rule explanation", rule.explanation.clone());
        push("rule scope", rule.scope.describe(household));
        push("rule action", rule.action.describe(household));
        for condition in &rule.conditions {
            push("rule condition", condition.describe(household));
        }
        for version in &rule.history {
            push("rule history", version.summary.clone());
        }
    }
    for pack in &household.tax_packs {
        push("tax pack", format!("{} · {} · {}", pack.name, pack.version, pack.jurisdiction));
        for rule in &pack.rules {
            push("tax rule", format!("{} · {} · {} · {} · {} · {}", rule.name, rule.tax_type, rule.scope, rule.describe_kind(), rule.source, rule.explanation));
        }
    }
    for policy in &household.policies {
        push("policy line", policy.describe(household).join(" · "));
        push("policy preset", policy.preset_label().to_string());
        if let Some(previous) = &policy.previous {
            push("policy previous", previous.clone());
        }
    }
    for grant in &household.grants {
        push("grant", format!("{} — {}", grant.grantee.describe(household), grant.purpose.describe(household)));
    }
    for event in &household.audit {
        push("audit event", format!("{} — {}", event.kind.label(), event.summary));
    }
    for problem in household.authorization_problems() {
        push("authorization problem", problem.text);
    }
    for policy in &household.policies {
        if matches!(policy.disclosure(viewer), Disclosure::Hidden | Disclosure::Aggregate) {
            push("denial explanation", household.explain_denial(viewer, policy.object));
        }
    }
    for person in &household.people {
        push("person role", format!("{} — {}", household.entity_name(EntityRef::Person(person.id)), person.role.label()));
    }
    for account in &household.accounts {
        if let Some(reason) = atlas_core::liquidity::household_exclusion_reason(household, account) {
            push("exclusion reason", reason);
        }
    }

    let through = fixtures::default_horizon();
    let comparison = atlas_core::scenario::compare(household, Boundary::Household, &[fixtures::ids::BUY_CAR], through, Case::Expected).unwrap();
    for row in &comparison.metrics {
        push("scenario metric", format!("{} — {} — {} — {}", row.name, row.baseline, row.scenario, row.note));
    }
    let (attribution, note) = atlas_core::scenario::project_attribution(household, viewer, &comparison.attribution, comparison.end_delta);
    for line in &attribution {
        push("scenario attribution", format!("{} — {}", line.kind, line.label));
    }
    if let Some(note) = note {
        push("scenario suppression note", note);
    }

    let plan = atlas_core::decision::default_plan_for(household, household.as_of, viewer);
    let decision = atlas_core::decision::evaluate(household, &plan, through).unwrap();
    push("decision statement", decision.statement.render());
    push("decision recommendation", decision.recommendation.render());
    push("decision strategies status", format!("{} — {}", decision.strategies.status, decision.strategies.search_space));
    push("decision grid status", decision.grid_status.clone());
    for strategy in &decision.strategies.strategies {
        push("strategy", format!("{} — {} — {}", strategy.name, strategy.future_tax_note, strategy.transfers));
        for step in &strategy.steps {
            push("strategy step", format!("{} — {}", step.source, step.note));
        }
        for text in strategy.violations.iter().chain(&strategy.caveats) {
            push("strategy caveat", text.clone());
        }
    }
    for metric in &decision.metrics {
        push("decision metric", format!("{} — {} — {}", metric.name, metric.value, metric.how));
    }
    for goal in &decision.goals {
        push("decision goal", goal.text.clone());
    }
    push("decision immediate cash", project(decision.immediate_cash.node()));

    let assessment = atlas_core::tax::assess(household, through, None, Case::Expected).unwrap();
    for event in &assessment.events {
        push("tax event", format!("{} — {} — {}", event.rule_name, event.base_label, event.kind.label()));
    }
    for (_, calc) in &assessment.by_entity {
        push("tax by entity", project(calc.node()));
    }
    push("tax reserve", project(assessment.payable_after_horizon.node()));
    for pack in &assessment.packs_used {
        push("tax pack used", pack.clone());
    }

    let sensitivity = atlas_core::sensitivity::one_at_a_time(household, Boundary::Household, ForecastOptions { through, scenario: None, case: Case::Expected }).unwrap();
    push("sensitivity caveat", sensitivity.caveat.to_string());
    for breakpoint in &sensitivity.breakpoints {
        push("sensitivity breakpoint", format!("{} — {}", breakpoint.statement, breakpoint.searched));
    }
    for series in &household.series {
        if !household.history_of(series.id).is_empty() {
            for derivation in atlas_core::assumptions::Derivation::ALL {
                match atlas_core::assumptions::derive(household, series.id, derivation) {
                    Ok(derived) => {
                        push("derivation statement", derived.statement.clone());
                        push("derivation chain", project(derived.calc.node()));
                    }
                    Err(err) => push("derivation error", err.to_string()),
                }
            }
        }
    }

    texts
}

#[test]
fn sample_household_renders_without_plan_references() {
    let household = fixtures::plan_household();
    let mut offending = Vec::new();
    for viewer in [Viewer::person(fixtures::ids::PERSON_A), Viewer::person(fixtures::ids::PERSON_B)] {
        let texts = rendered_texts(&household, viewer);
        assert!(texts.len() > 200, "only {} texts rendered for {}", texts.len(), viewer.person);
        for (what, text) in texts {
            if let Some(code) = plan_reference(&text) {
                offending.push(format!("{} / {what}: {code:?} in {text:?}", viewer.person));
            }
        }
    }
    assert!(offending.is_empty(), "plan references in rendered text:\n{}", offending.join("\n"));
}

/// The overlay toggle follows the household, not a hard-coded fixture id.
#[test]
fn overlay_scenario_is_the_first_scenario_the_viewer_may_see() {
    let household = fixtures::plan_household();
    assert_eq!(atlas_app::models::overlay_scenario(&household, Viewer::person(fixtures::ids::PERSON_A)), Some(fixtures::ids::BUY_CAR));
    assert_eq!(atlas_app::models::overlay_scenario(&household, Viewer::person(fixtures::ids::PERSON_B)), Some(fixtures::ids::BUY_CAR));
    let empty = Household::empty("Fresh", atlas_core::Currency::USD, household.as_of);
    assert_eq!(atlas_app::models::overlay_scenario(&empty, Viewer::person(fixtures::ids::PERSON_A)), None, "no scenario, no toggle");
}
