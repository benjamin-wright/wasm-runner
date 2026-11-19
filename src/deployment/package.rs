use super::release::{
    APPLICATION_TYPE, Artifact, COMPONENT_TYPE, MIGRATIONS_TYPE, RELEASE_TYPE, Release, digest,
};
use crate::project::{manifest, package};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

pub(crate) fn write(application: &Path, output: &Path) -> Result<()> {
    let manifest = manifest::load(application)?;
    let publish = manifest
        .publish
        .as_ref()
        .context("packaging requires publish.registry and publish.version")?;
    super::registry::Registry::new(&publish.registry)?;
    let deployment = package::deployment(application)?;
    deployment.validate()?;

    let mut release = Release {
        version: publish.version.clone(),
        application: manifest.runtime_config(),
        functions: BTreeMap::new(),
        migrations: None,
    };
    let mut payloads = Vec::new();
    for handler in deployment.handlers {
        release
            .functions
            .insert(handler.name.clone(), descriptor(&handler.component));
        payloads.push((handler.component, layer(COMPONENT_TYPE, handler.name)));
    }
    if deployment.sql.enabled {
        let bytes = serde_json::to_vec(&deployment.migrations)?;
        release.migrations = Some(descriptor(&bytes));
        payloads.push((bytes, layer(MIGRATIONS_TYPE, "migrations")));
    }
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

    let release_layer = layer(RELEASE_TYPE, "release");
    let mut layers = vec![descriptor_json(release_layer, &release_bytes)];
    let mut blobs = vec![(descriptor(&release_bytes), release_bytes)];
    for (bytes, media) in payloads {
        let artifact = descriptor(&bytes);
        layers.push(descriptor_json(media, &bytes));
        blobs.push((artifact, bytes));
    }
    let manifest = serde_json::to_vec(&json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "artifactType": APPLICATION_TYPE,
        "config": {"mediaType": "application/vnd.oci.empty.v1+json", "digest": digest(b"{}"), "size": 2},
        "layers": layers
    }))?;
    let manifest_digest = digest(&manifest);
    let manifest_size = manifest.len();

    fs::create_dir_all(output.join("blobs/sha256"))?;
    fs::write(
        output.join("blobs/sha256").join(&manifest_digest[7..]),
        manifest,
    )?;
    fs::write(output.join("blobs/sha256").join(&digest(b"{}")[7..]), b"{}")?;
    for (artifact, bytes) in blobs {
        fs::write(output.join("blobs/sha256").join(&artifact.blob[7..]), bytes)?;
    }
    fs::write(
        output.join("oci-layout"),
        br#"{"imageLayoutVersion":"1.0.0"}"#,
    )?;
    fs::write(
        output.join("index.json"),
        serde_json::to_vec(
            &json!({"schemaVersion": 2, "manifests": [{"mediaType": "application/vnd.oci.image.manifest.v1+json", "digest": manifest_digest, "size": manifest_size, "annotations": {"org.opencontainers.image.ref.name": publish.version}}]}),
        )?,
    )?;
    println!(
        "Packaged {}:{} at {}",
        deployment.name,
        publish.version,
        output.display()
    );
    Ok(())
}

fn descriptor(bytes: &[u8]) -> Artifact {
    Artifact {
        blob: digest(bytes),
        size: bytes.len() as u64,
    }
}

fn layer(media_type: &str, name: impl Into<String>) -> (&str, Value) {
    (
        media_type,
        json!({"org.opencontainers.image.title": name.into()}),
    )
}

fn descriptor_json((media_type, annotations): (&str, Value), bytes: &[u8]) -> Value {
    let artifact = descriptor(bytes);
    json!({
        "mediaType": media_type,
        "digest": artifact.blob,
        "size": artifact.size,
        "annotations": annotations
    })
}
