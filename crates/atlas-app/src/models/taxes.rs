//! Taxes: rule packs with their versions and effective dates, the tax events
//! of the window with their cash dates and entity attribution, the tax
//! reserve for what is payable later, and the with-vs-without extraction
//! comparison.

use atlas_core::authz::Viewer;
use atlas_core::forecast::Case;
use atlas_core::ids::{EntityRef, ObjectRef, ScenarioId};
use atlas_core::model::{Bracket, Household, TaxRulePack};
use atlas_core::tax::{TaxAssessment, YearStrategy, assess, multi_year_comparison};
use atlas_core::{Disclosure, EngineResult, Money};
use chrono::NaiveDate;
use gpui_kit::component::{
    Sizable as _,
    tag::Tag,
};
use gpui_kit::*;

use crate::widgets::figure::ExplainedFigure;
use crate::widgets::labels;

/// Which bracket schedule the extraction comparison uses.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum E05Schedule {
    /// A fictitious example: 10% on the first 100,000, 30% above.
    PlanExample,
    /// The household's effective annual brackets for the current year.
    HouseholdPack,
}

#[derive(Clone, Debug)]
pub struct TaxModel {
    pub assessment: TaxAssessment,
    pub scenario_on: bool,
    pub by_entity: Vec<(EntityRef, ExplainedFigure)>,
    pub reserve: ExplainedFigure,
    pub e05_amount: Money,
    pub e05_split: bool,
    pub e05_schedule: E05Schedule,
    pub e05_baseline_total: Money,
    pub e05_strategies: Vec<YearStrategy>,
    pub e05_incrementals: Vec<ExplainedFigure>,
    pub e05_brackets: Vec<Bracket>,
    /// Packs the viewer may see (company-only data is not a pack concern; all packs are household objects).
    pub packs: Vec<TaxRulePack>,
    /// The tax events as grid rows, formatted once (see `widgets::grid`).
    /// The scenario the toggle applies (see [`super::overlay_scenario`]).
    pub overlay_scenario: Option<ScenarioId>,
}

/// Columns of the tax-events grid, in display order.
impl TaxModel {
    pub fn compute(household: &Household, viewer: Viewer, through: NaiveDate, scenario_on: bool, e05_amount: Money, e05_split: bool, e05_schedule: E05Schedule) -> EngineResult<Self> {
        log::info!("computing tax model through {through} scenario={scenario_on} e05 amount={} split={e05_split}", e05_amount.format());
        let overlay_scenario = super::overlay_scenario(household, viewer);
        let scenario = if scenario_on { overlay_scenario } else { None };
        let assessment = assess(household, through, scenario, Case::Expected)?;
        let by_entity = assessment
            .by_entity
            .iter()
            .filter(|(entity, _)| match entity {
                EntityRef::Company(id) => matches!(household.disclosure_for(viewer, ObjectRef::Company(*id)), Disclosure::Full | Disclosure::SelectedFields),
                _ => true,
            })
            .map(|(entity, calc)| (*entity, ExplainedFigure::new(format!("tax-{}", entity.to_string().replace(' ', "-")), format!("{} — tax cash in the window", household.entity_name(*entity)), calc, household, viewer)))
            .collect();
        let reserve = ExplainedFigure::new("tax-reserve", "Tax incurred, payable after the horizon (reserve)", &assessment.payable_after_horizon, household, viewer);

        let currency = household.base_currency;
        let e05_brackets = match e05_schedule {
            E05Schedule::PlanExample => vec![
                Bracket { lower: Money::zero(currency), upper: Some(Money::from_major(100_000, currency)), rate_basis_points: 1_000 },
                Bracket { lower: Money::from_major(100_000, currency), upper: None, rate_basis_points: 3_000 },
            ],
            E05Schedule::HouseholdPack => atlas_core::tax::effective_rules(household, household.as_of)
                .into_iter()
                .find_map(|(_, rule)| match &rule.kind {
                    atlas_core::model::TaxKind::AnnualBrackets { brackets } => Some(brackets.clone()),
                    _ => None,
                })
                .unwrap_or_default(),
        };
        let baseline = vec![Money::from_major(60_000, currency), Money::from_major(60_000, currency)];
        let half = Money::new(e05_amount.minor() / 2, currency);
        let other_half = e05_amount.checked_sub(half)?;
        let strategies = vec![
            (format!("Extract {} in year 1", e05_amount.format()), vec![e05_amount, Money::zero(currency)]),
            (format!("Extract {} in each year", half.format()), vec![half, other_half]),
        ];
        let (e05_baseline_total, e05_strategies) = if e05_brackets.is_empty() {
            (Money::zero(currency), Vec::new())
        } else {
            multi_year_comparison(&e05_brackets, &baseline, if e05_split { &strategies } else { &strategies[..1] })?
        };
        let e05_incrementals = e05_strategies
            .iter()
            .enumerate()
            .map(|(index, s)| ExplainedFigure::new(format!("e05-incremental-{index}"), format!("{} — incremental tax", s.name), &s.incremental, household, viewer))
            .collect();
        Ok(TaxModel {
            assessment,
            scenario_on,
            by_entity,
            reserve,
            e05_amount,
            e05_split,
            e05_schedule,
            e05_baseline_total,
            e05_strategies,
            e05_incrementals,
            e05_brackets,
            packs: household.tax_packs.clone(),
            overlay_scenario,
        })
    }
}

/// Tag for a pack's verification status, shared with other screens.
pub fn verification_tag(verified: bool) -> Tag {
    if verified { labels::strength_tag(atlas_core::ResultStrength::ExactAccounting) } else { Tag::warning().xsmall().outline().child("unverified") }
}
