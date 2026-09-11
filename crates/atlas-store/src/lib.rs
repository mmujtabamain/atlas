//! Persistence for Atlas Financer (§23 "protection against data loss").
//!
//! A household is one SQLite file (`<name>.atlas.sqlite`): a `meta` table
//! (schema version, name, currency, reconciliation date, save time) and one
//! table per entity type holding each object as JSON with its id, in the
//! household's own order. Saving
//! is a single transaction; the previous file is copied into `backups/`
//! first (rolling, 20 kept). A sidecar `.lock` file names who has the
//! household open so two people on the same Mac take turns (Mujtaba's
//! choice: one editor at a time, no server).
//!
//! `atlas-core` stays I/O-free: this crate only serialises the model.

use atlas_core::model::{Household, SCHEMA_VERSION};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Why a store operation failed.
#[derive(Error, Debug)]
pub enum StoreError {
    #[error("database: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serialisation: {0}")]
    Json(#[from] serde_json::Error),
    #[error("file: {0}")]
    Io(#[from] std::io::Error),
    #[error("{path} was written by schema version {found}; this build reads version {supported}")]
    SchemaVersion { path: PathBuf, found: u32, supported: u32 },
    #[error("{path} is open by {owner} since {since}; take it over only if they are done")]
    Locked { path: PathBuf, owner: String, since: String },
    #[error("{0} is not an Atlas household file (no meta table)")]
    NotAHousehold(PathBuf),
}

pub type StoreResult<T> = Result<T, StoreError>;

/// Entity tables, in save order.
const TABLES: [&str; 14] = [
    "people",
    "companies",
    "accounts",
    "reservations",
    "series",
    "assumptions",
    "scenarios",
    "tax_packs",
    "policies",
    "actuals",
    "links",
    "history",
    "rules",
    "goals",
];

/// Rolling backups kept per household.
pub const BACKUPS_KEPT: usize = 20;

/// The sidecar lock's content.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Lock {
    pub owner: String,
    pub since: String,
    pub pid: u32,
}

/// Where a household lives on disk.
#[derive(Clone, Debug)]
pub struct HouseholdFile {
    path: PathBuf,
}

impl HouseholdFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        HouseholdFile { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn lock_path(&self) -> PathBuf {
        let mut name = self.path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
        name.push(".lock");
        self.path.with_file_name(name)
    }

    fn backups_dir(&self) -> PathBuf {
        self.path.parent().map(|p| p.join("backups")).unwrap_or_else(|| PathBuf::from("backups"))
    }

