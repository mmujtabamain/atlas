//! One module per sidebar section. [`Section`] is the navigation model: the
//! sidebar groups, labels, icons and the stable slugs used by `--screen`,
//! element ids and tests.

pub mod accounts;
pub mod assumptions;
pub mod companies;
pub mod decisions;
pub mod entities;
pub mod household;
pub mod liquidity;
pub mod people;
pub mod privacy;
pub mod rules;
pub mod scenarios;
pub mod projections;
pub mod settings;
pub mod taxes;
pub mod timeline;

use atlas_core::authz::Viewer;
use atlas_core::ids::{ObjectRef, ScenarioId};
use atlas_core::model::Household;
use atlas_core::Disclosure;
use gpui_kit::assets::IconName;

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

/// The sidebar destinations.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Section {
    Household,
    People,
    Companies,
    Accounts,
    Liquidity,
    Timeline,
    Projections,
    Assumptions,
    Taxes,
    Rules,
    Scenarios,
    Decisions,
    Privacy,
    Settings,
}

impl Section {
    pub const ALL: [Section; 14] = [
        Section::Household,
        Section::People,
        Section::Companies,
        Section::Accounts,
        Section::Liquidity,
        Section::Timeline,
        Section::Projections,
        Section::Assumptions,
        Section::Taxes,
        Section::Rules,
        Section::Scenarios,
        Section::Decisions,
        Section::Privacy,
        Section::Settings,
    ];

    /// Sidebar groups, in order, with their sections.
    pub const GROUPS: [(&'static str, &'static [Section]); 5] = [
        ("Household", &[Section::Household, Section::People, Section::Companies, Section::Accounts]),
        ("Money", &[Section::Liquidity, Section::Timeline, Section::Projections, Section::Assumptions]),
        ("Rules", &[Section::Taxes, Section::Rules]),
        ("Decisions", &[Section::Scenarios, Section::Decisions]),
        ("Governance", &[Section::Privacy, Section::Settings]),
    ];

    pub fn label(self) -> &'static str {
        match self {
            Section::Household => "Household",
            Section::People => "People",
            Section::Companies => "Companies",
            Section::Accounts => "Accounts",
            Section::Liquidity => "Liquidity & reservations",
            Section::Timeline => "Timeline",
            Section::Projections => "Projections",
            Section::Assumptions => "Assumptions",
            Section::Taxes => "Taxes",
            Section::Rules => "Rules",
            Section::Scenarios => "Scenarios",
            Section::Decisions => "Decisions",
            Section::Privacy => "Privacy",
            Section::Settings => "Settings",
        }
    }

    /// Stable identifier for `--screen`, element ids and tests.
    pub fn slug(self) -> &'static str {
        match self {
            Section::Household => "household",
            Section::People => "people",
            Section::Companies => "companies",
            Section::Accounts => "accounts",
            Section::Liquidity => "liquidity",
            Section::Timeline => "timeline",
            Section::Projections => "projections",
            Section::Assumptions => "assumptions",
            Section::Taxes => "taxes",
            Section::Rules => "rules",
            Section::Scenarios => "scenarios",
            Section::Decisions => "decisions",
            Section::Privacy => "privacy",
            Section::Settings => "settings",
        }
    }

    pub fn from_slug(slug: &str) -> Option<Section> {
        Section::ALL.into_iter().find(|s| s.slug() == slug)
    }

    pub fn slugs() -> Vec<&'static str> {
        Section::ALL.into_iter().map(Section::slug).collect()
    }

    pub fn icon(self) -> IconName {
        match self {
            Section::Household => IconName::House,
            Section::People => IconName::Users,
            Section::Companies => IconName::Building2,
            Section::Accounts => IconName::Landmark,
            Section::Liquidity => IconName::Wallet,
            Section::Timeline => IconName::CalendarDays,
            Section::Projections => IconName::ChartLine,
            Section::Assumptions => IconName::Lightbulb,
            Section::Taxes => IconName::Percent,
            Section::Rules => IconName::Gavel,
            Section::Scenarios => IconName::GitBranch,
            Section::Decisions => IconName::Target,
            Section::Privacy => IconName::ShieldCheck,
            Section::Settings => IconName::Settings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_round_trip_and_groups_cover_every_section() {
        for section in Section::ALL {
            assert_eq!(Section::from_slug(section.slug()), Some(section));
        }
        let grouped: usize = Section::GROUPS.iter().map(|(_, s)| s.len()).sum();
        assert_eq!(grouped, Section::ALL.len());
    }
}
