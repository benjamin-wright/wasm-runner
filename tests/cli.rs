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
    assert!(help.contains("<DIRECTORY>"));
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
        ".wr/wit/world.wit",
        ".vscode/settings.json",
        "${workspaceFolder}/functions",
        "workspace = true",
        "export!(Component)",
    ] {
        assert!(preview.contains(expected), "{preview}");
    }
    assert_eq!(fs::read_dir(&fixture.directory).unwrap().count(), 2);

    let output = fixture.run(&[]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("wr build"));
}

#[test]
fn setup_writes_the_previewed_files_beside_the_manifest_and_can_be_repeated() {
    let expected_settings = r#"{
  "rust-analyzer.vfs.extraIncludes": ["${workspaceFolder}/functions"]
}
"#;
    let fixture = ManifestFixture::new(SETUP_APPLICATION);
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
fn setup_preserves_existing_ignore_rules_and_appends_only_missing_entries() {
    let fixture = ManifestFixture::new(SETUP_APPLICATION);
    let path = fixture.directory.join(".gitignore");
    let existing = "# Local files\r\n.env\r\n/target/";
    let expected = "# Local files\r\n.env\r\n/target/\n.wr/\n/Cargo.toml\n";
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
    let fixture = ManifestFixture::new(&format!("{EXAMPLE}\ndependencies:\n  serde_json: '1.0'\n"));
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

    let output = fixture.build(&[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("wr update"));
    assert!(!fixture.directory.join("Cargo.lock").exists());
    assert!(!fixture.directory.join(".wr/components").exists());

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
        for name in ["echo", "greet", "inspect", "unlisted-target"] {
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

    serve_built_examples(&fixture.directory);
    #[cfg(unix)]
    deploy_built_examples(&fixture.directory);

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
    fs::write(fixture.directory.join("functions/greet.rs"), "invalid Rust").unwrap();
    let output = fixture.build(&[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Built "));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Cargo build failed"));
    assert_eq!(fs::read(destination).unwrap(), previous);
    assert_eq!(
        fs::read(fixture.directory.join("Cargo.lock")).unwrap(),
        updated_lockfile
    );
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

fn serve_built_examples(directory: &std::path::Path) {
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
        component: fs::read(directory.join(format!(".wr/components/{name}.wasm"))).unwrap(),
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
            component: fs::read(directory.join(".wr/components/greet.wasm")).unwrap(),
        }).collect();
        let error = server::serve(config, conflicting, std::future::pending(),
            |_| panic!("conflicting routes must fail before readiness")).await.unwrap_err();
        assert!(error.to_string().contains("collision"), "{error:#}");
    });
}

#[cfg(unix)]
struct DaemonFixture {
    directory: PathBuf,
}

