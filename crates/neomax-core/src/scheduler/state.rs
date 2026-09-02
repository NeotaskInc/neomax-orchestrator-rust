use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

use super::graph::DependencyGraph;
use super::types::Plan;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PartState {
    Pending,
    Running,
    Done,
    Failed,
    Conflict,
    Blocked,
    #[serde(other)]
    Unknown,
}

impl PartState {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Done | Self::Failed | Self::Conflict | Self::Blocked | Self::Unknown
        )
    }

    pub const fn succeeded(self) -> bool {
        matches!(self, Self::Done)
    }

    pub const fn failed(self) -> bool {
        matches!(
            self,
            Self::Failed | Self::Conflict | Self::Blocked | Self::Unknown
        )
    }
}

pub type PartStatus = PartState;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PartExecution {
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub launched_at: Option<f64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanState {
    pub states: BTreeMap<String, PartState>,
    #[serde(default)]
    pub executions: BTreeMap<String, PartExecution>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl PlanState {
    pub fn pending(plan: &Plan) -> Self {
        Self {
            states: plan
                .part_ids()
                .map(|id| (id.to_string(), PartState::Pending))
                .collect(),
            executions: BTreeMap::new(),
            extra: BTreeMap::new(),
        }
    }

    pub fn state(&self, id: &str) -> Option<PartState> {
        self.states.get(id).copied()
    }

    pub fn execution(&self, id: &str) -> Option<&PartExecution> {
        self.executions.get(id)
    }

    pub fn set_state(&mut self, id: &str, state: PartState) -> Result<()> {
        let current = self
            .states
            .get_mut(id)
            .ok_or_else(|| Error::NotFound(format!("scheduler part {id}")))?;
        if current.is_terminal() && *current != state {
            return Err(Error::Conflict(format!(
                "scheduler part {id} is already {}",
                state_name(*current)
            )));
        }
        *current = state;
        Ok(())
    }

    pub fn mark_running(
        &mut self,
        id: &str,
        run_id: impl Into<String>,
        branch: Option<String>,
        profile: Option<String>,
        launched_at: f64,
    ) -> Result<()> {
        self.set_state(id, PartState::Running)?;
        self.executions.insert(
            id.to_string(),
            PartExecution {
                run_id: Some(run_id.into()),
                branch,
                profile,
                launched_at: Some(launched_at),
                extra: BTreeMap::new(),
            },
        );
        Ok(())
    }

    pub fn mark_done(&mut self, id: &str) -> Result<()> {
        self.set_state(id, PartState::Done)
    }

    pub fn mark_failed(&mut self, id: &str) -> Result<()> {
        self.set_state(id, PartState::Failed)
    }

    pub fn mark_conflict(&mut self, id: &str) -> Result<()> {
        self.set_state(id, PartState::Conflict)
    }

    pub fn block_failed_dependencies(&mut self, graph: &DependencyGraph) -> Vec<String> {
        let blocked = self
            .states
            .iter()
            .filter_map(|(id, state)| {
                (*state == PartState::Pending
                    && graph.blocked_by_failed_dependency(id, &self.states))
                .then_some(id.clone())
            })
            .collect::<Vec<_>>();
        for id in &blocked {
            self.states.insert(id.clone(), PartState::Blocked);
        }
        blocked
    }

    pub fn ready(&self, graph: &DependencyGraph) -> Vec<String> {
        graph.ready_order(&self.states)
    }

    pub fn live_count(&self) -> usize {
        self.states
            .values()
            .filter(|state| **state == PartState::Running)
            .count()
    }

    pub fn finished(&self) -> bool {
        self.states.values().all(|state| state.is_terminal())
    }

    pub fn outstanding(&self) -> usize {
        self.states
            .values()
            .filter(|state| !state.is_terminal())
            .count()
    }
}

fn state_name(state: PartState) -> &'static str {
    match state {
        PartState::Pending => "pending",
        PartState::Running => "running",
        PartState::Done => "done",
        PartState::Failed => "failed",
        PartState::Conflict => "conflict",
        PartState::Blocked => "blocked",
        PartState::Unknown => "unknown",
    }
}
