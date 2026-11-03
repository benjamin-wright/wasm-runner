use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

#[derive(Clone, Default, Serialize)]
pub(crate) struct ApplicationStatus {
    pub source_commit: Option<String>,
    pub desired_version: Option<String>,
    pub active_version: Option<String>,
    pub active_digest: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Default, Serialize)]
pub(crate) struct Status {
    pub ready: bool,
    pub config_commit: Option<String>,
    pub last_poll: Option<u64>,
    pub last_successful_poll: Option<u64>,
    pub error: Option<String>,
    pub applications: BTreeMap<String, ApplicationStatus>,
}

pub(crate) type SharedStatus = Arc<RwLock<Status>>;
