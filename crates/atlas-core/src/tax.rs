//! Tax engine (§12, M23, M24, §25): taxes are first-class, effective-dated
//! events and liabilities with correct cash dates and entity attribution.
//!
//! - [`bracket_tax`] is M23: `T(y) = Σ_k r_k · min([y − l_k]₊, u_k − l_k)`.
//! - [`assess`] applies every effective rule of the household's packs to the
//!   occurrences in a window and produces [`TaxEvent`]s: withheld at source,
//!   immediate, or an annual assessment balance payable later (M24:
//!   `balance payable = T_assessed − creditable withholding`).
//! - [`cash_postings`] is what the forecast posts — each tax enters cash once.
//! - [`incremental_tax`] and [`multi_year_comparison`] are §12.5 and E05.
//!
//! Rule packs are selected by effective date: a 2027 occurrence never silently
//! uses a 2026 rule (§12.1, §25).

use crate::forecast::Case;
use crate::ids::*;
use crate::model::{Bracket, Household, TaxKind, TaxRule, TaxRulePack, TaxTiming, ThresholdBasis};
use crate::money::Money;
use crate::provenance::{Calc, ProvNode, ProvValue};
use crate::timeline::{Direction, Occurrence};
use crate::vocab::{MoneyClass, ResultStrength};
use crate::EngineResult;
use chrono::{Datelike, NaiveDate};

/// M23 — marginal bracket tax on a base, with one chain term per bracket.
pub fn bracket_tax(brackets: &[Bracket], base: Money, label: &str) -> EngineResult<Calc<Money>> {
    let currency = base.currency();
    let mut total = Money::zero(currency);
    let mut terms = Vec::new();
    for bracket in brackets {
        let above_lower = base.checked_sub(bracket.lower)?.clamped_at_zero();
        let width = match bracket.upper {
            Some(upper) => upper.checked_sub(bracket.lower)?,
            None => above_lower,
        };
        let taxed = above_lower.min(width)?;
        let tax = taxed.share_basis_points(bracket.rate_basis_points);
        total = total.checked_add(tax)?;
        let range = match bracket.upper {
            Some(upper) => format!("{}–{}", bracket.lower.format(), upper.format()),
            None => format!("above {}", bracket.lower.format()),
        };
        terms.push(
            ProvNode::formula(
                format!("{}% bracket ({range})", bracket.rate_basis_points / 100),
                tax,
                format!("min([{} − {}]₊, width) = {} × {}.{:02}%", base.format(), bracket.lower.format(), taxed.format(), bracket.rate_basis_points / 100, bracket.rate_basis_points % 100),
                Vec::new(),
            )
            .money_class(MoneyClass::ConditionalFuture),
        );
    }
    Ok(Calc::new(
        total,
        ProvNode::sum(label.to_string(), total, terms)
            .money_class(MoneyClass::ConditionalFuture)
            .strength(ResultStrength::ExactAccounting)
            .note(format!("Marginal brackets on a taxable base of {}; marginal, average and incremental rates differ.", base.format())),
    ))
}

/// How the tax cash moves for one event.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TaxEventKind {
    /// Withheld from the base transaction on its date.
    Withheld { creditable: bool },
    /// Paid on the base transaction's date.
    Immediate,
    /// The year's assessed tax minus creditable withholding, payable later.
    AssessmentBalance { tax_year: i32 },
}

impl TaxEventKind {
    pub fn label(self) -> String {
        match self {
            TaxEventKind::Withheld { creditable: true } => "withheld, creditable".into(),
            TaxEventKind::Withheld { creditable: false } => "withheld, final".into(),
            TaxEventKind::Immediate => "paid immediately".into(),
            TaxEventKind::AssessmentBalance { tax_year } => format!("{tax_year} assessment balance"),
        }
    }
}

