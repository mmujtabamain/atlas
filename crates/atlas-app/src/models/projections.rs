//! Projections: the chronological forecast of a boundary under a named case
//! — path chart against the hard floor, the calculation chain, lowest balance
//! and breaches, per-account paths and transfer points, the assumptions the
//! path depends on, and the record that makes the run reproducible.

use atlas_core::authz::Viewer;
use atlas_core::forecast::{BoundaryForecast, Case, ForecastOptions, forecast};
use atlas_core::ids::{ObjectRef, ScenarioId};
use atlas_core::liquidity::Boundary;
use atlas_core::model::Household;
use atlas_core::{Disclosure, EngineResult};
use chrono::NaiveDate;
use gpui_kit::*;

use crate::widgets::figure::ExplainedFigure;
use crate::widgets::grid::{self, Cell, GridColumn, Row};
use crate::widgets::statement::{Line, Statement};

/// One chart sample: the boundary balance after a posting instant.
#[derive(Clone, Debug)]
pub struct ChartPoint {
    pub label: SharedString,
    pub balance: f64,
    pub floor: f64,
}

#[derive(Clone, Debug)]
pub struct ProjectionModel {
    pub forecast: BoundaryForecast,
    pub boundaries: Vec<Boundary>,
    pub start: ExplainedFigure,
    pub end: ExplainedFigure,
    pub lowest: ExplainedFigure,
    pub injection: ExplainedFigure,
    pub chart: Vec<ChartPoint>,
    /// The path as exact rows for the `Values` table.
    pub path_rows: grid::Rows,
    /// The conditional statement of this run.
    pub statement: Statement,
    /// Assumptions the viewer may not read, counted only.
    pub hidden_assumptions: usize,
    /// The scenario the overlay toggle applies (see [`super::overlay_scenario`]).
    pub overlay_scenario: Option<ScenarioId>,
}

/// Columns of the `Values` table.
pub const PATH_COLUMNS: [GridColumn; 4] = [
    GridColumn::new("date", "Date", 180.),
    GridColumn::new("balance", "Balance after posting", 300.).right(),
    GridColumn::new("change", "Change", 250.).right(),
    GridColumn::new("floor", "Against the floor", 564.),
];

impl ProjectionModel {
    pub fn compute(household: &Household, viewer: Viewer, boundary: Boundary, case: Case, scenario: Option<ScenarioId>, through: NaiveDate) -> EngineResult<Self> {
        log::info!("computing projection {:?} {:?} scenario {:?} through {}", boundary, case, scenario, through);
        let mut boundaries = vec![Boundary::Household];
        boundaries.extend(household.people.iter().map(|p| Boundary::Person(p.id)));
        boundaries.extend(
            household
                .companies
                .iter()
                .filter(|c| matches!(household.disclosure_for(viewer, ObjectRef::Company(c.id)), Disclosure::Full | Disclosure::SelectedFields))
                .map(|c| Boundary::Company(c.id)),
        );
        let boundary = if boundaries.contains(&boundary) { boundary } else { Boundary::Household };
        let result = forecast(household, boundary, ForecastOptions { through, scenario, case })?;
        let slug = boundary.slug();
        let per_major = household.base_currency.minor_per_major() as f64;
        let floor = result.floor.minor() as f64 / per_major;
        let chart = result
            .path
            .iter()
            .map(|p| ChartPoint { label: SharedString::from(p.date.format("%d %b").to_string()), balance: p.balance.minor() as f64 / per_major, floor })
            .collect();
        let mut previous: Option<atlas_core::Money> = None;
        let path_rows: grid::Rows = std::sync::Arc::new(
            result
                .path
                .iter()
                .map(|p| {
                    let change = previous.map(|prev| p.balance - prev);
                    previous = Some(p.balance);
                    let against = match p.balance.checked_sub(result.floor) {
                        Ok(headroom) if !headroom.is_negative() => format!("{} above", headroom.format()),
                        Ok(deficit) => format!("{} below", deficit.abs().format()),
                        Err(_) => String::new(),
                    };
                    Row::new(vec![
                        Cell::text(p.date.format("%d %b %Y").to_string()),
                        Cell::money(p.balance),
                        Cell::muted(change.map(|c| c.format_signed()).unwrap_or_else(|| "start".into())),
                        Cell::muted(against),
                    ])
                })
                .collect(),
        );
        let visible_assumptions: Vec<Line> = result.assumptions.iter().filter(|a| a.private_to.is_none_or(|p| p == viewer.person)).map(|a| Line::from_assumption(a, household.as_of)).collect();
        let hidden_assumptions = result.assumptions.len() - visible_assumptions.len();
        let plan = scenario.and_then(|id| household.scenario(id)).map(|s| format!("with scenario “{}”", s.name)).unwrap_or_else(|| "baseline".into());
        let statement = Statement {
            claim: match result.breach.first_breach {
                None => format!("If every listed assumption holds, {} cash ends at {} on {} and never falls below the {} floor.", boundary.label(household), result.end.money().format(), through.format("%d %b %Y"), result.floor.format()),
                Some(first) => format!("If every listed assumption holds, {} cash ends at {} on {} but falls below the {} floor from {}.", boundary.label(household), result.end.money().format(), through.format("%d %b %Y"), result.floor.format(), first.format("%d %b %Y")),
            },
            through,
            scope: format!("{} · {} case · {plan}", boundary.label(household), case.label()),
            assumptions: visible_assumptions,
            some_hidden: hidden_assumptions > 0,
            strength: atlas_core::ResultStrength::ScenarioTested,
            coverage: format!("One explicit path under the {} case: every planned movement at its {} value, in posting order.", case.label().to_lowercase(), case.label().to_lowercase()),
            does_not_establish: "That the money will arrive; a probability of any outcome; that other cases behave the same.".into(),
            excluded_shocks: "Unplanned movements, changes to rules or tax packs after this run, and anything outside the assumptions above.".into(),
        };
        Ok(ProjectionModel {
            start: ExplainedFigure::new(format!("{slug}-proj-start"), "Reconciled starting cash", &result.start, household, viewer),
            end: ExplainedFigure::new(format!("{slug}-proj-end"), "Conditional projected cash at horizon", &result.end, household, viewer),
            lowest: ExplainedFigure::new(format!("{slug}-proj-lowest"), "Lowest projected cash", &result.lowest, household, viewer),
            injection: ExplainedFigure::new(format!("{slug}-proj-injection"), "Cash needed today to never breach", &result.breach.minimum_injection, household, viewer),
            chart,
            path_rows,
            statement,
            hidden_assumptions,
            boundaries,
            overlay_scenario: super::overlay_scenario(household, viewer),
            forecast: result,
        })
    }
}
