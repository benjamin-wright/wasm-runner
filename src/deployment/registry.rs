use super::package::PreparedPackage;
use super::release::{
    APPLICATION_TYPE, Artifact, COMPONENT_TYPE, LIMIT, MIGRATIONS_TYPE, RELEASE_TYPE, Release,
};
use anyhow::{Result, bail, ensure};
use oci_client::{
    Client, Reference,
    client::{ClientConfig, ClientProtocol, Config, ImageLayer},
    manifest::{OciDescriptor, OciImageManifest},
    secrets::RegistryAuth,
};
use std::{
    env,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::io::AsyncWrite;

const TITLE: &str = "org.opencontainers.image.title";
const RELEASE_LIMIT: usize = 1024 * 1024;

pub(crate) struct PublicArtifacts;

impl super::ports::Artifacts for PublicArtifacts {
    async fn release(&mut self, application: &str, version: &str) -> Result<(String, Release)> {
        Registry::new(application)?
            .release(application, version)
            .await
    }
    async fn artifact(
        &mut self,
        repository: &str,
        release_manifest: &str,
        expected: &Artifact,
        media_type: &str,
    ) -> Result<Vec<u8>> {
        Registry::new(repository)?
            .artifact(repository, release_manifest, expected, media_type)
            .await
    }
}

pub(crate) struct Registry {
    client: Client,
}

impl Registry {
    pub fn new(registry: &str) -> Result<Self> {
        let reference: Reference = format!("{registry}/wr-validation:check").parse()?;
        ensure!(
            registry.contains('/') && !registry.contains("://") && !registry.contains('@'),
            "registry must be a host and namespace, without a scheme or credentials"
        );
        let host = reference.registry();
        let loopback = host
            .split(':')
            .next()
            .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1"));
        let config = ClientConfig {
            protocol: if loopback {
                ClientProtocol::HttpsExcept(vec![host.to_owned()])
            } else {
                ClientProtocol::Https
            },
            ..Default::default()
        };
        Ok(Self {
            client: Client::new(config),
        })
    }

    pub async fn release(&self, application: &str, version: &str) -> Result<(String, Release)> {
        let reference: Reference = format!("{application}:{version}").parse()?;
        tokio::time::timeout(Duration::from_secs(60), async {
            let (manifest, manifest_digest) =
                self.manifest(&reference, &RegistryAuth::Anonymous).await?;
            ensure!(
                manifest.artifact_type.as_deref() == Some(APPLICATION_TYPE),
                "invalid WR application artifact type"
            );
            let release_layer = unique_layer(&manifest.layers, |layer| {
                title(layer) == Some("release") && layer.media_type == RELEASE_TYPE
            })?;
            let bytes = self.blob(&reference, release_layer, RELEASE_LIMIT).await?;
            let release: Release = serde_json::from_slice(&bytes)?;
            release.validate()?;
            ensure!(
                release.version == version,
                "release version does not match authorized version"
            );
            validate_layers(&manifest, &release)?;
            Ok((manifest_digest, release))
        })
        .await?
    }

    pub async fn artifact(
        &self,
        application: &str,
        release_manifest: &str,
        expected: &Artifact,
        media_type: &str,
    ) -> Result<Vec<u8>> {
        expected.validate()?;
        let reference: Reference = format!("{application}@{release_manifest}").parse()?;
        tokio::time::timeout(Duration::from_secs(60), async {
            let (manifest, _) = self.manifest(&reference, &RegistryAuth::Anonymous).await?;
            ensure!(
                manifest.artifact_type.as_deref() == Some(APPLICATION_TYPE),
                "invalid WR application artifact type"
            );
            let layer = unique_layer(&manifest.layers, |layer| {
                layer.media_type == media_type
                    && layer.digest == expected.blob
                    && layer.size >= 0
                    && layer.size as u64 == expected.size
            })?;
            self.blob(&reference, layer, LIMIT).await
        })
        .await?
    }

    pub async fn publish(&self, prepared: &PreparedPackage, auth: &RegistryAuth) -> Result<String> {
        let reference: Reference = format!(
            "{}/{}:{}",
            prepared.registry, prepared.name, prepared.version
        )
        .parse()?;
        let layers = prepared
            .layers
            .iter()
            .map(|layer| {
                ImageLayer::new(
                    layer.bytes.clone(),
                    layer.descriptor.media_type.clone(),
                    layer.descriptor.annotations.clone(),
                )
            })
            .collect::<Vec<_>>();
        let config = Config::new(
            prepared.config.clone(),
            prepared.manifest.config.media_type.clone(),
            None,
        );
        tokio::time::timeout(Duration::from_secs(60), async {
            self.client
                .push(
                    &reference,
                    &layers,
                    config,
                    auth,
                    Some(prepared.manifest.clone()),
                )
                .await?;
            let (published, digest) = self.manifest(&reference, auth).await?;
            ensure!(
                serde_json::to_value(published)? == serde_json::to_value(&prepared.manifest)?,
                "published manifest differs from prepared artifact"
            );
            Ok(digest)
        })
        .await?
    }

    async fn manifest(
        &self,
        reference: &Reference,
        auth: &RegistryAuth,
    ) -> Result<(OciImageManifest, String)> {
        let (manifest, digest) = self.client.pull_image_manifest(reference, auth).await?;
        super::release::validate_digest(&digest)?;
        if let Some(expected) = reference.digest() {
            ensure!(expected == digest, "OCI manifest digest mismatch");
        }
        Ok((manifest, digest))
    }

    async fn blob(
        &self,
        reference: &Reference,
        layer: &OciDescriptor,
        limit: usize,
    ) -> Result<Vec<u8>> {
        ensure!(
            layer.size >= 0 && layer.size as u64 <= limit as u64,
            "artifact exceeds declared limit"
        );
        let expected = Artifact {
            blob: layer.digest.clone(),
            size: layer.size as u64,
        };
        expected.validate()?;
        let mut output = LimitedBytes {
            bytes: Vec::new(),
            limit: layer.size as usize,
        };
        self.client.pull_blob(reference, layer, &mut output).await?;
        expected.verify(&output.bytes)?;
        Ok(output.bytes)
    }
}

pub(crate) fn auth_from_environment() -> Result<RegistryAuth> {
    let token = environment("WR_REGISTRY_TOKEN")?;
    let username = environment("WR_REGISTRY_USERNAME")?;
    let password = environment("WR_REGISTRY_PASSWORD")?;
    if token.is_some() && (username.is_some() || password.is_some()) {
        bail!("WR_REGISTRY_TOKEN cannot be combined with username or password credentials");
    }
    if let Some(token) = token {
        ensure!(!token.is_empty(), "WR_REGISTRY_TOKEN cannot be empty");
        return Ok(RegistryAuth::Bearer(token));
    }
    match (username, password) {
        (None, None) => Ok(RegistryAuth::Anonymous),
        (Some(username), Some(password)) => {
            ensure!(!username.is_empty(), "WR_REGISTRY_USERNAME cannot be empty");
            ensure!(!password.is_empty(), "WR_REGISTRY_PASSWORD cannot be empty");
            Ok(RegistryAuth::Basic(username, password))
        }
        _ => bail!("WR_REGISTRY_USERNAME and WR_REGISTRY_PASSWORD must be set together"),
    }
}

fn environment(name: &str) -> Result<Option<String>> {
    match env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => bail!("{name} must contain valid Unicode"),
    }
}

