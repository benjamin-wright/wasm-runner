use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use super::{
    Reconciler, SharedStatus, Status,
    config::Source,
    ports::{ApplicationHost, Artifacts, RepositoryFiles},
    release::{Artifact, Release, digest},
};
use crate::hosting::deployment::Deployment;

#[derive(Clone, Default)]
struct Files(Arc<Mutex<BTreeMap<String, Vec<u8>>>>);
impl Files {
    fn put(&self, path: &str, value: Value) {
        self.0
            .lock()
            .unwrap()
            .insert(path.into(), serde_json::to_vec(&value).unwrap());
    }
}
impl RepositoryFiles for Files {
    fn refresh(&mut self) {}
    async fn read(&mut self, source: &Source) -> Result<(String, Vec<u8>)> {
        let bytes = self
            .0
            .lock()
            .unwrap()
            .get(&source.path)
            .cloned()
            .context("repository unavailable")?;
        Ok((digest(&bytes)[7..].into(), bytes))
    }
}

#[derive(Default)]
struct Published {
    releases: BTreeMap<String, (String, Release)>,
    blobs: BTreeMap<String, Vec<u8>>,
    after_download: Option<Box<dyn FnOnce() + Send>>,
}
#[derive(Clone, Default)]
struct Registry(Arc<Mutex<Published>>);
impl Registry {
    fn publish(&self, name: &str, release: Release) {
        let digest = digest(&serde_json::to_vec(&release).unwrap());
        self.0.lock().unwrap().releases.insert(
            format!("registry.example/team/{name}:{}", release.version),
            (digest, release),
        );
    }
}
impl Artifacts for Registry {
    async fn release(&mut self, application: &str, version: &str) -> Result<(String, Release)> {
        self.0
            .lock()
            .unwrap()
            .releases
            .get(&format!("{application}:{version}"))
            .cloned()
            .context("release not published")
    }
    async fn artifact(&mut self, _: &str, expected: &Artifact, _: &str) -> Result<Vec<u8>> {
        let mut published = self.0.lock().unwrap();
        let bytes = published
            .blobs
            .get(&expected.blob)
            .cloned()
            .context("artifact unavailable")?;
        if let Some(change) = published.after_download.take() {
            change();
        }
        Ok(bytes)
    }
}

#[derive(Default)]
struct Applications {
    active: BTreeMap<String, Deployment>,
    rejected: BTreeSet<String>,
    installations: usize,
}
#[derive(Clone, Default)]
struct Host(Arc<Mutex<Applications>>);
impl ApplicationHost for Host {
    fn names(&self) -> Vec<String> {
        self.0.lock().unwrap().active.keys().cloned().collect()
    }
    async fn deploy(&self, deployment: Deployment) -> Result<()> {
        let mut host = self.0.lock().unwrap();
        if host.rejected.contains(&deployment.name) {
            bail!("application preparation failed");
        }
        host.active.insert(deployment.name.clone(), deployment);
        host.installations += 1;
        Ok(())
    }
    async fn remove(&self, name: &str) -> Result<()> {
        self.0.lock().unwrap().active.remove(name);
        Ok(())
    }
}

