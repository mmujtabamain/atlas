use atlas_core::authz::{CalculationAccess, Grantee};
use atlas_core::ids::ObjectRef;
use atlas_core::model::{
    AccountKind, CompanyConstraint, EntityRole, Holder, Household, HouseholdRole, SourceOfTruth,
};
use atlas_core::provenance::Disclosure;
use atlas_core::timeline::Direction;
use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait, Value};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{StoreError, StoreResult, now_millis};

const ROOT_TABLES: [&str; 16] = [
    "audit_events",
    "access_grants",
    "goals",
    "rules",
    "historical_payments",
    "reconciliation_links",
    "actual_transactions",
    "access_policies",
    "tax_packs",
    "scenarios",
    "assumptions",
    "event_series",
    "reservations",
    "accounts",
    "companies",
    "people",
];

pub(crate) async fn save<C: ConnectionTrait + TransactionTrait>(
    db: &C,
    household: &Household,
) -> StoreResult<()> {
    let transaction = db.begin().await.map_err(StoreError::db)?;
    for table in ROOT_TABLES {
        transaction
            .execute_unprepared(&format!("DELETE FROM {table}"))
            .await
            .map_err(StoreError::db)?;
    }
    let household_id = transaction
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT household_id FROM households WHERE singleton = 1",
        ))
        .await
        .map_err(StoreError::db)?
        .and_then(|row| row.try_get::<String>("", "household_id").ok())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    execute(
        &transaction,
        "INSERT INTO households (singleton, household_id, name, base_currency, as_of, rule_tie_break, last_saved_at, last_written_by_version, format_lineage) \
         VALUES (1, ?, ?, ?, ?, ?, ?, ?, 'normalized-v1') \
         ON CONFLICT(singleton) DO UPDATE SET name=excluded.name, base_currency=excluded.base_currency, as_of=excluded.as_of, \
         rule_tie_break=excluded.rule_tie_break, last_saved_at=excluded.last_saved_at, last_written_by_version=excluded.last_written_by_version, format_lineage=excluded.format_lineage",
        vec![
            household_id.into(),
            household.name.clone().into(),
            household.base_currency.code().into(),
            household.as_of.to_string().into(),
            household.rule_tie_break.slug().into(),
            now_millis().into(),
            env!("CARGO_PKG_VERSION").into(),
        ],
    )
    .await?;

    for (position, person) in household.people.iter().enumerate() {
        execute(
            &transaction,
            "INSERT INTO people (id, position, name, role, payload_json) VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET position=excluded.position, name=excluded.name, role=excluded.role, payload_json=excluded.payload_json",
            vec![
                i64::from(person.id.raw()).into(),
                position_value(position)?,
                person.name.clone().into(),
                household_role(person.role).into(),
                json(person)?.into(),
            ],
        )
        .await?;
    }

    for (position, company) in household.companies.iter().enumerate() {
        execute(
            &transaction,
            "INSERT INTO companies (id, position, name, jurisdiction, payload_json) VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET position=excluded.position, name=excluded.name, jurisdiction=excluded.jurisdiction, payload_json=excluded.payload_json",
            vec![
                i64::from(company.id.raw()).into(),
                position_value(position)?,
                company.name.clone().into(),
                company.jurisdiction.clone().into(),
                json(company)?.into(),
            ],
        )
        .await?;
    }
    for company in &household.companies {
        let company_id = i64::from(company.id.raw());
        for (position, owner) in company.owners.iter().enumerate() {
            execute(
                &transaction,
                "INSERT INTO company_owners (company_id, position, person_id, basis_points) VALUES (?, ?, ?, ?)",
                vec![company_id.into(), position_value(position)?, i64::from(owner.person.raw()).into(), i64::from(owner.basis_points).into()],
            )
            .await?;
        }
        for (position, role) in company.roles.iter().enumerate() {
            execute(
                &transaction,
                "INSERT INTO company_roles (company_id, position, person_id, role) VALUES (?, ?, ?, ?)",
                vec![company_id.into(), position_value(position)?, i64::from(role.person.raw()).into(), entity_role(role.role).into()],
            )
            .await?;
        }
        for (position, employee) in company.employees.iter().enumerate() {
            execute(
                &transaction,
                "INSERT INTO company_employees (company_id, position, person_id, name, monthly_gross_minor, currency, starts_on, ends_on) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                vec![
                    company_id.into(),
                    position_value(position)?,
                    employee.person.map(|id| i64::from(id.raw())).into(),
                    employee.name.clone().into(),
                    employee.monthly_gross.minor().into(),
                    employee.monthly_gross.currency().code().into(),
                    employee.start.to_string().into(),
                    employee.end.map(|date| date.to_string()).into(),
                ],
            )
            .await?;
        }
        for (position, constraint) in company.constraints.iter().enumerate() {
            execute(
                &transaction,
                "INSERT INTO company_constraints (company_id, position, kind, value_json) VALUES (?, ?, ?, ?)",
                vec![company_id.into(), position_value(position)?, company_constraint(constraint).into(), json(constraint)?.into()],
            )
            .await?;
        }
    }

    for (position, account) in household.accounts.iter().enumerate() {
        let (holder_kind, company_id) = match &account.holder {
            Holder::Persons(_) => ("persons", None),
            Holder::Company(id) => ("company", Some(i64::from(id.raw()))),
        };
        execute(
            &transaction,
            "INSERT INTO accounts (id, position, name, institution, kind, holder_kind, holder_company_id, currency, liquidity_json, \
             minimum_balance_minor, minimum_balance_currency, transfer_delay_days, tax_treatment, source_of_truth, last_reconciled, \
             withdrawals_permitted, include_in_household, settled_balance_minor, pending_balance_minor, payload_json) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET position=excluded.position, name=excluded.name, institution=excluded.institution, kind=excluded.kind, \
             holder_kind=excluded.holder_kind, holder_company_id=excluded.holder_company_id, currency=excluded.currency, liquidity_json=excluded.liquidity_json, \
             minimum_balance_minor=excluded.minimum_balance_minor, minimum_balance_currency=excluded.minimum_balance_currency, \
             transfer_delay_days=excluded.transfer_delay_days, tax_treatment=excluded.tax_treatment, source_of_truth=excluded.source_of_truth, \
             last_reconciled=excluded.last_reconciled, withdrawals_permitted=excluded.withdrawals_permitted, \
             include_in_household=excluded.include_in_household, settled_balance_minor=excluded.settled_balance_minor, \
             pending_balance_minor=excluded.pending_balance_minor, payload_json=excluded.payload_json",
            vec![
                i64::from(account.id.raw()).into(),
                position_value(position)?,
                account.name.clone().into(),
                account.institution.clone().into(),
                account_kind(account.kind).into(),
                holder_kind.into(),
                company_id.into(),
                account.currency.code().into(),
                json(&account.liquidity)?.into(),
                account.minimum_balance.map(|money| money.minor()).into(),
                account.minimum_balance.map(|money| money.currency().code().to_owned()).into(),
                i64::from(account.transfer_delay_days).into(),
                account.tax_treatment.clone().into(),
                source_of_truth(account.source_of_truth).into(),
                account.last_reconciled.map(|date| date.to_string()).into(),
                account.withdrawals_permitted.into(),
                account.include_in_household.into(),
                account.settled_balance.minor().into(),
                account.pending_balance.minor().into(),
                json(account)?.into(),
            ],
        )
        .await?;
        if let Holder::Persons(owners) = &account.holder {
            for (owner_position, owner) in owners.iter().enumerate() {
                execute(
                    &transaction,
                    "INSERT INTO account_owners (account_id, position, person_id, basis_points) VALUES (?, ?, ?, ?)",
                    vec![
                        i64::from(account.id.raw()).into(),
                        position_value(owner_position)?,
                        i64::from(owner.person.raw()).into(),
                        i64::from(owner.basis_points).into(),
                    ],
                )
                .await?;
            }
        }
        for (fee_position, fee) in account.fees.iter().enumerate() {
            execute(
                &transaction,
                "INSERT INTO account_fees (account_id, position, description, fixed_minor, fixed_currency, basis_points) VALUES (?, ?, ?, ?, ?, ?)",
                vec![
                    i64::from(account.id.raw()).into(),
                    position_value(fee_position)?,
                    fee.description.clone().into(),
                    fee.fixed.map(|money| money.minor()).into(),
                    fee.fixed.map(|money| money.currency().code().to_owned()).into(),
                    i64::from(fee.basis_points).into(),
                ],
            )
            .await?;
        }
        for (category_position, category) in account.funds_categories.iter().enumerate() {
            execute(
                &transaction,
                "INSERT INTO account_fund_categories (account_id, position, category) VALUES (?, ?, ?)",
                vec![i64::from(account.id.raw()).into(), position_value(category_position)?, category.clone().into()],
            )
            .await?;
        }
    }

    for (position, reservation) in household.reservations.iter().enumerate() {
        execute(
            &transaction,
            "INSERT INTO reservations (id, position, name, account_id, amount_minor, currency, coverage_json, nested_in_id, hardness, purpose, released_on, payload_json) \
             VALUES (?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET position=excluded.position, name=excluded.name, account_id=excluded.account_id, amount_minor=excluded.amount_minor, \
             currency=excluded.currency, coverage_json=excluded.coverage_json, nested_in_id=NULL, hardness=excluded.hardness, purpose=excluded.purpose, \
             released_on=excluded.released_on, payload_json=excluded.payload_json",
            vec![
                i64::from(reservation.id.raw()).into(),
                position_value(position)?,
                reservation.name.clone().into(),
                i64::from(reservation.account.raw()).into(),
                reservation.amount.minor().into(),
                reservation.amount.currency().code().into(),
                json(&reservation.coverage)?.into(),
                snake_tag(&reservation.hardness)?.into(),
                reservation.purpose.clone().into(),
                reservation.released_on.map(|date| date.to_string()).into(),
                json(reservation)?.into(),
            ],
        )
        .await?;
    }
    for reservation in &household.reservations {
        if let atlas_core::model::Coverage::NestedIn(parent) = reservation.coverage {
            execute(
                &transaction,
                "UPDATE reservations SET nested_in_id = ? WHERE id = ?",
                vec![
                    i64::from(parent.raw()).into(),
                    i64::from(reservation.id.raw()).into(),
                ],
            )
            .await?;
        }
    }

    for (position, series) in household.series.iter().enumerate() {
        execute(
            &transaction,
            "INSERT INTO event_series (id, position, name, direction_json, expected_minor, currency, recurrence_json, settlement_lag_days, \
             availability_lag_days, intraday_order, account_id, linked_account_id, entity_json, certainty, category, tax_treatment, scenario_id, notes, payload_json) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET position=excluded.position, name=excluded.name, direction_json=excluded.direction_json, \
             expected_minor=excluded.expected_minor, currency=excluded.currency, recurrence_json=excluded.recurrence_json, \
             settlement_lag_days=excluded.settlement_lag_days, availability_lag_days=excluded.availability_lag_days, intraday_order=excluded.intraday_order, \
             account_id=excluded.account_id, linked_account_id=excluded.linked_account_id, entity_json=excluded.entity_json, certainty=excluded.certainty, \
             category=excluded.category, tax_treatment=excluded.tax_treatment, scenario_id=excluded.scenario_id, notes=excluded.notes, payload_json=excluded.payload_json",
            vec![
                i64::from(series.id.raw()).into(),
                position_value(position)?,
                series.name.clone().into(),
                json(&series.direction)?.into(),
                series.amount.expected().minor().into(),
                series.amount.currency().code().into(),
                json(&series.recurrence)?.into(),
                i64::from(series.settlement_lag_days).into(),
                i64::from(series.availability_lag_days).into(),
                i64::from(series.intraday_order).into(),
                i64::from(series.account.raw()).into(),
                series.linked_account.map(|id| i64::from(id.raw())).into(),
                json(&series.entity)?.into(),
                snake_tag(&series.certainty)?.into(),
                series.category.clone().into(),
                series.tax_treatment.clone().into(),
                series.scenario.map(|id| i64::from(id.raw())).into(),
                series.notes.clone().into(),
                json(series)?.into(),
            ],
        )
        .await?;
        for (change_position, change) in series.amount_changes.iter().enumerate() {
            execute(
                &transaction,
                "INSERT INTO series_amount_changes (series_id, position, effective_from, amount_json) VALUES (?, ?, ?, ?)",
                vec![
                    i64::from(series.id.raw()).into(),
                    position_value(change_position)?,
                    change.effective_from.to_string().into(),
                    json(&change.amount)?.into(),
                ],
            )
            .await?;
        }
        for (exception_position, exception) in series.exceptions.iter().enumerate() {
            execute(
                &transaction,
                "INSERT INTO series_exceptions (series_id, position, original_due, kind_json) VALUES (?, ?, ?, ?)",
                vec![
                    i64::from(series.id.raw()).into(),
                    position_value(exception_position)?,
                    exception.original_due.to_string().into(),
                    json(&exception.kind)?.into(),
                ],
            )
            .await?;
        }
    }

    for (position, assumption) in household.assumptions.iter().enumerate() {
        execute(
            &transaction,
            "INSERT INTO assumptions (id, position, text, certainty, source_json, accepted_on, expires_on, private_to, payload_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET position=excluded.position, text=excluded.text, certainty=excluded.certainty, source_json=excluded.source_json, \
             accepted_on=excluded.accepted_on, expires_on=excluded.expires_on, private_to=excluded.private_to, payload_json=excluded.payload_json",
            vec![
                i64::from(assumption.id.raw()).into(),
                position_value(position)?,
                assumption.text.clone().into(),
                snake_tag(&assumption.certainty)?.into(),
                json(&assumption.source)?.into(),
                assumption.accepted_on.map(|date| date.to_string()).into(),
                assumption.expires_on.map(|date| date.to_string()).into(),
                assumption.private_to.map(|id| i64::from(id.raw())).into(),
                json(assumption)?.into(),
            ],
        )
        .await?;
        for (series_position, series_id) in assumption.applies_to.iter().enumerate() {
            execute(
                &transaction,
                "INSERT INTO assumption_series (assumption_id, position, series_id) VALUES (?, ?, ?)",
                vec![i64::from(assumption.id.raw()).into(), position_value(series_position)?, i64::from(series_id.raw()).into()],
            )
            .await?;
        }
    }

    for (position, scenario) in household.scenarios.iter().enumerate() {
        execute(
            &transaction,
            "INSERT INTO scenarios (id, position, name, description, private_to, payload_json) VALUES (?, ?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET position=excluded.position, name=excluded.name, description=excluded.description, \
             private_to=excluded.private_to, payload_json=excluded.payload_json",
            vec![
                i64::from(scenario.id.raw()).into(),
                position_value(position)?,
                scenario.name.clone().into(),
                scenario.description.clone().into(),
                scenario.private_to.map(|id| i64::from(id.raw())).into(),
                json(scenario)?.into(),
            ],
        )
        .await?;
    }
    for scenario in &household.scenarios {
        for (component_position, component) in scenario.composed_of.iter().enumerate() {
            execute(
                &transaction,
                "INSERT INTO scenario_components (scenario_id, position, component_id) VALUES (?, ?, ?)",
                vec![i64::from(scenario.id.raw()).into(), position_value(component_position)?, i64::from(component.raw()).into()],
            )
            .await?;
        }
        for (change_position, change) in scenario.changes.iter().enumerate() {
            execute(
                &transaction,
                "INSERT INTO scenario_changes (scenario_id, position, kind, value_json) VALUES (?, ?, ?, ?)",
                vec![i64::from(scenario.id.raw()).into(), position_value(change_position)?, change.kind().into(), json(change)?.into()],
            )
            .await?;
        }
    }

    for (position, pack) in household.tax_packs.iter().enumerate() {
        let pack_id = position_value(position + 1)?;
        execute(
            &transaction,
            "INSERT INTO tax_packs (id, position, name, version, jurisdiction, verified, payload_json) VALUES (?, ?, ?, ?, ?, ?, ?)",
            vec![pack_id.clone(), position_value(position)?, pack.name.clone().into(), pack.version.clone().into(), pack.jurisdiction.clone().into(), pack.verified.into(), json(pack)?.into()],
        )
        .await?;
        for (rule_position, rule) in pack.rules.iter().enumerate() {
            execute(
                &transaction,
                "INSERT INTO tax_rules (id, pack_id, position, name, tax_type, scope, kind_json, timing_json, effective_from, effective_to, source, explanation) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                vec![
                    i64::from(rule.id.raw()).into(),
                    pack_id.clone(),
                    position_value(rule_position)?,
                    rule.name.clone().into(),
                    rule.tax_type.clone().into(),
                    rule.scope.clone().into(),
                    json(&rule.kind)?.into(),
                    json(&rule.timing)?.into(),
                    rule.effective_from.to_string().into(),
                    rule.effective_to.map(|date| date.to_string()).into(),
                    rule.source.clone().into(),
                    rule.explanation.clone().into(),
                ],
            )
            .await?;
            for (category_position, category) in rule.categories.iter().enumerate() {
                execute(
                    &transaction,
                    "INSERT INTO tax_rule_categories (tax_rule_id, position, category) VALUES (?, ?, ?)",
                    vec![i64::from(rule.id.raw()).into(), position_value(category_position)?, category.clone().into()],
                )
                .await?;
            }
        }
    }

    for (position, policy) in household.policies.iter().enumerate() {
        execute(
            &transaction,
            "INSERT INTO access_policies (id, position, object_json, calculation_access, restricted_disclosure, effective_from, version, changed_by, changed_at, payload_json) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET position=excluded.position, object_json=excluded.object_json, calculation_access=excluded.calculation_access, \
             restricted_disclosure=excluded.restricted_disclosure, effective_from=excluded.effective_from, version=excluded.version, \
             changed_by=excluded.changed_by, changed_at=excluded.changed_at, payload_json=excluded.payload_json",
            vec![
                i64::from(policy.id.raw()).into(),
                position_value(position)?,
                json(&policy.object)?.into(),
                calculation_access(policy.calculation_access).into(),
                disclosure(policy.restricted_disclosure).into(),
                policy.effective_from.to_string().into(),
                i64::from(policy.version).into(),
                i64::from(policy.changed_by.raw()).into(),
                policy.changed_at.to_string().into(),
                json(policy)?.into(),
            ],
        )
        .await?;
        for (person_position, person) in policy.full_access.iter().enumerate() {
            execute(
                &transaction,
                "INSERT INTO policy_full_access (policy_id, position, person_id) VALUES (?, ?, ?)",
                vec![
                    i64::from(policy.id.raw()).into(),
                    position_value(person_position)?,
                    i64::from(person.raw()).into(),
                ],
            )
            .await?;
        }
        for (purpose_position, purpose) in policy.purposes.iter().enumerate() {
            execute(
                &transaction,
                "INSERT INTO policy_purposes (policy_id, position, purpose) VALUES (?, ?, ?)",
                vec![
                    i64::from(policy.id.raw()).into(),
                    position_value(purpose_position)?,
                    purpose.clone().into(),
                ],
            )
            .await?;
        }
    }

    for (position, actual) in household.actuals.iter().enumerate() {
        execute(
            &transaction,
            "INSERT INTO actual_transactions (id, position, transaction_date, account_id, amount_minor, currency, description, payload_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            vec![
                i64::from(actual.id.raw()).into(),
                position_value(position)?,
                actual.date.to_string().into(),
                i64::from(actual.account.raw()).into(),
                actual.amount.minor().into(),
                actual.amount.currency().code().into(),
                actual.description.clone().into(),
                json(actual)?.into(),
            ],
        )
        .await?;
    }

    for (position, link) in household.links.iter().enumerate() {
        execute(
            &transaction,
            "INSERT INTO reconciliation_links (position, series_id, original_due, transaction_id, amount_minor, currency, payload_json) VALUES (?, ?, ?, ?, ?, ?, ?)",
            vec![
                position_value(position)?,
                i64::from(link.series.raw()).into(),
                link.original_due.to_string().into(),
                i64::from(link.transaction.raw()).into(),
                link.amount.minor().into(),
                link.amount.currency().code().into(),
                json(link)?.into(),
            ],
        )
        .await?;
    }

    for (position, payment) in household.history.iter().enumerate() {
        execute(
            &transaction,
            "INSERT INTO historical_payments (position, series_id, payment_date, amount_minor, currency, payload_json) VALUES (?, ?, ?, ?, ?, ?)",
            vec![
                position_value(position)?,
                i64::from(payment.series.raw()).into(),
                payment.date.to_string().into(),
                payment.amount.minor().into(),
                payment.amount.currency().code().into(),
                json(payment)?.into(),
            ],
        )
        .await?;
    }

    for (position, rule) in household.rules.iter().enumerate() {
        execute(
            &transaction,
            "INSERT INTO rules (id, position, name, scope_json, trigger_json, action_json, priority, effective_from, effective_to, enabled, scenario_id, explanation, version, payload_json) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            vec![
                i64::from(rule.id.raw()).into(),
                position_value(position)?,
                rule.name.clone().into(),
                json(&rule.scope)?.into(),
                json(&rule.trigger)?.into(),
                json(&rule.action)?.into(),
                i64::from(rule.priority).into(),
                rule.effective_from.to_string().into(),
                rule.effective_to.map(|date| date.to_string()).into(),
                rule.enabled.into(),
                rule.scenario.map(|id| i64::from(id.raw())).into(),
                rule.explanation.clone().into(),
                i64::from(rule.version).into(),
                json(rule)?.into(),
            ],
        )
        .await?;
        for (condition_position, condition) in rule.conditions.iter().enumerate() {
            execute(
                &transaction,
                "INSERT INTO rule_conditions (rule_id, position, value_json) VALUES (?, ?, ?)",
                vec![
                    i64::from(rule.id.raw()).into(),
                    position_value(condition_position)?,
                    json(condition)?.into(),
                ],
            )
            .await?;
        }
        for (version_position, version) in rule.history.iter().enumerate() {
            execute(
                &transaction,
                "INSERT INTO rule_versions (rule_id, position, version, changed_on, summary) VALUES (?, ?, ?, ?, ?)",
                vec![
                    i64::from(rule.id.raw()).into(),
                    position_value(version_position)?,
                    i64::from(version.version).into(),
                    version.changed_on.to_string().into(),
                    version.summary.clone().into(),
                ],
            )
            .await?;
        }
    }

    for (position, goal) in household.goals.iter().enumerate() {
        execute(
            &transaction,
            "INSERT INTO goals (id, position, name, amount_minor, currency, target_on, priority, private_to, payload_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            vec![
                i64::from(goal.id.raw()).into(),
                position_value(position)?,
                goal.name.clone().into(),
                goal.amount.minor().into(),
                goal.amount.currency().code().into(),
                goal.target_on.to_string().into(),
                i64::from(goal.priority).into(),
                goal.private_to.map(|id| i64::from(id.raw())).into(),
                json(goal)?.into(),
            ],
        )
        .await?;
    }

    for (position, grant) in household.grants.iter().enumerate() {
        execute(
            &transaction,
            "INSERT INTO access_grants (id, position, object_json, grantee_json, purpose_json, disclosure, calculation_access, effective_from, effective_to, granted_by, granted_at, revoked_on, note, payload_json) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            vec![
                i64::from(grant.id.raw()).into(),
                position_value(position)?,
                json(&grant.object)?.into(),
                json(&grant.grantee)?.into(),
                json(&grant.purpose)?.into(),
                disclosure(grant.disclosure).into(),
                calculation_access(grant.calculation).into(),
                grant.effective_from.to_string().into(),
                grant.effective_to.map(|date| date.to_string()).into(),
                i64::from(grant.granted_by.raw()).into(),
                grant.granted_at.to_string().into(),
                grant.revoked_on.map(|date| date.to_string()).into(),
                grant.note.clone().into(),
                json(grant)?.into(),
            ],
        )
        .await?;
    }

    for (position, event) in household.audit.iter().enumerate() {
        execute(
            &transaction,
            "INSERT INTO audit_events (id, position, occurred_at, actor_id, object_json, kind_json, summary, policy_version, payload_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            vec![
                i64::from(event.id.raw()).into(),
                position_value(position)?,
                event.at.to_string().into(),
                i64::from(event.actor.raw()).into(),
                event.object.map(|object| json(&object)).transpose()?.into(),
                json(&event.kind)?.into(),
                event.summary.clone().into(),
                event.policy_version.map(i64::from).into(),
                json(event)?.into(),
            ],
        )
        .await?;
    }

    transaction.commit().await.map_err(StoreError::db)
}

