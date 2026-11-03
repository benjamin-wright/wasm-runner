use super::{
    cache::{Active, Cache, Snapshot},
    config::{Config, Source},
    ports, release,
    status::{ApplicationStatus, SharedStatus, Status},
};
use crate::project::manifest;
use anyhow::{Context, Result, ensure};
use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::watch;

pub(crate) struct Reconciler<Repositories, Artifacts, Applications> {
    applications: Applications,
    status: SharedStatus,
    repositories: Repositories,
    artifacts: Artifacts,
    cache: Cache,
    snapshot: Snapshot,
}

impl<Repositories, Artifacts, Applications> Reconciler<Repositories, Artifacts, Applications>
where
    Repositories: ports::RepositoryFiles,
    Artifacts: ports::Artifacts,
    Applications: ports::ApplicationHost,
{
    pub fn new(
        root: PathBuf,
        source: Source,
        applications: Applications,
        status: SharedStatus,
        repositories: Repositories,
        artifacts: Artifacts,
    ) -> Result<Self> {
        source.validate()?;
        let cache = Cache::new(root.join("deployment"))?;
        let snapshot = cache.snapshot(source)?;
        Ok(Self {
            applications,
            status,
            repositories,
            artifacts,
            cache,
            snapshot,
        })
    }

    pub async fn run(
        mut self,
        interval: Duration,
        mut stopped: watch::Receiver<bool>,
    ) -> Result<()> {
        self.restore().await;
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            if *stopped.borrow() {
                return Ok(());
            }
            tokio::select! {
                _ = stopped.changed() => return Ok(()),
                _ = ticker.tick() => self.poll().await?,
            }
        }
    }

    pub async fn restore(&mut self) {
        let Some(config) = &self.snapshot.config else {
            return;
        };
        let mut status = Status {
            config_commit: self.snapshot.commit.clone(),
            error: Some("Serving cached state; upstream not yet observed".into()),
            ..Status::default()
        };
        for name in config.applications.keys() {
            let mut application_status = ApplicationStatus::default();
            if let Some(active) = self.snapshot.active.get(name) {
                let result = match active
                    .release
                    .deployment(|artifact| self.cache.read(artifact))
                {
                    Ok(deployment) if &deployment.name == name => {
                        self.applications.deploy(deployment).await
                    }
                    Ok(_) => Err(anyhow::anyhow!("cached application identity mismatch")),
                    Err(error) => Err(error),
                };
                match result {
                    Ok(()) => {
                        application_status.active_version = Some(active.release.version.clone());
                        application_status.active_digest = Some(active.digest.clone());
                    }
                    Err(error) => {
                        application_status.error =
                            Some(format!("restoring cached release: {error:#}"))
                    }
                }
            }
            status.applications.insert(name.clone(), application_status);
        }
        status.ready = status
            .applications
            .values()
            .all(|app| app.active_version.is_some());
        *self.status.write().expect("deployment status") = status;
    }

    pub async fn poll(&mut self) -> Result<()> {
        self.repositories.refresh();
        let config = self
            .repositories
            .read(&self.snapshot.source)
            .await
            .and_then(|(commit, bytes)| Ok((commit, Config::parse(&bytes)?)));
        let mut status = self.status.read().expect("deployment status").clone();
        status.last_poll = Some(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs());
        match config {
            Ok((commit, config)) => {
                self.snapshot.commit = Some(commit.clone());
                self.snapshot.config = Some(config);
                status.config_commit = Some(commit);
                status.error = None;
                // Persist authorization before removing routes, including an explicitly empty deployment.
                self.cache.save(&self.snapshot)?;
            }
            Err(error) => {
                status.error = Some(format!("reading deployment config: {error:#}"));
            }
        }
        if let Some(config) = self.snapshot.config.clone() {
            for name in self.applications.names() {
                if !config.applications.contains_key(&name) {
                    self.applications.remove(&name).await?;
                }
            }
            self.snapshot
                .active
                .retain(|name, _| config.applications.contains_key(name));
            status
                .applications
                .retain(|name, _| config.applications.contains_key(name));
            self.cache.save(&self.snapshot)?;
            status.ready = config.applications.keys().all(|name| {
                status
                    .applications
                    .get(name)
                    .is_some_and(|app| app.active_version.is_some())
            });
            *self.status.write().expect("deployment status") = status.clone();
            for (name, source) in config.applications {
                let application_status = status.applications.entry(name.clone()).or_default();
                if let Err(error) = self.reconcile(&name, &source, application_status).await {
                    application_status.error = Some(format!("{error:#}"));
                    eprintln!("Reconciliation failed for {name}: {error:#}");
                }
                *self.status.write().expect("deployment status") = status.clone();
            }
            status.ready = status
                .applications
                .values()
                .all(|app| app.active_version.is_some());
        }
        if status.error.is_none() && status.applications.values().all(|app| app.error.is_none()) {
            status.last_successful_poll = status.last_poll;
        }
        if let Some(error) = &status.error {
            eprintln!("{error}");
        }
        *self.status.write().expect("deployment status") = status;
        Ok(())
    }

    async fn reconcile(
        &mut self,
        name: &str,
        source: &Source,
        status: &mut ApplicationStatus,
    ) -> Result<()> {
        let (commit, bytes) = self.repositories.read(source).await?;
        let manifest = manifest::parse(std::str::from_utf8(&bytes)?)?;
        ensure!(
            manifest.name == name,
            "registration name must match application name"
        );
        let publish = manifest
            .publish
            .as_ref()
            .context("production application requires publish.registry and publish.version")?;
        status.source_commit = Some(commit);
        status.desired_version = Some(publish.version.clone());
        let runtime = manifest.runtime_config();
        let base = format!("{}/{name}", publish.registry);
        let version_key = format!("{base}:{}", publish.version);

        if let Some(active) = self.snapshot.active.get(name)
            && status.active_digest.as_ref() == Some(&active.digest)
            && self.snapshot.versions.get(&version_key) == Some(&active.digest)
            && active.release.version == publish.version
        {
            ensure!(
                active.release.application == runtime,
                "runtime configuration changed without a new publish.version"
            );
            status.error = None;
            return Ok(());
        }
        let (digest, release) = self.artifacts.release(&base, &publish.version).await?;
        release.validate()?;
        ensure!(
            release.version == publish.version,
            "release version does not match authorized version"
        );
        ensure!(
            release.application == runtime,
            "release runtime configuration does not match authorized application manifest"
        );
        if let Some(previous) = self.snapshot.versions.get(&version_key) {
            ensure!(
                previous == &digest,
                "published version changed digest; publish a new version"
            );
        }
        self.snapshot.versions.insert(version_key, digest.clone());
        self.cache.save(&self.snapshot)?;
        self.cache_artifacts(&base, &release).await?;
        self.check_authorization(name, source, &runtime, publish)
            .await?;
        let deployment = release.deployment(|artifact| self.cache.read(artifact))?;
        self.applications.deploy(deployment).await?;
        status.active_version = Some(release.version.clone());
        status.active_digest = Some(digest.clone());
        status.error = None;
        self.snapshot
            .active
            .insert(name.into(), Active { digest, release });
        self.cache.save(&self.snapshot)?;
        println!("Activated {name} version {}", publish.version);
        Ok(())
    }

    async fn cache_artifacts(&mut self, base: &str, release: &release::Release) -> Result<()> {
        for (function, artifact) in &release.functions {
            if self.cache.read(artifact).is_err() {
                let bytes = self
                    .artifacts
                    .artifact(
                        &format!("{base}/functions/{function}"),
                        artifact,
                        release::COMPONENT_TYPE,
                    )
                    .await?;
                self.cache.store(artifact, &bytes)?;
            }
        }
        if let Some(artifact) = &release.migrations
            && self.cache.read(artifact).is_err()
        {
            let bytes = self
                .artifacts
                .artifact(
                    &format!("{base}/migrations"),
                    artifact,
                    release::MIGRATIONS_TYPE,
                )
                .await?;
            self.cache.store(artifact, &bytes)?;
        }
        Ok(())
    }

    async fn check_authorization(
        &mut self,
        name: &str,
        source: &Source,
        runtime: &serde_json::Value,
        publish: &manifest::Publish,
    ) -> Result<()> {
        // Recheck authorization after downloads and before any migration or publication.
        self.repositories.refresh();
        let (_, config) = self.repositories.read(&self.snapshot.source).await?;
        ensure!(
            Config::parse(&config)?.applications.get(name) == Some(source),
            "application registration changed during preparation"
        );
        let (_, current) = self.repositories.read(source).await?;
        let current = manifest::parse(std::str::from_utf8(&current)?)?;
        ensure!(&current.runtime_config() == runtime && current.publish.as_ref().is_some_and(|p| p.registry == publish.registry && p.version == publish.version), "application changed during preparation");
        Ok(())
    }
}
