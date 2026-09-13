use std::collections::{HashMap, HashSet};

use include_dir::{Dir, include_dir};
use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait, TryGetable};
use sha2::{Digest, Sha256};

use crate::{StoreError, StoreResult, now_millis};

static MIGRATIONS: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/migrations");

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MigrationPlan {
    pub pending: Vec<String>,
}

#[derive(Clone, Copy)]
struct EmbeddedMigration<'a> {
    version: &'a str,
    sql: &'a str,
    checksum: [u8; 32],
}

fn embedded() -> StoreResult<Vec<EmbeddedMigration<'static>>> {
    let mut migrations = Vec::new();
    let mut versions = HashSet::new();
    for file in MIGRATIONS.files().filter(|file| file.path().extension().and_then(|ext| ext.to_str()) == Some("sql")) {
        let version = file
            .path()
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| StoreError::Migration {
                version: "embedded".into(),
                message: "migration filename is not valid UTF-8".into(),
            })?;
        validate_filename(version)?;
        if !versions.insert(version) {
            return Err(StoreError::Migration {
                version: version.into(),
                message: "duplicate embedded migration version".into(),
            });
        }
        let sql = file.contents_utf8().ok_or_else(|| StoreError::Migration {
            version: version.into(),
            message: "migration SQL is not valid UTF-8".into(),
        })?;
        migrations.push(EmbeddedMigration { version, sql, checksum: Sha256::digest(sql.as_bytes()).into() });
    }
    migrations.sort_by_key(|migration| migration.version);
    Ok(migrations)
}

fn validate_filename(version: &str) -> StoreResult<()> {
    let Some((timestamp, name)) = version.split_once('_') else {
        return Err(StoreError::Migration { version: version.into(), message: "expected <UTC timestamp>_<snake_case_name>".into() });
    };
    let valid_name = !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    if timestamp.len() != 14 || !timestamp.bytes().all(|byte| byte.is_ascii_digit()) || !valid_name {
        return Err(StoreError::Migration { version: version.into(), message: "expected <UTC timestamp>_<snake_case_name>".into() });
    }
    Ok(())
}

pub(crate) async fn plan<C: ConnectionTrait>(db: &C) -> StoreResult<MigrationPlan> {
    let migrations = embedded()?;
    let known: HashMap<&str, [u8; 32]> = migrations.iter().map(|migration| (migration.version, migration.checksum)).collect();
    let has_history = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations') AS present",
        ))
        .await
        .map_err(StoreError::db)?
        .and_then(|row| row.try_get::<i64>("", "present").ok())
        .unwrap_or(0)
        != 0;
    if !has_history {
        return Ok(MigrationPlan { pending: migrations.iter().map(|migration| migration.version.to_owned()).collect() });
    }

    let mut applied = HashSet::new();
    for row in db
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT version, checksum FROM schema_migrations ORDER BY version",
        ))
        .await
        .map_err(StoreError::db)?
    {
        let version = row.try_get::<String>("", "version").map_err(StoreError::db)?;
        let checksum = row.try_get::<String>("", "checksum").map_err(StoreError::db)?;
        let expected = known.get(version.as_str()).ok_or_else(|| StoreError::IncompatibleVersion { found: version.clone(), supported: migrations.last().map(|m| m.version).unwrap_or("none").into() })?;
        if checksum != hex::encode(expected) {
            return Err(StoreError::Migration { version, message: "the applied migration checksum differs from this application".into() });
        }
        applied.insert(version);
    }
    Ok(MigrationPlan {
        pending: migrations
            .iter()
            .filter(|migration| !applied.contains(migration.version))
            .map(|migration| migration.version.to_owned())
            .collect(),
    })
}

pub(crate) async fn apply<C: ConnectionTrait + TransactionTrait>(db: &C) -> StoreResult<()> {
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS schema_migrations (\
           version TEXT PRIMARY KEY,\
           applied_at INTEGER NOT NULL,\
           checksum TEXT NOT NULL\
         )",
    )
    .await
    .map_err(StoreError::db)?;
    let pending: HashSet<String> = plan(db).await?.pending.into_iter().collect();
    for migration in embedded()?.into_iter().filter(|migration| pending.contains(migration.version)) {
        let transaction = db.begin().await.map_err(StoreError::db)?;
        transaction.execute_unprepared(migration.sql).await.map_err(|error| StoreError::Migration {
            version: migration.version.into(),
            message: error.to_string(),
        })?;
        transaction
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "INSERT INTO schema_migrations (version, applied_at, checksum) VALUES (?, ?, ?)",
                [migration.version.into(), now_millis().into(), hex::encode(migration.checksum).into()],
            ))
            .await
            .map_err(|error| StoreError::Migration { version: migration.version.into(), message: error.to_string() })?;
        foreign_key_check(&transaction).await?;
        transaction.commit().await.map_err(|error| StoreError::Migration { version: migration.version.into(), message: error.to_string() })?;
    }
    integrity_check(db).await
}

pub(crate) async fn foreign_key_check<C: ConnectionTrait>(db: &C) -> StoreResult<()> {
    let violations = db
        .query_all(Statement::from_string(DbBackend::Sqlite, "PRAGMA foreign_key_check"))
        .await
        .map_err(StoreError::db)?;
    if violations.is_empty() {
        Ok(())
    } else {
        Err(StoreError::Integrity("foreign-key validation failed".into()))
    }
}

pub(crate) async fn integrity_check<C: ConnectionTrait>(db: &C) -> StoreResult<()> {
    foreign_key_check(db).await?;
    let result = db
        .query_one(Statement::from_string(DbBackend::Sqlite, "PRAGMA quick_check"))
        .await
        .map_err(StoreError::db)?
        .and_then(|row| row.try_get_by_index::<String>(0).ok())
        .unwrap_or_default();
    if result == "ok" {
        Ok(())
    } else {
        Err(StoreError::Integrity("SQLite quick check failed".into()))
    }
}

#[cfg(test)]
pub(crate) fn embedded_versions() -> StoreResult<Vec<String>> {
    embedded().map(|migrations| migrations.into_iter().map(|migration| migration.version.into()).collect())
}
