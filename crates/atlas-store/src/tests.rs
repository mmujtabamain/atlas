use atlas_core::fixtures;
use atlas_core::forecast::{Case, ForecastOptions, forecast};
use atlas_core::liquidity::Boundary;
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use serde::Serialize;

use super::*;

const OWNER: &str = "store-test";

#[test]
fn normalized_round_trip_preserves_household_and_forecast() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let file = HouseholdFile::new(directory.path().join("plan.atlas.sqlite"));
    file.acquire(OWNER, false).expect("lock acquired");
    let household = fixtures::plan_household();
    file.save_owned(&household, OWNER).expect("household saved");
    let loaded = file.load().expect("household loaded");
    assert_eq!(loaded, household);

    let options = ForecastOptions {
        through: fixtures::default_horizon(),
        scenario: None,
        case: Case::Expected,
    };
    let before = forecast(&household, Boundary::Household, options).expect("forecast before save");
    let after = forecast(&loaded, Boundary::Household, options).expect("forecast after load");
    assert_eq!(before.record.input_hash, after.record.input_hash);
    assert_eq!(before.end.money(), after.end.money());
    file.release(OWNER).expect("lock released");
}

#[test]
fn repeated_open_is_idempotent_and_checksum_drift_is_rejected() {
    assert_eq!(
        migrations::embedded_versions().expect("embedded migrations are valid"),
        ["20260913180000_baseline"]
    );
    let directory = tempfile::tempdir().expect("temporary directory");
    let file = HouseholdFile::new(directory.path().join("plan.atlas.sqlite"));
    file.acquire(OWNER, false).expect("lock acquired");
    file.save_owned(&fixtures::plan_household(), OWNER)
        .expect("household saved");
    file.load().expect("first open");
    file.load().expect("second open");

    runtime().block_on(async {
        let database = connection::connect(file.path(), false)
            .await
            .expect("database opened");
        database
            .execute_unprepared("UPDATE schema_migrations SET checksum = 'rewritten'")
            .await
            .expect("checksum changed for test");
        database.close().await.expect("database closed");
    });
    assert!(matches!(file.load(), Err(StoreError::Migration { .. })));
}

#[test]
fn legacy_json_layout_upgrades_through_a_verified_replacement() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let file = HouseholdFile::new(directory.path().join("legacy.atlas.sqlite"));
    let household = fixtures::plan_household();
    write_legacy_fixture(&file, &household);
    file.acquire(OWNER, false).expect("lock acquired");

    let loaded = file.load().expect("legacy household upgraded");
    assert_eq!(loaded, household);
    assert!(
        directory
            .path()
            .join("backups")
            .read_dir()
            .expect("backup directory")
            .next()
            .is_some()
    );
    let peek = file.peek().expect("normalized metadata");
    assert_eq!(peek.schema_version, atlas_core::model::SCHEMA_VERSION);
}

#[test]
fn lock_creation_is_atomic_and_release_checks_process_ownership() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let file = HouseholdFile::new(directory.path().join("plan.atlas.sqlite"));
    let first = file.acquire("first", false).expect("first lock");
    let competing = file.acquire("second", false);
    assert!(matches!(competing, Err(StoreError::Locked { .. })));
    file.release("second")
        .expect("wrong owner release is ignored");
    assert_eq!(file.lock().expect("lock readable"), Some(first));
    file.release("first").expect("owner releases lock");
    assert_eq!(file.lock().expect("lock readable"), None);
}

fn write_legacy_fixture(file: &HouseholdFile, household: &Household) {
    runtime().block_on(async {
        let database = connection::connect(file.path(), true).await.expect("legacy database opened");
        database
            .execute_unprepared(
                "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);\
                 CREATE TABLE people (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);\
                 CREATE TABLE companies (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);\
                 CREATE TABLE accounts (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);\
                 CREATE TABLE reservations (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);\
                 CREATE TABLE series (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);\
                 CREATE TABLE assumptions (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);\
                 CREATE TABLE scenarios (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);\
                 CREATE TABLE tax_packs (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);\
                 CREATE TABLE policies (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);\
                 CREATE TABLE actuals (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);\
                 CREATE TABLE links (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);\
                 CREATE TABLE history (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);\
                 CREATE TABLE rules (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);\
                 CREATE TABLE goals (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);\
                 CREATE TABLE grants (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);\
                 CREATE TABLE audit (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);",
            )
            .await
            .expect("legacy schema created");
        for (key, value) in [
            ("schema_version", atlas_core::model::SCHEMA_VERSION.to_string()),
            ("name", household.name.clone()),
            ("base_currency", household.base_currency.code().to_owned()),
            ("as_of", household.as_of.to_string()),
            ("rule_tie_break", household.rule_tie_break.slug().to_owned()),
            ("saved_at", "2026-01-01T00:00:00Z".to_owned()),
        ] {
            database
                .execute(Statement::from_sql_and_values(
                    DbBackend::Sqlite,
                    "INSERT INTO meta (key, value) VALUES (?, ?)",
                    [key.into(), value.into()],
                ))
                .await
                .expect("legacy metadata inserted");
        }
        insert_legacy(&database, "people", &household.people).await;
        insert_legacy(&database, "companies", &household.companies).await;
        insert_legacy(&database, "accounts", &household.accounts).await;
        insert_legacy(&database, "reservations", &household.reservations).await;
        insert_legacy(&database, "series", &household.series).await;
        insert_legacy(&database, "assumptions", &household.assumptions).await;
        insert_legacy(&database, "scenarios", &household.scenarios).await;
        insert_legacy(&database, "tax_packs", &household.tax_packs).await;
        insert_legacy(&database, "policies", &household.policies).await;
        insert_legacy(&database, "actuals", &household.actuals).await;
        insert_legacy(&database, "links", &household.links).await;
        insert_legacy(&database, "history", &household.history).await;
        insert_legacy(&database, "rules", &household.rules).await;
        insert_legacy(&database, "goals", &household.goals).await;
        insert_legacy(&database, "grants", &household.grants).await;
        insert_legacy(&database, "audit", &household.audit).await;
        database.close().await.expect("legacy database closed");
    });
}

async fn insert_legacy<T: Serialize>(
    database: &sea_orm::DatabaseConnection,
    table: &str,
    rows: &[T],
) {
    for (index, row) in rows.iter().enumerate() {
        database
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                format!("INSERT INTO {table} (id, json) VALUES (?, ?)"),
                [
                    i64::try_from(index + 1).expect("fixture index fits").into(),
                    serde_json::to_string(row)
                        .expect("fixture serializes")
                        .into(),
                ],
            ))
            .await
            .expect("legacy row inserted");
    }
}
