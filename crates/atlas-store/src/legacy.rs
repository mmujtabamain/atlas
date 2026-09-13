use atlas_core::model::{Household, SCHEMA_VERSION};
use sea_orm::{ConnectionTrait, DbBackend, Statement, TryGetable};
use serde::de::DeserializeOwned;

use crate::{HouseholdMeta, StoreError, StoreResult};

pub(crate) async fn is_legacy<C: ConnectionTrait>(db: &C) -> StoreResult<bool> {
    if !table_exists(db, "meta").await? || table_exists(db, "atlas_schema_revisions").await? {
        return Ok(false);
    }
    let columns = db
        .query_all(Statement::from_string(DbBackend::Sqlite, "PRAGMA table_info('people')"))
        .await
        .map_err(StoreError::db)?;
    Ok(columns.iter().any(|row| row.try_get::<String>("", "name").ok().as_deref() == Some("json")))
}

pub(crate) async fn load<C: ConnectionTrait>(db: &C) -> StoreResult<Household> {
    if !is_legacy(db).await? {
        return Err(StoreError::NotAHousehold);
    }
    let version = required_meta(db, "schema_version").await?;
    let parsed_version = version.parse::<u32>().map_err(|_| StoreError::Integrity("legacy schema version is invalid".into()))?;
    if parsed_version > SCHEMA_VERSION {
        return Err(StoreError::SchemaVersion { found: parsed_version, supported: SCHEMA_VERSION });
    }
    let name = required_meta(db, "name").await?;
    let currency_code = required_meta(db, "base_currency").await?;
    let currency = atlas_core::Currency::from_code(&currency_code).ok_or_else(|| StoreError::Integrity("legacy household currency is invalid".into()))?;
    let as_of = required_meta(db, "as_of")
        .await?
        .parse()
        .map_err(|_| StoreError::Integrity("legacy reconciliation date is invalid".into()))?;
    let mut household = Household::empty(&name, currency, as_of);
    let tie_break = required_meta(db, "rule_tie_break").await?;
    household.rule_tie_break = atlas_core::rules::TieBreak::from_slug(&tie_break).ok_or_else(|| StoreError::Integrity("legacy rule tie-break mode is invalid".into()))?;
    household.people = load_required(db, "people").await?;
    household.companies = load_required(db, "companies").await?;
    household.accounts = load_required(db, "accounts").await?;
    household.reservations = load_required(db, "reservations").await?;
    household.series = load_required(db, "series").await?;
    household.assumptions = load_required(db, "assumptions").await?;
    household.scenarios = load_required(db, "scenarios").await?;
    household.tax_packs = load_required(db, "tax_packs").await?;
    household.policies = load_required(db, "policies").await?;
    household.actuals = load_required(db, "actuals").await?;
    household.links = load_required(db, "links").await?;
    household.history = load_required(db, "history").await?;
    household.rules = load_optional(db, "rules").await?;
    household.goals = load_optional(db, "goals").await?;
    household.grants = load_optional(db, "grants").await?;
    household.audit = load_optional(db, "audit").await?;
    Ok(household)
}

pub(crate) async fn peek<C: ConnectionTrait>(db: &C) -> StoreResult<HouseholdMeta> {
    let schema_version = required_meta(db, "schema_version")
        .await?
        .parse()
        .map_err(|_| StoreError::Integrity("legacy schema version is invalid".into()))?;
    Ok(HouseholdMeta {
        name: required_meta(db, "name").await?,
        base_currency: required_meta(db, "base_currency").await?,
        as_of: required_meta(db, "as_of").await?,
        saved_at: required_meta(db, "saved_at").await?,
        schema_version,
    })
}

async fn required_meta<C: ConnectionTrait>(db: &C, key: &str) -> StoreResult<String> {
    db.query_one(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "SELECT value FROM meta WHERE key = ?",
        [key.into()],
    ))
    .await
    .map_err(StoreError::db)?
    .and_then(|row| row.try_get::<String>("", "value").ok())
    .ok_or_else(|| StoreError::Integrity(format!("legacy metadata key {key} is missing")))
}

async fn load_required<T: DeserializeOwned, C: ConnectionTrait>(db: &C, table: &str) -> StoreResult<Vec<T>> {
    if !table_exists(db, table).await? {
        return Err(StoreError::Integrity(format!("legacy table {table} is missing")));
    }
    load_rows(db, table).await
}

async fn load_optional<T: DeserializeOwned, C: ConnectionTrait>(db: &C, table: &str) -> StoreResult<Vec<T>> {
    if table_exists(db, table).await? {
        load_rows(db, table).await
    } else {
        Ok(Vec::new())
    }
}

async fn load_rows<T: DeserializeOwned, C: ConnectionTrait>(db: &C, table: &str) -> StoreResult<Vec<T>> {
    let rows = db
        .query_all(Statement::from_string(DbBackend::Sqlite, format!("SELECT json FROM {table} ORDER BY seq")))
        .await
        .map_err(StoreError::db)?;
    rows.into_iter()
        .map(|row| {
            let json = row.try_get::<String>("", "json").map_err(StoreError::db)?;
            serde_json::from_str(&json).map_err(StoreError::from)
        })
        .collect()
}

pub(crate) async fn table_exists<C: ConnectionTrait>(db: &C, table: &str) -> StoreResult<bool> {
    Ok(db
        .query_one(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?) AS present",
            [table.into()],
        ))
        .await
        .map_err(StoreError::db)?
        .and_then(|row| row.try_get::<i64>("", "present").ok())
        .unwrap_or(0)
        != 0)
}