pub(crate) async fn load<C: ConnectionTrait>(db: &C) -> StoreResult<Household> {
    let metadata = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT name, base_currency, as_of, rule_tie_break FROM households WHERE singleton = 1",
        ))
        .await
        .map_err(StoreError::db)?
        .ok_or_else(|| StoreError::Integrity("household metadata is missing".into()))?;
    let name = metadata
        .try_get::<String>("", "name")
        .map_err(StoreError::db)?;
    let currency_code = metadata
        .try_get::<String>("", "base_currency")
        .map_err(StoreError::db)?;
    let currency = atlas_core::Currency::from_code(&currency_code)
        .ok_or_else(|| StoreError::Integrity("household currency is invalid".into()))?;
    let as_of = metadata
        .try_get::<String>("", "as_of")
        .map_err(StoreError::db)?
        .parse()
        .map_err(|_| StoreError::Integrity("household reconciliation date is invalid".into()))?;
    let tie_break = metadata
        .try_get::<String>("", "rule_tie_break")
        .map_err(StoreError::db)?;
    let mut household = Household::empty(&name, currency, as_of);
    household.rule_tie_break = atlas_core::rules::TieBreak::from_slug(&tie_break)
        .ok_or_else(|| StoreError::Integrity("rule tie-break mode is invalid".into()))?;
    household.people = load_payloads(db, "people").await?;
    household.companies = load_payloads(db, "companies").await?;
    household.accounts = load_payloads(db, "accounts").await?;
    household.reservations = load_payloads(db, "reservations").await?;
    household.series = load_payloads(db, "event_series").await?;
    household.assumptions = load_payloads(db, "assumptions").await?;
    household.scenarios = load_payloads(db, "scenarios").await?;
    household.tax_packs = load_payloads(db, "tax_packs").await?;
    household.policies = load_payloads(db, "access_policies").await?;
    household.actuals = load_payloads(db, "actual_transactions").await?;
    household.links = load_payloads(db, "reconciliation_links").await?;
    household.history = load_payloads(db, "historical_payments").await?;
    household.rules = load_payloads(db, "rules").await?;
    household.goals = load_payloads(db, "goals").await?;
    household.grants = load_payloads(db, "access_grants").await?;
    household.audit = load_payloads(db, "audit_events").await?;
    Ok(household)
}

