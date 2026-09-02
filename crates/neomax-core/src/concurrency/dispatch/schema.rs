use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::Engine;

use super::constants::STATE_VERSION;
use super::request::AdmissionRequest;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AdmissionLeaseView {
    pub id: String,
    pub task_id: String,
    pub engine: Option<Engine>,
    pub account: Option<String>,
    pub session: Option<String>,
    pub owner_pid: u32,
    pub created_at: f64,
    pub last_seen_at: f64,
}

impl From<&LeaseRecord> for AdmissionLeaseView {
    fn from(value: &LeaseRecord) -> Self {
        Self {
            id: value.id.clone(),
            task_id: value.task.clone(),
            engine: value.engine,
            account: value.account.clone(),
            session: value.session.clone(),
            owner_pid: value.owner_pid,
            created_at: value.created_at,
            last_seen_at: value.last_seen_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct AdmissionState {
    pub(super) version: u32,
    pub(super) leases: Vec<LeaseRecord>,
    #[serde(flatten)]
    pub(super) extra: BTreeMap<String, Value>,
}

impl Default for AdmissionState {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            leases: Vec::new(),
            extra: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct LeaseRecord {
    pub(super) id: String,
    pub(super) task: String,
    pub(super) engine: Option<Engine>,
    pub(super) account: Option<String>,
    pub(super) session: Option<String>,
    pub(super) owner_pid: u32,
    pub(super) created_at: f64,
    pub(super) last_seen_at: f64,
    #[serde(flatten)]
    pub(super) extra: BTreeMap<String, Value>,
}

impl LeaseRecord {
    pub(super) fn new(request: AdmissionRequest, owner_pid: u32, now: f64) -> Self {
        Self {
            id: request.lease_id,
            task: request.task_id,
            engine: request.engine,
            account: None,
            session: None,
            owner_pid,
            created_at: now,
            last_seen_at: now,
            extra: BTreeMap::new(),
        }
    }
}