/// §12.3 — one tax liability with its economic and cash dates.
#[derive(Clone, Debug)]
pub struct TaxEvent {
    /// The series whose occurrence(s) triggered the tax.
    pub base_series: Option<SeriesId>,
    /// When the tax was economically incurred.
    pub accrual_date: NaiveDate,
    /// When the cash moves (§12.3: the timeline shows the correct cash date).
    pub cash_date: NaiveDate,
    pub amount: Money,
    pub rule: TaxRuleId,
    pub rule_name: String,
    pub pack: String,
    /// §12.6 — the legal/economic entity that owes it.
    pub entity: EntityRef,
    pub account: AccountId,
    pub kind: TaxEventKind,
    pub base_label: String,
    pub base_amount: Money,
    pub chain: ProvNode,
}

/// §12 — everything the tax engine derived for a window.
#[derive(Clone, Debug)]
pub struct TaxAssessment {
    pub through: NaiveDate,
    pub events: Vec<TaxEvent>,
    /// Total tax cash inside the window per entity (§12.6).
    pub by_entity: Vec<(EntityRef, Calc<Money>)>,
    /// Tax incurred inside the window but payable after it (§12.4): the amount a
    /// tax reserve should hold.
    pub payable_after_horizon: Calc<Money>,
    /// Creditable withholding inside the window (M24).
    pub creditable_withholding: Money,
    /// Packs that supplied rules, with verification status (§12.7).
    pub packs_used: Vec<String>,
}

/// The pack and rule effective on `date`, for a category (§25 pinning).
pub fn effective_rules<'a>(household: &'a Household, date: NaiveDate) -> Vec<(&'a TaxRulePack, &'a TaxRule)> {
    household
        .tax_packs
        .iter()
        .flat_map(|pack| pack.rules.iter().filter(move |rule| rule.is_effective_on(date)).map(move |rule| (pack, rule)))
        .collect()
}

fn rate_label(bp: u32) -> String {
    format!("{}.{:02}%", bp / 100, bp % 100)
}

