use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use wasm_runner::persistence::{KvError, KvValue, Storage};

struct Database(PathBuf);
impl Database {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let base = fs::canonicalize(std::env::temp_dir()).unwrap();
        Self(base.join(format!(
            "wr-kv-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
    fn open(&self) -> Storage {
        Storage::new(&self.0)
    }
}
impl Drop for Database {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn typed_values_limits_and_explicit_sync_survive_reopen() {
    let directory = Database::new();
    let storage = directory.open();
    assert!(!directory.0.exists());
    assert!(!storage.inspect().opened);
    storage.sync().unwrap();
    assert!(!directory.0.exists());
    let store = storage.application("first").unwrap();
    assert_eq!(storage.inspect().keyspaces[0].disk_bytes, Some(0));
    assert!(Arc::ptr_eq(&store, &storage.application("first").unwrap()));
    assert_eq!(store.get("missing").unwrap(), None);
    assert!(!store.delete("missing").unwrap());
    let values = [
        KvValue::Bytes(vec![]),
        KvValue::Bytes(vec![0, 255]),
        KvValue::Text(String::new()),
        KvValue::Text("日本語 🦀".into()),
        KvValue::Integer(i64::MIN),
        KvValue::Integer(i64::MAX),
    ];
    for (index, value) in values.iter().enumerate() {
        let key = index.to_string();
        store.set(&key, value.clone()).unwrap();
        assert_eq!(store.get(&key).unwrap(), Some(value.clone()));
        store.set("replacement", value.clone()).unwrap();
        assert_eq!(store.get("replacement").unwrap(), Some(value.clone()));
    }
    for key in [String::new(), "é".repeat(513)] {
        assert_eq!(store.get(&key), Err(KvError::InvalidKey));
        assert_eq!(
            store.set(&key, KvValue::Integer(0)),
            Err(KvError::InvalidKey)
        );
        assert_eq!(store.delete(&key), Err(KvError::InvalidKey));
        assert_eq!(store.increment(&key), Err(KvError::InvalidKey));
    }
    store
        .set(&"é".repeat(512), KvValue::Bytes(vec![0; 1024 * 1024]))
        .unwrap();
    store
        .set("large-text", KvValue::Text("é".repeat(512 * 1024)))
        .unwrap();
    assert_eq!(
        store.set("replacement", KvValue::Bytes(vec![0; 1024 * 1024 + 1])),
        Err(KvError::ValueTooLarge)
    );
    assert_eq!(
        store.set("replacement", KvValue::Text("é".repeat(512 * 1024 + 1))),
        Err(KvError::ValueTooLarge)
    );
    assert_eq!(
        store.get("replacement").unwrap(),
        Some(KvValue::Integer(i64::MAX))
    );
    assert!(store.delete("replacement").unwrap());
    assert!(!store.delete("replacement").unwrap());
    storage.sync().unwrap();
    assert_eq!(storage.inspect().sync.last_succeeded, Some(true));
    drop(store);
    drop(storage);
    let storage = directory.open();
    let store = storage.application("first").unwrap();
    for (index, value) in values.iter().enumerate() {
        assert_eq!(store.get(&index.to_string()).unwrap(), Some(value.clone()));
    }
    assert_eq!(store.get("replacement").unwrap(), None);
    let info = storage.inspect();
    let keyspace_bytes = info.keyspaces[0].disk_bytes.expect("keyspace disk usage");
    assert!(info.disk_bytes.expect("database disk usage") >= keyspace_bytes);
}

#[test]
fn atomic_increment_returns_own_result_and_preserves_errors() {
    let directory = Database::new();
    let storage = Arc::new(directory.open());
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let storage = storage.clone();
            std::thread::spawn(move || {
                let store = storage.application("counter").unwrap();
                (0..100)
                    .map(|_| {
                        let started = std::time::Instant::now();
                        let value = store.increment("count").unwrap();
                        (value, started.elapsed().as_nanos())
                    })
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    let (mut results, concurrent_latencies): (Vec<_>, Vec<_>) = handles
        .into_iter()
        .flat_map(|handle| handle.join().unwrap())
        .unzip();
    results.sort_unstable();
    assert_eq!(results, (1..=800).collect::<Vec<_>>());
    let store = storage.application("counter").unwrap();
    assert_eq!(store.get("count").unwrap(), Some(KvValue::Integer(800)));
    store.set("negative", KvValue::Integer(-2)).unwrap();
    assert_eq!(store.increment("negative"), Ok(-1));
    for value in [
        KvValue::Text("1".into()),
        KvValue::Bytes(b"1".to_vec()),
        KvValue::Integer(i64::MAX),
    ] {
        store.set("error", value.clone()).unwrap();
        assert_eq!(
            store.increment("error"),
            Err(if matches!(value, KvValue::Integer(_)) {
                KvError::Overflow
            } else {
                KvError::TypeMismatch
            })
        );
        assert_eq!(store.get("error").unwrap(), Some(value));
    }
    let stats = storage.inspect();
    assert_eq!(stats.keyspaces.len(), 1);
    assert_eq!(stats.keyspaces[0].operations["increment"].count, 804);
    assert_eq!(stats.keyspaces[0].operations["increment"].failures, 3);
    assert!(stats.keyspaces[0].operations["increment"].max_micros > 0);
    // Atomic take allows precisely one winner, even under concurrent deletion.
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let store = store.clone();
            std::thread::spawn(move || store.delete("count").unwrap())
        })
        .collect();
    assert_eq!(
        handles
            .into_iter()
            .map(|handle| usize::from(handle.join().unwrap()))
            .sum::<usize>(),
        1
    );
    let sequential_latencies = (0..800)
        .map(|_| {
            let started = std::time::Instant::now();
            store.increment("sequential").unwrap();
            started.elapsed().as_nanos()
        })
        .collect();
    report_latency("sequential", sequential_latencies);
    report_latency("8 concurrent writers", concurrent_latencies);
    storage.sync().unwrap();
}

fn report_latency(workload: &str, mut samples: Vec<u128>) {
    use std::io::Write;
    samples.sort_unstable();
    // Keep these local smoke measurements visible in normal test output. They
    // establish no bound and deliberately do not determine pass/fail.
    let _ = writeln!(
        std::io::stderr(),
        "KV increment, {workload}, {} samples (nanoseconds): p50={} p95={} p99={} max={}",
        samples.len(),
        samples[samples.len() / 2],
        samples[samples.len() * 95 / 100],
        samples[samples.len() * 99 / 100],
        samples.last().unwrap()
    );
}

#[test]
fn identities_retained_handles_and_exclusive_ownership() {
    let directory = Database::new();
    let storage = directory.open();
    assert!(storage.application("../escape").is_err());
    assert!(!directory.0.exists());
    let first = storage.application("first").unwrap();
    let second = storage.application("second").unwrap();
    first.set("../second/key", KvValue::Integer(7)).unwrap();
    assert_eq!(second.get("../second/key").unwrap(), None);
    second.increment("../second/key").unwrap();
    assert_eq!(
        first.get("../second/key").unwrap(),
        Some(KvValue::Integer(7))
    );
    assert!(directory.open().application("first").is_err());
    storage.sync().unwrap();
    drop(storage);
    assert_eq!(first.increment("../second/key").unwrap(), 8);
    assert!(directory.open().application("first").is_err());
    drop(first);
    drop(second);
    let storage = directory.open();
    storage.application("third").unwrap();
    assert_eq!(
        storage
            .inspect()
            .keyspaces
            .iter()
            .map(|info| info.name.as_str())
            .collect::<Vec<_>>(),
        ["first", "second", "third"]
    );
    assert!(
        storage
            .inspect()
            .keyspaces
            .iter()
            .all(|info| info.operations.is_empty())
    );
    assert_eq!(
        storage
            .application("first")
            .unwrap()
            .get("../second/key")
            .unwrap(),
        Some(KvValue::Integer(8))
    );
}

#[cfg(unix)]
#[test]
fn rejects_nonprivate_and_redirected_storage_without_touching_data() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let directory = Database::new();
    fs::create_dir(&directory.0).unwrap();
    fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(directory.open().application("first").is_err());
    fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o700)).unwrap();
    let target = Database::new();
    fs::write(&target.0, "preserve").unwrap();
    symlink(&target.0, directory.0.join("managed")).unwrap();
    assert!(directory.open().application("first").is_err());
    assert_eq!(fs::read_to_string(&target.0).unwrap(), "preserve");
    fs::remove_file(&target.0).unwrap();
}

#[test]
fn rejects_unrecognized_existing_directory() {
    let directory = Database::new();
    fs::create_dir(&directory.0).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fs::write(directory.0.join("unrecognized"), "preserve").unwrap();
    assert!(
        directory.open().application("first").is_err(),
        "unexpectedly initialized existing directory: {:?}",
        fs::read_dir(&directory.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        fs::read_to_string(directory.0.join("unrecognized")).unwrap(),
        "preserve"
    );
}

#[test]
fn corrupt_or_missing_version_never_resets_existing_data() {
    let directory = Database::new();
    let storage = directory.open();
    storage
        .application("first")
        .unwrap()
        .set("saved", KvValue::Text("preserve".into()))
        .unwrap();
    storage.sync().unwrap();
    drop(storage);
    let marker = directory.0.join("version");
    let original = fs::read(&marker).unwrap();
    fs::write(&marker, "invalid version").unwrap();
    assert!(directory.open().application("first").is_err());
    assert_eq!(fs::read(&marker).unwrap(), b"invalid version");
    fs::remove_file(&marker).unwrap();
    assert!(directory.open().application("first").is_err());
    assert!(!marker.exists());
    fs::write(&marker, original).unwrap();
    assert_eq!(
        directory
            .open()
            .application("first")
            .unwrap()
            .get("saved")
            .unwrap(),
        Some(KvValue::Text("preserve".into()))
    );
}
