use std::collections::BTreeMap;

use neomax_core::Engine;
use neomax_core::sessions::SessionKind;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct QuotaView {
    pub five_hour_percent: Option<f64>,
    pub weekly_percent: Option<f64>,
    pub five_hour_reset_at: Option<i64>,
    pub weekly_reset_at: Option<i64>,
    pub cooldown_until: Option<i64>,
    pub hard_wall: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AccountView {
    pub engine: Engine,
    pub account: String,
    pub identity: String,
    pub role: String,
    pub auth_status: String,
    pub auth_methods: Vec<String>,
    pub credential_present: bool,
    pub authenticated: bool,
    pub worker_eligible: bool,
    pub orchestrator_eligible: bool,
    pub rotation_eligible: bool,
    pub managed_pool_eligible: bool,
    pub reserved: bool,
    pub paused: bool,
    pub live_workers: u32,
    pub mains: u32,
    pub subagents: u32,
    pub live: u32,
    pub agents: u32,
    pub default_model: String,
    pub available_models: Vec<String>,
    pub quota: QuotaView,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProviderView {
    pub engine: Engine,
    pub binary: String,
    pub binary_available: bool,
    pub version: Option<String>,
    pub connected: bool,
    pub orchestrator_eligible: bool,
    pub worker_eligible: bool,
    pub default_model: String,
    pub available_models: Vec<String>,
    pub accounts: Vec<AccountView>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RunView {
    pub id: String,
    pub engine: Engine,
    pub model: String,
    pub status: String,
    pub account: String,
    pub session: Option<String>,
    pub started: i64,
    pub ended: Option<i64>,
    pub worker_pid: Option<u32>,
    pub supervisor_pid: Option<u32>,
    pub attempt: u32,
    pub project: Option<String>,
    pub branch: Option<String>,
    pub tag: Option<String>,
    pub worktree_state: Option<String>,
    pub child_count: usize,
    pub has_error: bool,
    pub acknowledged: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionView {
    pub id: String,
    pub run_id: String,
    pub engine: Engine,
    pub account: String,
    pub model: String,
    pub status: String,
    pub started: i64,
    pub worker_pid: Option<u32>,
    pub child_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SubagentView {
    pub id: String,
    pub run_id: String,
    pub engine: Engine,
    pub account: String,
    pub status: String,
    pub label: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AmbientView {
    pub id: String,
    pub engine: Engine,
    pub account: String,
    pub model: Option<String>,
    pub project: Option<String>,
    pub branch: Option<String>,
    pub label: Option<String>,
    pub kind: SessionKind,
    pub parent_id: Option<String>,
    pub active: bool,
    pub working: bool,
    pub started: Option<i64>,
    pub last_active: Option<i64>,
    pub input: u64,
    pub output: u64,
    pub reasoning: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub requests: u64,
    pub completions: u64,
    pub errors: u64,
    pub rate_limits: u64,
    pub tool_calls: u64,
    pub tool_errors: u64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OrchestratorView {
    pub session: String,
    pub pid: Option<u32>,
    pub engine: Engine,
    pub account: String,
    pub project: Option<String>,
    pub model: String,
    pub reserved: bool,
    pub started: i64,
    pub last_seen: i64,
    pub live: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct QueueView {
    pub agent_budget: u32,
    pub task_budget: u32,
    pub used: u32,
    pub free: u32,
    pub active_tasks: usize,
    pub queued_tasks: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct StatusSummary {
    pub accounts_up: usize,
    pub accounts_total: usize,
    pub cooling: usize,
    pub paused: usize,
    pub running: usize,
    pub live_sessions: usize,
    pub subagents: usize,
    pub native_sessions: usize,
    pub native_subagents: usize,
    pub orchestrators: usize,
    pub workers: u32,
    pub agents_total: u32,
    pub queued_tasks: usize,
    pub inbox: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct StatusReport {
    pub now: i64,
    pub engines: BTreeMap<Engine, ProviderView>,
    pub accounts: Vec<AccountView>,
    pub runs: Vec<RunView>,
    pub run_ledger: Vec<RunView>,
    pub sessions: Vec<SessionView>,
    pub ambient: Vec<AmbientView>,
    pub subagents: Vec<SubagentView>,
    pub orchestrators: Vec<OrchestratorView>,
    pub queue: QueueView,
    pub connected_engines: Vec<String>,
    pub summary: StatusSummary,
}
