use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use fslite_core::{FsError, FsResult};
use rusqlite::{Connection as RusqliteConnection, OptionalExtension};
use tokio_rusqlite::Connection;

const MIGRATIONS: &[(i64, &str)] = &[(1, include_str!("../migrations/0001_initial.sql"))];

fn latest_schema_version() -> i64 {
    MIGRATIONS.last().map(|(version, _)| *version).unwrap_or(0)
}

/// Options controlling how a SQLite-backed filesystem opens its database connection.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct ConnectOptions {
    /// How long a connection waits on a locked database before failing.
    pub busy_timeout: Duration,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            busy_timeout: Duration::from_secs(5),
        }
    }
}

/// Returns the current Unix timestamp in milliseconds.
pub(crate) fn now_ms() -> i64 {
    UNIX_EPOCH
        .elapsed()
        .expect("system clock is before the Unix epoch")
        .as_millis() as i64
}

pub(crate) fn map_call_error(err: tokio_rusqlite::Error) -> FsError {
    FsError::internal_storage_failure(err)
}

pub(crate) async fn open_file(path: &Path, options: ConnectOptions) -> FsResult<Connection> {
    let owned_path: PathBuf = path.to_owned();
    let conn = Connection::open(owned_path).await.map_err(map_call_error)?;
    initialize(&conn, options).await?;
    Ok(conn)
}

pub(crate) async fn open_memory(options: ConnectOptions) -> FsResult<Connection> {
    let conn = Connection::open_in_memory().await.map_err(map_call_error)?;
    initialize(&conn, options).await?;
    Ok(conn)
}

async fn initialize(conn: &Connection, options: ConnectOptions) -> FsResult<()> {
    let current_version = conn
        .call(move |conn| {
            conn.pragma_update(None, "foreign_keys", "ON")?;
            conn.pragma_update(None, "journal_mode", "WAL")?;
            conn.pragma_update(None, "synchronous", "NORMAL")?;
            conn.busy_timeout(options.busy_timeout)?;
            Ok(read_current_version(conn)?)
        })
        .await
        .map_err(map_call_error)?;

    if current_version > latest_schema_version() {
        return Err(FsError::internal_storage_failure(format!(
            "database schema version {current_version} is newer than the supported version {}",
            latest_schema_version()
        )));
    }

    conn.call(move |conn| Ok(apply_migrations(conn, current_version)?))
        .await
        .map_err(map_call_error)
}

fn read_current_version(conn: &RusqliteConnection) -> rusqlite::Result<i64> {
    let table_exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations'",
        [],
        |row| row.get(0),
    )?;

    if table_exists == 0 {
        return Ok(0);
    }

    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )
    .optional()
    .map(|value| value.unwrap_or(0))
}

fn apply_migrations(conn: &mut RusqliteConnection, current_version: i64) -> rusqlite::Result<()> {
    for (version, sql) in MIGRATIONS {
        if *version <= current_version {
            continue;
        }

        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO schema_migrations(version, applied_at_ms) VALUES (?1, ?2)",
            rusqlite::params![version, now_ms()],
        )?;
        tx.commit()?;
    }

    Ok(())
}
