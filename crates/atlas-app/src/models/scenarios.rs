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

use crate::widgets::figure::ExplainedFigure;


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

/// The comparison's four headline figures, each projected for the viewer
/// **once**, when the model is computed.
///
/// The screen used to build these itself, per frame, from the engine's raw
/// chain and with `Disclosure::Full` hardcoded — so a calculation sheet on
/// this screen showed the whole chain whatever the viewer may see, while the
/// same figure on every other screen shows the projection. Building them here
/// is what makes them agree.
#[derive(Clone, Debug)]
pub struct ComparisonFigures {
    pub baseline_end: ExplainedFigure,
    pub overlaid_end: ExplainedFigure,
    pub baseline_lowest: ExplainedFigure,
    pub overlaid_lowest: ExplainedFigure,
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
    /// The comparison's headline figures, projected for the viewer.
    pub figures: Option<ComparisonFigures>,
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
        let figures = comparison.as_ref().map(|c| {
            let figure = |id: &'static str, label: &'static str, calc| ExplainedFigure::new(id, label, calc, household, viewer);
            ComparisonFigures {
                baseline_end: figure("comparison-baseline-end", "Baseline at the end", &c.baseline.end),
                overlaid_end: figure("comparison-overlaid-end", "With the scenarios", &c.overlaid.end),
                baseline_lowest: figure("comparison-baseline-lowest", "Lowest baseline", &c.baseline.lowest),
                overlaid_lowest: figure("comparison-overlaid-lowest", "Lowest with the scenarios", &c.overlaid.lowest),
            }
        });
        let (attribution, suppression_note) = match &comparison {
            Some(c) => project_attribution(household, viewer, &c.attribution, c.end_delta),
            None => (Vec::new(), None),
        };
        let per_major = 10f64.powi(household.base_currency.minor_digits() as i32);
        let chart = comparison
            .as_ref()
            .map(|c| c.merged_path.iter().map(|(date, base, over)| ComparisonPoint { label: SharedString::from(date.format("%d %b").to_string()), baseline: base.minor() as f64 / per_major, scenario: over.minor() as f64 / per_major }).collect())
            .unwrap_or_default();
        Ok(ScenariosModel { through, case, cards, selection, hidden_count, incompatibilities, comparison, figures, comparison_error, chart, attribution, suppression_note })
    }
}

#[cfg(test)]
mod tests {
    // Named imports, not a glob: `super::*` re-exports gpui-kit's own `test`
    // attribute over the standard one.
    use super::ScenariosModel;
    use atlas_core::authz::Viewer;
    use atlas_core::forecast::Case;
    use atlas_core::ids::ScenarioId;
    use atlas_core::fixtures;

    /// The comparison's calculation sheets show the chain the viewer may see.
    ///
    /// The screen used to build these itself, per frame, from the engine's raw
    /// chain with `Disclosure::Full` written in — so `ⓘ` on this one screen
    /// opened the whole calculation whatever the viewer's policies said, while
    /// the same figure elsewhere opened the projection. Building them in the
    /// model is what makes the two agree. The leak was real, not theoretical:
    /// the engine's chain is identical whoever asks, so rendering it directly
    /// showed every viewer the owner's calculation.
    #[test]
    fn comparison_figures_are_projected_for_the_viewer() {
        let household = fixtures::plan_household();
        let selection: Vec<ScenarioId> = household.scenarios.iter().map(|s| s.id).take(1).collect();
        assert!(!selection.is_empty(), "the fixture has a scenario to compare");

        let chain_for = |person| {
            let model = ScenariosModel::compute(&household, Viewer::person(person), fixtures::default_horizon(), Case::Expected, &selection)
                .expect("the comparison computes");
            let figures = model.figures.expect("a comparison has its figures");
            figures.overlaid_end.calc.node().render_chain()
        };

        // The engine's own chain is the same whoever is looking — it is the
        // calculation, not a view of it. That is exactly why the screen must
        // not render it directly, and it is what keeps the assertion below
        // from being vacuous.
        let raw = |person| {
            let model = ScenariosModel::compute(&household, Viewer::person(person), fixtures::default_horizon(), Case::Expected, &selection).unwrap();
            model.comparison.unwrap().overlaid.end.node().render_chain()
        };
        assert_eq!(raw(fixtures::ids::PERSON_A), raw(fixtures::ids::PERSON_B), "the engine's chain does not depend on the viewer");

        let owner = chain_for(fixtures::ids::PERSON_A);
        let other = chain_for(fixtures::ids::PERSON_B);
        assert!(!owner.is_empty() && !other.is_empty(), "both viewers get a chain");
        assert_ne!(
            owner, other,
            "the two viewers see different chains; an identical one means the chain was not projected and the sheet \
             is showing every viewer the owner's calculation"
        );
    }
}
