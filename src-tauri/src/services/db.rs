/*!
 * SOURCE OF TRUTH KEYWORDS: Db, database handle, SQLite connection, writer connection, read pool, WAL, migrations, pre-migration backup, storage error
 * WHAT:  Db: the handle every service uses to reach SQLite. `open` configures one writer connection plus a small
 *        pool of read-only connections (WAL, synchronous=NORMAL), copies the file to `echo.db.bak-<from_version>`
 *        when a migration is pending, and migrates forward to the latest schema (the SQL files in
 *        src-tauri/migrations).
 *        `read` / `write` lend a connection to one service call and turn any SQLite failure into
 *        `AppError::Storage` with the SQLite message as log-only detail.
 * WHY:   WAL lets the read pool serve history and dashboard queries while the pipeline writes a take, and a single
 *        writer serializes writes without SQLITE_BUSY races (02 §7.2). Every write runs in an IMMEDIATE transaction,
 *        so a service verb that issues several statements is atomic and a failure rolls it back. The backup uses
 *        SQLite's online backup API, a page copy: VACUUM INTO could renumber the implicit rowids the FTS index is
 *        keyed on (05 W29), and a plain file copy would miss pages still in the WAL. A database written by a newer
 *        build (schema ahead of this one) is refused rather than guessed at. Paths come from AppPaths, resolved by
 *        app/bootstrap; services never resolve a path themselves. `read`/`write` are visible to services only, so
 *        no other layer can issue SQL.
 * WHERE: Opened once by app/bootstrap and held by ipc::CommandCtx (and the pipeline later); used by every file
 *        in services/transcripts and services/settings; tests open it in memory with the real migrations.
 */

