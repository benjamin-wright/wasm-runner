use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

const EXAMPLE: &str = include_str!("fixtures/application.yaml");
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
            "name: routes\nfunctions:\n  get:\n    http: {{method: {method}, path: '/users/<id>'}}\n  post:\n    http: {{method: POST, path: /same}}\n  slash:\n    http: {{method: GET, path: /same/}}\n"
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
    let source = "name: !!str on\nfunctions:\n  greet: &handler\n    http: {method: GET, path: /hello}\n  echo:\n    <<: *handler\n    http: {method: POST, path: /echo}\n";
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
            String::from("name: hello\nfunctions: {}"),
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
        String::from("name: hello\nfunctions: null"),
        EXAMPLE.replace("name: hello", ""),
        String::from("name: hello\nfunctions: []"),
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
    for command in ["run", "setup", "build"] {
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

    let output = fixture.build(&["--locked"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(!fixture.directory.join("Cargo.lock").exists());
    assert!(!fixture.directory.join(".wr/components").exists());

    let mut debug_artifact_time = None;
    for arguments in [vec![], vec!["--locked"], vec!["--release", "--locked"]] {
        let output = fixture.build(&arguments);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
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

    let lockfile = fs::read(fixture.directory.join("Cargo.lock")).unwrap();
    let destination = fixture.directory.join(".wr/components/greet.wasm");
    let previous = fs::read(&destination).unwrap();
    fs::write(fixture.directory.join("functions/greet.rs"), "invalid Rust").unwrap();
    let output = fixture.build(&["--locked"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Built "));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Cargo build failed"));
    assert_eq!(fs::read(destination).unwrap(), previous);
    assert_eq!(
        fs::read(fixture.directory.join("Cargo.lock")).unwrap(),
        lockfile
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
