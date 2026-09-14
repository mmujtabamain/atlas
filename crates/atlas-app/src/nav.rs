//! Navigation: eight sidebar destinations, the routes inside them, and the
//! stable slugs `--screen`, element ids and tests address them by.
//!
//! A [`Destination`] is a sidebar entry; a [`Route`] is one addressable
//! surface (a screen, a detail of one object, or a stage of a flow). Every
//! route knows its destination and its local tab, so the shell can highlight
//! the sidebar and each workspace can draw its tab bar from the same value.

use atlas_core::authz::Viewer;
use atlas_core::ids::{AccountId, CompanyId, ObjectRef, PersonId, RuleId, SeriesId};
use atlas_core::model::Household;
use atlas_core::Disclosure;
use gpui_kit::assets::IconName;

/// The sidebar entries, in order. `Settings` lives in the footer.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Destination {
    Today,
    Decisions,
    Forecast,
    Accounts,
    Activity,
    Household,
    RulesTaxes,
    Sharing,
    Settings,
}

impl Destination {
    /// Sidebar groups, in order; the group breaks are visual separators.
    pub const GROUPS: [&'static [Destination]; 3] = [
        &[Destination::Today, Destination::Decisions, Destination::Forecast],
        &[Destination::Accounts, Destination::Activity, Destination::Household],
        &[Destination::RulesTaxes, Destination::Sharing],
    ];

    pub fn label(self) -> &'static str {
        match self {
            Destination::Today => "Today",
            Destination::Decisions => "Decisions",
            Destination::Forecast => "Forecast",
            Destination::Accounts => "Accounts",
            Destination::Activity => "Activity",
            Destination::Household => "People & companies",
            Destination::RulesTaxes => "Rules & taxes",
            Destination::Sharing => "Sharing",
            Destination::Settings => "Settings",
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            // Not a sun: the theme button in the title bar is a sun
            // whenever the dark theme is on, and two suns in one chrome read
            // as one control in two places.
            Destination::Today => IconName::Gauge,
            Destination::Decisions => IconName::Target,
            Destination::Forecast => IconName::ChartLine,
            Destination::Accounts => IconName::Landmark,
            Destination::Activity => IconName::CalendarDays,
            Destination::Household => IconName::Users,
            Destination::RulesTaxes => IconName::Gavel,
            Destination::Sharing => IconName::ShieldCheck,
            Destination::Settings => IconName::Settings,
        }
    }

    /// The route a sidebar click opens.
    pub fn home(self) -> Route {
        match self {
            Destination::Today => Route::Today,
            Destination::Decisions => Route::Purchase,
            Destination::Forecast => Route::ForecastPath,
            Destination::Accounts => Route::Accounts,
            Destination::Activity => Route::Upcoming,
            Destination::Household => Route::People,
            Destination::RulesTaxes => Route::Rules,
            Destination::Sharing => Route::Policies,
            Destination::Settings => Route::Settings,
        }
    }

    /// The local tabs of the workspace: label and the route each opens.
    pub fn tabs(self) -> &'static [(&'static str, Route)] {
        match self {
            Destination::Today | Destination::Settings => &[],
            Destination::Decisions => &[("Purchase", Route::Purchase), ("Scenarios", Route::Scenarios), ("Extraction timing", Route::Extraction)],
            Destination::Forecast => &[("Path", Route::ForecastPath), ("Assumptions", Route::Assumptions), ("Sensitivity", Route::Sensitivity)],
            Destination::Accounts => &[("Accounts", Route::Accounts), ("Earmarks", Route::Earmarks), ("Funding", Route::Funding)],
            Destination::Activity => &[("Upcoming", Route::Upcoming), ("Series", Route::Series), ("Actuals", Route::Actuals)],
            Destination::Household => &[("People", Route::People), ("Companies", Route::Companies)],
            Destination::RulesTaxes => &[("Rules", Route::Rules), ("Rule activity", Route::RuleActivity), ("Taxes", Route::Taxes), ("Tax packs", Route::TaxPacks)],
            Destination::Sharing => &[("Policies", Route::Policies), ("Grants", Route::Grants), ("Audit", Route::Audit)],
        }
    }
}

