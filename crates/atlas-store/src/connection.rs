use std::path::Path;
use std::time::Duration;

use sea_orm::{DatabaseConnection, SqlxSqliteConnector};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

use crate::{StoreError, StoreResult};

const MAX_CONNECTIONS: u32 = 4;

pub(crate) async fn connect(path: &Path, create: bool) -> StoreResult<DatabaseConnection> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(create)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .busy_timeout(Duration::from_secs(5))
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(MAX_CONNECTIONS)
        .connect_with(options)
        .await
        .map_err(|error| StoreError::database(path, error))?;
    Ok(SqlxSqliteConnector::from_sqlx_sqlite_pool(pool))
}

pub(crate) async fn connect_read_only(path: &Path) -> StoreResult<DatabaseConnection> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .create_if_missing(false)
        .busy_timeout(Duration::from_secs(5))
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|error| StoreError::database(path, error))?;
    Ok(SqlxSqliteConnector::from_sqlx_sqlite_pool(pool))
}