async fn load_payloads<T: DeserializeOwned, C: ConnectionTrait>(
    db: &C,
    table: &str,
) -> StoreResult<Vec<T>> {
    let rows = db
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            format!("SELECT payload_json FROM {table} ORDER BY position"),
        ))
        .await
        .map_err(StoreError::db)?;
    rows.into_iter()
        .map(|row| {
            let payload = row
                .try_get::<String>("", "payload_json")
                .map_err(StoreError::db)?;
            serde_json::from_str(&payload).map_err(StoreError::from)
        })
        .collect()
}

async fn execute<C: ConnectionTrait>(db: &C, sql: &str, values: Vec<Value>) -> StoreResult<()> {
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        sql,
        values,
    ))
    .await
    .map_err(StoreError::db)?;
    Ok(())
}

fn json<T: Serialize>(value: &T) -> StoreResult<String> {
    serde_json::to_string(value).map_err(StoreError::from)
}

fn position_value(position: usize) -> StoreResult<Value> {
    i64::try_from(position)
        .map(Into::into)
        .map_err(|_| StoreError::Validation("collection is too large to persist".into()))
}

fn snake_tag<T: Serialize>(value: &T) -> StoreResult<String> {
    let value = serde_json::to_value(value)?;
    let tag = match value {
        serde_json::Value::String(tag) => tag,
        serde_json::Value::Object(map) if map.len() == 1 => map
            .into_iter()
            .next()
            .map(|(key, _)| key)
            .unwrap_or_default(),
        _ => {
            return Err(StoreError::Serialization(
                "enum did not serialize to a stable tag".into(),
            ));
        }
    };
    let mut output = String::with_capacity(tag.len() + 4);
    for (index, character) in tag.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            output.push('_');
        }
        output.push(character.to_ascii_lowercase());
    }
    Ok(output)
}