/// One addressable surface.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Route {
    /// No household is open.
    Welcome,
    Today,
    // Decisions
    Purchase,
    PurchaseResult,
    Scenarios,
    ScenarioCompare,
    Extraction,
    // Forecast
    ForecastPath,
    Assumptions,
    Derive,
    Sensitivity,
    // Accounts
    Accounts,
    Account(AccountId),
    Earmarks,
    Funding,
    // Activity
    Upcoming,
    Series,
    SeriesDetail(SeriesId),
    Actuals,
    // People & companies
    People,
    Person(PersonId),
    Companies,
    Company(CompanyId),
    // Rules & taxes
    Rules,
    Rule(RuleId),
    CreateRule,
    RuleActivity,
    Taxes,
    TaxPacks,
    // Sharing
    Policies,
    Grants,
    Audit,
    Settings,
}

/// A `--screen` value naming a *kind* of detail rather than a record: a
/// command line cannot name a record, because a record's id is internal.
///
/// `Route::slug` has always emitted these — `Route::Person(_)` is `"person"` —
/// but `from_slug` had no way to turn one back into a route, so `--screen
/// person` resolved to nothing and the app opened on its default screen
/// instead, saying so only in the log. They now open the first record the
/// chosen viewer may see.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FirstDetail {
    Person,
    Company,
    Account,
    Series,
    Rule,
}

impl FirstDetail {
    pub fn from_slug(slug: &str) -> Option<Self> {
        Some(match slug {
            "person" => FirstDetail::Person,
            "company" => FirstDetail::Company,
            "account" => FirstDetail::Account,
            "series-detail" => FirstDetail::Series,
            "rule" => FirstDetail::Rule,
            _ => return None,
        })
    }

    /// Every detail slug, for `--help` and the unknown-screen warning.
    pub fn slugs() -> &'static [&'static str] {
        &["person", "company", "account", "series-detail", "rule"]
    }

    /// The register the detail belongs to — where a household with none of
    /// that record, or none this viewer may see, opens instead.
    pub fn register(self) -> Route {
        match self {
            FirstDetail::Person => Route::People,
            FirstDetail::Company => Route::Companies,
            FirstDetail::Account => Route::Accounts,
            FirstDetail::Series => Route::Series,
            FirstDetail::Rule => Route::Rules,
        }
    }

    /// The first record of this kind the viewer may see. A hidden record is
    /// never opened by a command-line flag: `--screen` is a convenience, not
    /// a way past a policy.
    pub fn resolve(self, household: &Household, viewer: Viewer) -> Route {
        let visible = |object: ObjectRef| !matches!(household.disclosure_for(viewer, object), Disclosure::Hidden);
        let route = match self {
            FirstDetail::Person => household.people.iter().find(|p| visible(ObjectRef::Person(p.id))).map(|p| Route::Person(p.id)),
            FirstDetail::Company => household.companies.iter().find(|c| visible(ObjectRef::Company(c.id))).map(|c| Route::Company(c.id)),
            FirstDetail::Account => household.accounts.iter().find(|a| visible(ObjectRef::Account(a.id))).map(|a| Route::Account(a.id)),
            FirstDetail::Series => household.series.iter().find(|s| visible(ObjectRef::Series(s.id))).map(|s| Route::SeriesDetail(s.id)),
            FirstDetail::Rule => household.rules.first().map(|r| Route::Rule(r.id)),
        };
        route.unwrap_or_else(|| {
            log::warn!("--screen {self:?}: this household has no such record this viewer may see; opening its register");
            self.register()
        })
    }
}

impl Route {
    pub fn destination(self) -> Option<Destination> {
        Some(match self {
            Route::Welcome => return None,
            Route::Today => Destination::Today,
            Route::Purchase | Route::PurchaseResult | Route::Scenarios | Route::ScenarioCompare | Route::Extraction => Destination::Decisions,
            Route::ForecastPath | Route::Assumptions | Route::Derive | Route::Sensitivity => Destination::Forecast,
            Route::Accounts | Route::Account(_) | Route::Earmarks | Route::Funding => Destination::Accounts,
            Route::Upcoming | Route::Series | Route::SeriesDetail(_) | Route::Actuals => Destination::Activity,
            Route::People | Route::Person(_) | Route::Companies | Route::Company(_) => Destination::Household,
            Route::Rules | Route::Rule(_) | Route::CreateRule | Route::RuleActivity | Route::Taxes | Route::TaxPacks => Destination::RulesTaxes,
            Route::Policies | Route::Grants | Route::Audit => Destination::Sharing,
            Route::Settings => Destination::Settings,
        })
    }

