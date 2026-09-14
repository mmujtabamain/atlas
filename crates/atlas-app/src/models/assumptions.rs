//! Assumptions: the register with kinds, sources, acceptance and freshness;
//! deterministic derivation from history with the formula and sample
//! disclosed; the vocabulary legends every value is tagged with; and
//! one-at-a-time sensitivity with the joint caveat.

use atlas_core::assumptions::{ConditionalStatement, DerivedAssumption, derive, Derivation};
use atlas_core::authz::Viewer;
use atlas_core::forecast::{Case, ForecastOptions};
use atlas_core::ids::{ObjectRef, ScenarioId, SeriesId};
use atlas_core::liquidity::Boundary;
use atlas_core::model::Household;
use atlas_core::sensitivity::{SensitivityReport, one_at_a_time};
use atlas_core::vocab::ResultStrength;
use atlas_core::{Disclosure, EngineError, EngineResult};
use chrono::NaiveDate;
use gpui_kit::*;

use crate::widgets::figure::ExplainedFigure;

#[derive(Clone, Debug)]
pub struct AssumptionsModel {
    pub horizon: NaiveDate,
    /// Series with reconciled history the derivation panel can use.
    pub derivable: Vec<SeriesId>,
    pub derivation_series: SeriesId,
    pub derivation: Derivation,
    pub derived: Result<DerivedAssumption, EngineError>,
    pub derived_figure: Option<ExplainedFigure>,
    pub sensitivity_boundaries: Vec<Boundary>,
    pub sensitivity_boundary: Boundary,
    pub sensitivity_scenario: bool,
    pub sensitivity: SensitivityReport,
    pub statement: ConditionalStatement,
    /// The scenario the sensitivity toggle applies (see [`super::overlay_scenario`]).
    pub overlay_scenario: Option<ScenarioId>,
}

impl AssumptionsModel {
    pub fn compute(
        household: &Household,
        viewer: Viewer,
        derivation_series: Option<SeriesId>,
        derivation: Derivation,
        sensitivity_boundary: Boundary,
        sensitivity_scenario: bool,
        horizon: NaiveDate,
    ) -> EngineResult<Self> {
        log::info!("computing assumptions model: derivation {:?} boundary {:?}", derivation, sensitivity_boundary);
        let mut derivable: Vec<SeriesId> = household
            .series
            .iter()
            .filter(|s| !household.history_of(s.id).is_empty())
            .filter(|s| !matches!(household.disclosure_for(viewer, ObjectRef::Series(s.id)), Disclosure::Hidden | Disclosure::Aggregate))
            .map(|s| s.id)
            .collect();
        derivable.sort_by_key(|id| id.raw());
        let derivation_series = derivation_series.filter(|id| derivable.contains(id)).or_else(|| derivable.first().copied()).unwrap_or(SeriesId::new(0));
        let derived = derive(household, derivation_series, derivation);
        let derived_figure = derived.as_ref().ok().map(|d| {
            ExplainedFigure::new(format!("derived-{}", derivation_series.raw()), "Derived expected amount", &d.calc, household, viewer)
        });

        let mut sensitivity_boundaries = vec![Boundary::Household];
        sensitivity_boundaries.extend(
            household
                .accounts
                .iter()
                .filter(|a| a.kind.is_cash() && !a.is_company_account())
                .filter(|a| matches!(household.disclosure_for(viewer, ObjectRef::Account(a.id)), Disclosure::Full | Disclosure::SelectedFields))
                .map(|a| Boundary::Account(a.id)),
        );
        let sensitivity_boundary = if sensitivity_boundaries.contains(&sensitivity_boundary) { sensitivity_boundary } else { Boundary::Household };
        let overlay_scenario = super::overlay_scenario(household, viewer);
        let options = ForecastOptions {
            through: horizon,
            scenario: if sensitivity_scenario { overlay_scenario } else { None },
            case: Case::Expected,
        };
        let sensitivity = one_at_a_time(household, sensitivity_boundary, options)?;
        let forecast = atlas_core::forecast::forecast(household, sensitivity_boundary, options)?;
        let statement = ConditionalStatement {
            claim: if sensitivity.baseline_breaches {
                format!(
                    "The {} path falls below its {} floor (lowest {})",
                    sensitivity_boundary.label(household),
                    sensitivity.floor.format(),
                    sensitivity.baseline_lowest.format()
                )
            } else {
                format!(
                    "The {} path stays above its {} floor (lowest {})",
                    sensitivity_boundary.label(household),
                    sensitivity.floor.format(),
                    sensitivity.baseline_lowest.format()
                )
            },
            horizon,
            coverage: ResultStrength::ConditionalPath,
            assumptions: ConditionalStatement::assumption_texts(&forecast.assumptions.iter().filter(|a| a.private_to.is_none_or(|p| p == viewer.person)).cloned().collect::<Vec<_>>()),
            excluded_shocks: vec![
                "unplanned expenses beyond the one-off limit below".into(),
                "several assumptions failing together (each limit below is found on its own)".into(),
                "changes to the effective tax packs or user rules".into(),
            ],
        };
        Ok(AssumptionsModel {
            horizon,
            derivable,
            derivation_series,
            derivation,
            derived,
            derived_figure,
            sensitivity_boundaries,
            sensitivity_boundary,
            sensitivity_scenario,
            sensitivity,
            statement,
            overlay_scenario,
        })
    }
}