/// Applies every effective rule to the live occurrences of the window and
/// returns the tax events with their cash dates (§12.3) and attribution (§12.6).
pub fn assess(household: &Household, through: NaiveDate, scenario: Option<ScenarioId>, case: Case) -> EngineResult<TaxAssessment> {
    if let Some(id) = scenario
        && household.scenario(id).is_some_and(|s| s.has_overlay())
    {
        let overlaid = household.apply_scenarios(&[id])?;
        return assess(&overlaid, through, scenario, case);
    }
    let currency = household.base_currency;
    let occurrences: Vec<Occurrence> = household.expand_all(household.as_of, through, scenario).into_iter().filter(|o| o.is_live()).collect();
    let mut events: Vec<TaxEvent> = Vec::new();
    let mut packs_used: Vec<String> = Vec::new();
    // Annual bases per (entity, tax year, rule id).
    let mut annual: Vec<(EntityRef, i32, TaxRuleId, Money, Vec<ProvNode>, AccountId)> = Vec::new();

    for occurrence in &occurrences {
        let Some(series) = household.series_by_id(occurrence.series) else { continue };
        let amount = case.amount(&occurrence.amount, series.direction).checked_sub(occurrence.fulfilled)?.clamped_at_zero();
        if amount.is_zero() {
            continue;
        }
        for (pack, rule) in effective_rules(household, occurrence.due) {
            if !rule.applies_to_category(&series.category) {
                continue;
            }
            let pack_label = format!("{} ({}{})", pack.name, pack.version, if pack.verified { ", verified" } else { ", unverified — fictitious" });
            if !packs_used.contains(&pack_label) {
                packs_used.push(pack_label.clone());
            }
            match &rule.kind {
                TaxKind::FlatAboveThreshold { rate_basis_points, threshold, basis, on_excess_only } => {
                    if *basis != ThresholdBasis::PerTransaction {
                        // Per-day / cumulative aggregation arrives with the rules engine (M7, V016).
                        continue;
                    }
                    if amount.minor() <= threshold.minor() {
                        continue;
                    }
                    let base = if *on_excess_only { amount.checked_sub(*threshold)? } else { amount };
                    let tax = base.share_basis_points(*rate_basis_points);
                    let chain = ProvNode::formula(
                        format!("{} on {}", rule.name, occurrence.label),
                        tax,
                        format!("{} × {} ({} exceeds the {} threshold {})", base.format(), rate_label(*rate_basis_points), amount.format(), basis.label(), threshold.format()),
                        vec![ProvNode::input(occurrence.label.clone(), amount, "occurrence amount").certainty(occurrence.certainty)],
                    )
                    .money_class(MoneyClass::ConditionalFuture)
                    .note(format!("Rule {} of pack {pack_label}, effective {} – {}", rule.name, rule.effective_from, rule.effective_to.map(|d| d.to_string()).unwrap_or_else(|| "open".into())));
                    events.push(TaxEvent {
                        base_series: Some(occurrence.series),
                        accrual_date: occurrence.due,
                        cash_date: occurrence.due,
                        amount: tax,
                        rule: rule.id,
                        rule_name: rule.name.clone(),
                        pack: pack.name.clone(),
                        entity: occurrence.entity,
                        account: occurrence.account,
                        kind: match rule.timing {
                            TaxTiming::WithheldAtSource { creditable } => TaxEventKind::Withheld { creditable },
                            _ => TaxEventKind::Immediate,
                        },
                        base_label: occurrence.label.clone(),
                        base_amount: amount,
                        chain,
                    });
                }
                TaxKind::FlatRate { rate_basis_points } => {
                    let tax = amount.share_basis_points(*rate_basis_points);
                    let chain = ProvNode::formula(
                        format!("{} on {}", rule.name, occurrence.label),
                        tax,
                        format!("{} × {}", amount.format(), rate_label(*rate_basis_points)),
                        vec![ProvNode::input(occurrence.label.clone(), amount, "occurrence amount").certainty(occurrence.certainty)],
                    )
                    .money_class(MoneyClass::ConditionalFuture)
                    .note(format!("Rule {} of pack {pack_label}", rule.name));
                    events.push(TaxEvent {
                        base_series: Some(occurrence.series),
                        accrual_date: occurrence.due,
                        cash_date: occurrence.due,
                        amount: tax,
                        rule: rule.id,
                        rule_name: rule.name.clone(),
                        pack: pack.name.clone(),
                        entity: occurrence.entity,
                        account: occurrence.account,
                        kind: match rule.timing {
                            TaxTiming::WithheldAtSource { creditable } => TaxEventKind::Withheld { creditable },
                            _ => TaxEventKind::Immediate,
                        },
                        base_label: occurrence.label.clone(),
                        base_amount: amount,
                        chain,
                    });
                }
                TaxKind::AnnualBrackets { .. } => {
                    if series.direction != Direction::Income {
                        continue;
                    }
                    let year = occurrence.due.year();
                    let node = ProvNode::input(format!("{} ({})", occurrence.label, occurrence.due.format("%d %b %Y")), amount, "taxable income inside the window")
                        .certainty(occurrence.certainty)
                        .money_class(MoneyClass::ExpectedFuture);
                    match annual.iter_mut().find(|(e, y, r, _, _, _)| *e == occurrence.entity && *y == year && *r == rule.id) {
                        Some(entry) => {
                            entry.3 = entry.3.checked_add(amount)?;
                            entry.4.push(node);
                        }
                        None => annual.push((occurrence.entity, year, rule.id, amount, vec![node], occurrence.account)),
                    }
                }
            }
        }
    }

    // Annual assessments: brackets on the year's base, minus creditable withholding of the same entity/year.
    for (entity, year, rule_id, base, inputs, account) in annual {
        let Some((pack, rule)) = effective_rules(household, NaiveDate::from_ymd_opt(year, 6, 30).expect("mid-year")).into_iter().find(|(_, r)| r.id == rule_id) else { continue };
        let TaxKind::AnnualBrackets { brackets } = &rule.kind else { continue };
        let TaxTiming::AnnualAssessment { due_month, due_day } = rule.timing else { continue };
        let assessed = bracket_tax(brackets, base, &format!("{} — {year} taxable base {}", rule.name, base.format()))?;
        let creditable: Money = Money::sum(
            currency,
            events
                .iter()
                .filter(|e| e.entity == entity && e.accrual_date.year() == year && matches!(e.kind, TaxEventKind::Withheld { creditable: true }))
                .map(|e| e.amount),
        )?;
        let balance = assessed.money().checked_sub(creditable)?;
        let cash_date = NaiveDate::from_ymd_opt(year + 1, due_month as u32, due_day as u32).unwrap_or_else(|| NaiveDate::from_ymd_opt(year + 1, 12, 31).expect("valid"));
        let mut chain = ProvNode::sum(
            format!("{} — {year} balance payable", rule.name),
            balance,
            vec![
                ProvNode::sum(format!("Assessed tax on {} of income inside the window", base.format()), assessed.money(), assessed.node().children().to_vec()).money_class(MoneyClass::ConditionalFuture),
                ProvNode::input("Creditable withholding in the year", creditable, "withheld-at-source events of the same entity and year").money_class(MoneyClass::ConditionalFuture).minus(),
            ],
        )
        .money_class(MoneyClass::ConditionalFuture)
        .strength(ResultStrength::ConditionalPath)
        .note(format!(
            "Income outside the forecast window is not assessed here; a full-year assessment needs reconciled history, so this is an estimate. Payable {}. Pack {} ({}).",
            cash_date.format("%d %b %Y"),
            pack.name,
            if pack.verified { "verified" } else { "unverified — fictitious" }
        ));
        if balance.is_negative() {
            chain = chain.note("A negative balance is a refund to come, not current cash.");
        }
        chain = ProvNode::sum(chain.label().to_string(), balance, chain.children().to_vec())
            .money_class(MoneyClass::ConditionalFuture)
            .strength(ResultStrength::ConditionalPath)
            .note(chain.notes().join(" "));
        let _ = inputs;
        events.push(TaxEvent {
            base_series: None,
            accrual_date: NaiveDate::from_ymd_opt(year, 12, 31).expect("valid"),
            cash_date,
            amount: balance,
            rule: rule.id,
            rule_name: rule.name.clone(),
            pack: pack.name.clone(),
            entity,
            account,
            kind: TaxEventKind::AssessmentBalance { tax_year: year },
            base_label: format!("{year} taxable income inside the window"),
            base_amount: base,
            chain,
        });
    }
    events.sort_by_key(|e| (e.cash_date, e.entity.to_string(), e.rule));

    // Attribution per entity (§12.6): tax cash within the window.
    let mut by_entity: Vec<(EntityRef, Calc<Money>)> = Vec::new();
    let mut entities: Vec<EntityRef> = events.iter().map(|e| e.entity).collect();
    entities.sort_by_key(|e| e.to_string());
    entities.dedup();
    for entity in entities {
        let mine: Vec<&TaxEvent> = events.iter().filter(|e| e.entity == entity && e.cash_date <= through).collect();
        let total = Money::sum(currency, mine.iter().map(|e| e.amount))?;
        let terms = mine
            .iter()
            .map(|e| ProvNode::input(format!("{} — {} ({})", e.rule_name, e.base_label, e.kind.label()), e.amount, format!("cash {}", e.cash_date.format("%d %b %Y"))).money_class(MoneyClass::ConditionalFuture))
            .collect();
        by_entity.push((
            entity,
            Calc::new(
                total,
                ProvNode::sum(format!("{} — tax cash inside the window", household.entity_name(entity)), total, terms)
                    .money_class(MoneyClass::ConditionalFuture)
                    .strength(ResultStrength::ConditionalPath)
                    .note("Attributed to the entity that owes it; a household summary may add these up but never loses the attribution."),
            ),
        ));
    }

    let later: Vec<&TaxEvent> = events.iter().filter(|e| e.cash_date > through && e.amount.is_positive()).collect();
    let later_total = Money::sum(currency, later.iter().map(|e| e.amount))?;
    let payable_after_horizon = Calc::new(
        later_total,
        ProvNode::sum(
            "Tax incurred inside the window, payable after it",
            later_total,
            later.iter().map(|e| ProvNode::input(format!("{} ({})", e.rule_name, household.entity_name(e.entity)), e.amount, format!("payable {}", e.cash_date.format("%d %b %Y"))).money_class(MoneyClass::ConditionalFuture)).collect(),
        )
        .money_class(MoneyClass::ReservedCurrent)
        .strength(ResultStrength::ConditionalPath)
        .note("The amount a tax reserve should hold; it reduces unreserved cash, not the bank balance."),
    );
    let creditable_withholding = Money::sum(currency, events.iter().filter(|e| matches!(e.kind, TaxEventKind::Withheld { creditable: true })).map(|e| e.amount))?;

    Ok(TaxAssessment { through, events, by_entity, payable_after_horizon, creditable_withholding, packs_used })
}

