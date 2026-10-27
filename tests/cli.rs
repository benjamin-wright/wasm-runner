use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

const EXAMPLE: &str = include_str!("fixtures/application.yaml");

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
        let path = directory.join("application.yaml");
        fs::write(&path, source).unwrap();
        Self { directory, path }
    }

    fn run(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_wr"))
            .current_dir(self.directory.parent().unwrap())
            .arg("run")
            .arg(&self.path)
            .args(arguments)
            .output()
            .expect("wr should launch")
    }
}

impl Drop for ManifestFixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.directory).unwrap();
    }
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
    assert!(help.contains("<MANIFEST>"));
    assert!(help.contains("--listen <IP:PORT>"));
    assert!(help.contains("127.0.0.1:8080"));
}

#[test]
fn invalid_invocations_are_rejected() {
    for arguments in [
        vec![],
        vec!["run"],
        vec!["setup"],
        vec!["build"],
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
    let fixture = ManifestFixture::new(include_str!("fixtures/setup/application.yaml"));
    let output = wr(&["setup", fixture.path.to_str().unwrap(), "--dry-run"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        include_str!("fixtures/setup/stdout.txt")
    );
    assert_eq!(fs::read_dir(&fixture.directory).unwrap().count(), 1);

    let output = fixture.run(&[]);
    assert!(output.status.success());
    let metadata = String::from_utf8(output.stdout).unwrap();
    for name in ["greet", "echo"] {
        assert!(
            metadata.contains(
                fixture
                    .directory
                    .join(format!(".wr/components/{name}.wasm"))
                    .to_str()
                    .unwrap()
            )
        );
    }
}

#[test]
fn setup_writes_the_previewed_files_beside_the_manifest_and_can_be_repeated() {
    let fixture = ManifestFixture::new(include_str!("fixtures/setup/application.yaml"));
    for _ in 0..2 {
        let output = wr(&["setup", fixture.path.to_str().unwrap()]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());

        for file in include_str!("fixtures/setup/stdout.txt")
            .split("\n=== ")
            .skip(1)
        {
            let (path, contents) = file.split_once(" ===\n").unwrap();
            let expected = contents.strip_suffix('\n').unwrap();
            assert_eq!(
                fs::read_to_string(fixture.directory.join(path)).unwrap(),
                expected,
                "{path}"
            );
        }
    }
}

#[test]
fn setup_preserves_existing_ignore_rules_and_appends_only_missing_entries() {
    let fixture = ManifestFixture::new(include_str!("fixtures/setup/application.yaml"));
    let path = fixture.directory.join(".gitignore");
    let existing = "# Local files\r\n.env\r\n/target/";
    let expected = "# Local files\r\n.env\r\n/target/\n.wr/\n/Cargo.toml\n";
    fs::write(&path, existing).unwrap();

    let output = wr(&["setup", fixture.path.to_str().unwrap(), "--dry-run"]);
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
    assert_eq!(fs::read_dir(&fixture.directory).unwrap().count(), 2);

    for _ in 0..2 {
        let output = wr(&["setup", fixture.path.to_str().unwrap()]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), expected);
    }
}

#[test]
fn setup_preserves_an_existing_handwritten_cargo_manifest() {
    let fixture = ManifestFixture::new(include_str!("fixtures/setup/application.yaml"));
    let path = fixture.directory.join("Cargo.toml");
    let handwritten = "[workspace]\nmembers = []\n";
    fs::write(&path, handwritten).unwrap();

    let output = wr(&["setup", fixture.path.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("refusing to overwrite"));
    assert_eq!(fs::read_to_string(path).unwrap(), handwritten);
    assert!(!fixture.directory.join(".wr").exists());
    assert!(!fixture.directory.join(".gitignore").exists());
}

#[test]
fn setup_rejects_invalid_dependencies_without_partial_output() {
    let source = include_str!("../examples/application.yaml");
    for invalid in [
        source.replace("anyhow: \"1.0\"", "anyhow: 1.0"),
        source.replace(
            "anyhow: \"1.0\"",
            "anyhow: {version: '1.0', features: derive}",
        ),
        source.replace("anyhow: \"1.0\"", "anyhow: {version: '1.0', typo: true}"),
        source.replace("serde_json:", "anyhow:"),
    ] {
        let fixture = ManifestFixture::new(&invalid);
        let output = wr(&["setup", fixture.path.to_str().unwrap()]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
        assert_eq!(fs::read_dir(&fixture.directory).unwrap().count(), 1);
    }
}

#[test]
fn run_prints_metadata_without_requiring_components_or_announcing_readiness() {
    let fixture = ManifestFixture::new(EXAMPLE);
    for address in [None, Some("0.0.0.0:9000"), Some("[::1]:9000")] {
        let mut arguments = vec![];
        if let Some(address) = address {
            arguments.extend(["--listen", address]);
        }

        let output = fixture.run(&arguments);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let metadata = String::from_utf8(output.stdout).unwrap();
        for expected in [
            "Application: hello",
            "Functions: 2",
            "GET",
            "POST",
            "/hello -> greet",
            "/echo -> echo",
            "Metadata only",
            address.unwrap_or("127.0.0.1:8080"),
            fixture.path.to_str().unwrap(),
            fixture
                .directory
                .join("./build/greet.wasm")
                .to_str()
                .unwrap(),
            fixture
                .directory
                .join("./build/echo.wasm")
                .to_str()
                .unwrap(),
        ] {
            assert!(
                metadata.contains(expected),
                "missing {expected:?} in {metadata}"
            );
        }
        assert!(!metadata.contains("listening on"));
        assert!(
            metadata.find("/echo -> echo").unwrap() < metadata.find("/hello -> greet").unwrap()
        );
    }
}

#[test]
fn run_accepts_absolute_modules_and_distinct_routes() {
    let fixture = ManifestFixture::new("");
    let module = fixture.directory.join("shared.wasm");
    let source = format!(
        "name: routes\nfunctions:\n  get:\n    module: '{}'\n    http: {{method: GET, path: /same}}\n  post:\n    module: '{}'\n    http: {{method: POST, path: /same}}\n  slash:\n    module: '{}'\n    http: {{method: GET, path: /same/}}\n",
        module.display(),
        module.display(),
        module.display()
    );
    fs::write(&fixture.path, source).unwrap();
    let output = fixture.run(&[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata = String::from_utf8(output.stdout).unwrap();
    assert_eq!(metadata.matches(module.to_str().unwrap()).count(), 3);
}

#[test]
fn yaml_strings_preserve_literal_values() {
    let source = EXAMPLE
        .replace("name: hello", "name: on")
        .replace("greet:", "off:")
        .replace("module: ./build/greet.wasm", "module: '42'");
    let fixture = ManifestFixture::new(&source);
    let output = fixture.run(&[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata = String::from_utf8(output.stdout).unwrap();
    assert!(metadata.contains("Application: on"));
    assert!(metadata.contains("/hello -> off"));
    assert!(metadata.contains(fixture.directory.join("42").to_str().unwrap()));
}

#[test]
fn yaml_aliases_and_merges_are_validated_after_resolution() {
    let source = "\
name: !!str on
functions:
  greet: &handler
    module: &module ./build/shared.wasm
    http: {method: GET, path: /hello}
  echo:
    <<: *handler
    module: *module
    http: {method: POST, path: /echo}
";
    let fixture = ManifestFixture::new(source);
    let output = fixture.run(&[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata = String::from_utf8(output.stdout).unwrap();
    assert!(metadata.contains("Application: on"));
    assert!(metadata.contains("/hello -> greet"));
    assert!(metadata.contains("/echo -> echo"));
    assert_eq!(
        metadata
            .matches(
                fixture
                    .directory
                    .join("./build/shared.wasm")
                    .to_str()
                    .unwrap()
            )
            .count(),
        2
    );

    for invalid in [
        source.replace("module: &module ./build/shared.wasm", "module: &module 42"),
        source.replace("    http: {method: POST, path: /echo}\n", ""),
    ] {
        let fixture = ManifestFixture::new(&invalid);
        let output = fixture.run(&[]);
        assert_eq!(output.status.code(), Some(1), "accepted {invalid}");
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("/functions/"), "{error}");
    }
}

#[test]
fn schema_accepts_identifier_boundaries_and_literal_paths() {
    let identifier = format!("a{}", "1-".repeat(31)) + "z";
    let source = EXAMPLE
        .replace("name: hello", &format!("name: {identifier}"))
        .replace("greet:", &format!("{identifier}:"))
        .replace("path: /hello", "path: /")
        .replace("path: /echo", "path: /café/");
    let fixture = ManifestFixture::new(&source);
    let output = fixture.run(&[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata = String::from_utf8(output.stdout).unwrap();
    assert!(metadata.contains(&format!("Application: {identifier}")));
    assert!(metadata.contains(&format!("/ -> {identifier}")));
    assert!(metadata.contains("/café/ -> echo"));
}

#[test]
fn all_supported_methods_are_printed_in_uppercase() {
    for method in ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"] {
        let fixture =
            ManifestFixture::new(&EXAMPLE.replace("method: GET", &format!("method: {method}")));
        let output = fixture.run(&[]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let metadata = String::from_utf8(output.stdout).unwrap();
        let route = metadata
            .lines()
            .find(|line| line.contains("/hello -> greet"))
            .unwrap();
        assert_eq!(route.split_whitespace().next(), Some(method));
    }
}

#[test]
fn invalid_manifests_fail_before_printing_metadata() {
    let cases = [
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
            String::from("name: hello\nfunctions: {}"),
            &["functions"][..],
        ),
        (
            EXAMPLE.replace("module: ./build/greet.wasm", "module: ''"),
            &["/functions/greet/module"][..],
        ),
        (
            EXAMPLE.replace("module: ./build/greet.wasm", "module: \"bad\\0path\""),
            &["/functions/greet/module"][..],
        ),
        (
            EXAMPLE.replace("method: GET", "method: get"),
            &["/functions/greet/http/method", "get"][..],
        ),
        (
            EXAMPLE.replace("path: /hello", "path: /hello?query"),
            &["/functions/greet/http/path"][..],
        ),
        (
            EXAMPLE.replace("path: /hello", "path: relative"),
            &["/functions/greet/http/path"][..],
        ),
        (
            EXAMPLE.replace("path: /hello", "path: '/white space'"),
            &["/functions/greet/http/path"][..],
        ),
        (
            EXAMPLE.replace("path: /hello", "path: \"/white\\u2003space\""),
            &["/functions/greet/http/path"][..],
        ),
        (
            EXAMPLE.replace("path: /hello", "path: \"/control\\u007f\""),
            &["/functions/greet/http/path"][..],
        ),
        (
            EXAMPLE.replace("path: /hello", "path: '/{parameter}'"),
            &["/functions/greet/http/path"][..],
        ),
        (
            EXAMPLE.replace("path: /hello", "path: '/percent%20escape'"),
            &["/functions/greet/http/path"][..],
        ),
        (
            EXAMPLE
                .replace("method: POST", "method: GET")
                .replace("path: /echo", "path: /hello"),
            &["duplicate"][..],
        ),
        (
            EXAMPLE.replace("name: hello", "name: hello\nextra: true"),
            &["extra"][..],
        ),
        (
            EXAMPLE.replace("module: ./build/greet.wasm", "extra: true"),
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
        assert!(error.contains(fixture.path.to_str().unwrap()), "{error}");
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
        String::from("name: hello\nfunctions: null"),
        EXAMPLE.replace("name: hello", ""),
        String::from("name: hello\nfunctions: []"),
        EXAMPLE.replace("name: hello", "name: true"),
        EXAMPLE.replace("greet:", "true:"),
        EXAMPLE.replace("module: ./build/greet.wasm", "module: 42"),
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
