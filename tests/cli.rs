use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

const EXAMPLE: &str = include_str!("fixtures/application.yaml");
const MOUNTED_APPLICATION: &str = include_str!("fixtures/mounted/application.yaml");
const SETUP_APPLICATION: &str = include_str!("fixtures/setup/application.yaml");
const GREET_FUNCTION: &str = include_str!("../examples/functions/greet.rs");
const ECHO_FUNCTION: &str = include_str!("../examples/functions/echo.rs");
const INSPECT_FUNCTION: &str = include_str!("../examples/functions/inspect.rs");
const HTTP_WORLD: &str = include_str!("../wit/world.wit");
const HTTP_KV_WORLD: &str = include_str!("fixtures/http-kv.wit");
const HTTP_ONLY_WORLD: &str = include_str!("fixtures/http-only.wit");
const SQL_GREETING_FUNCTION: &str = include_str!("../examples/functions/sql-greet.rs");
const SQL_FUNCTION: &str = include_str!("fixtures/sql.rs");
const SQL_APPLY: &str = include_str!("fixtures/0001-counters.apply.sql");
const SQL_ROLLBACK: &str = include_str!("fixtures/0001-counters.rollback.sql");
const KV_FUNCTION: &str = include_str!("fixtures/kv.rs");
const COUNTER_FUNCTION: &str = include_str!("../examples/functions/counter.rs");
const SHARED_LIBS: &str = include_str!("../examples/libs/mod.rs");
const SHARED_RESPONSES: &str = include_str!("../examples/libs/responses.rs");
const SHARED_GREETINGS: &str = include_str!("../examples/libs/greetings.rs");
const SHARED_VIEWS: &str = include_str!("../examples/libs/views.rs");
const HOME_FUNCTION: &str = include_str!("../examples/functions/home.rs");
const BASE_TEMPLATE: &str = include_str!("../examples/templates/base.html");
const HOME_TEMPLATE: &str = include_str!("../examples/templates/home.html");
const GREETINGS_MIGRATION: &str = include_str!("../examples/migrations/0001-greetings.apply.sql");

#[test]
fn askama_renders_sql_data_without_template_files_at_runtime() {
    use std::time::{Duration, Instant};
    use wasm_runner::{
        persistence::sql::{Migration, SqlStatement, SqlStorage, SqlValue},
        runtime::{Capabilities, Runtime},
    };

    let component = compiled_component("home");
    let fixture = ManifestFixture::new(EXAMPLE);
    let storage = SqlStorage::new(fs::canonicalize(&fixture.directory).unwrap().join("sql"));
    let store = storage.application("hello").unwrap();
    store
        .migrate(
            &[Migration {
                id: 1,
                name: "greetings".into(),
                apply: GREETINGS_MIGRATION.into(),
                rollback: None,
            }],
            None,
        )
        .unwrap();
    let mut runtime = Runtime::with_capabilities(
        Duration::from_secs(2),
        Capabilities {
            kv: None,
            sql: Some(store.clone()),
        },
    )
    .unwrap();
    let id = runtime.register(component).unwrap();
    let render = || {
        let response = runtime.execute(&id, &guest_request("/")).unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(
            response.headers,
            vec![("content-type".into(), "text/html; charset=utf-8".into())]
        );
        String::from_utf8(response.body).unwrap()
    };

    let body = render();
    assert!(body.starts_with("<!doctype html>"));
    assert!(body.contains("<title>Greetings · WASM Runner</title>"));
    assert!(body.contains("Hello from SQL!"));
    assert!(body.contains("<footer>"));
    let untrusted = "<script>alert(\"untrusted\")</script>&";
    store
        .execute(
            &SqlStatement {
                sql: "INSERT INTO greetings (id, message) VALUES (2, ?1)".into(),
                parameters: vec![SqlValue::Text(untrusted.into())],
            },
            Instant::now() + Duration::from_secs(2),
        )
        .unwrap();
    let body = render();
    assert!(body.contains("Hello from SQL!"));
    assert!(body.contains("untrusted"));
    assert!(!body.contains("<script>"));
    assert!(!body.contains("</script>"));
    assert!(!body.contains("\"untrusted\""));

    store
        .execute(
            &SqlStatement {
                sql: "DELETE FROM greetings".into(),
                parameters: vec![],
            },
            Instant::now() + Duration::from_secs(2),
        )
        .unwrap();
    assert!(render().contains("No greetings yet."));

    let mut disabled = Runtime::new(Duration::from_secs(2)).unwrap();
    let id = disabled.register(component).unwrap();
    let response = disabled.execute(&id, &guest_request("/")).unwrap();
    assert_eq!(response.status, 503);
    assert_eq!(response.body, b"Unable to load greetings");
}

#[test]
fn shared_libraries_run_in_multiple_components() {
    use std::time::Duration;
    use wasm_runner::{persistence::Storage, runtime::Runtime};

    let fixture = ManifestFixture::new(EXAMPLE);
    let root = fs::canonicalize(&fixture.directory)
        .unwrap()
        .join(".wr-data/kv");
    let storage = Storage::new(root);
    let mut runtime = Runtime::with_store(
        Duration::from_secs(2),
        storage.application("hello").unwrap(),
    )
    .unwrap();
    for (name, status, body) in [
        ("counter", 200, "1"),
        ("sql-greet", 500, "Unable to read greeting"),
    ] {
        let id = runtime.register(compiled_component(name)).unwrap();
        let response = runtime.execute(&id, &guest_request("/")).unwrap();
        assert_eq!(response.status, status);
        assert_eq!(response.body, body.as_bytes());
        assert_eq!(
            response.headers,
            vec![("content-type".into(), "text/plain; charset=utf-8".into())]
        );
    }
}

fn guest_request(path: &str) -> wasm_runner::runtime::Request {
    wasm_runner::runtime::Request {
        method: "GET".into(),
        path: path.into(),
        path_params: vec![],
        query: vec![],
        headers: vec![],
        body: vec![],
    }
}

#[test]
fn runtime_kv_isolates_calls_and_preserves_accepted_writes() {
    use std::{sync::Arc, time::Duration};
    use wasm_runner::{
        persistence::{KvValue, Storage},
        runtime::{ExecutionError, Runtime},
    };
    let fixture = ManifestFixture::new(EXAMPLE);
    let directory = &fixture.directory;
    let component = compiled_component("kv-test");
    let mut disabled = Runtime::new(Duration::from_secs(2)).unwrap();
    let id = disabled.register(component).unwrap();
    assert_eq!(
        disabled
            .execute(&id, &guest_request("/disabled"))
            .unwrap()
            .body,
        b"disabled"
    );
    assert!(!directory.join(".wr-data").exists());
    let root = fs::canonicalize(directory).unwrap().join(".wr-data/kv");
    let storage = Storage::new(&root);
    let store = storage.application("hello").unwrap();
    let mut runtime = Runtime::with_store(Duration::from_secs(2), store.clone()).unwrap();
    let id = runtime.register(component).unwrap();
    assert_eq!(
        runtime.execute(&id, &guest_request("/check")).unwrap().body,
        b"ok"
    );
    assert!(matches!(
        runtime.execute(&id, &guest_request("/trap")),
        Err(ExecutionError::Guest(_))
    ));
    assert_eq!(store.get("trap").unwrap(), Some(KvValue::Integer(1)));
    let mut short = Runtime::with_store(Duration::from_millis(100), store.clone()).unwrap();
    let short_id = short.register(component).unwrap();
    assert!(matches!(
        short.execute(&short_id, &guest_request("/timeout")),
        Err(ExecutionError::Timeout)
    ));
    assert_eq!(store.get("timeout").unwrap(), Some(KvValue::Integer(1)));
    // Both old and replacement runtimes retain the same capability.
    let mut replacement = Runtime::with_store(
        Duration::from_secs(2),
        storage.application("hello").unwrap(),
    )
    .unwrap();
    let replacement_id = replacement.register(component).unwrap();
    let runtime = Arc::new(runtime);
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let runtime = runtime.clone();
            let id = id.clone();
            std::thread::spawn(move || {
                String::from_utf8(runtime.execute(&id, &guest_request("/")).unwrap().body)
                    .unwrap()
                    .parse::<u64>()
                    .unwrap()
            })
        })
        .collect();
    let mut counts: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    counts.sort_unstable();
    assert_eq!(counts, (1..=8).collect::<Vec<_>>());
    assert_eq!(
        replacement
            .execute(&replacement_id, &guest_request("/"))
            .unwrap()
            .body,
        b"9"
    );
    storage.sync().unwrap();
    drop((runtime, replacement, short, store, storage));
    let storage = Storage::new(&root);
    assert_eq!(
        storage.application("hello").unwrap().get("count").unwrap(),
        Some(KvValue::Integer(9))
    );
    drop(storage);
}

