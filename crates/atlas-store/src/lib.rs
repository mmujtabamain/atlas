//! Durable SQLite persistence for Atlas Financer household files.
//!
//! Atlas CLI authors the checked-in schema migrations. The application embeds
//! and applies those migrations through SeaORM/SQLx before repository access.
//! `atlas-core` remains I/O-free and generated database entities never cross
//! this crate boundary.

mod connection;
mod legacy;
mod migrations;
mod repository;

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use atlas_core::model::{Household, SCHEMA_VERSION};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const BACKUPS_KEPT: usize = 20;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("database operation failed for household {path_id}: {message}")]
    Database { path_id: String, message: String },
    #[error("database operation failed: {0}")]
    DatabaseInternal(String),
    #[error("serialization failed: {0}")]
    Serialization(String),
    #[error("file operation failed: {0}")]
    FileSystem(#[from] std::io::Error),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error(
        "household file was written by schema version {found}; this build reads version {supported}"
    )]
    SchemaVersion { found: u32, supported: u32 },
    #[error("household migration {version} failed: {message}")]
    Migration { version: String, message: String },
    #[error("legacy household upgrade failed; the recoverable backup is {backup}: {message}")]
    LegacyMigration { backup: PathBuf, message: String },
    #[error("household upgrade failed; the recoverable backup is {backup}: {message}")]
    Upgrade { backup: PathBuf, message: String },
    #[error(
        "household contains migrations newer than this application ({found}; latest supported: {supported})"
    )]
    IncompatibleVersion { found: String, supported: String },
    #[error("household is open by {owner} since {since}; take it over only if they are done")]
    Locked {
        path: PathBuf,
        owner: String,
        since: String,
    },
    #[error("this process does not own the household lock")]
    LockRequired,
    #[error("the household changed on disk after it was opened")]
    ChangedOnDisk,
    #[error("the selected file is not an Atlas Financer household")]
    NotAHousehold,
    #[error("household integrity check failed: {0}")]
    Integrity(String),
    #[error("household file does not exist")]
    NotFound,
}

impl StoreError {
    pub(crate) fn database(path: &Path, error: impl std::fmt::Display) -> Self {
        StoreError::Database {
            path_id: path_identity(path),
            message: error.to_string(),
        }
    }