struct Fixture {
    root: PathBuf,
    files: Files,
    registry: Registry,
    host: Host,
    status: SharedStatus,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "wr-reconcile-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let fixture = Self {
            root,
            files: Files::default(),
            registry: Registry::default(),
            host: Host::default(),
            status: Arc::new(RwLock::new(Status::default())),
        };
        fixture.register(&[]);
        fixture
    }
    fn source(path: &str) -> Source {
        Source {
            repo: "https://repo.example/apps.git".into(),
            branch: "main".into(),
            path: path.into(),
        }
    }
    fn controller(&self) -> Reconciler<Files, Registry, Host> {
        Reconciler::new(
            self.root.clone(),
            Self::source("config.yaml"),
            self.host.clone(),
            self.status.clone(),
            self.files.clone(),
            self.registry.clone(),
        )
        .unwrap()
    }
    fn register(&self, names: &[&str]) {
        let applications: BTreeMap<_, _> = names
            .iter()
            .map(|name| (name.to_string(), Self::source(&format!("{name}.yaml"))))
            .collect();
        self.files
            .put("config.yaml", json!({"applications": applications}));
    }
    fn authorize(&self, name: &str, version: &str) -> Release {
        let application = json!({"name": name, "path-prefix": format!("/{name}"), "kv": false, "sql": {"enabled": false}, "functions": {"hello": {"http": {"method": "GET", "path": "/"}}}});
        let mut manifest = application.clone();
        manifest["publish"] = json!({"registry": "registry.example/team", "version": version});
        self.files.put(&format!("{name}.yaml"), manifest);
        let bytes = format!("component {name} {version}").into_bytes();
        let artifact = Artifact {
            manifest: digest(format!("OCI {name} {version}").as_bytes()),
            blob: digest(&bytes),
            size: bytes.len() as u64,
        };
        self.registry
            .0
            .lock()
            .unwrap()
            .blobs
            .insert(artifact.blob.clone(), bytes);
        Release {
            format_version: 1,
            source_commit: "a".repeat(40),
            version: version.into(),
            application,
            functions: BTreeMap::from([("hello".into(), artifact)]),
            migrations: None,
        }
    }
    fn error(&self, name: &str) -> Option<String> {
        self.status.read().unwrap().applications[name].error.clone()
    }
    fn active(&self, name: &str) -> Option<String> {
        self.status.read().unwrap().applications[name]
            .active_version
            .clone()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
async fn authorization_waits_for_a_complete_release_and_retries_without_git_changes() {
    let fixture = Fixture::new();
    let release = fixture.authorize("first", "1");
    fixture.register(&["first"]);
    let mut controller = fixture.controller();
    controller.poll().await.unwrap();
    assert!(fixture.error("first").unwrap().contains("not published"));
    assert!(fixture.host.names().is_empty());
    fixture.registry.publish("first", release.clone());
    let blob = &release.functions["hello"].blob;
    let bytes = fixture
        .registry
        .0
        .lock()
        .unwrap()
        .blobs
        .remove(blob)
        .unwrap();
    controller.poll().await.unwrap();
    assert!(
        fixture
            .error("first")
            .unwrap()
            .contains("artifact unavailable")
    );
    assert!(!fixture.status.read().unwrap().ready);
    fixture
        .registry
        .0
        .lock()
        .unwrap()
        .blobs
        .insert(blob.clone(), bytes);
    controller.poll().await.unwrap();
    assert_eq!(fixture.active("first").as_deref(), Some("1"));
    assert!(fixture.status.read().unwrap().ready);
    controller.poll().await.unwrap();
    assert_eq!(
        fixture.host.0.lock().unwrap().installations,
        1,
        "no-op polls must retain the generation"
    );
}

#[tokio::test]
async fn failed_replacement_preserves_active_application_and_does_not_block_others() {
    let fixture = Fixture::new();
    fixture
        .registry
        .publish("first", fixture.authorize("first", "1"));
    fixture.register(&["first"]);
    let mut controller = fixture.controller();
    controller.poll().await.unwrap();
    fixture
        .registry
        .publish("first", fixture.authorize("first", "2"));
    fixture
        .registry
        .publish("second", fixture.authorize("second", "1"));
    fixture.register(&["first", "second"]);
    fixture
        .host
        .0
        .lock()
        .unwrap()
        .rejected
        .insert("first".into());
    controller.poll().await.unwrap();
    assert_eq!(fixture.active("first").as_deref(), Some("1"));
    assert_eq!(fixture.active("second").as_deref(), Some("1"));
    assert_eq!(fixture.host.names(), ["first", "second"]);
    assert!(
        fixture
            .error("first")
            .unwrap()
            .contains("preparation failed")
    );
    fixture.host.0.lock().unwrap().rejected.clear();
    controller.poll().await.unwrap();
    assert_eq!(fixture.active("first").as_deref(), Some("2"));
}

#[tokio::test]
async fn invalid_configuration_preserves_membership_and_explicit_removal_survives_restart() {
    let mut fixture = Fixture::new();
    fixture
        .registry
        .publish("first", fixture.authorize("first", "1"));
    fixture
        .registry
        .publish("second", fixture.authorize("second", "1"));
    fixture.register(&["first", "second"]);
    let mut controller = fixture.controller();
    controller.poll().await.unwrap();
    fixture.files.put("config.yaml", json!({}));
    fixture.files.0.lock().unwrap().remove("first.yaml");
    controller.poll().await.unwrap();
    assert!(fixture.status.read().unwrap().error.is_some());
    assert_eq!(fixture.host.names(), ["first", "second"]);
    fixture.register(&["second"]);
    controller.poll().await.unwrap();
    assert_eq!(fixture.host.names(), ["second"]);
    drop(controller);
    fixture.files.0.lock().unwrap().clear();
    fixture.registry.0.lock().unwrap().releases.clear();
    fixture.registry.0.lock().unwrap().blobs.clear();
    fixture.host = Host::default();
    fixture.status = Arc::new(RwLock::new(Status::default()));
    let mut controller = fixture.controller();
    controller.restore().await;
    assert_eq!(fixture.host.names(), ["second"]);
    assert!(fixture.status.read().unwrap().ready);
    controller.poll().await.unwrap();
    assert_eq!(fixture.host.names(), ["second"]);
    fixture.register(&[]);
    controller.poll().await.unwrap();
    assert!(fixture.host.names().is_empty());
    assert!(fixture.status.read().unwrap().ready);
}

#[tokio::test]
async fn changed_runtime_configuration_requires_a_new_version() {
    let fixture = Fixture::new();
    fixture
        .registry
        .publish("first", fixture.authorize("first", "1"));
    fixture.register(&["first"]);
    let mut controller = fixture.controller();
    controller.poll().await.unwrap();
    let mut manifest: Value =
        serde_json::from_slice(&fixture.files.0.lock().unwrap()["first.yaml"]).unwrap();
    manifest["path-prefix"] = json!("/changed");
    fixture.files.put("first.yaml", manifest);
    controller.poll().await.unwrap();
    assert!(fixture.error("first").unwrap().contains("without a new"));
    assert_eq!(
        fixture.host.0.lock().unwrap().active["first"].path_prefix,
        "/first"
    );
}

#[tokio::test]
async fn corrupt_artifacts_and_reassigned_versions_never_install() {
    let fixture = Fixture::new();
    let mut release = fixture.authorize("first", "1");
    fixture.registry.publish("first", release.clone());
    fixture.register(&["first"]);
    fixture
        .registry
        .0
        .lock()
        .unwrap()
        .blobs
        .insert(release.functions["hello"].blob.clone(), b"corrupt".to_vec());
    let mut controller = fixture.controller();
    controller.poll().await.unwrap();
    assert!(fixture.error("first").unwrap().contains("mismatch"));
    assert!(fixture.host.names().is_empty());
    release.source_commit = "b".repeat(40);
    fixture.registry.publish("first", release);
    controller.poll().await.unwrap();
    assert!(fixture.error("first").unwrap().contains("changed digest"));
    assert!(fixture.host.names().is_empty());
}

#[tokio::test]
async fn withdrawn_authorization_during_download_prevents_installation() {
    let fixture = Fixture::new();
    fixture
        .registry
        .publish("first", fixture.authorize("first", "1"));
    fixture.register(&["first"]);
    let files = fixture.files.clone();
    fixture.registry.0.lock().unwrap().after_download = Some(Box::new(move || {
        files.put("config.yaml", json!({"applications": {}}))
    }));
    let mut controller = fixture.controller();
    controller.poll().await.unwrap();
    assert!(
        fixture
            .error("first")
            .unwrap()
            .contains("registration changed")
    );
    assert!(fixture.host.names().is_empty());
    controller.poll().await.unwrap();
    assert!(fixture.status.read().unwrap().applications.is_empty());
}