fn validate_layers(manifest: &OciImageManifest, release: &Release) -> Result<()> {
    let expected_count = 1 + release.functions.len() + usize::from(release.migrations.is_some());
    ensure!(
        manifest.layers.len() == expected_count,
        "application manifest layers do not match release"
    );
    for (name, artifact) in &release.functions {
        matching_layer(&manifest.layers, name, COMPONENT_TYPE, artifact)?;
    }
    if let Some(artifact) = &release.migrations {
        matching_layer(&manifest.layers, "migrations", MIGRATIONS_TYPE, artifact)?;
    }
    Ok(())
}

fn matching_layer<'a>(
    layers: &'a [OciDescriptor],
    name: &str,
    media_type: &str,
    expected: &Artifact,
) -> Result<&'a OciDescriptor> {
    unique_layer(layers, |layer| {
        title(layer) == Some(name)
            && layer.media_type == media_type
            && layer.digest == expected.blob
            && layer.size >= 0
            && layer.size as u64 == expected.size
    })
}

fn unique_layer(
    layers: &[OciDescriptor],
    predicate: impl Fn(&OciDescriptor) -> bool,
) -> Result<&OciDescriptor> {
    let mut matches = layers.iter().filter(|layer| predicate(layer));
    let layer = matches
        .next()
        .ok_or_else(|| anyhow::anyhow!("application manifest is missing a required layer"))?;
    ensure!(
        matches.next().is_none(),
        "application manifest contains duplicate required layers"
    );
    Ok(layer)
}

fn title(layer: &OciDescriptor) -> Option<&str> {
    layer.annotations.as_ref()?.get(TITLE).map(String::as_str)
}

struct LimitedBytes {
    bytes: Vec<u8>,
    limit: usize,
}
impl AsyncWrite for LimitedBytes {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Poll::Ready(Err(std::io::Error::other("artifact exceeds declared size")));
        }
        self.bytes.extend_from_slice(bytes);
        Poll::Ready(Ok(bytes.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
