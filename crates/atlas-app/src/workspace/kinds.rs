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
//!
//! A pane's definition **follows the pane**: when a pane drills from Accounts
//! into one account, the workspace replaces the pane's definition with the
//! account's, so a saved layout reopens on that account and "open account X"
//! finds the pane that already shows it. The pane's own state that a layout
//! keeps ([`view_state_of`] / [`history_from`]) is its Back history, as
//! definitions again; scroll positions are not kept.
//!
//! A definition that this build cannot show — an unknown kind from a newer
//! version, a record that was deleted or that the viewer may not see
//! ([`availability`]) — is not dropped from the layout: the pane shows a
//! placeholder offering to replace or close it, and the rest of the layout
//! loads as saved.

use atlas_core::authz::Viewer;
use atlas_core::ids::{AccountId, CompanyId, ObjectRef, PersonId, RuleId, SeriesId};
use atlas_core::model::Household;
use atlas_core::Disclosure;
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

/// Whether a screen lives in a floating window of its own rather than in a
/// pane of the window it was asked for from: Settings does, so it never
/// takes the place of the work it is about.
pub fn opens_in_own_window(route: Route) -> bool {
    matches!(route, Route::Settings)
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

/// Whether the record a detail route names exists and may be shown to
/// `viewer`; the reason when not, in the words the placeholder shows. Routes
/// without a record are always available.
pub fn availability(route: Route, household: &Household, viewer: Viewer) -> Result<(), String> {
    let visible = |object: ObjectRef| !matches!(household.disclosure_for(viewer, object), Disclosure::Hidden);
    let (exists, allowed, what) = match route {
        Route::Account(id) => (household.account(id).is_some(), visible(ObjectRef::Account(id)), "account"),
        Route::SeriesDetail(id) => (household.series_by_id(id).is_some(), visible(ObjectRef::Series(id)), "series"),
        Route::Person(id) => (household.person(id).is_some(), visible(ObjectRef::Person(id)), "person"),
        Route::Company(id) => (household.company(id).is_some(), visible(ObjectRef::Company(id)), "company"),
        Route::Rule(id) => (household.rule(id).is_some(), true, "rule"),
        _ => return Ok(()),
    };
    if !exists {
        return Err(format!("This {what} no longer exists in the household."));
    }
    if !allowed {
        return Err(format!("This {what} is not shared with the person looking."));
    }
    Ok(())
}

/// A pane's Back history as the state a layout keeps for it.
pub fn view_state_of(history: &[Route]) -> Value {
    let entries: Vec<Value> = history
        .iter()
        .map(|route| {
            let definition = definition_of(*route);
            match definition.resource {
                Some(resource) => json!({ "kind": definition.kind, "resource": resource }),
                None => json!({ "kind": definition.kind }),
            }
        })
        .collect();
    json!({ "history": entries })
}

/// The Back history a layout kept for a pane; entries this build cannot read
/// are skipped.
pub fn history_from(view_state: &Value) -> Vec<Route> {
    view_state
        .get("history")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let kind = entry.get("kind")?.as_str()?;
                    let mut definition = PaneDefinition::new(kind);
                    if let Some(resource) = entry.get("resource") {
                        definition = definition.with_resource(resource.clone());
                    }
                    route_of(&definition)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A readable name for a pane kind this build does not know (`tax-review` →
/// `Tax review`), for the placeholder's title.
pub fn label_of_unknown_kind(kind: &str) -> String {
    let words: Vec<String> = kind.split(['-', '_']).filter(|word| !word.is_empty()).map(str::to_string).collect();
    match words.split_first() {
        Some((first, rest)) => {
            let mut label = first.chars().take(1).flat_map(char::to_uppercase).collect::<String>() + first.chars().skip(1).collect::<String>().as_str();
            for word in rest {
                label.push(' ');
                label.push_str(word);
            }
            label
        }
        None => "Unknown pane".to_string(),
    }
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
    fn availability_names_missing_and_hidden_records() {
        let household = atlas_core::fixtures::plan_household();
        let viewer_a = Viewer::person(household.people[0].id);
        assert_eq!(availability(Route::Today, &household, viewer_a), Ok(()));
        assert_eq!(availability(Route::Account(household.accounts[0].id), &household, viewer_a), Ok(()));
        let gone = availability(Route::Account(AccountId::new(9_999)), &household, viewer_a).unwrap_err();
        assert!(gone.contains("no longer exists"), "{gone}");
        let missing_rule = availability(Route::Rule(RuleId::new(9_999)), &household, viewer_a).unwrap_err();
        assert!(missing_rule.contains("rule"), "{missing_rule}");
        // A record the other person may not see is unavailable to them.
        let viewer_b = Viewer::person(household.people[1].id);
        let hidden = household.accounts.iter().find(|account| matches!(household.disclosure_for(viewer_b, ObjectRef::Account(account.id)), Disclosure::Hidden));
        if let Some(account) = hidden {
            let reason = availability(Route::Account(account.id), &household, viewer_b).unwrap_err();
            assert!(reason.contains("not shared"), "{reason}");
        }
    }

    #[test]
    fn history_round_trips_through_the_view_state_and_skips_what_it_cannot_read() {
        let history = [Route::Accounts, Route::Account(AccountId::new(3)), Route::Today];
        let state = view_state_of(&history);
        assert_eq!(state["history"][1]["kind"], "account");
        assert_eq!(state["history"][1]["resource"]["accountId"], 3);
        assert_eq!(history_from(&state), history);
        let mixed = json!({ "history": [{ "kind": "today" }, { "kind": "never-heard-of" }, { "kind": "account" }] });
        assert_eq!(history_from(&mixed), vec![Route::Today], "unknown kinds and a record-less detail are skipped");
        assert!(history_from(&json!({})).is_empty());
        assert_eq!(label_of_unknown_kind("tax-review"), "Tax review");
        assert_eq!(label_of_unknown_kind(""), "Unknown pane");
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
