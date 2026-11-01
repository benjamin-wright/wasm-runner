//! Application-scoped SQLite capabilities and host-owned migrations.
mod access;
mod connection;
mod cursor;
mod migrations;

pub use access::MAX_READ_CONNECTIONS;
pub use cursor::SqlCursor;
pub use migrations::{Migration, SqlConfig, validate_migrations};

use std::{
    collections::BTreeMap,
    fmt, fs,
    path::PathBuf,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use anyhow::{Context, Result, ensure};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::{OperationStats, paths::prepare_directory};

const HISTORY_SCHEMA: &str = include_str!("history.sql");
pub(crate) const MAX_SQL_BYTES: usize = 1024 * 1024;
const MAX_ROW_BYTES: usize = 1024 * 1024;
const MAX_STATEMENTS: usize = 100;

#[derive(Clone, Debug, PartialEq)]
pub enum SqlValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Bytes(Vec<u8>),
}

#[derive(Clone, Debug)]
pub struct SqlStatement {
    pub sql: String,
    pub parameters: Vec<SqlValue>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlError {
    InvalidStatement,
    Denied,
    Constraint,
    Busy,
    Limit,
    Timeout,
    Storage,
}

impl fmt::Display for SqlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidStatement => "invalid SQL statement or parameters",
            Self::Denied => "SQL operation is not permitted",
            Self::Constraint => "SQL constraint violation",
            Self::Busy => "SQL database is busy",
            Self::Limit => "SQL size limit exceeded",
            Self::Timeout => "SQL execution timed out",
            Self::Storage => "SQL storage failure",
        })
    }
}
impl std::error::Error for SqlError {}

impl From<rusqlite::Error> for SqlError {
    fn from(error: rusqlite::Error) -> Self {
        use rusqlite::{Error, ErrorCode};
        match error {
            Error::SqliteFailure(error, _) => match error.code {
                ErrorCode::AuthorizationForStatementDenied | ErrorCode::PermissionDenied => {
                    Self::Denied
                }
                ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => Self::Busy,
                ErrorCode::ConstraintViolation => Self::Constraint,
                ErrorCode::OperationInterrupted => Self::Timeout,
                ErrorCode::TooBig => Self::Limit,
                ErrorCode::Unknown => Self::InvalidStatement,
                _ => Self::Storage,
            },
            Error::InvalidParameterCount(..)
            | Error::InvalidParameterName(_)
            | Error::MultipleStatement
            | Error::ExecuteReturnedResults
            | Error::InvalidQuery => Self::InvalidStatement,
            _ => Self::Storage,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SqlInfo {
    pub name: String,
    pub migration: u64,
    pub disk_bytes: Option<u64>,
    pub operations: BTreeMap<String, OperationStats>,
}

/// Lazy owner. Capabilities retain the exclusive file lock after this is dropped.
pub struct SqlStorage {
    root: PathBuf,
    opening: Mutex<()>,
    stores: RwLock<BTreeMap<String, Arc<SqlStore>>>,
}

pub struct SqlStore {
    connection: Arc<access::ConnectionPool>,
    readers: Arc<access::ConnectionPool>,
    _lock: fs::File,
    path: PathBuf,
    migration: AtomicU64,
    operations: Mutex<BTreeMap<String, OperationStats>>,
}

impl SqlStorage {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            opening: Mutex::new(()),
            stores: RwLock::new(BTreeMap::new()),
        }
    }

    pub fn application(&self, name: &str) -> Result<Arc<SqlStore>> {
        ensure!(
            crate::identifier::is_valid(name),
            "invalid application name"
        );
        if let Some(store) = self.stores.read().expect("SQL handles").get(name) {
            return Ok(store.clone());
        }
        let _opening = self.opening.lock().expect("SQL initialization");
        if let Some(store) = self.stores.read().expect("SQL handles").get(name) {
            return Ok(store.clone());
        }
        prepare_directory(&self.root)?;
        // Discover retained databases only when SQL is first enabled.
        if self.stores.read().expect("SQL handles").is_empty() {
            let mut retained = BTreeMap::new();
            for entry in fs::read_dir(&self.root)? {
                let entry = entry?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("invalid SQL storage entry"))?;
                ensure!(
                    entry.file_type()?.is_dir() && crate::identifier::is_valid(&name),
                    "invalid SQL storage entry"
                );
                retained.insert(name, Arc::new(SqlStore::open(entry.path())?));
            }
            *self.stores.write().expect("SQL handles") = retained;
        }
        if let Some(store) = self.stores.read().expect("SQL handles").get(name) {
            return Ok(store.clone());
        }
        let store = Arc::new(SqlStore::open(self.root.join(name))?);
        self.stores
            .write()
            .expect("SQL handles")
            .insert(name.to_owned(), store.clone());
        Ok(store)
    }