#[test]
fn runtime_accepts_pre_sql_components() {
    use std::time::Duration;
    use wasm_runner::runtime::Runtime;

    for (world, source, request, expected) in [
        (HTTP_ONLY_WORLD, GREET_FUNCTION, "/", "Hello, world!"),
        (HTTP_KV_WORLD, KV_FUNCTION, "/disabled", "disabled"),
    ] {
        let fixture = ManifestFixture::new(EXAMPLE);
        let directory = &fixture.directory;
        let output = wr(&["update", directory.to_str().unwrap()]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::write(directory.join(".wr/wit/world.wit"), world).unwrap();
        fs::write(directory.join("functions/greet.rs"), source).unwrap();
        let entry = directory.join(".wr/functions/greet/lib.rs");
        let mut original = fs::read_to_string(&entry)
            .unwrap()
            .replace("pub use local::sql::database as sql;\n", "");
        if world == HTTP_ONLY_WORLD {
            original = original.replace("pub use local::kv::store as kv;\n", "");
        }
        fs::write(entry, original).unwrap();
        let output = Command::new("cargo")
            .current_dir(directory)
            .args([
                "build",
                "--locked",
                "--target",
                "wasm32-wasip2",
                "-p",
                "wr-function-greet",
            ])
            .env("CARGO_TARGET_DIR", directory.join("target"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let old =
            fs::read(directory.join("target/wasm32-wasip2/debug/wr_function_greet.wasm")).unwrap();
        let mut runtime = Runtime::new(Duration::from_secs(2)).unwrap();
        let id = runtime.register(&old).unwrap();
        assert_eq!(
            runtime.execute(&id, &guest_request(request)).unwrap().body,
            expected.as_bytes()
        );
    }
}

#[cfg(unix)]
#[test]
fn foreground_kv_survives_restart_and_clean() {
    let application = ManifestFixture::new(
        "name: first\npath-prefix: /moved\nkv: true\nfunctions:\n  counter:\n    http: {method: GET, path: '/<path..>'}\n",
    );
    application.write_component("counter", compiled_component("kv-test"));
    assert_eq!(foreground_counter(&application), "1");
    assert_eq!(foreground_counter(&application), "2");
    let preserved = application.directory.join(".wr-data/keep");
    fs::create_dir_all(preserved.parent().unwrap()).unwrap();
    fs::write(&preserved, "data").unwrap();
    assert!(
        wr(&["clean", application.directory.to_str().unwrap()])
            .status
            .success()
    );
    assert_eq!(fs::read_to_string(preserved).unwrap(), "data");
}

#[cfg(unix)]
#[test]
fn runtime_sql_checks_capabilities_and_preserves_committed_writes() {
    use std::time::Duration;
    use wasm_runner::{
        persistence::sql::{Migration, SqlStorage},
        runtime::{Capabilities, ExecutionError, Runtime},
    };
    let component = compiled_component("sql-test");
    let fixture = ManifestFixture::new(EXAMPLE);
    let storage = SqlStorage::new(
        fs::canonicalize(&fixture.directory)
            .unwrap()
            .join(".wr-data/sql"),
    );
    let store = storage.application("first").unwrap();
    store
        .migrate(
            &[Migration {
                id: 1,
                name: "counters".into(),
                apply: SQL_APPLY.into(),
                rollback: Some(SQL_ROLLBACK.into()),
            }],
            None,
        )
        .unwrap();
    let mut disabled = Runtime::new(Duration::from_secs(2)).unwrap();
    let id = disabled.register(component).unwrap();
    assert_eq!(
        disabled
            .execute(&id, &guest_request("/disabled"))
            .unwrap()
            .body,
        b"disabled"
    );
    let mut runtime = Runtime::with_capabilities(
        Duration::from_millis(500),
        Capabilities {
            kv: None,
            sql: Some(store.clone()),
        },
    )
    .unwrap();
    let id = runtime.register(component).unwrap();
    let held_readers: Vec<_> = (0..15)
        .map(|_| {
            store
                .query(
                    &wasm_runner::persistence::sql::SqlStatement {
                        sql: "SELECT 1 UNION ALL SELECT 2".into(),
                        parameters: vec![],
                    },
                    std::time::Instant::now() + Duration::from_secs(10),
                )
                .ok()
                .unwrap()
        })
        .collect();
    assert_eq!(
        runtime
            .execute(&id, &guest_request("/nested-limit"))
            .unwrap()
            .body,
        b"ok"
    );
    drop(held_readers);

    assert_eq!(
        runtime.execute(&id, &guest_request("/check")).unwrap().body,
        b"ok"
    );
    assert_eq!(
        runtime.execute(&id, &guest_request("/scan")).unwrap().body,
        b"5000"
    );
    for _ in 0..5 {
        assert_eq!(
            runtime
                .execute(&id, &guest_request("/cursor-limit"))
                .unwrap()
                .body,
            b"ok"
        );
        assert!(matches!(
            runtime.execute(&id, &guest_request("/cursor-trap")),
            Err(ExecutionError::Guest(_))
        ));
    }
    assert!(matches!(
        runtime.execute(&id, &guest_request("/cursor-timeout")),
        Err(ExecutionError::Timeout)
    ));
    assert_eq!(
        runtime
            .execute(&id, &guest_request("/cursor-limit"))
            .unwrap()
            .body,
        b"ok"
    );
    assert_eq!(
        runtime
            .execute(&id, &guest_request("/cursor-write"))
            .unwrap()
            .body,
        b"0"
    );
    assert!(matches!(
        runtime.execute(&id, &guest_request("/trap")),
        Err(ExecutionError::Guest(_))
    ));
    assert_eq!(
        runtime.execute(&id, &guest_request("/read")).unwrap().body,
        b"2"
    );
    assert!(matches!(
        runtime.execute(&id, &guest_request("/timeout")),
        Err(ExecutionError::Timeout)
    ));
    assert_eq!(
        runtime.execute(&id, &guest_request("/read")).unwrap().body,
        b"2"
    );
}

fn write_sql_migrations(fixture: &ManifestFixture) {
    let directory = fixture.directory.join("migrations");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("0001-counters.apply.sql"), SQL_APPLY).unwrap();
    fs::write(directory.join("0001-counters.rollback.sql"), SQL_ROLLBACK).unwrap();
}

#[test]
fn migration_packaging_rejects_invalid_files_without_opening_storage() {
    let source = format!("{EXAMPLE}\nsql: {{enabled: true}}\n");
    for (filename, contents) in [
        ("1-short.apply.sql", "SELECT 1;"),
        ("0002-orphan.rollback.sql", "SELECT 1;"),
        ("0001-different.rollback.sql", "SELECT 1;"),
        ("00001-counters.apply.sql", "SELECT 1;"),
        ("0002-empty.apply.sql", "  \n"),
        ("readme.txt", "SELECT 1;"),
    ] {
        let fixture = ManifestFixture::new(&source);
        for name in ["greet", "echo"] {
            fixture.write_component(name, b"not reached");
        }
        write_sql_migrations(&fixture);
        fs::write(
            fixture.directory.join("migrations").join(filename),
            contents,
        )
        .unwrap();
        let output = wr(&[
            "run",
            fixture.directory.to_str().unwrap(),
            "--listen",
            "127.0.0.1:0",
        ]);
        assert!(!output.status.success(), "accepted {filename}");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            !error.contains("registering handler"),
            "packaging must fail before component preparation: {error}"
        );
        assert!(!fixture.directory.join(".wr-data").exists());
    }
}

#[cfg(unix)]
#[test]
fn foreground_sql_migrates_and_preserves_data_across_restart_and_clean() {
    let application = ManifestFixture::new(
        "name: first\npath-prefix: /moved\nsql: {enabled: true}\nfunctions:\n  counter:\n    http: {method: GET, path: '/<path..>'}\n",
    );
    application.write_component("counter", compiled_component("sql-test"));
    write_sql_migrations(&application);
    assert_eq!(foreground_counter(&application), "1");
    assert_eq!(foreground_counter(&application), "2");
    assert!(
        wr(&["clean", application.directory.to_str().unwrap()])
            .status
            .success()
    );
    assert!(
        application
            .directory
            .join(".wr-data/sql/first/data.sqlite")
            .exists()
    );
    assert!(
        application
            .directory
            .join("migrations/0001-counters.apply.sql")
            .exists()
    );
}

#[cfg(unix)]
fn foreground_counter(application: &ManifestFixture) -> String {
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Stdio},
        time::Duration,
    };
    struct Running(Child);
    impl Drop for Running {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut running = Running(
        Command::new(env!("CARGO_BIN_EXE_wr"))
            .arg("run")
            .arg(&application.directory)
            .args(["--listen", "127.0.0.1:0"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let stdout = running.0.stdout.take().unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(address) = line.strip_prefix("Listening on http://") {
                let _ = send.send(address.to_owned());
            }
        }
    });
    let address = receive
        .recv_timeout(Duration::from_secs(30))
        .expect("foreground KV readiness");
    let response = http(&address, "GET", "/moved/");
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(
        Command::new("kill")
            .args(["-TERM", &running.0.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    assert!(running.0.wait().unwrap().success());
    reader.join().unwrap();
    response.split("\r\n\r\n").nth(1).unwrap().to_owned()
}

struct ManifestFixture {
    directory: PathBuf,
    path: PathBuf,
}

impl ManifestFixture {
    fn new(source: &str) -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let directory = std::env::temp_dir().join(format!(
            "wr cli {} {}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let functions = directory.join("functions");
        fs::create_dir(&functions).unwrap();
        if let Ok(value) = yaml_serde::from_str::<yaml_serde::Value>(source)
            && let Some(entries) = value
                .get("functions")
                .and_then(yaml_serde::Value::as_mapping)
        {
            for name in entries.keys().filter_map(yaml_serde::Value::as_str) {
                if !name.is_empty()
                    && name.len() <= 64
                    && name.bytes().all(|character| {
                        character.is_ascii_alphanumeric() || matches!(character, b'-' | b'_')
                    })
                {
                    fs::write(functions.join(format!("{name}.rs")), GREET_FUNCTION).unwrap();
                }
            }
        }
        let path = directory.join("application.yaml");
        fs::write(&path, source).unwrap();
        Self { directory, path }
    }

    fn write_shared_libraries(&self) {
        fs::create_dir_all(self.directory.join("libs")).unwrap();
        fs::create_dir_all(self.directory.join("templates")).unwrap();
        for (path, contents) in [
            ("libs/mod.rs", SHARED_LIBS),
            ("libs/responses.rs", SHARED_RESPONSES),
            ("libs/greetings.rs", SHARED_GREETINGS),
            ("libs/views.rs", SHARED_VIEWS),
            ("templates/base.html", BASE_TEMPLATE),
            ("templates/home.html", HOME_TEMPLATE),
        ] {
            fs::write(self.directory.join(path), contents).unwrap();
        }
    }

    #[cfg(unix)]
    fn write_component(&self, name: &str, bytes: &[u8]) {
        let directory = self.directory.join(".wr/components");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join(format!("{name}.wasm")), bytes).unwrap();
    }

    fn run(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_wr"))
            .current_dir(self.directory.parent().unwrap())
            .arg("run")
            .arg(&self.directory)
            .args(arguments)
            .output()
            .expect("wr should launch")
    }

    fn build(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_wr"))
            .current_dir(self.directory.parent().unwrap())
            .arg("build")
            .arg(&self.directory)
            .args(arguments)
            .env("CARGO_TARGET_DIR", self.directory.join("ignored-target"))
            .output()
            .expect("wr should launch")
    }
}

impl Drop for ManifestFixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.directory).unwrap();
    }
}