/// One tax cash posting for the forecast.
#[derive(Clone, Debug)]
pub struct TaxPosting {
    pub date: NaiveDate,
    pub account: AccountId,
    /// Negative: cash leaves.
    pub amount: Money,
    pub label: String,
    pub rule: TaxRuleId,
    pub base_series: Option<SeriesId>,
    pub entity: EntityRef,
}

/// The cash postings the forecast makes for taxes inside the window — each tax
/// enters cash exactly once (M01).
pub fn cash_postings(household: &Household, through: NaiveDate, scenario: Option<ScenarioId>, case: Case) -> EngineResult<Vec<TaxPosting>> {
    let assessment = assess(household, through, scenario, case)?;
    Ok(assessment
        .events
        .into_iter()
        .filter(|e| e.cash_date <= through && !e.amount.is_zero())
        .map(|e| TaxPosting {
            date: e.cash_date,
            account: e.account,
            amount: e.amount.negated(),
            label: format!("Tax: {} ({})", e.rule_name, e.kind.label()),
            rule: e.rule,
            base_series: e.base_series,
            entity: e.entity,
        })
        .collect())
}

/// §12.5 — `incremental = tax with the action − tax without it`.
pub fn incremental_tax(with_action: Money, without_action: Money, label: &str) -> EngineResult<Calc<Money>> {
    let delta = with_action.checked_sub(without_action)?;
    Ok(Calc::new(
        delta,
        ProvNode::sum(
            label.to_string(),
            delta,
            vec![
                ProvNode::input("Tax under the scenario with the proposed action", with_action, "assessment with the action").money_class(MoneyClass::ConditionalFuture),
                ProvNode::input("Tax under the otherwise-identical baseline", without_action, "assessment without the action").money_class(MoneyClass::ConditionalFuture).minus(),
            ],
        )
        .money_class(MoneyClass::ConditionalFuture)
        .strength(ResultStrength::ConditionalPath)
        .note("Incremental tax cost: the two assessments differ only by the proposed action."),
    ))
}

