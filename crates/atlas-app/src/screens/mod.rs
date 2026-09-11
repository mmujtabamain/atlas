//! One module per sidebar section. [`Section`] is the navigation model; every
//! section maps to the plan sections it implements and to the milestone that
//! delivers it, so a screen that is not built yet can say so precisely.

pub mod accounts;
pub mod assumptions;
pub mod companies;
pub mod entities;
pub mod household;
pub mod liquidity;
pub mod people;
pub mod placeholder;
pub mod rules;
pub mod projections;
pub mod settings;
pub mod taxes;
pub mod timeline;

use gpui_kit::assets::IconName;

/// The sidebar destinations (plan doc §2 "Screen map").
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

/// A milestone that delivers a section, with its DevBench board task.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Milestone {
    pub number: u8,
    pub task_id: u32,
    pub title: &'static str,
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

    /// The `plan.md` sections a screen implements.
    pub fn plan_sections(self) -> &'static str {
        match self {
            Section::Household => "§5, §6, §7, §2.1",
            Section::People => "§5.2, §7",
            Section::Companies => "§5.3, §8",
            Section::Accounts => "§5.4, §7",
            Section::Liquidity => "§6, §17, M02, E01, E08",
            Section::Timeline => "§9, §15, §16, M05",
            Section::Projections => "§11, §2.1, §2.4, §10.6",
            Section::Assumptions => "§10, §32.2",
            Section::Taxes => "§12, M23, M24, E05",
            Section::Rules => "§14, M54",
            Section::Scenarios => "§18",
            Section::Decisions => "§13, §19, §20, §26, E02, E03, E07",
            Section::Privacy => "§7.1–7.6, §18.5, M55",
            Section::Settings => "—",
        }
    }

    /// The milestone that delivers the screen when it is not built yet.
    pub fn pending_milestone(self) -> Option<Milestone> {
        let m = |number, task_id, title| Some(Milestone { number, task_id, title });
        match self {
            Section::Household | Section::Settings | Section::People | Section::Companies | Section::Accounts | Section::Liquidity | Section::Timeline | Section::Projections | Section::Assumptions | Section::Taxes => None,
            Section::Rules => m(7, 2776, "Rules engine"),
            Section::Scenarios => m(8, 2783, "Scenarios & comparison"),
            Section::Decisions => m(9, 2789, "Funding optimizer & affordability decisions"),
            Section::Privacy => m(10, 2796, "Privacy & authorization"),
        }
    }

    /// What the screen will show, for the placeholder.
    pub fn promise(self) -> &'static [&'static str] {
        match self {
            Section::People => &["Roles, owned and co-owned accounts with shares", "Income sources and companies per person"],
            Section::Companies => &["Separate company ledger: accounts, employees, payroll", "Constraints: payroll reserve, working-capital floor", "Extractable cash with the §8.5/M27 legal-capacity caveat"],
            Section::Accounts => &["The full §7 property table", "Account detail with the balance-definition strip"],
            Section::Rules => &["User rules: scope, trigger, conditions, action, priority, version", "Conflict-resolution inspector", "Rule simulation"],
            Section::Scenarios => &["Overlays over the baseline and their composition", "Side-by-side comparison metrics"],
            Section::Decisions => &["Funding optimizer with gross-up to net (E02) and §13.5 strategy comparison", "Affordability grid (E03) with conditional result", "Goal trade-offs and the recommendation contract"],
            Section::Privacy => &["Viewer switcher and per-object access policies", "Provenance projection and difference-attack suppression", "Privacy audit log"],
            Section::Household | Section::Settings | Section::Liquidity | Section::Timeline | Section::Projections | Section::Assumptions | Section::Taxes => &[],
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