fn source_application() -> ManifestFixture {
    let fixture = ManifestFixture::new(&format!(
        "{EXAMPLE}\ndependencies:\n  serde_json: '1.0'\n  askama: {{version: '0.16', default-features: false, features: [derive, alloc, config]}}\n"
    ));
    fixture.write_shared_libraries();
    fs::write(fixture.directory.join("functions/home.rs"), HOME_FUNCTION).unwrap();
    fs::write(
        fixture.directory.join("functions/counter.rs"),
        COUNTER_FUNCTION,
    )
    .unwrap();
    fs::write(fixture.directory.join("functions/echo.rs"), ECHO_FUNCTION).unwrap();
    fs::write(
        fixture.directory.join("functions/inspect.rs"),
        INSPECT_FUNCTION,
    )
    .unwrap();
    fs::write(
        fixture.directory.join("functions/unlisted-target.rs"),
        GREET_FUNCTION,
    )
    .unwrap();
    fs::write(fixture.directory.join("functions/kv-test.rs"), KV_FUNCTION).unwrap();
    fs::write(
        fixture.directory.join("functions/sql-test.rs"),
        SQL_FUNCTION,
    )
    .unwrap();
    fs::write(
        fixture.directory.join("functions/sql-greet.rs"),
        SQL_GREETING_FUNCTION,
    )
    .unwrap();

    fixture
}

#[cfg(unix)]
fn mounted_application(source: &str) -> ManifestFixture {
    let application = ManifestFixture::new(source);
    for (name, component) in [
        ("greet", "greet"),
        ("root", "greet"),
        ("inspect", "inspect"),
        ("files", "inspect"),
        ("echo", "echo"),
    ] {
        application.write_component(name, compiled_component(component));
    }
    application
}

fn compiled_component(name: &str) -> &'static [u8] {
    use std::{collections::BTreeMap, sync::OnceLock};

    static COMPONENTS: OnceLock<BTreeMap<String, Vec<u8>>> = OnceLock::new();
    let components = COMPONENTS.get_or_init(|| {
        let fixture = source_application();
        let output = fixture.build(&["--unlocked"]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        [
            "greet",
            "echo",
            "inspect",
            "kv-test",
            "sql-test",
            "counter",
            "sql-greet",
            "home",
        ]
        .into_iter()
        .map(|name| {
            let bytes = fs::read(
                fixture
                    .directory
                    .join(format!(".wr/components/{name}.wasm")),
            )
            .unwrap();
            (name.to_owned(), bytes)
        })
        .collect()
    });
    &components[name]
}

fn wr(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_wr"))
        .args(arguments)
        .output()
        .expect("wr should launch")
}

#[test]
fn help_and_version_succeed() {
    for arguments in [["--help"], ["--version"]] {
        let output = wr(&arguments);
        assert!(output.status.success());
        assert!(!output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }

    let output = wr(&["run", "--help"]);
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("<PATH>"));
    assert!(help.contains("--listen"));
    assert!(help.contains("--port"));
}