/// One strategy of an E05-style comparison.
#[derive(Clone, Debug)]
pub struct YearStrategy {
    pub name: String,
    /// Extra taxable income per year, in the order of `baseline_by_year`.
    pub extraction_by_year: Vec<Money>,
    pub taxable_by_year: Vec<Money>,
    pub tax_by_year: Vec<Calc<Money>>,
    pub total_tax: Money,
    pub incremental: Calc<Money>,
}

/// E05 — compares timing strategies of a lawful discretionary extraction over
/// several tax years against the same baseline, undiscounted.
pub fn multi_year_comparison(brackets: &[Bracket], baseline_by_year: &[Money], strategies: &[(String, Vec<Money>)]) -> EngineResult<(Money, Vec<YearStrategy>)> {
    let currency = baseline_by_year.first().map(|m| m.currency()).unwrap_or(crate::Currency::PKR);
    let mut baseline_total = Money::zero(currency);
    for base in baseline_by_year {
        baseline_total = baseline_total.checked_add(bracket_tax(brackets, *base, "baseline")?.money())?;
    }
    let mut out = Vec::new();
    for (name, extraction) in strategies {
        let mut taxable_by_year = Vec::new();
        let mut tax_by_year = Vec::new();
        let mut total = Money::zero(currency);
        for (index, base) in baseline_by_year.iter().enumerate() {
            let extra = extraction.get(index).copied().unwrap_or(Money::zero(currency));
            let taxable = base.checked_add(extra)?;
            let tax = bracket_tax(brackets, taxable, &format!("{name} — year {}", index + 1))?;
            total = total.checked_add(tax.money())?;
            taxable_by_year.push(taxable);
            tax_by_year.push(tax);
        }
        let incremental = incremental_tax(total, baseline_total, &format!("{name} — incremental tax versus the baseline"))?;
        out.push(YearStrategy { name: name.clone(), extraction_by_year: extraction.clone(), taxable_by_year, tax_by_year, total_tax: total, incremental });
    }
    Ok((baseline_total, out))
}

