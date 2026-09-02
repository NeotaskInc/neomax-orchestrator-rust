use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::policy::SelfHealPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairAction {
    Resume,
    Retry,
    Kill,
}

impl RepairAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Resume => "resume",
            Self::Retry => "retry",
            Self::Kill => "kill",
        }
    }
}

impl std::fmt::Display for RepairAction {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileClass {
    Resolved,
    Orphaned,
    NeedsResume,
    NeedsRetry,
    HasChanges,
    Running,
}

impl ReconcileClass {
    pub const fn action(self) -> Option<RepairAction> {
        match self {
            Self::Orphaned => Some(RepairAction::Resume),
            Self::NeedsResume => Some(RepairAction::Resume),
            Self::NeedsRetry => Some(RepairAction::Retry),
            Self::Resolved | Self::HasChanges | Self::Running => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReconcileCandidate {
    pub run_id: String,
    pub class: ReconcileClass,
    pub action: Option<RepairAction>,
    pub status: String,
    pub started: i64,
    pub ended: Option<i64>,
    pub age_reference: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairPlan {
    pub run_id: String,
    pub class: ReconcileClass,
    pub action: RepairAction,
    pub status: String,
    pub attempt: u32,
    pub next_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HealSkipReason {
    AlreadyHealed,
    Backoff,
    CapReached,
    TooOld,
    Excluded,
    NoAction,
    LiveWorker,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HealSkip {
    pub run_id: String,
    pub reason: HealSkipReason,
    pub action: Option<RepairAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HealResult {
    pub run_id: String,
    pub action: RepairAction,
    pub attempt: u32,
    pub completed: bool,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ReconcileReport {
    pub candidates: Vec<ReconcileCandidate>,
    pub healed: Vec<HealResult>,
    pub skipped: Vec<HealSkip>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileRequest {
    pub now: i64,
    pub policy: SelfHealPolicy,
    pub allow_repeat: bool,
    pub excluded_run_ids: std::collections::BTreeSet<String>,
}

impl ReconcileRequest {
    pub fn new(now: i64) -> Self {
        Self {
            now,
            policy: SelfHealPolicy::default(),
            allow_repeat: false,
            excluded_run_ids: std::collections::BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfHealEvent {
    #[serde(default)]
    pub ts: i64,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub outcome: Option<String>,
    #[serde(default)]
    pub in_flight: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl SelfHealEvent {
    pub fn new(ts: i64, action: RepairAction, in_flight: bool) -> Self {
        Self {
            ts,
            action: action.to_string(),
            outcome: None,
            in_flight,
            extra: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SelfHealRecord {
    #[serde(default)]
    pub attempts: u32,
    #[serde(default)]
    pub history: Vec<SelfHealEvent>,
    #[serde(default)]
    pub next_at: Option<i64>,
    #[serde(default)]
    pub last_at: Option<i64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl SelfHealRecord {
    pub fn last_action(&self) -> Option<RepairAction> {
        self.history
            .iter()
            .rev()
            .find_map(|event| match event.action.as_str() {
                "resume" => Some(RepairAction::Resume),
                "retry" => Some(RepairAction::Retry),
                "kill" => Some(RepairAction::Kill),
                _ => None,
            })
    }

    pub fn push_event(&mut self, event: SelfHealEvent, max_history: usize) {
        self.history.push(event);
        if self.history.len() > max_history {
            let remove = self.history.len() - max_history;
            self.history.drain(..remove);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SelfHealState {
    pub runs: BTreeMap<String, SelfHealRecord>,
    pub extra: BTreeMap<String, serde_json::Value>,
    pub(crate) wrapped: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealDecision {
    Eligible { attempt: u32, next_at: i64 },
    AlreadyHealed,
    Backoff { next_at: i64 },
    CapReached,
}
