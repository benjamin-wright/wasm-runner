use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
use wasm_runner::persistence::sql::{
    MAX_READ_CONNECTIONS, Migration, SqlError, SqlStatement, SqlStorage, SqlStore, SqlValue,
};

#[derive(Debug, PartialEq)]
struct Rows {
    columns: Vec<String>,
    rows: Vec<Vec<SqlValue>>,
}

trait CollectQuery {
    fn collect_query(&self, input: &SqlStatement, deadline: Instant) -> Result<Rows, SqlError>;
}

impl CollectQuery for std::sync::Arc<SqlStore> {
    fn collect_query(&self, input: &SqlStatement, deadline: Instant) -> Result<Rows, SqlError> {
        let mut cursor = self.query(input, deadline)?;
        let mut result = Rows {
            columns: cursor.columns().to_vec(),
            rows: Vec::new(),
        };
        while let Some(row) = cursor.next_row()? {
            result.rows.push(row);
        }
        Ok(result)
    }
}

struct Database(PathBuf);

impl Database {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        Self(
            fs::canonicalize(std::env::temp_dir())
                .unwrap()
                .join(format!(
                    "wr-sql-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                )),
        )
    }
    fn open(&self) -> SqlStorage {
        SqlStorage::new(&self.0)
    }
}
impl Drop for Database {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn statement(sql: &str) -> SqlStatement {
    SqlStatement {
        sql: sql.into(),
        parameters: vec![],
    }
}
fn migrations() -> Vec<Migration> {
    vec![
        Migration { id: 1, name: "users".into(), apply: "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL); INSERT INTO users VALUES (1, 'first');".into(), rollback: Some("DROP TABLE users;".into()) },
        Migration { id: 3, name: "timezone".into(), apply: "ALTER TABLE users ADD COLUMN timezone TEXT; UPDATE users SET timezone = 'UTC';".into(), rollback: Some("ALTER TABLE users DROP COLUMN timezone;".into()) },
    ]
}

#[test]
fn migrations_preserve_history_across_targets_retries_and_restarts() {
    let directory = Database::new();
    let storage = directory.open();
    assert!(!directory.0.exists());
    assert!(storage.inspect().is_empty());
    let store = storage.application("first").unwrap();
    let migrations = migrations();
    store.migrate(&migrations, Some(1)).unwrap();
    store.migrate(&migrations, None).unwrap();
    store.migrate(&migrations, None).unwrap();
    assert_eq!(storage.inspect()[0].migration, 3);
    assert_eq!(
        store
            .collect_query(&statement("SELECT name, timezone FROM users"), deadline())
            .unwrap()
            .rows,
        vec![vec![
            SqlValue::Text("first".into()),
            SqlValue::Text("UTC".into())
        ]]
    );
    store.migrate(&migrations, Some(1)).unwrap();
    assert_eq!(storage.inspect()[0].migration, 1);
    assert_eq!(
        store
            .collect_query(&statement("SELECT * FROM users"), deadline())
            .unwrap()
            .columns,
        vec!["id", "name"]
    );
    let mut changed = migrations.clone();
    changed[1].rollback.as_mut().unwrap().push(' ');
    assert!(
        store
            .migrate(&changed, None)
            .unwrap_err()
            .to_string()
            .contains("differs")
    );
    changed = migrations.clone();
    changed[0].apply.push(' ');
    assert!(store.migrate(&changed, None).is_err());
    assert!(store.migrate(&migrations[..1], None).is_err());
    assert!(store.migrate(&migrations, Some(2)).is_err());
    let mut inserted = migrations.clone();
    inserted.insert(
        1,
        Migration {
            id: 2,
            name: "inserted".into(),
            apply: "SELECT 1;".into(),
            rollback: None,
        },
    );
    assert!(store.migrate(&inserted, None).is_err());
    store.migrate(&migrations, Some(0)).unwrap();
    assert!(
        store
            .collect_query(&statement("SELECT * FROM users"), deadline())
            .is_err()
    );
    drop(storage);
    assert!(
        directory.open().application("first").is_err(),
        "capability keeps ownership alive"
    );
    drop(store);
    let storage = directory.open();
    let store = storage.application("first").unwrap();
    assert_eq!(storage.inspect()[0].migration, 0);
    store.migrate(&migrations, None).unwrap();
    storage.application("second").unwrap();
    assert_eq!(storage.inspect()[0].migration, 3);
    drop((store, storage));
    let storage = directory.open();
    storage.application("third").unwrap();
    assert_eq!(
        storage
            .inspect()
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "second", "third"]
    );
}

#[test]
fn failed_transitions_and_irreversible_rollbacks_leave_schema_and_history_unchanged() {
    let directory = Database::new();
    let storage = directory.open();
    let store = storage.application("first").unwrap();
    let mut migration = migrations();
    migration[1]
        .apply
        .push_str(" INSERT INTO missing VALUES (1);");
    assert!(store.migrate(&migration, None).is_err());
    assert_eq!(storage.inspect()[0].migration, 0);
    assert!(
        store
            .collect_query(&statement("SELECT * FROM users"), deadline())
            .is_err()
    );
    let mut migration = migrations();
    migration[0].rollback = None;
    store.migrate(&migration, None).unwrap();
    assert!(
        store
            .migrate(&migration, Some(0))
            .unwrap_err()
            .to_string()
            .contains("no rollback")
    );
    assert_eq!(storage.inspect()[0].migration, 3);
    assert!(
        store
            .collect_query(&statement("SELECT timezone FROM users"), deadline())
            .is_ok()
    );

    let second = storage.application("second").unwrap();
    let mut migration = migrations();
    migration[0].rollback = Some("DROP TABLE absent;".into());
    second.migrate(&migration, None).unwrap();
    assert!(second.migrate(&migration, Some(0)).is_err());
    assert!(
        second
            .collect_query(&statement("SELECT timezone FROM users"), deadline())
            .is_ok(),
        "earlier rollback in transition is undone"
    );
}

#[test]
fn sql_values_atomic_batches_limits_deadlines_and_isolation() {
    let directory = Database::new();
    let storage = directory.open();
    let store = storage.application("first").unwrap();
    store.migrate(&migrations(), None).unwrap();
    let values = vec![
        SqlValue::Null,
        SqlValue::Integer(i64::MAX),
        SqlValue::Real(1.25),
        SqlValue::Text("日本語 ' ?".into()),
        SqlValue::Bytes(vec![0, 255]),
    ];
    let rows = store
        .collect_query(
            &SqlStatement {
                sql: "SELECT ?1, ?2, ?3, ?4, ?5".into(),
                parameters: values.clone(),
            },
            deadline(),
        )
        .unwrap();
    assert_eq!(rows.rows, vec![values]);
    let insert = statement("INSERT INTO users(id, name) VALUES (2, 'second')");
    assert_eq!(
        store.batch(&[insert.clone(), insert.clone()], deadline()),
        Err(SqlError::Constraint)
    );
    assert_eq!(
        store
            .collect_query(&statement("SELECT COUNT(*) FROM users"), deadline())
            .unwrap()
            .rows,
        vec![vec![SqlValue::Integer(1)]]
    );
    assert_eq!(store.execute(&insert, deadline()), Ok(1));
    assert!(
        storage
            .application("second")
            .unwrap()
            .collect_query(&statement("SELECT * FROM users"), deadline())
            .is_err()
    );
    assert_eq!(
        store.execute(
            &statement("DELETE FROM users; DELETE FROM users"),
            deadline()
        ),
        Err(SqlError::InvalidStatement)
    );
    assert_eq!(
        store.collect_query(&statement("DELETE FROM users RETURNING id"), deadline()),
        Err(SqlError::InvalidStatement)
    );
    assert_eq!(
        store.execute(&statement("SELECT 1"), deadline()),
        Err(SqlError::InvalidStatement)
    );
    assert_eq!(
        store.collect_query(&statement("SELECT ?1"), deadline()),
        Err(SqlError::InvalidStatement)
    );

    assert_eq!(
        store.collect_query(&statement("SELECT zeroblob(1048577)"), deadline()),
        Err(SqlError::Limit)
    );
    assert_eq!(store.collect_query(&statement("WITH RECURSIVE n(x) AS (VALUES(0) UNION ALL SELECT x+1 FROM n) SELECT SUM(x) FROM n"), Instant::now() + Duration::from_millis(10)), Err(SqlError::Timeout));
    assert_eq!(
        store
            .collect_query(&statement("SELECT 42"), deadline())
            .unwrap()
            .rows,
        vec![vec![SqlValue::Integer(42)]],
        "connection recovers after cancellation"
    );
}

#[test]
fn sql_and_migrations_cannot_escape_the_capability_or_change_history() {
    let directory = Database::new();
    let storage = directory.open();
    let store = storage.application("first").unwrap();
    store.migrate(&migrations(), None).unwrap();
    for sql in [
        "SELECT * FROM _wr_migrations",
        "DELETE FROM _wr_migrations",
        "DROP TABLE _wr_migrations",
        "ALTER TABLE _wr_migrations RENAME TO history",
        "CREATE INDEX stolen ON _wr_migrations(name)",
        "CREATE TRIGGER stolen AFTER INSERT ON _wr_migrations BEGIN SELECT 1; END",
        "ATTACH DATABASE ':memory:' AS other",
        "PRAGMA user_version = 10",
        "SELECT * FROM pragma_table_info('_wr_migrations')",
        "SELECT load_extension('missing')",
        "COMMIT",
        "ROLLBACK",
        "SAVEPOINT stolen",
        "CREATE VIRTUAL TABLE v USING fts5(value)",
    ] {
        assert!(
            store.execute(&statement(sql), deadline()).is_err(),
            "guest accepted {sql}"
        );
        assert!(
            store.collect_query(&statement(sql), deadline()).is_err(),
            "query accepted {sql}"
        );
        let mut candidate = migrations();
        candidate.push(Migration {
            id: 4,
            name: "forbidden".into(),
            apply: sql.into(),
            rollback: None,
        });
        assert!(
            store.migrate(&candidate, None).is_err(),
            "migration accepted {sql}"
        );
        assert_eq!(storage.inspect()[0].migration, 3);
    }
    assert_eq!(
        store.execute(
            &statement("CREATE TABLE guest_schema (id INTEGER)"),
            deadline()
        ),
        Err(SqlError::Denied)
    );
    assert!(
        store
            .collect_query(&statement("SELECT name FROM users"), deadline())
            .is_ok()
    );
}

#[test]
fn sql_refuses_missing_or_corrupt_owned_databases() {
    let directory = Database::new();
    let storage = directory.open();
    storage
        .application("first")
        .unwrap()
        .migrate(&migrations(), None)
        .unwrap();
    drop(storage);
    let database = directory.0.join("first/data.sqlite");
    let backup = fs::read(&database).unwrap();
    fs::remove_file(&database).unwrap();
    assert!(directory.open().application("first").is_err());
    assert!(!database.exists());
    fs::write(&database, b"not a database").unwrap();
    assert!(directory.open().application("first").is_err());
    assert_eq!(fs::read(&database).unwrap(), b"not a database");
    fs::write(&database, backup).unwrap();
    assert!(directory.open().application("first").is_ok());
}

#[test]
fn concurrent_sql_batches_and_migrations_wait_for_the_connection() {
    let directory = Database::new();
    let storage = directory.open();
    let store = storage.application("first").unwrap();
    store.migrate(&migrations(), Some(1)).unwrap();
    let start = std::sync::Barrier::new(9);
    std::thread::scope(|scope| {
        let mut workers = Vec::new();
        for _ in 0..8 {
            workers.push(scope.spawn(|| {
                start.wait();
                for _ in 0..10 {
                    let update = statement("UPDATE users SET name = name || 'x' WHERE id = 1");
                    assert_eq!(
                        store.batch(&[update.clone(), update], deadline()).unwrap(),
                        vec![1, 1]
                    );
                }
            }));
        }
        start.wait();
        store.migrate(&migrations(), None).unwrap();
        for worker in workers {
            worker.join().unwrap();
        }
    });
    let rows = store
        .collect_query(
            &statement("SELECT length(name), timezone FROM users WHERE id = 1"),
            deadline(),
        )
        .unwrap();
    assert_eq!(
        rows.rows,
        vec![vec![SqlValue::Integer(165), SqlValue::Text("UTC".into())]]
    );
}

#[test]
fn read_pool_waits_without_blocking_writes_and_releases_cursors() {
    let directory = Database::new();
    let storage = directory.open();
    let store = storage.application("first").unwrap();
    store.migrate(&migrations(), None).unwrap();
    let query = statement("SELECT name FROM users UNION ALL SELECT name FROM users");
    let mut cursors: Vec<_> = (0..MAX_READ_CONNECTIONS)
        .map(|_| store.query(&query, deadline()).ok().unwrap())
        .collect();
    assert_eq!(
        store
            .query(&query, Instant::now() + Duration::from_millis(20))
            .err(),
        Some(SqlError::Timeout)
    );
    assert_eq!(
        store.execute(&statement("UPDATE users SET name = 'new'"), deadline()),
        Ok(1)
    );
    assert_eq!(
        cursors[0].next_row().unwrap(),
        Some(vec![SqlValue::Text("first".into())])
    );
    std::thread::scope(|scope| {
        let waiting = scope.spawn(|| store.query(&query, deadline()));
        drop(cursors.pop());
        let mut next = waiting.join().unwrap().ok().unwrap();
        assert_eq!(
            next.next_row().unwrap(),
            Some(vec![SqlValue::Text("new".into())])
        );
        next.close();
        assert!(next.next_row().unwrap().is_none());
    });
    drop(cursors);
    let mut expired = store
        .query(&query, Instant::now() + Duration::from_millis(20))
        .ok()
        .unwrap();
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(expired.next_row(), Err(SqlError::Timeout));
    let mut cursors: Vec<_> = (0..MAX_READ_CONNECTIONS)
        .map(|_| store.query(&query, deadline()).ok().unwrap())
        .collect();
    while cursors[0].next_row().unwrap().is_some() {}
    assert!(
        store.query(&query, deadline()).is_ok(),
        "exhaustion returns the connection before dropping the cursor"
    );
}

#[test]
fn cursors_stream_large_results_keep_snapshots_and_survive_thread_moves() {
    let directory = Database::new();
    let storage = directory.open();
    let store = storage.application("first").unwrap();
    store.migrate(&migrations(), Some(1)).unwrap();
    let mut old = store
        .query(&statement("SELECT * FROM users"), deadline())
        .ok()
        .unwrap();
    store.migrate(&migrations(), None).unwrap();
    assert_eq!(old.columns(), &["id", "name"]);
    assert_eq!(
        old.next_row().unwrap(),
        Some(vec![SqlValue::Integer(1), SqlValue::Text("first".into())])
    );
    let mut cursor = store.query(&statement("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x < 5000) SELECT x, zeroblob(1024) FROM n"), deadline()).ok().unwrap();
    std::thread::scope(|scope| {
        scope
            .spawn(move || {
                for expected in 1..=5000 {
                    let row = cursor.next_row().unwrap().unwrap();
                    assert_eq!(row[0], SqlValue::Integer(expected));
                    assert_eq!(row[1], SqlValue::Bytes(vec![0; 1024]));
                }
                assert!(cursor.next_row().unwrap().is_none());
                assert!(cursor.next_row().unwrap().is_none());
            })
            .join()
            .unwrap();
    });
    assert!(
        store
            .query(&statement("SELECT timezone FROM users"), deadline())
            .is_ok()
    );
    let retained = store
        .query(&statement("SELECT 1 UNION ALL SELECT 2"), deadline())
        .ok()
        .unwrap();
    drop(old);
    drop(store);
    drop(storage);
    assert!(
        directory.open().application("first").is_err(),
        "a live cursor retains database ownership"
    );
    drop(retained);
    assert!(directory.open().application("first").is_ok());
}

#[test]
fn cursor_payloads_and_late_errors_release_read_connections() {
    let directory = Database::new();
    let storage = directory.open();
    let store = storage.application("first").unwrap();
    let values = vec![
        SqlValue::Text(String::new()),
        SqlValue::Bytes(Vec::new()),
        SqlValue::Text("a\0b".into()),
        SqlValue::Integer(i64::MIN),
    ];
    let mut cursor = store
        .query(
            &SqlStatement {
                sql: "; SELECT ?1, ?2, ?3, ?4; -- trailing comment".into(),
                parameters: values.clone(),
            },
            deadline(),
        )
        .ok()
        .unwrap();
    assert_eq!(cursor.next_row().unwrap(), Some(values));
    assert!(cursor.next_row().unwrap().is_none());
    for _ in 0..MAX_READ_CONNECTIONS + 1 {
        let mut bad = store
            .query(
                &statement("SELECT zeroblob(16) UNION ALL SELECT zeroblob(1048577)"),
                deadline(),
            )
            .ok()
            .unwrap();
        assert!(bad.next_row().unwrap().is_some());
        assert_eq!(bad.next_row(), Err(SqlError::Limit));
        assert!(bad.next_row().unwrap().is_none());
    }
    let _all: Vec<_> = (0..MAX_READ_CONNECTIONS)
        .map(|_| {
            store
                .query(&statement("SELECT 1 UNION ALL SELECT 2"), deadline())
                .ok()
                .unwrap()
        })
        .collect();
}
