use std::{
    collections::BTreeMap,
    fmt, fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock, RwLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use anyhow::{Context, Result, ensure};
use fjall::{
    KeyspaceCreateOptions, PersistMode, Readable, SingleWriterTxDatabase, SingleWriterTxKeyspace,
};
use serde::{Deserialize, Serialize};

const MAX_KEY: usize = 1024;
const MAX_VALUE: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KvValue {
    Bytes(Vec<u8>),
    Text(String),
    Integer(i64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KvError {
    InvalidKey,
    ValueTooLarge,
    TypeMismatch,
    Overflow,
    Storage,
}

impl fmt::Display for KvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidKey => "invalid key",
            Self::ValueTooLarge => "value too large",
            Self::TypeMismatch => "type mismatch",
            Self::Overflow => "integer overflow",
            Self::Storage => "storage failure",
        })
    }
}
impl std::error::Error for KvError {}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OperationStats {
    pub count: u64,
    pub failures: u64,
    pub total_micros: u64,
    pub max_micros: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoreInfo {
    pub name: String,
    pub disk_bytes: Option<u64>,
    pub estimated_entries: u64,
    pub operations: BTreeMap<String, OperationStats>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SyncInfo {
    pub count: u64,
    pub failures: u64,
    pub last_micros: Option<u64>,
    pub last_succeeded: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StorageInfo {
    pub opened: bool,
    pub disk_bytes: Option<u64>,
    pub journal_count: Option<usize>,
    pub storage_errors: u64,
    pub sync: SyncInfo,
    pub keyspaces: Vec<StoreInfo>,
}

struct Engine {
    db: SingleWriterTxDatabase,
    errors: AtomicU64,
}

/// A lazy database owner. `new` performs no filesystem operations.
pub struct Storage {
    root: PathBuf,
    opening: Mutex<()>,
    engine: OnceLock<Arc<Engine>>,
    stores: RwLock<BTreeMap<String, Arc<KvStore>>>,
    sync_info: Mutex<SyncInfo>,
}

/// A capability for one application. It cannot select other keyspaces.
pub struct KvStore {
    keyspace: SingleWriterTxKeyspace,
    engine: Arc<Engine>,
    operations: Mutex<BTreeMap<String, OperationStats>>,
}

impl Storage {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            opening: Mutex::new(()),
            engine: OnceLock::new(),
            stores: RwLock::new(BTreeMap::new()),
            sync_info: Mutex::new(SyncInfo::default()),
        }
    }

    pub fn application(&self, name: &str) -> Result<Arc<KvStore>> {
        ensure!(
            !name.is_empty()
                && name.len() <= 64
                && name.as_bytes()[0].is_ascii_lowercase()
                && name
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-'),
            "invalid application name"
        );
        if let Some(store) = self.stores.read().expect("storage handles").get(name) {
            return Ok(store.clone());
        }
        let _opening = self.opening.lock().expect("storage initialization");
        if self.engine.get().is_none() {
            prepare_directory(&self.root)?;
            // Fjall initializes when its version marker is missing, even in a
            // nonempty directory. Never mistake damaged/existing data for new storage.
            if !self.root.join("version").try_exists()? {
                ensure!(
                    fs::read_dir(&self.root)?.next().transpose()?.is_none(),
                    "refusing to initialize a nonempty storage directory without its version marker"
                );
            }
            let db = SingleWriterTxDatabase::builder(&self.root)
                .open()
                .context("opening application storage")?;
            let engine = Arc::new(Engine {
                db,
                errors: AtomicU64::new(0),
            });
            let mut retained = BTreeMap::new();
            for name in engine.db.list_keyspace_names() {
                let keyspace = engine.db.keyspace(&name, KeyspaceCreateOptions::default)?;
                retained.insert(
                    name.to_string(),
                    Arc::new(KvStore::new(keyspace, engine.clone())),
                );
            }
            *self.stores.write().expect("storage handles") = retained;
            let _ = self.engine.set(engine);
        }
        if let Some(store) = self.stores.read().expect("storage handles").get(name) {
            return Ok(store.clone());
        }
        let engine = self.engine.get().expect("opened storage");
        let keyspace = engine
            .db
            .keyspace(name, KeyspaceCreateOptions::default)
            .context("opening application keyspace")?;
        let store = Arc::new(KvStore::new(keyspace, engine.clone()));
        self.stores
            .write()
            .expect("storage handles")
            .insert(name.to_owned(), store.clone());
        Ok(store)
    }

    /// Call only after all preparation and guest operations have drained.
    pub fn sync(&self) -> Result<()> {
        let Some(engine) = self.engine.get() else {
            return Ok(());
        };
        let started = Instant::now();
        let result = engine.db.persist(PersistMode::SyncAll);
        let mut info = self.sync_info.lock().expect("sync statistics");
        info.count += 1;
        info.failures += u64::from(result.is_err());
        info.last_micros = Some(micros(started));
        info.last_succeeded = Some(result.is_ok());
        eprintln!(
            "shared application database: durable sync {} in {} microseconds",
            if result.is_ok() {
                "succeeded"
            } else {
                "failed"
            },
            info.last_micros.unwrap_or_default()
        );
        if result.is_err() {
            engine.errors.fetch_add(1, Ordering::Relaxed);
        }
        result.context("durably syncing shared application database")
    }

    /// Inexpensive engine estimates; no entry scan or exact live-size accounting.
    pub fn inspect(&self) -> StorageInfo {
        let Some(engine) = self.engine.get() else {
            return StorageInfo::default();
        };
        let stores: Vec<_> = self
            .stores
            .read()
            .expect("storage handles")
            .iter()
            .map(|(name, store)| (name.clone(), store.clone()))
            .collect();
        let disk_bytes = engine
            .db
            .disk_space()
            .map_err(|_| {
                engine.errors.fetch_add(1, Ordering::Relaxed);
                eprintln!("shared application database: disk usage inspection failed");
            })
            .ok();
        StorageInfo {
            opened: true,
            disk_bytes,
            journal_count: Some(engine.db.journal_count()),
            storage_errors: engine.errors.load(Ordering::Relaxed),
            sync: self.sync_info.lock().expect("sync statistics").clone(),
            keyspaces: stores
                .into_iter()
                .map(|(name, store)| StoreInfo {
                    name,
                    disk_bytes: Some(store.keyspace.inner().disk_space()),
                    estimated_entries: store.keyspace.approximate_len() as u64,
                    operations: store
                        .operations
                        .lock()
                        .expect("operation statistics")
                        .clone(),
                })
                .collect(),
        }
    }
}

impl KvStore {
    fn new(keyspace: SingleWriterTxKeyspace, engine: Arc<Engine>) -> Self {
        Self {
            keyspace,
            engine,
            operations: Mutex::new(BTreeMap::new()),
        }
    }