use std::{
    fmt::Display,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use parking_lot::Mutex;
use rusqlite::{Connection, MAIN_DB, OpenFlags, TransactionBehavior};
use rusqlite_migration::{M, Migrations};

use crate::types::{AppError, AppPaths, PortError, PortResult};

/// Every schema migration in order; a shipped entry is never edited, a change is a new file (forward only).
const STEPS: &[M<'static>] = &[
    M::up(include_str!("../../migrations/0001_init.sql")),
    M::up(include_str!(
        "../../migrations/0002_accelerator_benchmarks.sql"
    )),
];

/// The migrations `open` applies.
const MIGRATIONS: Migrations<'static> = Migrations::from_slice(STEPS);

/// Read-only connections in the pool; enough for the UI's concurrent queries while the pipeline writes.
const READERS: usize = 4;

/// How long a connection waits for a lock (a checkpoint, the writer) before failing with SQLITE_BUSY.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// The SQLite database: one writer and a read pool (or, in memory, one connection for both).
#[derive(Clone)]
pub struct Db {
    pool: Arc<Pool>,
}

struct Pool {
    writer: Mutex<Connection>,
    readers: Vec<Mutex<Connection>>,
    next_reader: AtomicUsize,
}

impl Db {
    /// Opens (creating if needed) `paths.database()`, backs it up if a migration is pending, and migrates it.
    pub fn open(paths: &AppPaths) -> PortResult<Self> {
        let file = paths.database();
        Self::open_with(&file, &MIGRATIONS, |from| paths.database_backup(from))
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: open database, configure pragmas, migrate, read pool setup
     * WHAT:  Opens the writer, applies the pragmas, runs `migrations` (backing up first), then opens the readers.
     * WHY:   Readers are opened after migrating so they never see a half-built schema, and read-only so a bug
     *        can never write through them. Separate from `open` so tests can pass a longer migration list and
     *        exercise the backup path that a single shipped migration cannot reach yet.
     * WHERE: `open`; tests below.
     */
    fn open_with(
        file: &Path,
        migrations: &Migrations<'_>,
        backup: impl FnOnce(usize) -> PathBuf,
    ) -> PortResult<Self> {
        let mut writer = Connection::open(file).map_err(storage)?;
        configure_writer(&writer)?;
        migrate(&mut writer, migrations, backup)?;
        let readers = (0..READERS)
            .map(|_| open_reader(file).map(Mutex::new))
            .collect::<Result<Vec<_>, _>>()
            .map_err(storage)?;
        Ok(Self::from_connections(writer, readers))
    }

    /// A private in-memory database with the real migrations; reads and writes share one connection.
    #[cfg(test)]
    pub fn open_in_memory() -> PortResult<Self> {
        let mut connection = Connection::open_in_memory().map_err(storage)?;
        configure_writer(&connection)?;
        migrate(&mut connection, &MIGRATIONS, |_| PathBuf::new())?;
        Ok(Self::from_connections(connection, Vec::new()))
    }

    fn from_connections(writer: Connection, readers: Vec<Mutex<Connection>>) -> Self {
        Self {
            pool: Arc::new(Pool {
                writer: Mutex::new(writer),
                readers,
                next_reader: AtomicUsize::new(0),
            }),
        }
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: Db::read, read connection, reader pool checkout, query
     * WHAT:  Runs `query` on a free read-only connection (the next one in turn if all are busy).
     * WHY:   Round-robin with a try-lock first spreads concurrent UI queries over the pool; only when every
     *        reader is busy does a caller wait, and then on one connection rather than spinning. An in-memory
     *        database has no readers (each in-memory connection is its own database), so it reads on the writer.
     * WHERE: Read verbs in services/transcripts and services/settings.
     */
    pub(in crate::services) fn read<T>(
        &self,
        query: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> PortResult<T> {
        let readers = &self.pool.readers;
        if readers.is_empty() {
            return query(&self.pool.writer.lock()).map_err(storage);
        }
        let start = self.pool.next_reader.fetch_add(1, Ordering::Relaxed) % readers.len();
        let free = (0..readers.len())
            .find_map(|offset| readers[(start + offset) % readers.len()].try_lock());
        let connection = free.unwrap_or_else(|| readers[start].lock());
        query(&connection).map_err(storage)
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: Db::write, write transaction, immediate transaction, atomic service verb
     * WHAT:  Runs `change` inside an IMMEDIATE transaction on the writer and commits it; any error rolls back.
     * WHY:   IMMEDIATE takes the write lock up front, so a verb never fails half way with SQLITE_BUSY on upgrade;
     *        dropping an uncommitted transaction rolls it back, so an early `?` leaves the table untouched.
     * WHERE: Write verbs in services/transcripts and services/settings.
     */
    pub(in crate::services) fn write<T>(
        &self,
        change: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> PortResult<T> {
        let mut writer = self.pool.writer.lock();
        let transaction = writer
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let output = change(&transaction).map_err(storage)?;
        transaction.commit().map_err(storage)?;
        Ok(output)
    }
}

/// WAL + synchronous=NORMAL (durable across app crashes, fast commits), a busy timeout and foreign keys.
fn configure_writer(connection: &Connection) -> PortResult<()> {
    connection.busy_timeout(BUSY_TIMEOUT).map_err(storage)?;
    let mode: String = connection
        .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))
        .map_err(storage)?;
    // In-memory databases report `memory` and have no WAL; that is expected in tests.
    if !mode.eq_ignore_ascii_case("wal") && !mode.eq_ignore_ascii_case("memory") {
        return Err(PortError::new(AppError::Storage)
            .with_detail(format!("sqlite: journal_mode stayed {mode}")));
    }
    connection
        .pragma_update(None, "synchronous", "NORMAL")
        .map_err(storage)?;
    connection
        .pragma_update(None, "foreign_keys", true)
        .map_err(storage)
}

fn open_reader(file: &Path) -> rusqlite::Result<Connection> {
    let connection = Connection::open_with_flags(
        file,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI,
    )?;
    connection.busy_timeout(BUSY_TIMEOUT)?;
    Ok(connection)
}

/**
 * SOURCE OF TRUTH KEYWORDS: migrate, schema version, user_version, pre-migration backup, echo.db.bak, database too new
 * WHAT:  Brings the schema to the latest version: refuses a database from a newer build, copies an existing
 *        database to `backup(from_version)` before the first pending migration, then applies all pending ones in
 *        one transaction (rusqlite_migration).
 * WHY:   02 §7.2 / §11: a newer build must never corrupt history, so the pre-migration state is kept on disk. A
 *        fresh database (version 0) has nothing to keep. A backup file that already exists is the copy taken by
 *        an earlier attempt from the same version and is left as is, because the database may have changed since
 *        only through that failed attempt's rollback.
 * WHERE: `Db::open_with`, `Db::open_in_memory`.
 */
fn migrate(
    connection: &mut Connection,
    migrations: &Migrations<'_>,
    backup: impl FnOnce(usize) -> PathBuf,
) -> PortResult<()> {
    let from = usize::from(&migrations.current_version(connection).map_err(storage)?);
    let pending = migrations.pending_migrations(connection).map_err(storage)?;
    if pending < 0 {
        return Err(PortError::new(AppError::Storage).with_detail(format!(
            "database schema version {from} is newer than this build supports"
        )));
    }
    if pending == 0 {
        return Ok(());
    }
    if from > 0 {
        let target = backup(from);
        if !target.exists() {
            connection.backup(MAIN_DB, &target, None).map_err(storage)?;
            tracing::info!(from_version = from, "database backed up before migrating");
        }
    }
    migrations.to_latest(connection).map_err(storage)?;
    tracing::info!(from_version = from, "database migrated");
    Ok(())
}

/// A storage failure: the UI sees `Storage`, the log sees the SQLite message (never row contents).
pub(in crate::services) fn storage(error: impl Display) -> PortError {
    PortError::new(AppError::Storage).with_detail(format!("sqlite: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::testing::TempDir;

    fn paths(dir: &TempDir) -> AppPaths {
        AppPaths::new(dir.path(), dir.path())
    }

    const V1: &[M<'static>] = &[M::up("CREATE TABLE notes (body TEXT);")];
    const V2: &[M<'static>] = &[
        M::up("CREATE TABLE notes (body TEXT);"),
        M::up("ALTER TABLE notes ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;"),
    ];

    fn version(db: &Db) -> i64 {
        db.read(|connection| connection.pragma_query_value(None, "user_version", |row| row.get(0)))
            .unwrap()
    }

    #[test]
    fn shipped_migrations_are_valid() {
        MIGRATIONS.validate().unwrap();
    }

    #[test]
    fn a_new_file_is_created_in_wal_mode_at_the_latest_version_without_a_backup() {
        let dir = TempDir::new("db");
        let paths = paths(&dir);
        let db = Db::open(&paths).unwrap();
        assert!(paths.database().exists());
        assert_eq!(version(&db), i64::try_from(STEPS.len()).unwrap());
        let (mode, synchronous): (String, i64) = db
            .write(|connection| {
                Ok((
                    connection.pragma_query_value(None, "journal_mode", |row| row.get(0))?,
                    connection.pragma_query_value(None, "synchronous", |row| row.get(0))?,
                ))
            })
            .unwrap();
        assert_eq!(mode, "wal");
        assert_eq!(synchronous, 1, "NORMAL");
        assert!(!paths.database_backup(0).exists());
        assert_eq!(db.pool.readers.len(), READERS);
    }

    #[test]
    fn reopening_an_up_to_date_database_keeps_its_rows() {
        let dir = TempDir::new("db");
        let paths = paths(&dir);
        Db::open(&paths)
            .unwrap()
            .write(|connection| {
                connection.execute(
                    "INSERT INTO settings (key, value_json, updated_at) VALUES ('a.b', 'true', 1)",
                    [],
                )
            })
            .unwrap();
        let db = Db::open(&paths).unwrap();
        let count: i64 = db
            .read(|connection| {
                connection.query_row("SELECT COUNT(*) FROM settings", [], |row| row.get(0))
            })
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn a_pending_migration_backs_up_the_previous_version_first() {
        let dir = TempDir::new("db");
        let file = dir.join("notes.db");
        let backup_of = |from: usize| dir.join(format!("notes.db.bak-{from}"));

        let v1 = Db::open_with(&file, &Migrations::from_slice(V1), backup_of).unwrap();
        v1.write(|connection| connection.execute("INSERT INTO notes (body) VALUES ('kept')", []))
            .unwrap();
        drop(v1);

        let v2 = Db::open_with(&file, &Migrations::from_slice(V2), backup_of).unwrap();
        assert_eq!(version(&v2), 2);
        let pinned: i64 = v2
            .read(|connection| {
                connection.query_row("SELECT pinned FROM notes", [], |row| row.get(0))
            })
            .unwrap();
        assert_eq!(pinned, 0);

        let backup = Connection::open(backup_of(1)).unwrap();
        let (body, backup_version): (String, i64) = backup
            .query_row(
                "SELECT body, (SELECT user_version FROM pragma_user_version) FROM notes",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((body.as_str(), backup_version), ("kept", 1));
    }

    #[test]
    fn a_database_from_a_newer_build_is_refused_and_not_backed_up() {
        let dir = TempDir::new("db");
        let file = dir.join("notes.db");
        let backup_of = |from: usize| dir.join(format!("notes.db.bak-{from}"));
        drop(Db::open_with(&file, &Migrations::from_slice(V2), backup_of).unwrap());

        let error = Db::open_with(&file, &Migrations::from_slice(V1), backup_of)
            .err()
            .unwrap();
        assert_eq!(error.error(), &AppError::Storage);
        assert!(error.detail().unwrap().contains("newer"));
        assert!(!backup_of(2).exists());
    }

    #[test]
    fn readers_see_committed_writes_and_cannot_write() {
        let dir = TempDir::new("db");
        let db = Db::open(&paths(&dir)).unwrap();
        db.write(|connection| {
            connection.execute(
                "INSERT INTO settings (key, value_json, updated_at) VALUES ('a.b', '1', 1)",
                [],
            )
        })
        .unwrap();
        for _ in 0..READERS {
            let value: String = db
                .read(|connection| {
                    connection.query_row("SELECT value_json FROM settings", [], |row| row.get(0))
                })
                .unwrap();
            assert_eq!(value, "1");
        }
        let error = db
            .read(|connection| connection.execute("DELETE FROM settings", []))
            .unwrap_err();
        assert_eq!(error.error(), &AppError::Storage);
    }

    #[test]
    fn a_failed_write_rolls_back_every_statement() {
        let db = Db::open_in_memory().unwrap();
        let result = db.write(|connection| {
            connection.execute(
                "INSERT INTO settings (key, value_json, updated_at) VALUES ('a.b', '1', 1)",
                [],
            )?;
            connection.execute("INSERT INTO missing_table VALUES (1)", [])
        });
        let error = result.unwrap_err();
        assert_eq!(error.error(), &AppError::Storage);
        assert!(error.detail().unwrap().starts_with("sqlite: "));
        let count: i64 = db
            .read(|connection| {
                connection.query_row("SELECT COUNT(*) FROM settings", [], |row| row.get(0))
            })
            .unwrap();
        assert_eq!(count, 0);
    }
}
