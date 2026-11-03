use super::release::{Artifact, COMPONENT_TYPE, MIGRATIONS_TYPE, RELEASE_TYPE, Release, digest};
use crate::project::{manifest, package};
use anyhow::{Context, Result, ensure};
use serde_json::json;
use std::{collections::BTreeMap, fs, path::Path};

pub(crate) fn write(application: &Path, output: &Path, source_commit: String) -> Result<()> {
    let manifest = manifest::load(application)?;
    let publish = manifest
        .publish
        .as_ref()
        .context("packaging requires publish.registry and publish.version")?;
    super::registry::Registry::new(&publish.registry)?;
    let deployment = package::deployment(application)?;
    deployment.validate()?;
    let mut release = Release {
        format_version: 1,
        source_commit,
        version: publish.version.clone(),
        application: manifest.runtime_config(),
        functions: BTreeMap::new(),
        migrations: None,
    };
    let mut artifacts = Vec::new();
    for handler in deployment.handlers {
        artifacts.push((
            format!("functions/{}", handler.name),
            handler.component,
            COMPONENT_TYPE,
        ));
    }
    if deployment.sql.enabled {
        artifacts.push((
            "migrations".into(),
            serde_json::to_vec(&deployment.migrations)?,
            MIGRATIONS_TYPE,
        ));
    }
    let prepared: Vec<_> = artifacts
        .into_iter()
        .map(|(path, bytes, media)| {
            let (descriptor, manifest) = descriptor(&bytes, media);
            if let Some(name) = path.strip_prefix("functions/") {
                release.functions.insert(name.into(), descriptor.clone());
            } else {
                release.migrations = Some(descriptor.clone());
            }
            (path, bytes, descriptor, manifest)
        })
        .collect();
    release.validate()?;
    let release_bytes = serde_json::to_vec(&release)?;
    ensure!(
        release_bytes.len() <= 1024 * 1024,
        "release descriptor exceeds 1 MiB"
    );
    ensure!(
        !output.exists(),
        "package output already exists; select a new output directory"
    );
    fs::create_dir_all(output)?;
    for (path, bytes, descriptor, manifest) in prepared {
        layout(
            &output.join(path),
            &bytes,
            &descriptor,
            &manifest,
            &publish.version,
        )?;
    }
    let bytes = release_bytes;
    let (descriptor, manifest) = descriptor(&bytes, RELEASE_TYPE);
    layout(
        &output.join("release"),
        &bytes,
        &descriptor,
        &manifest,
        &publish.version,
    )?;
    println!(
        "Packaged {}:{} at {}",
        deployment.name,
        publish.version,
        output.display()
    );
    Ok(())
}

fn descriptor(bytes: &[u8], media: &str) -> (Artifact, Vec<u8>) {
    let blob = digest(bytes);
    let manifest = serde_json::to_vec(&json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "artifactType": media,
        "config": {"mediaType": "application/vnd.oci.empty.v1+json", "digest": digest(b"{}"), "size": 2},
        "layers": [{"mediaType": media, "digest": blob, "size": bytes.len()}]
    })).expect("serializable OCI manifest");
    (
        Artifact {
            manifest: digest(&manifest),
            blob,
            size: bytes.len() as u64,
        },
        manifest,
    )
}

fn layout(
    directory: &Path,
    bytes: &[u8],
    descriptor: &Artifact,
    manifest: &[u8],
    version: &str,
) -> Result<()> {
    let blobs = directory.join("blobs/sha256");
    fs::create_dir_all(&blobs)?;
    fs::write(blobs.join(&descriptor.blob[7..]), bytes)?;
    fs::write(blobs.join(&descriptor.manifest[7..]), manifest)?;
    fs::write(blobs.join(&digest(b"{}")[7..]), b"{}")?;
    fs::write(
        directory.join("oci-layout"),
        br#"{"imageLayoutVersion":"1.0.0"}"#,
    )?;
    fs::write(
        directory.join("index.json"),
        serde_json::to_vec(
            &json!({"schemaVersion": 2, "manifests": [{"mediaType": "application/vnd.oci.image.manifest.v1+json", "digest": descriptor.manifest, "size": manifest.len(), "annotations": {"org.opencontainers.image.ref.name": version}}]}),
        )?,
    )?;
    Ok(())
}
