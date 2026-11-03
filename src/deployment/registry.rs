use super::release::{Artifact, LIMIT, RELEASE_TYPE, Release};
use anyhow::{Result, ensure};
use oci_client::{
    Client, Reference,
    client::{ClientConfig, ClientProtocol},
    secrets::RegistryAuth,
};
use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::io::AsyncWrite;

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
        expected: &Artifact,
        media_type: &str,
    ) -> Result<Vec<u8>> {
        Registry::new(repository)?
            .artifact(repository, expected, media_type)
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

    async fn payload(
        &self,
        reference: &str,
        media_type: &str,
        limit: usize,
    ) -> Result<(String, Vec<u8>, Artifact)> {
        let reference: Reference = reference.parse()?;
        tokio::time::timeout(Duration::from_secs(60), async {
            let (manifest, manifest_digest) = self
                .client
                .pull_image_manifest(&reference, &RegistryAuth::Anonymous)
                .await?;
            super::release::validate_digest(&manifest_digest)?;
            if let Some(expected) = reference.digest() {
                ensure!(expected == manifest_digest, "OCI manifest digest mismatch");
            }
            ensure!(
                manifest.layers.len() == 1,
                "WR artifacts require exactly one layer"
            );
            let layer = &manifest.layers[0];
            ensure!(
                layer.media_type == media_type
                    && layer.size >= 0
                    && layer.size as u64 <= limit as u64,
                "invalid artifact media type or size"
            );
            let artifact = Artifact {
                manifest: manifest_digest.clone(),
                blob: layer.digest.clone(),
                size: layer.size as u64,
            };
            artifact.validate()?;
            let mut output = LimitedBytes {
                bytes: Vec::new(),
                limit: layer.size as usize,
            };
            self.client
                .pull_blob(&reference, layer, &mut output)
                .await?;
            artifact.verify(&output.bytes)?;
            Ok((manifest_digest, output.bytes, artifact))
        })
        .await?
    }

    pub async fn release(&self, base: &str, version: &str) -> Result<(String, Release)> {
        let (digest, bytes, _) = self
            .payload(
                &format!("{base}/release:{version}"),
                RELEASE_TYPE,
                1024 * 1024,
            )
            .await?;
        let release: Release = serde_json::from_slice(&bytes)?;
        release.validate()?;
        ensure!(
            release.version == version,
            "release version does not match authorized version"
        );
        Ok((digest, release))
    }

    pub async fn artifact(
        &self,
        repository: &str,
        expected: &Artifact,
        media_type: &str,
    ) -> Result<Vec<u8>> {
        expected.validate()?;
        let (_, bytes, actual) = self
            .payload(
                &format!("{repository}@{}", expected.manifest),
                media_type,
                LIMIT,
            )
            .await?;
        ensure!(
            actual.blob == expected.blob && actual.size == expected.size,
            "artifact descriptor does not match release"
        );
        Ok(bytes)
    }
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