    pub(crate) fn db(error: impl std::fmt::Display) -> Self {
        StoreError::DatabaseInternal(error.to_string())
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(error: serde_json::Error) -> Self {
        StoreError::Serialization(error.to_string())
    }
}

pub type StoreResult<T> = Result<T, StoreError>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Lock {
    pub owner: String,
    pub since: String,
    pub pid: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileStamp {
    database: Option<(u64, u128)>,
    wal: Option<(u64, u128)>,
}

#[derive(Clone, Debug)]
pub struct HouseholdFile {
    path: PathBuf,
    observed: Arc<Mutex<Option<FileStamp>>>,
}

impl HouseholdFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        HouseholdFile {
            path: path.into(),
            observed: Arc::new(Mutex::new(None)),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn identity(&self) -> String {
        path_identity(&self.path)
    }

    pub fn exists(&self) -> bool {
        self.path.is_file()
    }

    pub fn lock(&self) -> StoreResult<Option<Lock>> {
        match fs::read_to_string(self.lock_path()) {
            Ok(contents) => serde_json::from_str(&contents)
                .map(Some)
                .map_err(|_| StoreError::Integrity("the household lock file is malformed".into())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn acquire(&self, owner: &str, take_over: bool) -> StoreResult<Lock> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let lock = Lock {
            owner: owner.to_owned(),
            since: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            pid: std::process::id(),
        };
        let encoded = serde_json::to_vec(&lock)?;
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.lock_path())
        {
            Ok(mut file) => {
                file.write_all(&encoded)?;
                file.sync_all()?;
                Ok(lock)
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = self.lock()?.ok_or(StoreError::LockRequired)?;
                if existing.owner == owner && existing.pid == std::process::id() {
                    return Ok(existing);
                }
                if !take_over {
                    return Err(StoreError::Locked {
                        path: self.path.clone(),
                        owner: existing.owner,
                        since: existing.since,
                    });
                }
                let replacement = self.temporary_sibling("lock");
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&replacement)?;
                file.write_all(&encoded)?;
                file.sync_all()?;
                fs::rename(&replacement, self.lock_path())?;
                Ok(lock)
            }
            Err(error) => Err(error.into()),
        }
    }

    pub fn release(&self, owner: &str) -> StoreResult<()> {
        if let Some(existing) = self.lock()?
            && existing.owner == owner
            && existing.pid == std::process::id()
        {
            fs::remove_file(self.lock_path())?;
        }
        Ok(())
    }

    pub fn save(&self, household: &Household) -> StoreResult<()> {
        runtime().block_on(self.save_async(household))
    }

    pub fn save_owned(&self, household: &Household, owner: &str) -> StoreResult<()> {
        runtime().block_on(self.save_owned_async(household, owner))
    }

    pub async fn save_async(&self, household: &Household) -> StoreResult<()> {
        if self.exists() {
            self.verify_process_lock(None)?;
        }
        self.persist(household).await
    }

    pub async fn save_owned_async(&self, household: &Household, owner: &str) -> StoreResult<()> {
        self.verify_process_lock(Some(owner))?;
        self.persist(household).await
    }

    async fn persist(&self, household: &Household) -> StoreResult<()> {
        self.validate_creation_path()?;
        if self.exists() {
            self.ensure_unchanged()?;
            self.backup_async().await?;
        }
        let database = connection::connect(&self.path, true).await?;
        migrations::apply(&database).await?;
        repository::save(&database, household).await?;
        migrations::integrity_check(&database).await?;
        database
            .execute_unprepared("PRAGMA wal_checkpoint(PASSIVE)")
            .await
            .map_err(StoreError::db)?;
        database.close().await.map_err(StoreError::db)?;
        self.remember_stamp()?;
        log::info!(
            "household save succeeded path_id={} people={} accounts={} series={} policies={}",
            self.identity(),
            household.people.len(),
            household.accounts.len(),
            household.series.len(),
            household.policies.len()
        );
        Ok(())
    }

    pub fn load(&self) -> StoreResult<Household> {
        runtime().block_on(self.load_async())
    }

    pub async fn load_async(&self) -> StoreResult<Household> {
        if !self.exists() {
            return Err(StoreError::NotFound);
        }
        self.verify_process_lock(None)?;
        let inspection = connection::connect_read_only(&self.path).await?;
        if legacy::is_legacy(&inspection).await? {
            let household = legacy::load(&inspection).await?;
            inspection.close().await.map_err(StoreError::db)?;
            return self.upgrade_legacy(household).await;
        }
        if !legacy::table_exists(&inspection, "schema_migrations").await? {
            return Err(StoreError::NotAHousehold);
        }
        let plan = migrations::plan(&inspection).await?;
        inspection.close().await.map_err(StoreError::db)?;
        if !plan.pending.is_empty() {
            let backup = self.backup_async().await?;
            let database = connection::connect(&self.path, false).await?;
            if let Err(error) = migrations::apply(&database).await {
                let _ = database.close().await;
                return Err(StoreError::Upgrade { backup, message: error.to_string() });
            }
            database.close().await.map_err(StoreError::db)?;
        }
        let database = connection::connect_read_only(&self.path).await?;
        migrations::integrity_check(&database).await?;
        let household = repository::load(&database).await?;
        database.close().await.map_err(StoreError::db)?;
        self.remember_stamp()?;
        log::info!(
            "household load succeeded path_id={} records={}",
            self.identity(),
            record_count(&household)
        );
        Ok(household)
    }

    pub fn load_read_only(&self) -> StoreResult<Household> {
        runtime().block_on(self.load_read_only_async())
    }

    pub async fn load_read_only_async(&self) -> StoreResult<Household> {
        if !self.exists() {
            return Err(StoreError::NotFound);
        }
        let database = connection::connect_read_only(&self.path).await?;
        let household = if legacy::is_legacy(&database).await? {
            legacy::load(&database).await?
        } else {
            migrations::plan(&database).await?;
            migrations::integrity_check(&database).await?;
            repository::load(&database).await?
        };
        database.close().await.map_err(StoreError::db)?;
        self.remember_stamp()?;
        Ok(household)
    }

    async fn upgrade_legacy(&self, household: Household) -> StoreResult<Household> {
        let backup = self.backup_async().await?;
        let temporary = self.temporary_sibling("upgrade.atlas.sqlite");
        let result = async {
            let database = connection::connect(&temporary, true).await?;
            migrations::apply(&database).await?;
            repository::save(&database, &household).await?;
            migrations::integrity_check(&database).await?;
            let reloaded = repository::load(&database).await?;
            if reloaded != household {
                return Err(StoreError::Integrity(
                    "legacy conversion changed household values".into(),
                ));
            }
            database
                .execute_unprepared("PRAGMA wal_checkpoint(TRUNCATE)")
                .await
                .map_err(StoreError::db)?;
            database.close().await.map_err(StoreError::db)?;
            fs::rename(&temporary, &self.path)?;
            remove_sqlite_sidecars(&self.path);
            self.remember_stamp()?;
            Ok(reloaded)
        }
        .await;
        if let Err(error) = result {
            remove_sqlite_files(&temporary);
            return Err(StoreError::LegacyMigration {
                backup,
                message: error.to_string(),
            });
        }
        log::info!(
            "legacy household upgrade succeeded path_id={}",
            self.identity()
        );
        result
    }

    pub fn backup(&self) -> StoreResult<PathBuf> {
        runtime().block_on(self.backup_async())
    }

    pub async fn backup_async(&self) -> StoreResult<PathBuf> {
        if !self.exists() {
            return Err(StoreError::NotFound);
        }
        let directory = self.backups_dir();
        fs::create_dir_all(&directory)?;
        let stem = self
            .path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("household");
        let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.9fZ");
        let target = directory.join(format!(
            "{stem}-{timestamp}-{}.sqlite",
            uuid::Uuid::new_v4().simple()
        ));
        let quoted = target
            .to_str()
            .ok_or_else(|| StoreError::Validation("backup path is not valid UTF-8".into()))?
            .replace('\'', "''");
        let database = connection::connect_read_only(&self.path).await?;
        database
            .execute_unprepared(&format!("VACUUM INTO '{quoted}'"))
            .await
            .map_err(StoreError::db)?;
        database.close().await.map_err(StoreError::db)?;
        let backup = connection::connect_read_only(&target).await?;
        migrations::integrity_check(&backup).await?;
        backup.close().await.map_err(StoreError::db)?;
        self.prune_backups(&target)?;
        log::info!("household backup succeeded path_id={}", self.identity());
        Ok(target)
    }

    pub fn restore_backup(&self, backup: &Path, owner: &str) -> StoreResult<()> {
        runtime().block_on(self.restore_backup_async(backup, owner))
    }

    pub async fn restore_backup_async(&self, backup: &Path, owner: &str) -> StoreResult<()> {
        self.verify_process_lock(Some(owner))?;
        let source = connection::connect_read_only(backup).await?;
        migrations::integrity_check(&source).await?;
        source.close().await.map_err(StoreError::db)?;
        if self.exists() {
            self.backup_async().await?;
        }
        let replacement = self.temporary_sibling("restore.atlas.sqlite");
        fs::copy(backup, &replacement)?;
        fs::rename(&replacement, &self.path)?;
        remove_sqlite_sidecars(&self.path);
        self.remember_stamp()
    }

    pub fn peek(&self) -> StoreResult<HouseholdMeta> {
        runtime().block_on(self.peek_async())
    }

    pub async fn peek_async(&self) -> StoreResult<HouseholdMeta> {
        if !self.exists() {
            return Err(StoreError::NotFound);
        }
        let database = connection::connect_read_only(&self.path).await?;
        let metadata = if legacy::is_legacy(&database).await? {
            legacy::peek(&database).await?
        } else {
            migrations::plan(&database).await?;
            let row = database
                .query_one(Statement::from_string(
                    DbBackend::Sqlite,
                    "SELECT name, base_currency, as_of, last_saved_at FROM households WHERE singleton = 1",
                ))
                .await
                .map_err(StoreError::db)?
                .ok_or(StoreError::NotAHousehold)?;
            HouseholdMeta {
                name: row.try_get("", "name").map_err(StoreError::db)?,
                base_currency: row.try_get("", "base_currency").map_err(StoreError::db)?,
                as_of: row.try_get("", "as_of").map_err(StoreError::db)?,
                saved_at: row
                    .try_get::<i64>("", "last_saved_at")
                    .map_err(StoreError::db)?
                    .to_string(),
                schema_version: SCHEMA_VERSION,
            }
        };
        database.close().await.map_err(StoreError::db)?;
        Ok(metadata)
    }

    fn verify_process_lock(&self, owner: Option<&str>) -> StoreResult<()> {
        let lock = self.lock()?.ok_or(StoreError::LockRequired)?;
        if lock.pid != std::process::id() || owner.is_some_and(|expected| expected != lock.owner) {
            return Err(StoreError::Locked {
                path: self.path.clone(),
                owner: lock.owner,
                since: lock.since,
            });
        }
        Ok(())
    }

    fn ensure_unchanged(&self) -> StoreResult<()> {
        let observed = self
            .observed
            .lock()
            .map_err(|_| StoreError::Integrity("file observation lock is poisoned".into()))?;
        if let Some(expected) = observed.as_ref()
            && *expected != file_stamp(&self.path)?
        {
            return Err(StoreError::ChangedOnDisk);
        }
        Ok(())
    }

    fn remember_stamp(&self) -> StoreResult<()> {
        let mut observed = self
            .observed
            .lock()
            .map_err(|_| StoreError::Integrity("file observation lock is poisoned".into()))?;
        *observed = Some(file_stamp(&self.path)?);
        Ok(())
    }

    fn validate_creation_path(&self) -> StoreResult<()> {
        if self.path.file_name().is_none() {
            return Err(StoreError::Validation(
                "household path has no filename".into(),
            ));
        }
        if self.path.exists() && !self.path.is_file() {
            return Err(StoreError::Validation(
                "household path is not a regular file".into(),
            ));
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(())
    }

    fn lock_path(&self) -> PathBuf {
        let mut name = self
            .path
            .file_name()
            .map(|name| name.to_os_string())
            .unwrap_or_default();
        name.push(".lock");
        self.path.with_file_name(name)
    }

    fn backups_dir(&self) -> PathBuf {
        self.path
            .parent()
            .map(|parent| parent.join("backups"))
            .unwrap_or_else(|| PathBuf::from("backups"))
    }

    fn temporary_sibling(&self, suffix: &str) -> PathBuf {
        let mut name = self
            .path
            .file_name()
            .map(|name| name.to_os_string())
            .unwrap_or_else(|| "household".into());
        name.push(format!(".{}.{}", uuid::Uuid::new_v4().simple(), suffix));
        self.path.with_file_name(name)
    }

    fn prune_backups(&self, protected: &Path) -> StoreResult<()> {
        let directory = self.backups_dir();
        let stem = self
            .path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("household");
        let mut backups: Vec<PathBuf> = fs::read_dir(directory)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with(&format!("{stem}-")) && name.ends_with(".sqlite")
                    })
            })
            .collect();
        backups.sort();
        while backups.len() > BACKUPS_KEPT {
            let oldest = backups.remove(0);
            if oldest != protected {
                fs::remove_file(oldest)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HouseholdMeta {
    pub name: String,
    pub base_currency: String,
    pub as_of: String,
    pub saved_at: String,
    pub schema_version: u32,
}

pub fn default_folder() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Documents")
        .join("Atlas")
}

pub fn path_identity(path: &Path) -> String {
    let digest = Sha256::digest(path.to_string_lossy().as_bytes());
    format!("sha256:{}", &hex::encode(digest)[..12])
}

pub(crate) fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("persistence runtime builds")
    })
}