    /// The current lock, if any.
    pub fn lock(&self) -> StoreResult<Option<Lock>> {
        match fs::read_to_string(self.lock_path()) {
            Ok(text) => Ok(serde_json::from_str(&text).ok()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    /// Takes the lock for `owner`; refuses when another owner holds it unless
    /// `take_over` is set.
    pub fn acquire(&self, owner: &str, take_over: bool) -> StoreResult<Lock> {
        if let Some(existing) = self.lock()?
            && existing.owner != owner
            && !take_over
        {
            return Err(StoreError::Locked { path: self.path.clone(), owner: existing.owner, since: existing.since });
        }
        let lock = Lock { owner: owner.to_string(), since: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(), pid: std::process::id() };
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(self.lock_path(), serde_json::to_string(&lock)?)?;
        log::info!("locked {} for {owner}", self.path.display());
        Ok(lock)
    }

    /// Releases the lock (only the owner's lock is removed).
    pub fn release(&self, owner: &str) -> StoreResult<()> {
        if let Some(existing) = self.lock()?
            && existing.owner == owner
        {
            fs::remove_file(self.lock_path())?;
            log::info!("unlocked {}", self.path.display());
        }
        Ok(())
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Saves the household: backup of the previous file, then one transaction
    /// that rewrites every table.
    pub fn save(&self, household: &Household) -> StoreResult<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        if self.path.exists() {
            self.backup()?;
        }
        let mut connection = Connection::open(&self.path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        let tx = connection.transaction()?;
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS people (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS companies (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS accounts (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS reservations (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS series (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS assumptions (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS scenarios (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS tax_packs (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS policies (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS actuals (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS links (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS history (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS rules (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS goals (seq INTEGER PRIMARY KEY AUTOINCREMENT, id INTEGER NOT NULL, json TEXT NOT NULL);",
        )?;
        for table in TABLES {
            tx.execute(&format!("DELETE FROM {table}"), [])?;
        }
        let mut meta = tx.prepare("INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)")?;
        meta.execute(params!["schema_version", SCHEMA_VERSION.to_string()])?;
        meta.execute(params!["name", household.name.clone()])?;
        meta.execute(params!["base_currency", household.base_currency.code().to_string()])?;
        meta.execute(params!["as_of", household.as_of.to_string()])?;
        meta.execute(params!["rule_tie_break", household.rule_tie_break.slug()])?;
        meta.execute(params!["saved_at", chrono::Local::now().to_rfc3339()])?;
        meta.execute(params!["app_version", env!("CARGO_PKG_VERSION")])?;
        drop(meta);

        insert_all(&tx, "people", household.people.iter().map(|p| (p.id.raw() as i64, p)))?;
        insert_all(&tx, "companies", household.companies.iter().map(|c| (c.id.raw() as i64, c)))?;
        insert_all(&tx, "accounts", household.accounts.iter().map(|a| (a.id.raw() as i64, a)))?;
        insert_all(&tx, "reservations", household.reservations.iter().map(|r| (r.id.raw() as i64, r)))?;
        insert_all(&tx, "series", household.series.iter().map(|s| (s.id.raw() as i64, s)))?;
        insert_all(&tx, "assumptions", household.assumptions.iter().map(|a| (a.id.raw() as i64, a)))?;
        insert_all(&tx, "scenarios", household.scenarios.iter().map(|s| (s.id.raw() as i64, s)))?;
        insert_all(&tx, "tax_packs", household.tax_packs.iter().enumerate().map(|(i, p)| (i as i64 + 1, p)))?;
        insert_all(&tx, "policies", household.policies.iter().map(|p| (p.id.raw() as i64, p)))?;
        insert_all(&tx, "actuals", household.actuals.iter().map(|t| (t.id.raw() as i64, t)))?;
        insert_all(&tx, "links", household.links.iter().enumerate().map(|(i, l)| (i as i64 + 1, l)))?;
        insert_all(&tx, "history", household.history.iter().enumerate().map(|(i, h)| (i as i64 + 1, h)))?;
        insert_all(&tx, "rules", household.rules.iter().map(|r| (r.id.raw() as i64, r)))?;
        insert_all(&tx, "goals", household.goals.iter().map(|g| (g.id.raw() as i64, g)))?;
        tx.commit()?;
        log::info!(
            "saved {} to {} ({} people, {} accounts, {} series, {} policies)",
            household.name,
            self.path.display(),
            household.people.len(),
            household.accounts.len(),
            household.series.len(),
            household.policies.len()
        );
        Ok(())
    }

    /// Loads the household; the schema version must match.
    pub fn load(&self) -> StoreResult<Household> {
        let connection = Connection::open_with_flags(&self.path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let has_meta: bool = connection
            .query_row("SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'meta'", [], |row| row.get::<_, i64>(0))
            .map(|n| n > 0)?;
        if !has_meta {
            return Err(StoreError::NotAHousehold(self.path.clone()));
        }
        let version: u32 = meta_value(&connection, "schema_version")?.and_then(|v| v.parse().ok()).unwrap_or(0);
        if version != SCHEMA_VERSION {
            return Err(StoreError::SchemaVersion { path: self.path.clone(), found: version, supported: SCHEMA_VERSION });
        }
        let name = meta_value(&connection, "name")?.unwrap_or_else(|| "Household".into());
        let currency = meta_value(&connection, "base_currency")?
            .and_then(|c| atlas_core::Currency::from_code(&c))
            .unwrap_or(atlas_core::Currency::USD);
        let as_of = meta_value(&connection, "as_of")?
            .and_then(|d| d.parse().ok())
            .unwrap_or_else(|| chrono::NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid"));
        let mut household = Household::empty(&name, currency, as_of);
        household.rule_tie_break = meta_value(&connection, "rule_tie_break")?.and_then(|t| atlas_core::rules::TieBreak::from_slug(&t)).unwrap_or_default();
        household.people = load_all(&connection, "people")?;
        household.companies = load_all(&connection, "companies")?;
        household.accounts = load_all(&connection, "accounts")?;
        household.reservations = load_all(&connection, "reservations")?;
        household.series = load_all(&connection, "series")?;
        household.assumptions = load_all(&connection, "assumptions")?;
        household.scenarios = load_all(&connection, "scenarios")?;
        household.tax_packs = load_all(&connection, "tax_packs")?;
        household.policies = load_all(&connection, "policies")?;
        household.actuals = load_all(&connection, "actuals")?;
        household.links = load_all(&connection, "links")?;
        household.history = load_all(&connection, "history")?;
        // Tables added after the first files were written are optional on read (M7 rules).
        household.rules = load_optional(&connection, "rules")?;
        household.goals = load_optional(&connection, "goals")?;
        log::info!("loaded {} from {} ({} accounts, {} series)", household.name, self.path.display(), household.accounts.len(), household.series.len());
        Ok(household)
    }

    /// Copies the current file into `backups/` and prunes to [`BACKUPS_KEPT`].
    pub fn backup(&self) -> StoreResult<PathBuf> {
        let dir = self.backups_dir();
        fs::create_dir_all(&dir)?;
        let stem = self.path.file_stem().and_then(|s| s.to_str()).unwrap_or("household");
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S%.3f");
        let target = dir.join(format!("{stem}-{stamp}.sqlite"));
        fs::copy(&self.path, &target)?;
        let mut backups: Vec<PathBuf> = fs::read_dir(&dir)?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&format!("{stem}-")) && n.ends_with(".sqlite")))
            .collect();
        backups.sort();
        while backups.len() > BACKUPS_KEPT {
            let oldest = backups.remove(0);
            let _ = fs::remove_file(oldest);
        }
        Ok(target)
    }

    /// Metadata without loading the whole household (for the Open dialog).
    pub fn peek(&self) -> StoreResult<HouseholdMeta> {
        let connection = Connection::open_with_flags(&self.path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        Ok(HouseholdMeta {
            name: meta_value(&connection, "name")?.unwrap_or_default(),
            base_currency: meta_value(&connection, "base_currency")?.unwrap_or_default(),
            as_of: meta_value(&connection, "as_of")?.unwrap_or_default(),
            saved_at: meta_value(&connection, "saved_at")?.unwrap_or_default(),
            schema_version: meta_value(&connection, "schema_version")?.and_then(|v| v.parse().ok()).unwrap_or(0),
        })
    }
}

/// What `peek` returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HouseholdMeta {
    pub name: String,
    pub base_currency: String,
    pub as_of: String,
    pub saved_at: String,
    pub schema_version: u32,
}

fn meta_value(connection: &Connection, key: &str) -> StoreResult<Option<String>> {
    Ok(connection.query_row("SELECT value FROM meta WHERE key = ?1", params![key], |row| row.get(0)).optional()?)
}

fn insert_all<'a, T: Serialize + 'a>(tx: &rusqlite::Transaction<'_>, table: &str, rows: impl Iterator<Item = (i64, &'a T)>) -> StoreResult<()> {
    let mut statement = tx.prepare(&format!("INSERT INTO {table} (id, json) VALUES (?1, ?2)"))?;
    for (id, row) in rows {
        statement.execute(params![id, serde_json::to_string(row)?])?;
    }
    Ok(())
}

fn load_optional<T: for<'de> Deserialize<'de>>(connection: &Connection, table: &str) -> StoreResult<Vec<T>> {
    let exists: bool = connection
        .query_row("SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1", params![table], |row| row.get::<_, i64>(0))
        .map(|n| n > 0)?;
    if exists { load_all(connection, table) } else { Ok(Vec::new()) }
}

fn load_all<T: for<'de> Deserialize<'de>>(connection: &Connection, table: &str) -> StoreResult<Vec<T>> {
    let mut statement = connection.prepare(&format!("SELECT json FROM {table} ORDER BY seq"))?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for json in rows {
        out.push(serde_json::from_str(&json?)?);
    }
    Ok(out)
}

/// The default folder for household files (`~/Documents/Atlas`).
pub fn default_folder() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Documents")
        .join("Atlas")
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_core::fixtures;
    use atlas_core::forecast::{Case, ForecastOptions, forecast};
    use atlas_core::liquidity::Boundary;

    #[test]
    fn round_trip_keeps_the_household_and_its_forecast_identical() {
        let dir = tempfile::tempdir().unwrap();
        let file = HouseholdFile::new(dir.path().join("plan.atlas.sqlite"));
        let household = fixtures::plan_household();
        file.save(&household).unwrap();
        let loaded = file.load().unwrap();
        assert_eq!(loaded, household);
        let options = ForecastOptions { through: fixtures::default_horizon(), scenario: None, case: Case::Expected };
        let before = forecast(&household, Boundary::Household, options).unwrap();
        let after = forecast(&loaded, Boundary::Household, options).unwrap();
        assert_eq!(before.record.input_hash, after.record.input_hash, "§42: the same inputs reproduce the same result");
        assert_eq!(before.end.money(), after.end.money());
        let meta = file.peek().unwrap();
        assert_eq!(meta.name, household.name);
        assert_eq!(meta.base_currency, "PKR");
        assert_eq!(meta.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn saving_twice_keeps_a_backup_and_prunes() {
        let dir = tempfile::tempdir().unwrap();
        let file = HouseholdFile::new(dir.path().join("h.atlas.sqlite"));
        let mut household = fixtures::plan_household();
        file.save(&household).unwrap();
        for i in 0..(BACKUPS_KEPT + 3) {
            household.name = format!("version {i}");
            file.save(&household).unwrap();
        }
        let backups = fs::read_dir(dir.path().join("backups")).unwrap().count();
        assert_eq!(backups, BACKUPS_KEPT);
        assert_eq!(file.load().unwrap().name, format!("version {}", BACKUPS_KEPT + 2));
    }

    #[test]
    fn lock_refuses_a_second_owner_unless_taken_over() {
        let dir = tempfile::tempdir().unwrap();
        let file = HouseholdFile::new(dir.path().join("h.atlas.sqlite"));
        file.acquire("Person A", false).unwrap();
        let err = file.acquire("Person B", false).unwrap_err();
        assert!(matches!(err, StoreError::Locked { ref owner, .. } if owner == "Person A"));
        file.acquire("Person A", false).unwrap();
        let taken = file.acquire("Person B", true).unwrap();
        assert_eq!(taken.owner, "Person B");
        file.release("Person A").unwrap();
        assert!(file.lock().unwrap().is_some(), "only the owner's lock is removed");
        file.release("Person B").unwrap();
        assert!(file.lock().unwrap().is_none());
    }

    #[test]
    fn foreign_files_and_schema_mismatches_are_named() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("other.sqlite");
        Connection::open(&path).unwrap().execute_batch("CREATE TABLE x (a)").unwrap();
        assert!(matches!(HouseholdFile::new(&path).load().unwrap_err(), StoreError::NotAHousehold(_)));
        let file = HouseholdFile::new(dir.path().join("h.atlas.sqlite"));
        file.save(&Household::default()).unwrap();
        Connection::open(file.path()).unwrap().execute("UPDATE meta SET value = '99' WHERE key = 'schema_version'", []).unwrap();
        assert!(matches!(file.load().unwrap_err(), StoreError::SchemaVersion { found: 99, .. }));
    }

    #[test]
    fn an_empty_household_round_trips_with_usd() {
        let dir = tempfile::tempdir().unwrap();
        let file = HouseholdFile::new(dir.path().join("empty.atlas.sqlite"));
        let household = Household::empty("Our household", atlas_core::Currency::USD, chrono::NaiveDate::from_ymd_opt(2026, 9, 11).unwrap());
        file.save(&household).unwrap();
        assert_eq!(file.load().unwrap(), household);
    }
}