fn household_role(role: HouseholdRole) -> &'static str {
    match role {
        HouseholdRole::Owner => "owner",
        HouseholdRole::Member => "member",
        HouseholdRole::Dependent => "dependent",
        HouseholdRole::Adviser => "adviser",
        HouseholdRole::ReadOnly => "read_only",
    }
}

fn entity_role(role: EntityRole) -> &'static str {
    match role {
        EntityRole::OwnerDirector => "owner_director",
        EntityRole::FinanceAdministrator => "finance_administrator",
        EntityRole::PayrollOperator => "payroll_operator",
        EntityRole::Employee => "employee",
        EntityRole::ReadOnlyAdviser => "read_only_adviser",
    }
}

fn company_constraint(constraint: &CompanyConstraint) -> &'static str {
    match constraint {
        CompanyConstraint::MinimumWorkingCapital(_) => "minimum_working_capital",
        CompanyConstraint::PayrollMonthsReserve(_) => "payroll_months_reserve",
        CompanyConstraint::TaxReserve(_) => "tax_reserve",
        CompanyConstraint::NoDistributionBefore(_) => "no_distribution_before",
        CompanyConstraint::MaximumOwnerSalary(_) => "maximum_owner_salary",
    }
}

fn account_kind(kind: AccountKind) -> &'static str {
    match kind {
        AccountKind::Checking => "checking",
        AccountKind::Savings => "savings",
        AccountKind::CashWallet => "cash_wallet",
        AccountKind::CreditCard => "credit_card",
        AccountKind::Loan => "loan",
        AccountKind::Brokerage => "brokerage",
        AccountKind::FixedDeposit => "fixed_deposit",
        AccountKind::TaxReserve => "tax_reserve",
        AccountKind::CompanyOperating => "company_operating",
        AccountKind::CompanyPayroll => "company_payroll",
        AccountKind::CorporateCard => "corporate_card",
    }
}

fn source_of_truth(source: SourceOfTruth) -> &'static str {
    match source {
        SourceOfTruth::Manual => "manual",
        SourceOfTruth::Imported => "imported",
        SourceOfTruth::Synchronized => "synchronized",
    }
}

fn calculation_access(access: CalculationAccess) -> &'static str {
    match access {
        CalculationAccess::Excluded => "excluded",
        CalculationAccess::RestrictedContribution => "restricted_contribution",
        CalculationAccess::Full => "full",
    }
}

fn disclosure(value: Disclosure) -> &'static str {
    match value {
        Disclosure::Hidden => "hidden",
        Disclosure::Aggregate => "aggregate",
        Disclosure::BalanceOnly => "balance_only",
        Disclosure::SelectedFields => "selected_fields",
        Disclosure::Full => "full",
    }
}

#[allow(dead_code)]
fn _domain_reference_guards(
    direction: Direction,
    object: ObjectRef,
    grantee: Grantee,
) -> (Direction, ObjectRef, Grantee) {
    (direction, object, grantee)
}
