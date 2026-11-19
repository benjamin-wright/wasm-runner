use super::release::{
    APPLICATION_TYPE, Artifact, COMPONENT_TYPE, MIGRATIONS_TYPE, RELEASE_TYPE, Release, digest,
};
use crate::project::{manifest, package};
use anyhow::{Context, Result, ensure};
use oci_client::manifest::{OciDescriptor, OciImageManifest};
use serde_json::json;
use std::{collections::BTreeMap, fs, path::Path};

const MANIFEST_TYPE: &str = "application/vnd.oci.image.manifest.v1+json";
const EMPTY_TYPE: &str = "application/vnd.oci.empty.v1+json";
const TITLE: &str = "org.opencontainers.image.title";

pub(crate) struct PreparedPackage {
    pub(crate) name: String,
    pub(crate) registry: String,
    pub(crate) version: String,
    pub(crate) manifest: OciImageManifest,
    pub(crate) manifest_bytes: Vec<u8>,
    pub(crate) config: Vec<u8>,
    pub(crate) layers: Vec<PreparedLayer>,
}

pub(crate) struct PreparedLayer {
    pub(crate) descriptor: OciDescriptor,
    pub(crate) bytes: Vec<u8>,
}

pub(crate) fn prepare(application: &Path) -> Result<PreparedPackage> {
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
        payloads.push(prepared_layer(
            COMPONENT_TYPE,
            handler.name,
            handler.component,
        ));
    }
    if deployment.sql.enabled {
        let bytes = serde_json::to_vec(&deployment.migrations)?;
        release.migrations = Some(descriptor(&bytes));
        payloads.push(prepared_layer(MIGRATIONS_TYPE, "migrations", bytes));
    }
    release.validate()?;

    let release_bytes = serde_json::to_vec(&release)?;
    ensure!(
        release_bytes.len() <= 1024 * 1024,
        "release descriptor exceeds 1 MiB"
    );
    let release_layer = prepared_layer(RELEASE_TYPE, "release", release_bytes);
    let mut layers = vec![release_layer];
    layers.extend(payloads);
    let config = b"{}".to_vec();
    let manifest = OciImageManifest {
        schema_version: 2,
        media_type: Some(MANIFEST_TYPE.into()),
        config: oci_descriptor(EMPTY_TYPE, &config, None),
        layers: layers
            .iter()
            .map(|layer| layer.descriptor.clone())
            .collect(),
        subject: None,
        artifact_type: Some(APPLICATION_TYPE.into()),
        annotations: None,
    };
    let manifest_bytes = serde_json::to_vec(&manifest)?;

    Ok(PreparedPackage {
        name: deployment.name,
        registry: publish.registry.clone(),
        version: publish.version.clone(),
        manifest,
        manifest_bytes,
        config,
        layers,
    })
}

pub(crate) fn write(prepared: &PreparedPackage, output: &Path) -> Result<()> {
    ensure!(
        !output.exists(),
        "package output already exists; select a new output directory"
    );

    let blobs = output.join("blobs/sha256");
    fs::create_dir_all(&blobs)?;
    let manifest_digest = digest(&prepared.manifest_bytes);
    fs::write(blobs.join(&manifest_digest[7..]), &prepared.manifest_bytes)?;
    fs::write(
        blobs.join(&prepared.manifest.config.digest[7..]),
        &prepared.config,
    )?;
    for layer in &prepared.layers {
        fs::write(blobs.join(&layer.descriptor.digest[7..]), &layer.bytes)?;
    }
    fs::write(
        output.join("oci-layout"),
        br#"{"imageLayoutVersion":"1.0.0"}"#,
    )?;
    fs::write(
        output.join("index.json"),
        serde_json::to_vec(&json!({
            "schemaVersion": 2,
            "manifests": [{
                "mediaType": MANIFEST_TYPE,
                "digest": manifest_digest,
                "size": prepared.manifest_bytes.len(),
                "annotations": {"org.opencontainers.image.ref.name": prepared.version}
            }]
        }))?,
    )?;
    println!(
        "Packaged {}:{} at {}",
        prepared.name,
        prepared.version,
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

fn prepared_layer(media_type: &str, title: impl Into<String>, bytes: Vec<u8>) -> PreparedLayer {
    PreparedLayer {
        descriptor: oci_descriptor(media_type, &bytes, Some(title.into())),
        bytes,
    }
}

fn oci_descriptor(media_type: &str, bytes: &[u8], title: Option<String>) -> OciDescriptor {
    OciDescriptor {
        media_type: media_type.into(),
        digest: digest(bytes),
        size: bytes.len() as i64,
        urls: None,
        annotations: title.map(|title| BTreeMap::from([(TITLE.into(), title)])),
    }
}
