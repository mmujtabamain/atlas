//! Scenarios: overlays over the baseline, listed with every change they
//! make; composition with the compatibility check; baseline versus scenario
//! side by side with the comparison metrics; and the difference attribution,
//! which sums to the end-of-window difference exactly.

use atlas_core::authz::Viewer;
use atlas_core::forecast::Case;
use atlas_core::ids::{ObjectRef, ScenarioId};
use atlas_core::liquidity::Boundary;
use atlas_core::model::Household;
use atlas_core::scenario::{AttributionLine, Incompatibility, OverlayEntry, ScenarioComparison, compare, project_attribution};
use atlas_core::{Disclosure, EngineResult};
use chrono::NaiveDate;
use gpui_kit::*;


/// One scenario as the viewer may see it.
#[derive(Clone, Debug)]
pub struct ScenarioCard {
    pub id: ScenarioId,
    pub name: String,
    pub description: String,
    pub private: bool,
    pub selected: bool,
    pub overlay: Vec<OverlayEntry>,
}

#[derive(Clone, Debug)]
pub struct ComparisonPoint {
    pub label: SharedString,
    pub baseline: f64,
    pub scenario: f64,
}

#[derive(Clone, Debug)]
pub struct ScenariosModel {
    pub through: NaiveDate,
    pub case: Case,
    pub cards: Vec<ScenarioCard>,
    pub selection: Vec<ScenarioId>,
    pub hidden_count: usize,
    pub incompatibilities: Vec<Incompatibility>,
    pub comparison: Option<ScenarioComparison>,
    pub comparison_error: Option<String>,
    pub chart: Vec<ComparisonPoint>,
    /// The attribution as this viewer may see it.
    pub attribution: Vec<AttributionLine>,
    pub suppression_note: Option<String>,
}

impl ScenariosModel {
    pub fn compute(household: &Household, viewer: Viewer, through: NaiveDate, case: Case, selection: &[ScenarioId]) -> EngineResult<Self> {
        // A private scenario never appears in another person's list; a composition is
        // visible only when every member is.
        let visible = |id: ScenarioId| -> bool {
            let closure = household.scenario_closure(&[id]);
            closure.iter().all(|s| !matches!(household.disclosure_for(viewer, ObjectRef::Scenario(*s)), Disclosure::Hidden))
        };
        let cards: Vec<ScenarioCard> = household
            .scenarios
            .iter()
            .filter(|s| visible(s.id))
            .map(|s| ScenarioCard { id: s.id, name: s.name.clone(), description: s.description.clone(), private: s.private_to.is_some(), selected: selection.contains(&s.id), overlay: household.scenario_overlay(s.id) })
            .collect();
        let hidden_count = household.scenarios.len() - cards.len();
        let selection: Vec<ScenarioId> = selection.iter().copied().filter(|id| cards.iter().any(|c| c.id == *id)).collect();
        log::info!("scenarios: {} visible to {} ({hidden_count} hidden), comparing {selection:?} ({} case)", cards.len(), viewer.person, case.label());
        let incompatibilities = household.check_compatibility(&selection);
        let (comparison, comparison_error) = if selection.is_empty() {
            (None, None)
        } else {
            match compare(household, Boundary::Household, &selection, through, case) {
                Ok(c) => (Some(c), None),
                Err(err) => {
                    crate::alerting::report(crate::alerting::Level::Warning, format!("scenario comparison failed for {selection:?}: {err}"));
                    (None, Some(err.to_string()))
                }
            }
        };
        let (attribution, suppression_note) = match &comparison {
            Some(c) => project_attribution(household, viewer, &c.attribution, c.end_delta),
            None => (Vec::new(), None),
        };
        let per_major = 10f64.powi(household.base_currency.minor_digits() as i32);
        let chart = comparison
            .as_ref()
            .map(|c| c.merged_path.iter().map(|(date, base, over)| ComparisonPoint { label: SharedString::from(date.format("%d %b").to_string()), baseline: base.minor() as f64 / per_major, scenario: over.minor() as f64 / per_major }).collect())
            .unwrap_or_default();
        Ok(ScenariosModel { through, case, cards, selection, hidden_count, incompatibilities, comparison, comparison_error, chart, attribution, suppression_note })
    }
}
