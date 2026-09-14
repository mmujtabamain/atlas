//! Liquidity & reservations: the money definitions of a chosen boundary, its
//! hard floors and headroom, the runway of the household path against those
//! floors, and the earmarks themselves — with a reservation editor and the
//! pay-and-release action.

use atlas_core::authz::Viewer;
use atlas_core::breach::{self, BreachReport};
use atlas_core::forecast::{Case, ForecastOptions, forecast};
use atlas_core::ids::{ObjectRef, ReservationId};
use atlas_core::liquidity::{Boundary, boundary_liquidity};
use atlas_core::model::Household;
use atlas_core::{Disclosure, EngineResult, Money};
use chrono::NaiveDate;

use crate::widgets::figure::ExplainedFigure;

/// Everything the screen shows for one boundary.
#[derive(Clone, Debug)]
pub struct LiquidityModel {
    pub boundary: Boundary,
    pub boundaries: Vec<Boundary>,
    pub figures: Vec<ExplainedFigure>,
    pub hard_floor: ExplainedFigure,
    pub headroom: ExplainedFigure,
    pub spendable_display: Money,
    pub deficit: Money,
    /// Household only: the liquid-cash path against the hard floors.
    pub runway: Option<BreachReport>,
    pub minimum_injection: Option<ExplainedFigure>,
    pub horizon: NaiveDate,
    /// Reservations on the boundary's accounts, active and released.
    pub reservations: Vec<ReservationId>,
}

impl LiquidityModel {
    pub fn compute(household: &Household, viewer: Viewer, boundary: Boundary, horizon: NaiveDate) -> EngineResult<Self> {
        log::info!("computing liquidity for boundary {:?} viewer {}", boundary, viewer.person);
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

        let report = boundary_liquidity(household, boundary)?;
        let slug = boundary.slug();
        let figures = report
            .figures
            .iter()
            .enumerate()
            .map(|(index, (label, calc))| ExplainedFigure::new(format!("{slug}-figure-{index}"), label.clone(), calc, household, viewer))
            .collect();
        let hard_floor = ExplainedFigure::new(format!("{slug}-hard-floor"), "Hard floors", &report.hard_floor, household, viewer);
        let headroom = ExplainedFigure::new(format!("{slug}-headroom"), "Headroom over hard floors", &report.headroom, household, viewer);
        let deficit = report.headroom.money().negated().clamped_at_zero();

        let (runway, minimum_injection) = if boundary == Boundary::Household {
            let projection = forecast(household, Boundary::Household, ForecastOptions { through: horizon, scenario: None, case: Case::Expected })?;
            let breach = breach::analyse(&projection.path, report.hard_floor.money(), horizon)?;
            let injection = ExplainedFigure::new(format!("{slug}-injection"), "Cash needed today to never breach", &breach.minimum_injection, household, viewer);
            (Some(breach), Some(injection))
        } else {
            (None, None)
        };

        let reservations = household
            .reservations
            .iter()
            .filter(|r| report.accounts.contains(&r.account))
            .filter(|r| !matches!(household.disclosure_for(viewer, ObjectRef::Reservation(r.id)), Disclosure::Hidden | Disclosure::Aggregate))
            .map(|r| r.id)
            .collect();

        Ok(LiquidityModel {
            boundary,
            boundaries,
            figures,
            hard_floor,
            headroom,
            spendable_display: report.headroom.money().clamped_at_zero(),
            deficit,
            runway,
            minimum_injection,
            horizon,
            reservations,
        })
    }
}

/// The viewer must have full disclosure on the account to add earmarks to it.
pub fn editable_accounts(household: &Household, viewer: Viewer) -> Vec<atlas_core::ids::AccountId> {
    household
        .accounts
        .iter()
        .filter(|a| !a.is_company_account() || household.disclosure_for(viewer, ObjectRef::Account(a.id)) == Disclosure::Full)
        .filter(|a| matches!(household.disclosure_for(viewer, ObjectRef::Account(a.id)), Disclosure::Full))
        .map(|a| a.id)
        .collect()
}
