use std::future::Future;

use anyhow::Result;

use super::{
    config::Source,
    release::{Artifact, Release},
};
use crate::hosting::deployment::Deployment;

pub(crate) trait RepositoryFiles: Send {
    /// Start a new observation; reads may share resolved commits until refreshed.
    fn refresh(&mut self);
    fn read(&mut self, source: &Source) -> impl Future<Output = Result<(String, Vec<u8>)>> + Send;
}

pub(crate) trait Artifacts: Send {
    fn release(
        &mut self,
        application: &str,
        version: &str,
    ) -> impl Future<Output = Result<(String, Release)>> + Send;
    fn artifact(
        &mut self,
        repository: &str,
        expected: &Artifact,
        media_type: &str,
    ) -> impl Future<Output = Result<Vec<u8>>> + Send;
}

pub(crate) trait ApplicationHost: Send + Sync {
    fn names(&self) -> Vec<String>;
    fn deploy(&self, deployment: Deployment) -> impl Future<Output = Result<()>> + Send;
    fn remove(&self, name: &str) -> impl Future<Output = Result<()>> + Send;
}

impl ApplicationHost for std::sync::Arc<crate::hosting::applications::Applications> {
    fn names(&self) -> Vec<String> {
        self.list().into_iter().map(|app| app.name).collect()
    }
    async fn deploy(&self, deployment: Deployment) -> Result<()> {
        self.as_ref().deploy(deployment).await
    }
    async fn remove(&self, name: &str) -> Result<()> {
        self.as_ref().remove(name).await
    }
}