    /// The index of the workspace tab this route belongs to.
    pub fn tab_index(self) -> Option<usize> {
        let tab = match self {
            Route::Purchase | Route::PurchaseResult => Route::Purchase,
            Route::Scenarios | Route::ScenarioCompare => Route::Scenarios,
            Route::Assumptions | Route::Derive => Route::Assumptions,
            Route::Account(_) => Route::Accounts,
            Route::SeriesDetail(_) => Route::Series,
            Route::Person(_) => Route::People,
            Route::Company(_) => Route::Companies,
            Route::Rule(_) | Route::CreateRule => Route::Rules,
            other => other,
        };
        self.destination()?.tabs().iter().position(|(_, route)| *route == tab)
    }

    /// Stable identifier for `--screen`, `screen-<slug>` element ids and tests.
    /// Object routes share their collection's slug.
    pub fn slug(self) -> &'static str {
        match self {
            Route::Welcome => "welcome",
            Route::Today => "today",
            Route::Purchase => "purchase",
            Route::PurchaseResult => "purchase-result",
            Route::Scenarios => "scenarios",
            Route::ScenarioCompare => "compare",
            Route::Extraction => "extraction",
            Route::ForecastPath => "forecast",
            Route::Assumptions => "assumptions",
            Route::Derive => "derive",
            Route::Sensitivity => "sensitivity",
            Route::Accounts => "accounts",
            Route::Account(_) => "account",
            Route::Earmarks => "earmarks",
            Route::Funding => "funding",
            Route::Upcoming => "upcoming",
            Route::Series => "series",
            Route::SeriesDetail(_) => "series-detail",
            Route::Actuals => "actuals",
            Route::People => "people",
            Route::Person(_) => "person",
            Route::Companies => "companies",
            Route::Company(_) => "company",
            Route::Rules => "rules",
            Route::Rule(_) => "rule",
            Route::CreateRule => "create-rule",
            Route::RuleActivity => "rule-activity",
            Route::Taxes => "taxes",
            Route::TaxPacks => "tax-packs",
            Route::Policies => "policies",
            Route::Grants => "grants",
            Route::Audit => "audit",
            Route::Settings => "settings",
        }
    }

    /// Resolves a `--screen` value: the new slugs and the fourteen original
    /// area slugs, which stay launchable as aliases.
    pub fn from_slug(slug: &str) -> Option<Route> {
        Some(match slug {
            "welcome" => Route::Welcome,
            "today" | "household" => Route::Today,
            "purchase" | "decisions" => Route::Purchase,
            "purchase-result" => Route::PurchaseResult,
            "scenarios" => Route::Scenarios,
            "compare" => Route::ScenarioCompare,
            "extraction" => Route::Extraction,
            "forecast" | "projections" => Route::ForecastPath,
            "assumptions" => Route::Assumptions,
            "derive" => Route::Derive,
            "sensitivity" => Route::Sensitivity,
            "accounts" => Route::Accounts,
            "earmarks" | "liquidity" => Route::Earmarks,
            "funding" => Route::Funding,
            "upcoming" | "timeline" => Route::Upcoming,
            "series" => Route::Series,
            "actuals" => Route::Actuals,
            "people" => Route::People,
            "companies" => Route::Companies,
            "rules" => Route::Rules,
            "create-rule" => Route::CreateRule,
            "rule-activity" => Route::RuleActivity,
            "taxes" => Route::Taxes,
            "tax-packs" => Route::TaxPacks,
            "policies" | "privacy" => Route::Policies,
            "grants" => Route::Grants,
            "audit" => Route::Audit,
            "settings" => Route::Settings,
            _ => return None,
        })
    }

    /// Every launchable screen slug, for `--help` and the unknown-screen
    /// warning. The detail slugs are [`FirstDetail::slugs`].
    pub fn slugs() -> &'static [&'static str] {
        &[
            "today", "purchase", "scenarios", "compare", "extraction", "forecast", "assumptions", "derive", "sensitivity", "accounts", "earmarks", "funding", "upcoming",
            "series", "actuals", "people", "companies", "rules", "create-rule", "rule-activity", "taxes", "tax-packs", "policies", "grants", "audit", "settings",
            "welcome",
        ]
    }

    /// The route to return to from a detail or flow.
    pub fn parent(self) -> Route {
        match self {
            Route::Account(_) => Route::Accounts,
            Route::SeriesDetail(_) => Route::Series,
            Route::Person(_) => Route::People,
            Route::Company(_) => Route::Companies,
            Route::Rule(_) | Route::CreateRule => Route::Rules,
            Route::PurchaseResult => Route::Purchase,
            Route::ScenarioCompare => Route::Scenarios,
            Route::Derive => Route::Assumptions,
            other => other.destination().map(Destination::home).unwrap_or(Route::Welcome),
        }
    }

    /// The heading the workspace shows for a route without an object.
    pub fn title(self) -> &'static str {
        match self {
            Route::Welcome => "Atlas Financer",
            Route::Today => "Today",
            Route::Purchase => "Build a purchase",
            Route::PurchaseResult => "Purchase result",
            Route::Scenarios => "Scenarios",
            Route::ScenarioCompare => "Scenario comparison",
            Route::Extraction => "Extraction timing illustration",
            Route::ForecastPath => "Forecast",
            Route::Assumptions => "Assumptions",
            Route::Derive => "Derive from history",
            Route::Sensitivity => "Sensitivity",
            Route::Accounts => "Accounts",
            Route::Account(_) => "Account",
            Route::Earmarks => "Earmarks and money by owner",
            Route::Funding => "Funding and expense accounts",
            Route::Upcoming => "Upcoming movements",
            Route::Series => "Planned series",
            Route::SeriesDetail(_) => "Series",
            Route::Actuals => "Actual transactions",
            Route::People => "People",
            Route::Person(_) => "Person",
            Route::Companies => "Companies",
            Route::Company(_) => "Company",
            Route::Rules => "Rules",
            Route::Rule(_) => "Rule",
            Route::CreateRule => "Create rule",
            Route::RuleActivity => "Rule activity",
            Route::Taxes => "Tax cash and events",
            Route::TaxPacks => "Tax packs",
            Route::Policies => "Sharing policies",
            Route::Grants => "Purpose-specific grants",
            Route::Audit => "Sharing audit",
            Route::Settings => "Settings",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_slug_round_trips_and_the_legacy_areas_still_launch() {
        for slug in Route::slugs() {
            let route = Route::from_slug(slug).unwrap_or_else(|| panic!("{slug}"));
            assert_eq!(Route::from_slug(route.slug()), Some(route), "{slug}");
        }
        for legacy in ["household", "people", "companies", "accounts", "liquidity", "timeline", "projections", "assumptions", "taxes", "rules", "scenarios", "decisions", "privacy", "settings"] {
            assert!(Route::from_slug(legacy).is_some(), "legacy slug {legacy} must stay launchable");
        }
    }

    #[test]
    fn every_route_with_a_destination_has_a_tab_or_is_a_single_screen() {
        for destination in Destination::GROUPS.iter().flat_map(|g| g.iter()) {
            let home = destination.home();
            assert_eq!(home.destination(), Some(*destination));
            if !destination.tabs().is_empty() {
                assert_eq!(home.tab_index(), Some(0));
            }
        }
        assert_eq!(Route::Account(AccountId::new(1)).tab_index(), Some(0));
        assert_eq!(Route::PurchaseResult.tab_index(), Some(0));
        assert_eq!(Route::Derive.tab_index(), Some(1));
        assert_eq!(Route::Settings.destination(), Some(Destination::Settings));
        assert_eq!(Route::Welcome.destination(), None);
    }
}