    pub fn inspect(&self) -> Vec<SqlInfo> {
        let stores: Vec<_> = self
            .stores
            .read()
            .expect("SQL handles")
            .iter()
            .map(|(name, store)| (name.clone(), store.clone()))
            .collect();
        stores
            .into_iter()
            .map(|(name, store)| SqlInfo {
                name,
                migration: store.migration.load(Ordering::Acquire),
                disk_bytes: disk_bytes(&store.path),
                operations: store.operations.lock().expect("SQL statistics").clone(),
            })
            .collect()
    }
}

impl SqlStore {
    fn open(directory: PathBuf) -> Result<Self> {
        prepare_directory(&directory)?;
        let path = directory.join("data.sqlite");
        let existed = path.try_exists()?;
        ensure!(
            existed || fs::read_dir(&directory)?.next().is_none(),
            "SQL database is missing from nonempty storage directory"
        );
        let mut options = fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options.open(directory.join("owner.lock"))?;
        lock.try_lock().context("SQL database is already owned")?;
        let connection = Connection::open(&path).context("opening SQL database")?;
        connection::configure(&connection)?;
        let mode: String =
            connection.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
        ensure!(
            mode.eq_ignore_ascii_case("wal"),
            "SQL database requires WAL mode"
        );
        if !existed {
            connection.execute_batch(HISTORY_SCHEMA)?;
        }
        let migration = connection
            .query_row(
                "SELECT COALESCE(MAX(id), 0) FROM _wr_migrations WHERE applied = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .context("reading SQL migration history")?;
        Ok(Self {
            connection: access::ConnectionPool::writer(connection),
            readers: access::ConnectionPool::readers(path.clone()),
            _lock: lock,
            path,
            migration: AtomicU64::new(migration as u64),
            operations: Mutex::new(BTreeMap::new()),
        })
    }

    fn operation<T, E>(&self, name: &str, run: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
        let started = Instant::now();
        let result = run();
        let elapsed = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
        let mut operations = self.operations.lock().expect("SQL statistics");
        let stats = operations.entry(name.to_owned()).or_default();
        stats.count += 1;
        stats.failures += u64::from(result.is_err());
        stats.total_micros = stats.total_micros.saturating_add(elapsed);
        stats.max_micros = stats.max_micros.max(elapsed);
        result
    }

    pub fn execute(&self, statement: &SqlStatement, deadline: Instant) -> Result<u64, SqlError> {
        self.operation("execute", || {
            self.with_transaction(deadline, |connection| {
                connection::execute(connection, statement)
            })
        })
    }

    pub fn query(
        self: &Arc<Self>,
        statement: &SqlStatement,
        deadline: Instant,
    ) -> Result<SqlCursor, SqlError> {
        self.query_with_wait(statement, deadline, true)
    }

    pub(crate) fn query_with_wait(
        self: &Arc<Self>,
        statement: &SqlStatement,
        deadline: Instant,
        wait: bool,
    ) -> Result<SqlCursor, SqlError> {
        self.operation("query", || {
            connection::validate_input(statement)?;
            let lease = self.readers.acquire(deadline, wait)?;
            SqlCursor::open(self.clone(), lease, statement, deadline)
        })
    }

    pub fn batch(
        &self,
        statements: &[SqlStatement],
        deadline: Instant,
    ) -> Result<Vec<u64>, SqlError> {
        self.operation("batch", || {
            if statements.len() > MAX_STATEMENTS
                || statements.iter().map(|s| s.sql.len()).sum::<usize>() > MAX_SQL_BYTES
            {
                return Err(SqlError::Limit);
            }
            self.with_transaction(deadline, |connection| {
                statements
                    .iter()
                    .map(|statement| connection::execute(connection, statement))
                    .collect()
            })
        })
    }

    fn with_transaction<T>(
        &self,
        deadline: Instant,
        run: impl FnOnce(&Connection) -> Result<T, SqlError>,
    ) -> Result<T, SqlError> {
        let mut connection = self.connection.acquire(deadline, true)?;
        let transaction = connection::Transaction::begin(&mut connection, false)?;
        let result = {
            let restricted = connection::Restricted::new(transaction.connection, false, deadline)?;
            run(restricted.0)?
        };
        if Instant::now() >= deadline {
            return Err(SqlError::Timeout);
        }
        transaction.commit()?;
        Ok(result)
    }
}

fn disk_bytes(path: &std::path::Path) -> Option<u64> {
    let main = fs::metadata(path).ok()?.len();
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    match fs::metadata(wal) {
        Ok(metadata) => main.checked_add(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(main),
        Err(_) => None,
    }
}