#[test]
fn invalid_invocations_are_rejected() {
    for arguments in [
        vec![],
        vec!["run"],
        vec!["setup"],
        vec!["build"],
        vec!["update"],
        vec!["clean"],
        vec!["run", "application.yaml", "--listen"],
        vec!["run", "application.yaml", "--listen", "localhost:9000"],
        vec!["run", "application.yaml", "--listen", "127.0.0.1:65536"],
        vec!["run", "application.yaml", "--unknown"],
    ] {
        let output = wr(&arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(!output.stderr.is_empty());
    }
}

#[test]
fn setup_previews_workspace_without_writing_files() {
    let fixture = ManifestFixture::new(SETUP_APPLICATION);
    let output = wr(&["setup", fixture.directory.to_str().unwrap(), "--dry-run"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let preview = String::from_utf8(output.stdout).unwrap();
    for expected in [
        ".wr/functions/echo/Cargo.toml",
        ".wr/functions/greet/lib.rs",
        ".wr/functions/greet/askama.toml",
        ".wr/wit/world.wit",
        ".vscode/settings.json",
        "${workspaceFolder}/functions",
        "workspace = true",
        "export!(Component)",
    ] {
        assert!(preview.contains(expected), "{preview}");
    }
    assert_eq!(fs::read_dir(&fixture.directory).unwrap().count(), 2);
    assert!(!preview.contains("pub mod libs;"));

    let output = fixture.run(&[]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("wr build"));
}

#[test]
fn setup_writes_the_previewed_files_beside_the_manifest_and_can_be_repeated() {
    let expected_settings = r#"{
  "rust-analyzer.vfs.extraIncludes": ["${workspaceFolder}/functions", "${workspaceFolder}/libs"]
}
"#;
    let fixture = ManifestFixture::new(SETUP_APPLICATION);
    fixture.write_shared_libraries();
    let preview = wr(&["setup", fixture.directory.to_str().unwrap(), "--dry-run"]);
    assert!(preview.status.success());
    let preview = String::from_utf8(preview.stdout).unwrap();
    for _ in 0..2 {
        let output = wr(&["setup", fixture.directory.to_str().unwrap()]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());

        for file in preview.split("\n=== ").skip(1) {
            let (path, contents) = file.split_once(" ===\n").unwrap();
            let expected = contents.strip_suffix('\n').unwrap();
            assert_eq!(
                fs::read_to_string(fixture.directory.join(path)).unwrap(),
                expected,
                "{path}"
            );
        }
        assert_eq!(
            fs::read_to_string(fixture.directory.join(".vscode/settings.json")).unwrap(),
            expected_settings
        );
    }
}

#[test]
fn setup_refreshes_optional_shared_library_entry_point() {
    let fixture = ManifestFixture::new(SETUP_APPLICATION);
    let directory = &fixture.directory;
    fs::create_dir(directory.join("libs")).unwrap();
    fs::write(directory.join("libs/unused.rs"), "not a Rust module").unwrap();
    let entry = directory.join(".wr/functions/greet/lib.rs");

    for has_libs in [false, true, false] {
        if has_libs {
            fixture.write_shared_libraries();
        } else if directory.join("libs/mod.rs").exists() {
            fs::remove_file(directory.join("libs/mod.rs")).unwrap();
        }
        let output = wr(&["setup", directory.to_str().unwrap()]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read_to_string(&entry)
                .unwrap()
                .contains("pub mod libs;"),
            has_libs
        );
        assert_eq!(
            fs::read_to_string(directory.join("libs/unused.rs")).unwrap(),
            "not a Rust module"
        );
    }

    let previous = fs::read(&entry).unwrap();
    fs::create_dir(directory.join("libs/mod.rs")).unwrap();
    let output = wr(&["setup", directory.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("libs/mod.rs"));
    assert_eq!(fs::read(entry).unwrap(), previous);
}

#[test]
fn setup_preserves_existing_ignore_rules_and_appends_only_missing_entries() {
    let fixture = ManifestFixture::new(SETUP_APPLICATION);
    let path = fixture.directory.join(".gitignore");
    let existing = "# Local files\r\n.env\r\n/target/";
    let expected = "# Local files\r\n.env\r\n/target/\n.wr/\n/Cargo.toml\n/.wr-data/\n";
    fs::write(&path, existing).unwrap();

    let output = wr(&["setup", fixture.directory.to_str().unwrap(), "--dry-run"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains(&format!("=== .gitignore ===\n{expected}\n"))
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), existing);
    assert_eq!(fs::read_dir(&fixture.directory).unwrap().count(), 3);

    for _ in 0..2 {
        let output = wr(&["setup", fixture.directory.to_str().unwrap()]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), expected);
    }
}

#[test]
fn setup_merges_editor_settings_without_losing_existing_content() {
    let input_without_includes = r#"{
    // Editor preferences
    "editor.tabSize": 4,
}
"#;
    let expected_with_includes = r#"{
    // Editor preferences
    "editor.tabSize": 4,
    "rust-analyzer.vfs.extraIncludes": ["${workspaceFolder}/functions"],
}
"#;

    let input_with_shared_include = r#"{
    /* Extra source trees */
    "rust-analyzer.vfs.extraIncludes": [
        "${workspaceFolder}/shared", // Keep this path
    ],
    "editor.tabSize": 4,
}
"#;
    let expected_with_both_includes = r#"{
    /* Extra source trees */
    "rust-analyzer.vfs.extraIncludes": [
        "${workspaceFolder}/shared", // Keep this path
        "${workspaceFolder}/functions",
    ],
    "editor.tabSize": 4,
}
"#;

    for (input, expected) in [
        (input_without_includes, expected_with_includes),
        (input_with_shared_include, expected_with_both_includes),
        (expected_with_both_includes, expected_with_both_includes),
    ] {
        let fixture = ManifestFixture::new(SETUP_APPLICATION);
        fs::create_dir(fixture.directory.join(".vscode")).unwrap();
        let path = fixture.directory.join(".vscode/settings.json");
        fs::write(&path, input).unwrap();
        let preview = wr(&["setup", fixture.directory.to_str().unwrap(), "--dry-run"]);
        assert!(
            preview.status.success(),
            "{}",
            String::from_utf8_lossy(&preview.stderr)
        );
        assert!(
            String::from_utf8(preview.stdout)
                .unwrap()
                .contains(expected)
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), input);
        assert!(!fixture.directory.join("Cargo.toml").exists());

        for _ in 0..2 {
            let output = wr(&["setup", fixture.directory.to_str().unwrap()]);
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), expected);
        }
    }
}

#[test]
fn setup_rejects_invalid_editor_settings_before_writing_files() {
    for existing in [
        "{ invalid",
        "[]",
        r#"{
    "rust-analyzer.vfs.extraIncludes": "functions"
}"#,
        r#"{
    "rust-analyzer.vfs.extraIncludes": [123]
}"#,
    ] {
        let fixture = ManifestFixture::new(SETUP_APPLICATION);
        fs::create_dir(fixture.directory.join(".vscode")).unwrap();
        let path = fixture.directory.join(".vscode/settings.json");
        fs::write(&path, existing).unwrap();
        for arguments in [vec!["--dry-run"], vec![]] {
            let mut command = vec!["setup", fixture.directory.to_str().unwrap()];
            command.extend(arguments);
            let output = wr(&command);
            assert_eq!(output.status.code(), Some(1));
            assert!(output.stdout.is_empty());
            assert!(String::from_utf8_lossy(&output.stderr).contains(".vscode/settings.json"));
            assert_eq!(fs::read_to_string(&path).unwrap(), existing);
            assert_eq!(fs::read_dir(&fixture.directory).unwrap().count(), 3);
        }
    }
}

