//! The pane registry: what a pane shows, as data the layout model can keep
//! and save, and the way back to the screen it names.
//!
//! The model ([`atlas_workspace`]) knows a pane only as a
//! [`PaneDefinition`] — a `kind` string and an optional JSON `resource`. This
//! module is the one place that maps a [`Route`] onto that pair and back, so
//! that a saved layout, the resolver's "is this already open?" question and
//! the pane's title all agree on what a pane is about:
//!
//! | route | kind | resource |
//! |---|---|---|
//! | `Route::Today`, `Route::Accounts`, … (no object) | the route's slug | none |
//! | `Route::Account(id)` | `"account"` | `{"accountId": n}` |
//! | `Route::SeriesDetail(id)` | `"series-detail"` | `{"seriesId": n}` |
//! | `Route::Person(id)` | `"person"` | `{"personId": n}` |
//! | `Route::Company(id)` | `"company"` | `{"companyId": n}` |
//! | `Route::Rule(id)` | `"rule"` | `{"ruleId": n}` |
//!
//! The kind is [`Route::slug`], which is already the stable name `--screen`
//! and the element ids use; a detail route's slug names the kind of record and
//! the resource says which one. `Welcome` is not a pane (it is the screen shown
//! while there is no household) and never round-trips.

use atlas_core::ids::{AccountId, CompanyId, PersonId, RuleId, SeriesId};
use atlas_core::model::Household;
use atlas_workspace::PaneDefinition;
use gpui_kit::SharedString;
use gpui_kit::assets::IconName;
use serde_json::{Value, json};

use crate::nav::{Destination, Route};

/// The definition the layout model keeps for a pane showing `route`.
pub fn definition_of(route: Route) -> PaneDefinition {
    let definition = PaneDefinition::new(route.slug());
    match route {
        Route::Account(id) => definition.with_resource(json!({ "accountId": id.raw() })),
        Route::SeriesDetail(id) => definition.with_resource(json!({ "seriesId": id.raw() })),
        Route::Person(id) => definition.with_resource(json!({ "personId": id.raw() })),
        Route::Company(id) => definition.with_resource(json!({ "companyId": id.raw() })),
        Route::Rule(id) => definition.with_resource(json!({ "ruleId": id.raw() })),
        _ => definition,
    }
}

/// The route a definition names: `None` for a kind this build does not know,
/// a detail kind without its record id, or `Welcome`, which is not a pane.
pub fn route_of(definition: &PaneDefinition) -> Option<Route> {
    let resource = definition.resource.as_ref();
    let route = match definition.kind.as_str() {
        "account" => Route::Account(AccountId::new(resource_id(resource, "accountId")?)),
        "series-detail" => Route::SeriesDetail(SeriesId::new(resource_id(resource, "seriesId")?)),
        "person" => Route::Person(PersonId::new(resource_id(resource, "personId")?)),
        "company" => Route::Company(CompanyId::new(resource_id(resource, "companyId")?)),
        "rule" => Route::Rule(RuleId::new(resource_id(resource, "ruleId")?)),
        other => Route::from_slug(other)?,
    };
    (route != Route::Welcome).then_some(route)
}

/// One numeric field of a resource object.
fn resource_id(resource: Option<&Value>, key: &str) -> Option<u32> {
    resource?.get(key)?.as_u64()?.try_into().ok()
}

/// The pane's title: the screen's heading, plus the record's name for a detail
/// route (`Account · Checking`).
pub fn title_of(route: Route, household: &Household) -> SharedString {
    let name = match route {
        Route::Account(id) => household.account(id).map(|account| account.name.clone()),
        Route::SeriesDetail(id) => household.series_by_id(id).map(|series| series.name.clone()),
        Route::Person(id) => household.person(id).map(|person| person.name.clone()),
        Route::Company(id) => household.company(id).map(|company| company.name.clone()),
        Route::Rule(id) => household.rule(id).map(|rule| rule.name.clone()),
        _ => None,
    };
    match name {
        Some(name) => SharedString::from(format!("{} · {name}", route.title())),
        None => SharedString::from(route.title()),
    }
}

/// The destination a route belongs to, as the muted context beside the title
/// when the title alone would not say where the screen lives (`Earmarks and
/// money by owner` is an Accounts screen; `Accounts` itself needs no context).
pub fn context_of(route: Route) -> Option<&'static str> {
    let destination = route.destination()?;
    let label = destination.label();
    (route != destination.home() && label != route.title()).then_some(label)
}

/// The icon of the destination the route belongs to.
pub fn icon_of(route: Route) -> IconName {
    route.destination().map(Destination::icon).unwrap_or(IconName::Wallet)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_screen_slug_round_trips_through_a_definition() {
        for slug in Route::slugs() {
            let route = Route::from_slug(slug).unwrap_or_else(|| panic!("{slug}"));
            let definition = definition_of(route);
            assert_eq!(definition.kind, route.slug(), "{slug}");
            assert!(definition.resource.is_none(), "{slug} has no record");
            let expected = (route != Route::Welcome).then_some(route);
            assert_eq!(route_of(&definition), expected, "{slug}");
        }
    }

    #[test]
    fn detail_routes_carry_their_record_and_come_back() {
        let routes = [
            Route::Account(AccountId::new(7)),
            Route::SeriesDetail(SeriesId::new(3)),
            Route::Person(PersonId::new(1)),
            Route::Company(CompanyId::new(2)),
            Route::Rule(RuleId::new(9)),
        ];
        for route in routes {
            let definition = definition_of(route);
            assert!(definition.resource.is_some(), "{route:?} names a record");
            assert_eq!(route_of(&definition), Some(route));
        }
        assert_eq!(definition_of(Route::Account(AccountId::new(7))).resource, Some(json!({ "accountId": 7 })));
        // A detail kind without its record, and a kind this build does not know, are not panes.
        assert_eq!(route_of(&PaneDefinition::new("account")), None);
        assert_eq!(route_of(&PaneDefinition::new("no-such-screen")), None);
    }

    #[test]
    fn titles_name_the_record_and_context_names_the_destination() {
        let household = atlas_core::fixtures::plan_household();
        let account = household.accounts[0].id;
        let title = title_of(Route::Account(account), &household);
        assert!(title.starts_with("Account · "), "{title}");
        assert!(title.ends_with(household.accounts[0].name.as_str()), "{title}");
        assert_eq!(title_of(Route::Today, &household).as_ref(), "Today");
        assert_eq!(context_of(Route::Today), None);
        assert_eq!(context_of(Route::Accounts), None);
        assert_eq!(context_of(Route::Earmarks), Some("Accounts"));
        assert_eq!(icon_of(Route::Earmarks), Destination::Accounts.icon());
    }
}