/// Renders a value for display in the tax screens.
pub fn value_text(value: &ProvValue) -> String {
    value.render()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{self, ids, pkr};

    fn e05_brackets() -> Vec<Bracket> {
        vec![
            Bracket { lower: pkr(0), upper: Some(pkr(100_000)), rate_basis_points: 1_000 },
            Bracket { lower: pkr(100_000), upper: None, rate_basis_points: 3_000 },
        ]
    }

    #[test]
    fn v013_brackets_at_and_around_every_threshold() {
        let brackets = e05_brackets();
        let tax = |minor_major: i64| bracket_tax(&brackets, pkr(minor_major), "t").unwrap().money();
        assert_eq!(tax(0), pkr(0));
        // One minor unit below the threshold: 9,999,999 × 10% = 999,999.9 → 1,000,000 minor (rounded half away from zero).
        assert_eq!(bracket_tax(&brackets, Money::new(pkr(100_000).minor() - 1, crate::Currency::PKR), "t").unwrap().money(), Money::new(1_000_000, crate::Currency::PKR));
        assert_eq!(tax(100_000), pkr(10_000));
        assert_eq!(bracket_tax(&brackets, Money::new(pkr(100_000).minor() + 1, crate::Currency::PKR), "t").unwrap().money().minor(), pkr(10_000).minor());
        assert_eq!(tax(160_000), pkr(28_000));
        assert_eq!(tax(60_000), pkr(6_000));
        let calc = bracket_tax(&brackets, pkr(160_000), "t").unwrap();
        assert!(calc.node().verify_sums().is_empty());
        assert_eq!(calc.node().children().len(), 2);
    }

    #[test]
    fn e05_splitting_an_extraction_reduces_the_modeled_incremental_tax() {
        let brackets = e05_brackets();
        let baseline = vec![pkr(60_000), pkr(60_000)];
        let (baseline_total, strategies) = multi_year_comparison(
            &brackets,
            &baseline,
            &[("Extract 100,000 in year 1".into(), vec![pkr(100_000), pkr(0)]), ("Extract 50,000 in each year".into(), vec![pkr(50_000), pkr(50_000)])],
        )
        .unwrap();
        assert_eq!(baseline_total, pkr(12_000));
        assert_eq!(strategies[0].tax_by_year[0].money(), pkr(28_000));
        assert_eq!(strategies[0].tax_by_year[1].money(), pkr(6_000));
        assert_eq!(strategies[0].incremental.money(), pkr(22_000));
        assert_eq!(strategies[1].tax_by_year[0].money(), pkr(13_000));
        assert_eq!(strategies[1].incremental.money(), pkr(14_000));
        assert_eq!(strategies[0].incremental.money() - strategies[1].incremental.money(), pkr(8_000));
        assert!(strategies[1].incremental.node().verify_sums().is_empty());
    }

    #[test]
    fn assessment_withholds_at_source_and_credits_it_at_assessment() {
        let household = fixtures::plan_household();
        let assessment = assess(&household, fixtures::default_horizon(), None, Case::Expected).unwrap();
        // Salary A: 3 × 500,000 × 7% withheld (2026 pack); salary B: 5 payments, Jan is 2027 → 8%.
        let withheld_a: Vec<&TaxEvent> = assessment.events.iter().filter(|e| e.entity == EntityRef::Person(ids::PERSON_A) && e.rule_name == "DEMO salary withholding").collect();
        assert_eq!(withheld_a.len(), 3);
        assert!(withheld_a.iter().all(|e| e.amount == pkr(35_000) && e.kind == TaxEventKind::Withheld { creditable: true }));
        let jan_b = assessment
            .events
            .iter()
            .find(|e| e.entity == EntityRef::Person(ids::PERSON_B) && e.rule_name == "DEMO salary withholding" && e.cash_date.year() == 2027)
            .expect("January salary withholding");
        assert_eq!(jan_b.amount, pkr(24_000), "the 2027 pack's 8% applies to the January payment");
        assert_eq!(jan_b.pack, "DEMO-JURISDICTION-2027-v1");
        // ATM withdrawal 80,000 > 50,000 threshold → 0.6% on the full amount = 480.
        let atm = assessment.events.iter().find(|e| e.rule_name == "DEMO cash withdrawal withholding").unwrap();
        assert_eq!(atm.amount, pkr(480));
        // Foreign subscription 12,000 × 5% = 600, immediate, on the Visa.
        let foreign = assessment.events.iter().find(|e| e.rule_name == "DEMO foreign card tax").unwrap();
        assert_eq!(foreign.amount, pkr(600));
        assert_eq!(foreign.kind, TaxEventKind::Immediate);
        assert_eq!(foreign.account, ids::PERSON_A_VISA);
        // 2026 assessment for A: taxable inside window = 3×500,000 + 300,000 freelance + 200,000 receivable remainder = 2,000,000
        // → brackets: 0 on 600,000, 10% on 1,400,000 = 140,000; creditable withholding 105,000 salary + 3 × 480 ATM
        //   (Oct, Nov, Dec) → balance 33,560; payable 30 Sep 2027.
        let a_2026 = assessment
            .events
            .iter()
            .find(|e| e.entity == EntityRef::Person(ids::PERSON_A) && e.kind == TaxEventKind::AssessmentBalance { tax_year: 2026 })
            .expect("assessment");
        assert_eq!(a_2026.base_amount, pkr(2_000_000));
        assert_eq!(a_2026.amount, pkr(140_000 - 105_000 - 3 * 480), "creditable withholding reduces the balance due");
        assert_eq!(a_2026.cash_date, NaiveDate::from_ymd_opt(2027, 9, 30).unwrap());
        assert!(a_2026.chain.verify_sums().is_empty());
        // The balance is payable after the horizon: it belongs in the tax reserve, not in the window's cash.
        assert!(assessment.payable_after_horizon.money().minor() >= a_2026.amount.minor());
        let postings = cash_postings(&household, fixtures::default_horizon(), None, Case::Expected).unwrap();
        assert!(postings.iter().all(|p| p.date <= fixtures::default_horizon()));
        assert!(postings.iter().any(|p| p.account == ids::PERSON_A_CURRENT && p.amount == pkr(-35_000)));
        assert!(assessment.packs_used.iter().any(|p| p.contains("unverified")));
    }

    #[test]
    fn incremental_tax_is_the_difference_of_two_assessments() {
        let calc = incremental_tax(pkr(34_000), pkr(12_000), "x").unwrap();
        assert_eq!(calc.money(), pkr(22_000));
        assert!(calc.node().verify_sums().is_empty());
    }
}
