//! What a screen is shown, computed once: each module turns the household and
//! the viewer into the figures, rows and chains its screen renders, so a
//! screen file only lays out what it is given. Drawing lives in `screens`.
//!
//! Navigation — the destinations, their routes and slugs — lives in
//! [`crate::nav`]; the workspace's panes decide what is on show.

pub(crate) mod assumptions;
pub(crate) mod decisions;
pub mod entities;
pub(crate) mod household;
pub(crate) mod liquidity;
pub(crate) mod privacy;
pub(crate) mod rules;
pub(crate) mod scenarios;
pub(crate) mod projections;
pub(crate) mod taxes;
pub(crate) mod timeline;

use atlas_core::authz::Viewer;
use atlas_core::ids::{ObjectRef, ScenarioId};
use atlas_core::model::Household;
use atlas_core::Disclosure;

/// The scenario behind the “overlay scenario …” toggles of the Timeline,
/// Projections, Assumptions, Taxes and Rules screens: the household's first
/// scenario the viewer may see. `None` hides the toggle.
pub fn overlay_scenario(household: &Household, viewer: Viewer) -> Option<ScenarioId> {
    household
        .scenarios
        .iter()
        .map(|s| s.id)
        .find(|id| !matches!(household.disclosure_for(viewer, ObjectRef::Scenario(*id)), Disclosure::Hidden))
}