#[test]
fn setup_preserves_an_existing_handwritten_cargo_manifest() {
    let fixture = ManifestFixture::new(SETUP_APPLICATION);
    let path = fixture.directory.join("Cargo.toml");
    let handwritten = "[workspace]\nmembers = []\n";
    fs::write(&path, handwritten).unwrap();

    let output = wr(&["setup", fixture.directory.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("refusing to overwrite"));
    assert_eq!(fs::read_to_string(path).unwrap(), handwritten);
    assert!(!fixture.directory.join(".wr").exists());
    assert!(!fixture.directory.join(".gitignore").exists());
}

#[test]
fn setup_rejects_invalid_dependencies_without_partial_output() {
    let source = SETUP_APPLICATION;
    for invalid in [
        source.replace("anyhow: \"1.0\"", "anyhow: 1.0"),
        source.replace(
            "anyhow: \"1.0\"",
            "anyhow: {version: '1.0', features: derive}",
        ),
        source.replace("anyhow: \"1.0\"", "anyhow: {version: '1.0', typo: true}"),
        source.replace("serde_json:", "anyhow:"),
        source.replace("serde_json:", "wit-bindgen:"),
    ] {
        let fixture = ManifestFixture::new(&invalid);
        let output = wr(&["setup", fixture.directory.to_str().unwrap()]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
        assert_eq!(fs::read_dir(&fixture.directory).unwrap().count(), 2);
    }
}

#[test]
fn clean_removes_generated_files_and_preserves_user_files() {
    let fixture = ManifestFixture::new(SETUP_APPLICATION);
    fixture.write_shared_libraries();
    let output = wr(&["setup", fixture.directory.to_str().unwrap()]);
    assert!(output.status.success());
    fs::create_dir_all(fixture.directory.join("target/wasm32-wasip2/debug")).unwrap();
    fs::write(
        fixture
            .directory
            .join("target/wasm32-wasip2/debug/greet.wasm"),
        b"build output",
    )
    .unwrap();
    fs::write(
        fixture.directory.join(".wr/obsolete.wasm"),
        b"old component",
    )
    .unwrap();
    fs::write(
        fixture.directory.join("Cargo.lock"),
        "application lockfile\n",
    )
    .unwrap();
    let preserved: Vec<_> = [
        "application.yaml",
        "functions/greet.rs",
        "functions/echo.rs",
        "libs/mod.rs",
        "libs/responses.rs",
        "libs/greetings.rs",
        "libs/views.rs",
        "templates/base.html",
        "templates/home.html",
        "Cargo.lock",
        ".gitignore",
        ".vscode/settings.json",
    ]
    .into_iter()
    .map(|name| (name, fs::read(fixture.directory.join(name)).unwrap()))
    .collect();

    for _ in 0..2 {
        let output = wr(&["clean", fixture.directory.to_str().unwrap()]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        for name in [".wr", "target", "Cargo.toml"] {
            assert!(!fixture.directory.join(name).exists(), "{name}");
        }
        for (name, contents) in &preserved {
            assert_eq!(
                fs::read(fixture.directory.join(name)).unwrap(),
                *contents,
                "{name}"
            );
        }
    }

    let output = wr(&["setup", fixture.directory.to_str().unwrap()]);
    assert!(output.status.success());
    assert!(
        fixture
            .directory
            .join(".wr/functions/greet/lib.rs")
            .is_file()
    );
}

#[test]
fn clean_refuses_a_handwritten_cargo_manifest_before_removing_anything() {
    let handwritten = r#"[workspace]
members = []
"#;
    let fixture = ManifestFixture::new(EXAMPLE);
    fs::write(fixture.directory.join("Cargo.toml"), handwritten).unwrap();
    for name in [".wr", "target"] {
        fs::create_dir(fixture.directory.join(name)).unwrap();
        fs::write(fixture.directory.join(name).join("keep.txt"), "keep\n").unwrap();
    }

    let output = wr(&["clean", fixture.directory.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("refusing to remove"));
    assert_eq!(
        fs::read_to_string(fixture.directory.join("Cargo.toml")).unwrap(),
        handwritten
    );
    for name in [".wr", "target"] {
        assert_eq!(
            fs::read_to_string(fixture.directory.join(name).join("keep.txt")).unwrap(),
            "keep\n"
        );
    }
}

#[test]
fn clean_does_not_require_a_valid_manifest_or_function_sources() {
    for manifest in [Some("invalid: ["), None] {
        let fixture = ManifestFixture::new(EXAMPLE);
        let output = wr(&["setup", fixture.directory.to_str().unwrap()]);
        assert!(output.status.success());
        match manifest {
            Some(contents) => fs::write(&fixture.path, contents).unwrap(),
            None => fs::remove_file(&fixture.path).unwrap(),
        }
        fs::remove_dir_all(fixture.directory.join("functions")).unwrap();

        let output = wr(&["clean", fixture.directory.to_str().unwrap()]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!fixture.directory.join(".wr").exists());
        assert!(!fixture.directory.join("Cargo.toml").exists());
        assert_eq!(fs::read_to_string(&fixture.path).ok().as_deref(), manifest);
    }
}

#[cfg(unix)]
#[test]
fn clean_removes_directory_symlinks_without_following_them() {
    let fixture = ManifestFixture::new(EXAMPLE);
    let outside = ManifestFixture::new(EXAMPLE);
    for name in [".wr", "target"] {
        std::os::unix::fs::symlink(&outside.directory, fixture.directory.join(name)).unwrap();
    }

    let output = wr(&["clean", fixture.directory.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_to_string(&outside.path).unwrap(), EXAMPLE);
    assert_eq!(
        fs::read_to_string(outside.directory.join("functions/greet.rs")).unwrap(),
        GREET_FUNCTION
    );
    for name in [".wr", "target"] {
        assert!(fs::symlink_metadata(fixture.directory.join(name)).is_err());
    }
}

#[test]
fn run_requires_built_compatible_components_before_readiness() {
    let fixture = ManifestFixture::new(EXAMPLE);
    let output = fixture.run(&[]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("wr build"));
    assert!(error.contains(".wasm"));
    fs::create_dir_all(fixture.directory.join(".wr/components")).unwrap();
    for name in ["greet", "echo"] {
        fs::write(
            fixture
                .directory
                .join(format!(".wr/components/{name}.wasm")),
            b"invalid component",
        )
        .unwrap();
    }
    let output = fixture.run(&[]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("registering handler"));
}

#[test]
fn setup_accepts_distinct_routes_and_supported_methods() {
    for method in ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"] {
        let source = format!(
            "name: routes\npath-prefix: /\nfunctions:\n  get:\n    http: {{method: {method}, path: '/users/<id>'}}\n  post:\n    http: {{method: POST, path: /same}}\n  slash:\n    http: {{method: GET, path: /same/}}\n"
        );
        let fixture = ManifestFixture::new(&source);
        let output = wr(&["setup", fixture.directory.to_str().unwrap(), "--dry-run"]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn yaml_strings_preserve_literal_values() {
    let source = EXAMPLE
        .replace("name: hello", "name: on")
        .replace("greet:", "off:");
    let fixture = ManifestFixture::new(&source);
    let output = wr(&["setup", fixture.directory.to_str().unwrap(), "--dry-run"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let preview = String::from_utf8(output.stdout).unwrap();
    assert!(preview.contains("Workspace preview for on"));
    assert!(preview.contains(".wr/functions/off/"));
}

#[test]
fn yaml_aliases_and_merges_are_validated_after_resolution() {
    let source = "name: !!str on\npath-prefix: /\nfunctions:\n  greet: &handler\n    http: {method: GET, path: /hello}\n  echo:\n    <<: *handler\n    http: {method: POST, path: /echo}\n";
    let fixture = ManifestFixture::new(source);
    let output = wr(&["setup", fixture.directory.to_str().unwrap(), "--dry-run"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let preview = String::from_utf8(output.stdout).unwrap();
    assert!(preview.contains("Workspace preview for on"));
    for name in ["greet", "echo"] {
        assert!(preview.contains(&format!(".wr/functions/{name}/")));
    }
    for invalid in [
        source.replace("method: GET", "method: invalid"),
        source.replace("path: /hello", "path: relative"),
    ] {
        let fixture = ManifestFixture::new(&invalid);
        let output = fixture.run(&[]);
        assert_eq!(output.status.code(), Some(1), "accepted {invalid}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("/functions/"));
    }
}

#[test]
fn schema_accepts_identifier_boundaries_and_literal_paths() {
    let identifier = format!("a{}", "1-".repeat(31)) + "z";
    let source = EXAMPLE
        .replace("name: hello", &format!("name: {identifier}"))
        .replace("greet:", &format!("{identifier}:"))
        .replace("path: /hello", "path: /")
        .replace("path: /echo", "path: /caf%C3%A9/");
    let fixture = ManifestFixture::new(&source);
    let output = wr(&["setup", fixture.directory.to_str().unwrap(), "--dry-run"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let preview = String::from_utf8(output.stdout).unwrap();
    assert!(preview.contains(&format!("Workspace preview for {identifier}")));
    assert!(preview.contains(&format!(".wr/functions/{identifier}/")));
}

#[test]
fn invalid_manifests_fail_before_printing_metadata() {
    let cases = [
        (format!("{EXAMPLE}\nkv: 'true'\n"), &["/kv"][..]),
        (format!("{EXAMPLE}\nkv: 1\n"), &["/kv"][..]),
        (format!("{EXAMPLE}\nsql: true\n"), &["/sql"][..]),
        (
            format!("{EXAMPLE}\nsql: {{enabled: false, migration-target: 1}}\n"),
            &["/sql"][..],
        ),
        (
            format!("{EXAMPLE}\nsql: {{migration-target: 1}}\n"),
            &["/sql"][..],
        ),
        (
            format!("{EXAMPLE}\nsql: {{enabled: true, migration-target: -1}}\n"),
            &["/sql"][..],
        ),
        (
            EXAMPLE.replace("name: hello", "name: Invalid"),
            &["/name", "invalid"][..],
        ),
        (
            EXAMPLE.replace("greet:", "bad_id:"),
            &["/functions", "bad_id"][..],
        ),
        (
            EXAMPLE.replace("name: hello", &format!("name: {}", "a".repeat(65))),
            &["/name"][..],
        ),
        (
            EXAMPLE.replace("greet:", &format!("{}:", "a".repeat(65))),
            &["/functions"][..],
        ),
        (EXAMPLE.replace("name: hello", "name: ''"), &["/name"][..]),
        (
            String::from("name: hello\npath-prefix: /\nfunctions: {}"),
            &["functions"][..],
        ),
        (
            EXAMPLE.replace("greet:", "greet:\n    module: build/greet.wasm"),
            &["module"][..],
        ),
        (
            EXAMPLE.replace("greet:", "greet:\n    module: ''"),
            &["module"][..],
        ),
        (
            EXAMPLE.replace("method: GET", "method: get"),
            &["/functions/greet/http/method", "get"][..],
        ),
        (
            EXAMPLE.replace("path: /hello", "path: relative"),
            &["/functions/greet/http/path"][..],
        ),
        (
            EXAMPLE.replace("name: hello", "name: hello\nextra: true"),
            &["extra"][..],
        ),
        (
            EXAMPLE.replace("greet:", "greet:\n    extra: true"),
            &["extra"][..],
        ),
        (EXAMPLE.replace("method: GET", "extra: GET"), &["extra"][..]),
        (
            EXAMPLE.replace("name: hello", "name: hello\nname: again"),
            &["duplicate"][..],
        ),
        (EXAMPLE.replace("echo:", "greet:"), &["duplicate"][..]),
        (format!("{EXAMPLE}\n---\n{EXAMPLE}"), &["document"][..]),
        (String::from("name: ["), &[][..]),
    ];
    for (source, expected) in cases {
        let fixture = ManifestFixture::new(&source);
        let output = fixture.run(&[]);
        assert_eq!(output.status.code(), Some(1), "accepted {source}");
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(
            error.contains(fixture.path.to_str().unwrap()),
            "{source}\n{error}"
        );
        for fragment in expected {
            assert!(
                error.to_lowercase().contains(fragment),
                "expected {fragment:?} in {error}"
            );
        }
    }
}

#[test]
fn missing_and_wrong_typed_fields_are_rejected() {
    for source in [
        String::from("[]"),
        String::from("name: hello\npath-prefix: /\nfunctions: null"),
        EXAMPLE.replace("name: hello", ""),
        String::from("name: hello\npath-prefix: /\nfunctions: []"),
        EXAMPLE.replace("name: hello", "name: true"),
        EXAMPLE.replace("greet:", "true:"),
        EXAMPLE.replace("greet:", "greet:\n    module: 42"),
        EXAMPLE.replace("method: GET", "method: [GET]"),
        EXAMPLE.replace("method: GET", "method: {GET: null}"),
        EXAMPLE.replace("method: GET", "method: 0"),
        EXAMPLE.replace("method: GET", "method: true"),
        EXAMPLE.replace("method: GET", "method: CONNECT"),
        EXAMPLE.replace("path: /hello", "path: ['/hello']"),
        EXAMPLE.replace("path: /hello", "path: null"),
    ] {
        let fixture = ManifestFixture::new(&source);
        let output = fixture.run(&[]);
        assert_eq!(output.status.code(), Some(1), "accepted {source}");
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}

#[test]
fn missing_manifest_reports_its_path() {
    let fixture = ManifestFixture::new(EXAMPLE);
    fs::remove_file(&fixture.path).unwrap();
    let output = fixture.run(&[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains(fixture.path.to_str().unwrap())
    );
}

#[test]
fn commands_require_a_directory_and_report_missing_function_sources() {
    let fixture = ManifestFixture::new(EXAMPLE);
    let output = wr(&["setup", fixture.path.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("application.yaml"));

    let source = fixture.directory.join("functions/greet.rs");
    fs::remove_file(&source).unwrap();
    for command in ["run", "setup", "build", "update"] {
        let output = wr(&[command, fixture.directory.to_str().unwrap()]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains(source.to_str().unwrap()));
    }
    assert!(!fixture.directory.join("Cargo.toml").exists());
}

#[test]
fn setup_discovers_all_direct_rust_sources_and_rejects_invalid_names() {
    let fixture = ManifestFixture::new(EXAMPLE);
    let functions = fixture.directory.join("functions");
    fs::write(functions.join("unlisted-z.rs"), "").unwrap();
    fs::write(functions.join("unlisted-a.rs"), "").unwrap();
    fs::write(functions.join("notes.txt"), "").unwrap();
    fs::create_dir(functions.join("nested")).unwrap();
    fs::write(functions.join("nested/helper.rs"), "").unwrap();
    fs::create_dir(functions.join("directory.rs")).unwrap();

    let output = wr(&["setup", fixture.directory.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest = fs::read_to_string(fixture.directory.join("Cargo.toml")).unwrap();
    assert!(manifest.find("unlisted-a").unwrap() < manifest.find("unlisted-z").unwrap());
    assert!(!manifest.contains("nested"));
    assert!(!manifest.contains("notes"));
    assert!(!manifest.contains("directory.rs"));
    let package = fs::read_to_string(
        fixture
            .directory
            .join(".wr/functions/unlisted-a/Cargo.toml"),
    )
    .unwrap();
    assert!(package.contains("name = \"wr_function_unlisted_a\""));
    assert_eq!(
        fs::read_to_string(fixture.directory.join(".wr/wit/world.wit")).unwrap(),
        HTTP_WORLD,
    );

    fs::write(functions.join("bad_name.rs"), "").unwrap();
    let output = wr(&["setup", fixture.directory.to_str().unwrap(), "--dry-run"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("bad_name.rs"));
    assert_eq!(
        fs::read_to_string(fixture.directory.join("Cargo.toml")).unwrap(),
        manifest
    );
}

#[test]
fn build_produces_http_components_and_preserves_outputs_on_cargo_failure() {
    let fixture = source_application();

    let output = fixture.build(&[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("wr update"));
    assert!(!fixture.directory.join("Cargo.lock").exists());
    assert!(!fixture.directory.join(".wr/components").exists());
    assert!(!fixture.directory.join("askama.toml").exists());

    let output = wr(&["update", fixture.directory.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let lockfile = fs::read(fixture.directory.join("Cargo.lock")).unwrap();

    let mut debug_artifact_time = None;
    for arguments in [vec![], vec![], vec!["--release"]] {
        let output = fixture.build(&arguments);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read(fixture.directory.join("Cargo.lock")).unwrap(),
            lockfile
        );
        let profile = if arguments.contains(&"--release") {
            "release"
        } else {
            "debug"
        };
        for name in ["echo", "greet", "inspect", "unlisted-target", "home"] {
            let component = fs::read(
                fixture
                    .directory
                    .join(format!(".wr/components/{name}.wasm")),
            )
            .unwrap();
            assert_eq!(
                component,
                fs::read(fixture.directory.join(format!(
                    "target/wasm32-wasip2/{profile}/wr_function_{}.wasm",
                    name.replace('-', "_"),
                )))
                .unwrap(),
            );
            let wit_component::DecodedWasm::Component(resolve, world) =
                wit_component::decode(&component).expect("valid component with WIT exports")
            else {
                panic!("expected a component, not a WIT package");
            };
            assert!(
                resolve.worlds[world]
                    .exports
                    .keys()
                    .any(|key| { resolve.name_world_key(key) == "local:http/handler@0.2.0" })
            );
        }
        assert!(!fixture.directory.join("ignored-target").exists());
        if profile == "debug" {
            let modified = fs::metadata(
                fixture
                    .directory
                    .join("target/wasm32-wasip2/debug/wr_function_greet.wasm"),
            )
            .unwrap()
            .modified()
            .unwrap();
            if let Some(previous) = debug_artifact_time {
                assert_eq!(
                    modified, previous,
                    "unchanged builds should reuse Cargo artifacts"
                );
            }
            debug_artifact_time = Some(modified);
        }
    }

    let destination = fixture.directory.join(".wr/components/greet.wasm");
    let previous = fs::read(&destination).unwrap();
    fs::write(fixture.directory.join("functions/added.rs"), GREET_FUNCTION).unwrap();
    let output = fixture.build(&[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(!fixture.directory.join(".wr/components/added.wasm").exists());
    assert_eq!(fs::read(&destination).unwrap(), previous);
    assert_eq!(
        fs::read(fixture.directory.join("Cargo.lock")).unwrap(),
        lockfile
    );

    let output = fixture.build(&["--unlocked"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        fixture
            .directory
            .join(".wr/components/added.wasm")
            .is_file()
    );
    let updated_lockfile = fs::read(fixture.directory.join("Cargo.lock")).unwrap();
    assert_ne!(updated_lockfile, lockfile);
    let previous = fs::read(&destination).unwrap();
    for (path, invalid) in [
        ("libs/responses.rs", "invalid Rust"),
        ("templates/base.html", "{% invalid_template_tag %}"),
    ] {
        let path = fixture.directory.join(path);
        let original = fs::read(&path).unwrap();
        fs::write(&path, invalid).unwrap();
        let output = fixture.build(&[]);
        assert_eq!(output.status.code(), Some(1));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("Built "));
        assert!(String::from_utf8_lossy(&output.stderr).contains("Cargo build failed"));
        assert_eq!(fs::read(&destination).unwrap(), previous);
        assert_eq!(
            fs::read(fixture.directory.join("Cargo.lock")).unwrap(),
            updated_lockfile
        );
        fs::write(path, original).unwrap();
    }
}

#[test]
fn update_refreshes_dependencies_without_building() {
    let source = format!("{EXAMPLE}\ndependencies:\n  itoa: '=1.0.17'\n");
    let fixture = ManifestFixture::new(&source);
    let locked_version = || {
        let lockfile = fs::read_to_string(fixture.directory.join("Cargo.lock")).unwrap();
        let lockfile: toml::Value = toml::from_str(&lockfile).unwrap();
        lockfile["package"]
            .as_array()
            .unwrap()
            .iter()
            .find(|package| package["name"].as_str() == Some("itoa"))
            .unwrap()["version"]
            .as_str()
            .unwrap()
            .to_owned()
    };

    let output = wr(&["update", fixture.directory.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(locked_version(), "1.0.17");
    assert!(!fixture.directory.join("target").exists());
    assert!(!fixture.directory.join(".wr/components").exists());

    let components = fixture.directory.join(".wr/components");
    fs::create_dir(&components).unwrap();
    let published = components.join("greet.wasm");
    fs::write(&published, b"previously published component").unwrap();
    fs::write(
        &fixture.path,
        source.replace("=1.0.17", ">=1.0.17, <=1.0.18"),
    )
    .unwrap();
    let output = wr(&["update", fixture.directory.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(locked_version(), "1.0.18");
    assert!(!fixture.directory.join("target").exists());
    assert_eq!(
        fs::read(&published).unwrap(),
        b"previously published component"
    );

    fs::write(&fixture.path, source.replace("=1.0.17", "invalid version")).unwrap();
    let output = wr(&["update", fixture.directory.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Cargo update failed"));
    assert_eq!(
        fs::read(&published).unwrap(),
        b"previously published component"
    );
}

#[test]
fn http_serving_routes_requests_and_rejects_collisions() {
    use std::{
        io::{Read, Write},
        net::TcpStream,
        time::Duration,
    };
    use wasm_runner::server::{self, Config, Handler};

    let handlers = [
        ("inspect", "GET", "/users/<user_id>/orders"),
        ("inspect", "POST", "/users/<owner>/orders"),
        ("inspect", "GET", "/files/<path..>"),
        ("greet", "GET", "/users/me/orders"),
        ("greet", "GET", "/hello/"),
        ("echo", "POST", "/echo"),
    ]
    .into_iter()
    .map(|(name, method, path)| Handler {
        name: name.into(),
        method: method.into(),
        path: path.into(),
        component: compiled_component(name).to_vec(),
    })
    .collect();
    tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap().block_on(async {
        let config = Config {
            listen: "127.0.0.1:0".parse().unwrap(),
            max_request_body: 64,
            ..Config::default()
        };
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let (ready, address) = tokio::sync::oneshot::channel();
        let running = tokio::spawn(server::serve(config.clone(), handlers,
            async { let _ = stopped.await; },
            move |address| { let _ = ready.send(address); }));
        let address = address.await.unwrap();
        let client = tokio::task::spawn_blocking(move || {
            let request = |method: &str, path: &str, body: &str| {
                let mut socket = TcpStream::connect(address).unwrap();
                socket.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
                write!(socket, "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
                let mut response = String::new();
                socket.read_to_string(&mut response).unwrap();
                response
            };
            let response = request("GET", "/users/a%2Fb/orders?tag=a&flag&tag=b&plus=%2B+", "");
            assert!(response.starts_with("HTTP/1.1 200"), "{response}");
            let body: serde_json::Value = serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
            assert_eq!(body["path"], "/users/a%2Fb/orders");
            assert_eq!(body["path_params"], serde_json::json!([["user_id", "a/b"]]));
            assert_eq!(body["query"], serde_json::json!([["tag", ["a", "b"]], ["flag", [""]], ["plus", ["+ "]]]));
            let response = request("POST", "/users/42/orders", "");
            assert!(response.contains("[[\"owner\",\"42\"]]"), "{response}");
            let response = request("GET", "/users/me/orders", "");
            assert!(response.ends_with("Hello, world!"), "{response}");
            let response = request("POST", "/users/me/orders", "");
            assert!(response.contains("[[\"owner\",\"me\"]]"), "{response}");
            let response = request("GET", "/files/a/b%20c", "");
            assert!(response.contains("[[\"path\",\"a/b c\"]]"), "{response}");
            let response = request("HEAD", "/hello/", "");
            assert!(response.starts_with("HTTP/1.1 200"), "{response}");
            assert!(response.split_once("\r\n\r\n").unwrap().1.is_empty());
            assert!(response.to_ascii_lowercase().contains("content-length: 13\r\n"), "{response}");
            for (method, path, code) in [("GET", "/hello", 200), ("OPTIONS", "/hello/", 404), ("GET", "/users//orders", 404)] {
                let response = request(method, path, "");
                assert!(response.starts_with(&format!("HTTP/1.1 {code}")), "{response}");
            }
            assert!(request("POST", "/echo", "{}").ends_with("{}"));
            assert!(request("POST", "/echo", "invalid").starts_with("HTTP/1.1 400"));
            assert!(request("POST", "/echo", &"x".repeat(65)).starts_with("HTTP/1.1 413"));
        }).await;
        let _ = stop.send(());
        running.await.unwrap().unwrap();
        client.unwrap();

        // Both patterns have Rocket's partial-path rank and overlap at /a/b.
        let conflicting = ["/a/<x>", "/<y>/b"].into_iter().map(|path| Handler {
            name: path.into(),
            method: "GET".into(),
            path: path.into(),
            component: compiled_component("greet").to_vec(),
        }).collect();
        let error = server::serve(config, conflicting, std::future::pending(),
            |_| panic!("conflicting routes must fail before readiness")).await.unwrap_err();
        assert!(error.to_string().contains("collision"), "{error:#}");
    });
}

#[cfg(unix)]
fn http(address: &str, method: &str, path: &str) -> String {
    use std::io::{Read, Write};
    let mut stream = std::net::TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

#[test]
fn manifests_require_literal_canonical_path_prefixes() {
    let invalid = [
        "",
        "relative",
        "/trailing/",
        "//double",
        "/a//b",
        "/.",
        "/a/../b",
        "/a/./b",
        "/<capture>",
        "/a?b=c",
        "/a#b",
        "/a%2Fb",
        "/two words",
        "/café",
    ];
    for prefix in invalid {
        let fixture = ManifestFixture::new(
            &EXAMPLE.replace("path-prefix: /", &format!("path-prefix: '{prefix}'")),
        );
        let output = wr(&["setup", fixture.directory.to_str().unwrap(), "--dry-run"]);
        assert!(!output.status.success(), "{prefix:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("path-prefix"));
        assert!(!fixture.directory.join(".wr").exists());
    }
    let fixture = ManifestFixture::new(&EXAMPLE.replace("path-prefix: /\n", ""));
    for command in ["setup", "run"] {
        let output = wr(&[command, fixture.directory.to_str().unwrap()]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("path-prefix"));
    }
    for prefix in ["/", "/api", "/api/v1", "/.well-known/a_b-1~2.3"] {
        let fixture = ManifestFixture::new(
            &EXAMPLE.replace("path-prefix: /", &format!("path-prefix: '{prefix}'")),
        );
        let output = wr(&["setup", fixture.directory.to_str().unwrap(), "--dry-run"]);
        assert!(
            output.status.success(),
            "{prefix:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[cfg(unix)]
#[test]
fn foreground_serving_honors_application_prefix() {
    let application = mounted_application(MOUNTED_APPLICATION);
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Stdio},
        sync::mpsc,
        time::Duration,
    };
    struct Running(Child);
    impl Drop for Running {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for restart in 0..2 {
        let mut running = Running(
            Command::new(env!("CARGO_BIN_EXE_wr"))
                .arg("run")
                .arg(&application.path)
                .args(["--port", "0", "--admin-listen", "127.0.0.1:0"])
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let stdout = running.0.stdout.take().unwrap();
        let (sent, received) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let line = line.unwrap();
                if let Some(address) = line.strip_prefix("Listening on http://") {
                    let _ = sent.send((false, address.to_owned()));
                }
                if let Some(address) = line.strip_prefix("Admin UI listening on http://") {
                    let _ = sent.send((true, address.to_owned()));
                }
            }
        });
        let mut address = String::new();
        let mut admin = String::new();
        for _ in 0..2 {
            let (is_admin, bound) = received
                .recv_timeout(Duration::from_secs(30))
                .expect("foreground readiness");
            if is_admin {
                admin = bound;
            } else {
                address = bound;
            }
        }
        assert!(http(&address, "GET", "/apps/one/hello").ends_with("Hello, world!"));
        assert!(http(&address, "GET", "/hello").starts_with("HTTP/1.1 404"));
        assert!(http(&admin, "GET", "/ready").starts_with("HTTP/1.1 200"));
        let response = http(&admin, "GET", "/status");
        assert!(response.starts_with("HTTP/1.1 200"));
        let status: serde_json::Value =
            serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(status["ready"], true);
        let applications = status["applications"].as_object().unwrap();
        assert_eq!(applications.len(), 1);
        assert_eq!(
            applications["first"],
            serde_json::json!({
                "source_commit": null,
                "desired_version": null,
                "active_version": null,
                "active_digest": null,
                "error": null,
            })
        );
        assert!(http(&admin, "GET", "/").contains("Handler latency"));
        let locked = wr(&["run", application.path.to_str().unwrap(), "--port", "0"]);
        assert!(!locked.status.success());
        assert!(String::from_utf8_lossy(&locked.stderr).contains("owns this state directory"));
        if restart == 0 {
            running.0.kill().unwrap();
            running.0.wait().unwrap();
        } else {
            Command::new("kill")
                .args(["-TERM", &running.0.id().to_string()])
                .status()
                .unwrap();
            assert!(running.0.wait().unwrap().success());
        }
        reader.join().unwrap();
    }
}

#[cfg(unix)]
#[test]
fn package_exports_a_complete_deterministic_release_without_building() {
    use sha2::{Digest, Sha256};
    let fixture = ManifestFixture::new(
        "name: hello\npath-prefix: /hello\npublish: {registry: registry.example/team, version: '1.0'}\nsql: {enabled: true}\nfunctions:\n  greet:\n    http: {method: GET, path: /}\n",
    );
    fixture.write_component("greet", b"prebuilt-component");
    let first = fixture.directory.join("package-one");
    let second = fixture.directory.join("package-two");
    for output in [&first, &second] {
        let result = wr(&[
            "package",
            fixture.path.to_str().unwrap(),
            "--source-commit",
            &"a".repeat(40),
            "--output",
            output.to_str().unwrap(),
        ]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    for artifact in ["functions/greet", "migrations", "release"] {
        let first = first.join(artifact);
        let second = second.join(artifact);
        let index = fs::read(first.join("index.json")).unwrap();
        assert_eq!(index, fs::read(second.join("index.json")).unwrap());
        let index: serde_json::Value = serde_json::from_slice(&index).unwrap();
        let digest = index["manifests"][0]["digest"].as_str().unwrap();
        let manifest = fs::read(first.join("blobs/sha256").join(&digest[7..])).unwrap();
        assert_eq!(format!("sha256:{:x}", Sha256::digest(&manifest)), digest);
        let manifest: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
        let blob = manifest["layers"][0]["digest"].as_str().unwrap();
        let bytes = fs::read(first.join("blobs/sha256").join(&blob[7..])).unwrap();
        assert_eq!(format!("sha256:{:x}", Sha256::digest(&bytes)), blob);
        assert_eq!(
            bytes.len() as u64,
            manifest["layers"][0]["size"].as_u64().unwrap()
        );
    }
    assert!(!fixture.directory.join("Cargo.toml").exists());
    assert!(!fixture.directory.join(".wr-data").exists());
}
