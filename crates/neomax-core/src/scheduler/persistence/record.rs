use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::super::state::PlanState;
use super::super::types::Plan;
use super::types::{PlanControlMarkers, PlanStatus, SupervisorLease, initial_revision};
use super::validation::validate_plan_id;
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanRecord {
    pub plan_id: String,
    #[serde(default = "initial_revision")]
    pub revision: u64,
    #[serde(rename = "repo", alias = "repository", default)]
    pub repository: Option<PathBuf>,
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub integration_branch: Option<String>,
    #[serde(default)]
    pub worktree: Option<PathBuf>,
    pub plan: Plan,
    pub state: PlanState,
    pub status: PlanStatus,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub started_at: Option<i64>,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub ended_at: Option<i64>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub recovery_count: u32,
    #[serde(default)]
    pub cleanup_requested: bool,
    #[serde(default)]
    pub cleanup_completed: bool,
    #[serde(default)]
    pub cleanup_error: Option<String>,
    #[serde(default)]
    pub killed: bool,
    #[serde(default)]
    pub interrupted: bool,
    #[serde(default)]
    pub kill_requested: bool,
    #[serde(default)]
    pub supervisor_lease: Option<SupervisorLease>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl PlanRecord {
    pub fn new(
        plan_id: impl Into<String>,
        mut plan: Plan,
        worktree: Option<PathBuf>,
        now: i64,
    ) -> Result<Self> {
        let plan_id = plan_id.into();
        validate_plan_id(&plan_id)?;
        if let Some(existing) = plan.plan_id.as_deref() {
            if existing != plan_id {
                return Err(Error::Conflict(format!(
                    "scheduler plan id {plan_id:?} does not match normalized plan id {existing:?}"
                )));
            }
        } else {
            plan.plan_id = Some(plan_id.clone());
        }
        let record = Self {
            plan_id,
            revision: 1,
            repository: plan.repo.clone(),
            base: plan.base.clone(),
            integration_branch: plan.integration_branch.clone(),
            worktree,
            state: PlanState::pending(&plan),
            plan,
            status: PlanStatus::Pending,
            created_at: now,
            started_at: None,
            updated_at: now,
            ended_at: None,
            error: None,
            errors: Vec::new(),
            recovery_count: 0,
            cleanup_requested: false,
            cleanup_completed: false,
            cleanup_error: None,
            killed: false,
            interrupted: false,
            kill_requested: false,
            supervisor_lease: None,
            extra: BTreeMap::new(),
        };
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> Result<()> {
        if self.revision == 0 {
            return Err(Error::InvalidState {
                path: PathBuf::from(self.plan_id.clone()),
                message: "scheduler plan revision must be positive".into(),
            });
        }
        validate_plan_id(&self.plan_id)?;
        if self.plan.plan_id.as_deref() != Some(self.plan_id.as_str()) {
            return Err(Error::InvalidState {
                path: PathBuf::from(self.plan_id.clone()),
                message: "normalized plan id does not match record id".into(),
            });
        }
        if self.repository != self.plan.repo
            || self.base != self.plan.base
            || self.integration_branch != self.plan.integration_branch
        {
            return Err(Error::InvalidState {
                path: PathBuf::from(self.plan_id.clone()),
                message: "plan metadata does not match record metadata".into(),
            });
        }
        self.plan.graph().map_err(|error| Error::InvalidState {
            path: PathBuf::from(self.plan_id.clone()),
            message: error.to_string(),
        })?;
        let part_ids = self
            .plan
            .part_ids()
            .collect::<std::collections::BTreeSet<_>>();
        let state_ids = self
            .state
            .states
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        if state_ids != part_ids {
            return Err(Error::InvalidState {
                path: PathBuf::from(self.plan_id.clone()),
                message: "plan state does not match normalized plan parts".into(),
            });
        }
        if self
            .state
            .executions
            .keys()
            .any(|part_id| !part_ids.contains(part_id.as_str()))
        {
            return Err(Error::InvalidState {
                path: PathBuf::from(self.plan_id.clone()),
                message: "plan execution state contains an unknown part".into(),
            });
        }
        if self.created_at < 0
            || self.updated_at < self.created_at
            || self.started_at.is_some_and(|value| value < self.created_at)
            || self.ended_at.is_some_and(|value| value < self.created_at)
        {
            return Err(Error::InvalidState {
                path: PathBuf::from(self.plan_id.clone()),
                message: "plan timestamps are out of order".into(),
            });
        }
        if let Some(lease) = self.supervisor_lease.as_ref() {
            lease.validate(self.created_at)?;
        }
        Ok(())
    }

    pub fn control_markers(&self) -> PlanControlMarkers {
        PlanControlMarkers {
            killed: self.killed,
            interrupted: self.interrupted,
            kill_requested: self.kill_requested,
        }
    }

    pub fn preserve_control_markers(&mut self, previous: PlanControlMarkers) {
        self.killed |= previous.killed;
        self.interrupted |= previous.interrupted;
        self.kill_requested |= previous.kill_requested;
    }

    pub fn record_error(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.error = Some(message.clone());
        self.errors.push(message);
    }
}
