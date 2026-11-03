use super::{
    config::{Config, Source},
    release::{Artifact, LIMIT, Release},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Active {
    pub digest: String,
    pub release: Release,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Snapshot {
    pub source: Source,
    pub config: Option<Config>,
    pub commit: Option<String>,
    pub active: BTreeMap<String, Active>,
    pub versions: BTreeMap<String, String>,
}

pub(crate) struct Cache {
    root: PathBuf,
}
impl Cache {
    pub fn new(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(root.join("blobs"))?;
        Ok(Self { root })
    }
    pub fn snapshot(&self, source: Source) -> Result<Snapshot> {
        let path = self.root.join("snapshot.json");
        if !path.exists() {
            return Ok(Snapshot {
                source,
                config: None,
                commit: None,
                active: BTreeMap::new(),
                versions: BTreeMap::new(),
            });
        }
        let snapshot: Snapshot = serde_json::from_slice(&read(&path, LIMIT)?)?;
        ensure!(
            snapshot.source == source,
            "state directory belongs to a different config source"
        );
        if let Some(config) = &snapshot.config {
            Config::parse(&serde_json::to_vec(config)?)?;
        }
        Ok(snapshot)
    }
    pub fn save(&self, snapshot: &Snapshot) -> Result<()> {
        let bytes = serde_json::to_vec(snapshot)?;
        ensure!(bytes.len() <= LIMIT, "deployment snapshot exceeds 128 MiB");
        atomic_write(&self.root.join("snapshot.json"), &bytes)
    }
    pub fn read(&self, artifact: &Artifact) -> Result<Vec<u8>> {
        artifact.validate()?;
        let bytes = read(&self.root.join("blobs").join(&artifact.blob[7..]), LIMIT)?;
        artifact.verify(&bytes)?;
        Ok(bytes)
    }
    pub fn store(&self, artifact: &Artifact, bytes: &[u8]) -> Result<()> {
        artifact.verify(bytes)?;
        atomic_write(&self.root.join("blobs").join(&artifact.blob[7..]), bytes)
    }
}

fn read(path: &Path, limit: usize) -> Result<Vec<u8>> {
    ensure!(
        fs::symlink_metadata(path)?.is_file(),
        "cache entry must be a regular file"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= limit, "cache entry exceeds size limit");
    Ok(bytes)
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if fs::read(path).ok().as_deref() == Some(bytes) {
        return Ok(());
    }
    let temporary = path.with_extension("tmp");
    if temporary.exists() {
        ensure!(
            fs::symlink_metadata(&temporary)?.is_file(),
            "temporary cache entry must be a regular file"
        );
    }
    let mut file = fs::File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&temporary, path).with_context(|| format!("saving {}", path.display()))?;
    #[cfg(unix)]
    fs::File::open(path.parent().context("cache path has no parent")?)?.sync_all()?;
    Ok(())
}