fn file_stamp(path: &Path) -> StoreResult<FileStamp> {
    let mut wal = path.as_os_str().to_os_string();
    wal.push("-wal");
    Ok(FileStamp {
        database: metadata_stamp(path)?,
        wal: metadata_stamp(Path::new(&wal))?,
    })
}

fn metadata_stamp(path: &Path) -> StoreResult<Option<(u64, u128)>> {
    match fs::metadata(path) {
        Ok(metadata) => {
            let modified = metadata
                .modified()
                .unwrap_or(SystemTime::UNIX_EPOCH)
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            Ok(Some((metadata.len(), modified)))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn remove_sqlite_sidecars(path: &Path) {
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let _ = fs::remove_file(PathBuf::from(sidecar));
    }
}

fn remove_sqlite_files(path: &Path) {
    let _ = fs::remove_file(path);
    remove_sqlite_sidecars(path);
}

fn record_count(household: &Household) -> usize {
    household.people.len()
        + household.companies.len()
        + household.accounts.len()
        + household.reservations.len()
        + household.series.len()
        + household.assumptions.len()
        + household.scenarios.len()
        + household.tax_packs.len()
        + household.policies.len()
        + household.actuals.len()
        + household.links.len()
        + household.history.len()
        + household.rules.len()
        + household.goals.len()
        + household.grants.len()
        + household.audit.len()
}

#[cfg(test)]
mod tests;