#[cfg(unix)]
impl DaemonFixture {
    fn new() -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        // macOS limits Unix socket paths to 104 bytes; the host TMPDIR may be longer.
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "wrd-{}-{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir(&directory).unwrap();
        Self { directory }
    }

    fn call(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_wr"))
            .arg("--state-dir")
            .arg(self.directory.join("d"))
            .args(arguments)
            .output()
            .unwrap()
    }

    fn success(&self, arguments: &[&str]) -> String {
        let output = self.call(arguments);
        assert!(
            output.status.success(),
            "{arguments:?}: {}\nlog: {}",
            String::from_utf8_lossy(&output.stderr),
            fs::read_to_string(self.directory.join("d/daemon.log")).unwrap_or_default()
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn start(&self) -> String {
        let output = self.success(&["daemon", "start", "--listen", "127.0.0.1:0"]);
        output
            .lines()
            .find_map(|line| line.strip_prefix("Listening on http://"))
            .unwrap()
            .to_owned()
    }
}

#[cfg(unix)]
impl Drop for DaemonFixture {
    fn drop(&mut self) {
        let _ = self.call(&["daemon", "stop"]);
        let _ = fs::remove_dir_all(&self.directory);
    }
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

#[cfg(unix)]
struct AdminFixture {
    child: std::process::Child,
    address: String,
}

#[cfg(unix)]
impl AdminFixture {
    fn start(state_directory: &std::path::Path) -> Self {
        use std::{
            io::{BufRead, BufReader},
            process::Stdio,
            sync::mpsc,
            time::Duration,
        };

        let mut admin = Self {
            child: Command::new(env!("CARGO_BIN_EXE_wr"))
                .arg("--state-dir")
                .arg(state_directory)
                .args(["admin", "--listen", "127.0.0.1:0"])
                .env("ROCKET_ADDRESS", "0.0.0.0")
                .env("ROCKET_PORT", "1")
                .current_dir(state_directory.parent().unwrap())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
            address: String::new(),
        };
        let stdout = admin.child.stdout.take().unwrap();
        let (sent, received) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if let Some(address) = line.unwrap().strip_prefix("Admin UI listening on http://") {
                    let _ = sent.send(address.to_owned());
                    break;
                }
            }
        });
        admin.address = received
            .recv_timeout(Duration::from_secs(30))
            .expect("admin readiness");
        reader.join().unwrap();
        assert!(
            admin
                .address
                .parse::<std::net::SocketAddr>()
                .unwrap()
                .ip()
                .is_loopback()
        );
        admin
    }

    fn page(&self) -> String {
        http(&self.address, "GET", "/")
    }

    fn stop(&mut self, signal: &str) {
        assert!(
            Command::new("kill")
                .args([signal, &self.child.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "admin shutdown timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert!(std::net::TcpStream::connect(&self.address).is_err());
    }
}

#[cfg(unix)]
impl Drop for AdminFixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(unix)]
#[test]
fn admin_ui_reports_live_status_and_recovers() {
    let daemon = DaemonFixture::new();
    for address in ["0.0.0.0:0", "[::]:0", "192.0.2.1:0"] {
        let output = daemon.call(&["admin", "--listen", address]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("loopback"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("Admin UI listening"));
    }
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let output = daemon.call(&[
        "admin",
        "--listen",
        &occupied.local_addr().unwrap().to_string(),
    ]);
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Admin UI listening"));

    let mut admin = AdminFixture::start(&daemon.directory.join("d"));
    assert!(admin.page().starts_with("HTTP/1.1 503"));
    let address = daemon.start();
    let page = admin.page();
    assert!(page.starts_with("HTTP/1.1 200"));
    assert!(page.to_lowercase().contains("content-type: text/html"));
    assert!(page.to_lowercase().contains("cache-control: no-store"));
    assert!(page.contains(&format!("http://{address}")));
    assert!(page.contains("No deployed applications."));
    let status = daemon.success(&["daemon", "status"]);
    let pid = status
        .split("PID ")
        .nth(1)
        .unwrap()
        .split(')')
        .next()
        .unwrap();
    assert!(page.contains(&format!("<dd>{pid}</dd>")));
    assert!(!page.contains("<script"));
    let styles = http(&admin.address, "GET", "/style.css");
    assert!(styles.starts_with("HTTP/1.1 200"));
    assert!(styles.to_lowercase().contains("content-type: text/css"));
    assert!(styles.contains("@media"));
    assert!(http(&admin.address, "POST", "/").starts_with("HTTP/1.1 404"));
    daemon.success(&["daemon", "stop"]);
    let page = admin.page();
    assert!(page.starts_with("HTTP/1.1 503"));
    assert!(!page.contains("<dd>Running</dd>"));
    let address = daemon.start();
    assert!(admin.page().contains(&format!("http://{address}")));
    admin.stop("-INT");
    assert!(daemon.success(&["daemon", "status"]).contains(&address));

    let escaped = AdminFixture::start(&daemon.directory.join("missing<&>\"'"));
    let page = escaped.page();
    assert!(page.contains("missing&lt;&amp;&gt;&quot;&#39;"));
    assert!(!page.contains("missing<&>\"'"));
}

#[cfg(unix)]
#[test]
fn admin_ui_cancels_stalled_metadata_requests() {
    use std::{
        io::Read,
        os::unix::net::UnixListener,
        time::{Duration, Instant},
    };
    let daemon = DaemonFixture::new();
    let state = daemon.directory.join("d");
    fs::create_dir(&state).unwrap();
    let socket = state.join("control.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let admin = AdminFixture::start(&state);
    std::thread::scope(|scope| {
        let peer = scope.spawn(|| {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut request = Vec::new();
            // The client must close its connection after timing out, without a reply.
            stream.read_to_end(&mut request).unwrap();
            let request: serde_json::Value = serde_json::from_slice(&request[4..]).unwrap();
            assert_eq!(request["command"], "status");
        });
        let started = Instant::now();
        let page = admin.page();
        assert!(page.starts_with("HTTP/1.1 503"));
        assert!(page.contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(10));
        peer.join().unwrap();
    });
    drop(listener);
    fs::remove_file(socket).unwrap();
    assert!(admin.page().starts_with("HTTP/1.1 503"));
}

#[cfg(unix)]
#[test]
fn daemon_lifecycle_owns_both_listeners_and_restarts_empty() {
    use std::io::{Read, Write};
    use std::os::unix::fs::PermissionsExt;
    let daemon = DaemonFixture::new();
    assert!(!daemon.call(&["daemon", "status"]).status.success());
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let output = daemon.call(&[
        "daemon",
        "start",
        "--listen",
        &occupied.local_addr().unwrap().to_string(),
    ]);
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Listening"));
    let address = daemon.start();
    assert!(http(&address, "GET", "/").starts_with("HTTP/1.1 404"));
    let state = daemon.directory.join("d");
    // The control server must enforce prefixes even when no manifest loader is involved.
    for prefix in [None, Some("/invalid/../mount")] {
        let mut application = serde_json::json!({
            "name": "direct", "handlers": [{"name": "greet", "method": "GET", "path": "/", "component": []}]
        });
        if let Some(prefix) = prefix {
            application["path-prefix"] = prefix.into();
        }
        let bytes = serde_json::to_vec(
            &serde_json::json!({"command": "deploy", "application": application}),
        )
        .unwrap();
        let mut socket =
            std::os::unix::net::UnixStream::connect(state.join("control.sock")).unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        socket
            .write_all(&(bytes.len() as u32).to_be_bytes())
            .unwrap();
        socket.write_all(&bytes).unwrap();
        let mut size = [0; 4];
        socket.read_exact(&mut size).unwrap();
        let mut bytes = vec![0; u32::from_be_bytes(size) as usize];
        socket.read_exact(&mut bytes).unwrap();
        let response: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(response["result"], "error");
        assert!(
            response["message"]
                .as_str()
                .unwrap()
                .contains("path-prefix")
        );
    }
    assert_eq!(
        fs::metadata(&state).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(state.join("control.sock"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let status = daemon.success(&["daemon", "status"]);
    assert!(status.contains(&address));
    let log = fs::read(state.join("daemon.log")).unwrap();
    let repeated = daemon.success(&[
        "daemon",
        "start",
        "--listen",
        &occupied.local_addr().unwrap().to_string(),
    ]);
    assert!(repeated.contains("already running"));
    assert!(repeated.contains(&address));
    assert_eq!(daemon.success(&["daemon", "status"]), status);
    assert_eq!(fs::read(state.join("daemon.log")).unwrap(), log);
    daemon.success(&["daemon", "stop"]);
    assert!(!state.join("control.sock").exists());
    assert!(std::net::TcpStream::connect(&address).is_err());
    assert!(!daemon.call(&["daemon", "status"]).status.success());
    // A crash can leave a socket pathname; acquiring the process lock authorizes recovery.
    let stale = std::os::unix::net::UnixListener::bind(state.join("control.sock")).unwrap();
    drop(stale);
    daemon.start();
    assert!(daemon.success(&["apps", "list"]).is_empty());
    daemon.success(&["daemon", "stop"]);
}

#[cfg(unix)]
fn deploy_built_examples(directory: &std::path::Path) {
    let daemon = DaemonFixture::new();
    let address = daemon.start();
    let mut admin = AdminFixture::start(&daemon.directory.join("d"));
    let first = ManifestFixture::new(MOUNTED_APPLICATION);
    let second = ManifestFixture::new(
        &MOUNTED_APPLICATION
            .replace("name: first", "name: second")
            .replace("/apps/one", "/apps/one-more"),
    );
    for application in [&first, &second] {
        let components = application.directory.join(".wr/components");
        fs::create_dir_all(&components).unwrap();
        for (name, source) in [
            ("greet", "greet"),
            ("root", "greet"),
            ("inspect", "inspect"),
            ("files", "inspect"),
            ("echo", "echo"),
        ] {
            fs::copy(
                directory.join(format!(".wr/components/{source}.wasm")),
                components.join(format!("{name}.wasm")),
            )
            .unwrap();
        }
        daemon.success(&["deploy", application.directory.to_str().unwrap()]);
    }
    serve_prefixed_foreground(&first);
    let page = admin.page();
    assert!(page.contains("/apps/one</code>"));
    assert!(page.contains("/apps/one-more</code>"));
    assert!(page.find(">first</th>").unwrap() < page.find(">second</th>").unwrap());
    let first_row = page
        .split(">first</th>")
        .nth(1)
        .unwrap()
        .split("</tr>")
        .next()
        .unwrap();
    let functions = [
        ("echo", "POST", "/echo"),
        ("files", "GET", "/files/&lt;path..&gt;"),
        ("greet", "GET", "/hello"),
        ("inspect", "GET", "/users/&lt;user_id&gt;/orders"),
        ("root", "GET", "/"),
    ];
    let mut previous = 0;
    for (name, method, path) in functions {
        let entry =
            format!("<strong>{name}</strong><span><code>{method}</code> <code>{path}</code>");
        let position = first_row
            .find(&entry)
            .expect("deployed function with method and escaped path");
        assert!(position >= previous, "functions sorted by name");
        previous = position;
    }
    assert!(!page.contains("<user_id>"));
    assert!(!page.contains("<path..>"));
    assert_eq!(daemon.start(), address);
    assert_eq!(
        daemon.success(&["apps", "list"]),
        "first\t/apps/one\nsecond\t/apps/one-more\n"
    );
    assert!(
        daemon
            .success(&["daemon", "status"])
            .contains("first\t/apps/one")
    );
    for prefix in ["/apps/one", "/apps/one-more"] {
        for suffix in ["", "/", "/hello"] {
            assert!(http(&address, "GET", &format!("{prefix}{suffix}")).ends_with("Hello, world!"));
        }
        let response = http(&address, "HEAD", &format!("{prefix}/hello"));
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(
            response
                .to_ascii_lowercase()
                .contains("content-length: 13\r\n")
        );
        assert!(response.ends_with("\r\n\r\n"));
        let response = http(
            &address,
            "GET",
            &format!("{prefix}/users/a%2Fb/orders?tag=a&tag=b&plus=%2B+"),
        );
        let body: serde_json::Value =
            serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(body["path"], "/users/a%2Fb/orders");
        assert_eq!(body["path_params"], serde_json::json!([["user_id", "a/b"]]));
        assert_eq!(
            body["query"],
            serde_json::json!([["tag", ["a", "b"]], ["plus", ["+ "]]])
        );
        assert!(http(&address, "GET", &format!("{prefix}/files/a/b%20c")).contains("a/b c"));
        assert!(http(&address, "POST", &format!("{prefix}/echo")).starts_with("HTTP/1.1 400"));
        assert!(http(&address, "GET", &format!("{prefix}/echo")).starts_with("HTTP/1.1 404"));
    }
    for path in [
        "/hello",
        "/apps/on",
        "/apps/ones/hello",
        "/apps/%6Fne/hello",
        "/apps/one/missing",
    ] {
        assert!(
            http(&address, "GET", path).starts_with("HTTP/1.1 404"),
            "{path}"
        );
    }

    fs::write(
        first.directory.join(".wr/components/greet.wasm"),
        b"invalid",
    )
    .unwrap();
    assert!(
        !daemon
            .call(&["deploy", first.directory.to_str().unwrap()])
            .status
            .success()
    );
    assert!(http(&address, "GET", "/apps/one/hello").ends_with("Hello, world!"));
    assert_eq!(
        admin.page().split_once("\r\n\r\n").unwrap().1,
        page.split_once("\r\n\r\n").unwrap().1,
        "failed deployment preserves function metadata"
    );
    fs::copy(
        directory.join(".wr/components/greet.wasm"),
        first.directory.join(".wr/components/greet.wasm"),
    )
    .unwrap();

    // A small candidate keeps prefix-conflict checks from recompiling unrelated handlers.
    let greeting = "name: first\npath-prefix: /apps/one\nfunctions:\n  greet:\n    http: {method: GET, path: /hello}\n";
    // Equal, descendant, ancestor, and root mounts conflict even with independent routers.
    for prefix in ["/apps/one-more", "/apps/one-more/nested", "/apps", "/"] {
        fs::write(&first.path, greeting.replace("/apps/one", prefix)).unwrap();
        let output = daemon.call(&["deploy", first.directory.to_str().unwrap()]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("overlaps"));
        assert!(http(&address, "GET", "/apps/one/hello").ends_with("Hello, world!"));
        assert!(http(&address, "GET", "/apps/one-more/hello").ends_with("Hello, world!"));
    }
    // Conflicting routes inside one application still fail before replacing its mount.
    fs::write(
        &first.path,
        MOUNTED_APPLICATION.replace("path: /}", "path: /hello}"),
    )
    .unwrap();
    assert!(
        !daemon
            .call(&["deploy", first.directory.to_str().unwrap()])
            .status
            .success()
    );
    assert!(http(&address, "GET", "/apps/one/hello").ends_with("Hello, world!"));

    fs::write(
        &first.path,
        greeting
            .replace("/apps/one", "/replacement")
            .replace("path: /hello", "path: /updated-hello"),
    )
    .unwrap();
    std::thread::scope(|scope| {
        let requests = scope.spawn(|| {
            for _ in 0..40 {
                assert!(http(&address, "GET", "/apps/one-more/hello").ends_with("Hello, world!"));
            }
        });
        daemon.success(&["deploy", first.directory.to_str().unwrap()]);
        requests.join().unwrap();
    });
    assert!(http(&address, "GET", "/apps/one/hello").starts_with("HTTP/1.1 404"));
    assert!(http(&address, "GET", "/replacement/updated-hello").ends_with("Hello, world!"));
    let page = admin.page();
    assert!(page.contains("/replacement</code>"));
    assert!(!page.contains("/apps/one</code>"));
    let first_row = page
        .split(">first</th>")
        .nth(1)
        .unwrap()
        .split("</tr>")
        .next()
        .unwrap();
    assert!(first_row.contains("<strong>greet</strong>"));
    assert!(first_row.contains("<code>/updated-hello</code>"));
    assert!(!first_row.contains("<strong>echo</strong>"));
    assert!(!first_row.contains("<code>/hello</code>"));
    let second_row = page
        .split(">second</th>")
        .nth(1)
        .unwrap()
        .split("</tr>")
        .next()
        .unwrap();
    assert!(second_row.contains("<strong>echo</strong>"));
    assert!(second_row.contains("<code>/hello</code>"));
    assert!(
        daemon
            .success(&["apps", "list"])
            .contains("first\t/replacement")
    );
    daemon.success(&["remove", "first"]);
    let page = admin.page();
    assert!(!page.contains(">first</th>"));
    assert!(page.contains(">second</th>"));
    assert!(!page.contains("<code>/updated-hello</code>"));
    assert!(http(&address, "GET", "/replacement/updated-hello").starts_with("HTTP/1.1 404"));
    assert_eq!(
        daemon.success(&["apps", "list"]),
        "second\t/apps/one-more\n"
    );
    assert!(!daemon.call(&["remove", "missing"]).status.success());

    // A sole application may take the root mount; no other mount can coexist with it.
    fs::write(&first.path, MOUNTED_APPLICATION.replace("/apps/one", "/")).unwrap();
    daemon.success(&["remove", "second"]);
    daemon.success(&["deploy", first.directory.to_str().unwrap()]);
    assert!(http(&address, "GET", "/hello").ends_with("Hello, world!"));
    assert!(
        !daemon
            .call(&["deploy", second.directory.to_str().unwrap()])
            .status
            .success()
    );
    fs::remove_file(first.directory.join(".wr/components/greet.wasm")).unwrap();
    assert!(http(&address, "GET", "/hello").ends_with("Hello, world!"));
    let page = admin.page();
    assert!(page.contains(">first</th>"));
    assert!(page.contains("<code>/</code>"));
    assert!(page.contains("<strong>greet</strong><span><code>GET</code> <code>/hello</code>"));
    admin.stop("-TERM");
    assert!(http(&address, "GET", "/hello").ends_with("Hello, world!"));
    daemon.success(&["daemon", "stop"]);
    let address = daemon.start();
    assert!(daemon.success(&["apps", "list"]).is_empty());
    assert!(http(&address, "GET", "/hello").starts_with("HTTP/1.1 404"));
    daemon.success(&["daemon", "stop"]);
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
    for command in ["setup", "run", "deploy"] {
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
fn serve_prefixed_foreground(application: &ManifestFixture) {
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
    let mut running = Running(
        Command::new(env!("CARGO_BIN_EXE_wr"))
            .arg("run")
            .arg(&application.directory)
            .args(["--listen", "127.0.0.1:0"])
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
                let _ = sent.send(address.to_owned());
                break;
            }
        }
    });
    let address = received
        .recv_timeout(Duration::from_secs(30))
        .expect("foreground readiness");
    reader.join().unwrap();
    assert!(http(&address, "GET", "/apps/one/hello").ends_with("Hello, world!"));
    assert!(http(&address, "GET", "/hello").starts_with("HTTP/1.1 404"));
}