    fn operation<T>(
        &self,
        operation: &str,
        key: &str,
        call: impl FnOnce() -> Result<T, KvError>,
    ) -> Result<T, KvError> {
        let started = Instant::now();
        let result = if key.is_empty() || key.len() > MAX_KEY {
            Err(KvError::InvalidKey)
        } else {
            call()
        };
        if matches!(result, Err(KvError::Storage)) {
            self.engine.errors.fetch_add(1, Ordering::Relaxed);
            eprintln!("shared application database: {operation} failed");
        }
        let elapsed = micros(started);
        let mut operations = self.operations.lock().expect("operation statistics");
        let stats = operations.entry(operation.to_owned()).or_default();
        stats.count += 1;
        stats.failures += u64::from(result.is_err());
        stats.total_micros = stats.total_micros.saturating_add(elapsed);
        stats.max_micros = stats.max_micros.max(elapsed);
        result
    }

    pub fn get(&self, key: &str) -> Result<Option<KvValue>, KvError> {
        self.operation("get", key, || {
            self.keyspace
                .get(key)
                .map_err(|_| KvError::Storage)?
                .map(|value| decode(&value))
                .transpose()
        })
    }

    pub fn set(&self, key: &str, value: KvValue) -> Result<(), KvError> {
        self.operation("set", key, || {
            let encoded = encode(value)?;
            self.keyspace
                .insert(key, encoded)
                .map_err(|_| KvError::Storage)
        })
    }

    pub fn delete(&self, key: &str) -> Result<bool, KvError> {
        self.operation("delete", key, || {
            self.keyspace
                .take(key)
                .map(|value| value.is_some())
                .map_err(|_| KvError::Storage)
        })
    }

    pub fn increment(&self, key: &str) -> Result<i64, KvError> {
        self.operation("increment", key, || {
            let mut tx = self.engine.db.write_tx();
            let current = tx.get(&self.keyspace, key).map_err(|_| KvError::Storage)?;
            let value = match current.map(|value| decode(&value)).transpose()? {
                None => 0,
                Some(KvValue::Integer(value)) => value,
                Some(_) => return Err(KvError::TypeMismatch),
            }
            .checked_add(1)
            .ok_or(KvError::Overflow)?;
            tx.insert(&self.keyspace, key, encode(KvValue::Integer(value))?);
            tx.commit().map_err(|_| KvError::Storage)?;
            Ok(value)
        })
    }
}

fn micros(started: Instant) -> u64 {
    started.elapsed().as_micros().min(u64::MAX as u128) as u64
}

fn encode(value: KvValue) -> Result<Vec<u8>, KvError> {
    let (tag, payload) = match value {
        KvValue::Bytes(value) => (0, value),
        KvValue::Text(value) => (1, value.into_bytes()),
        KvValue::Integer(value) => (2, value.to_le_bytes().to_vec()),
    };
    if payload.len() > MAX_VALUE {
        return Err(KvError::ValueTooLarge);
    }
    let mut encoded = Vec::with_capacity(payload.len() + 1);
    encoded.push(tag);
    encoded.extend(payload);
    Ok(encoded)
}

fn decode(value: &[u8]) -> Result<KvValue, KvError> {
    match value.split_first() {
        Some((0, bytes)) => Ok(KvValue::Bytes(bytes.to_vec())),
        Some((1, bytes)) => String::from_utf8(bytes.to_vec())
            .map(KvValue::Text)
            .map_err(|_| KvError::Storage),
        Some((2, bytes)) => bytes
            .try_into()
            .map(i64::from_le_bytes)
            .map(KvValue::Integer)
            .map_err(|_| KvError::Storage),
        _ => Err(KvError::Storage),
    }
}

fn prepare_directory(root: &Path) -> Result<()> {
    // Validate each existing ancestor: create_dir_all otherwise follows symlinks.
    for ancestor in root.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => ensure!(
                metadata.is_dir(),
                "storage path must contain only directories: {}",
                ancestor.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(root)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ensure!(
            fs::metadata(root)?.permissions().mode() & 0o077 == 0,
            "storage directory must be private (mode 0700)"
        );
    }
    validate_managed_paths(root)
}

fn validate_managed_paths(root: &Path) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        ensure!(
            kind.is_file() || kind.is_dir(),
            "invalid managed storage path: {}",
            entry.path().display()
        );
        if kind.is_dir() {
            validate_managed_paths(&entry.path())?;
        }
    }
    Ok(())
}
