use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use neomax_core::projects::Project;
use neomax_core::queue::QueueState;
use neomax_core::runs::HistorySummary;
use neomax_core::sessions::SessionRecord;
use neomax_core::usage::UsageReport;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PortalSnapshot {
    pub now: i64,
    #[serde(default)]
    pub engines: BTreeMap<String, EngineView>,
    #[serde(default)]
    pub runs: Vec<RunView>,
    #[serde(default)]
    pub inbox: usize,
    #[serde(default)]
    pub ambient: Vec<SessionRecord>,
    #[serde(default)]
    pub summary: SummaryView,
    #[serde(default)]
    pub tasks: Vec<Value>,
    #[serde(default)]
    pub projects: BTreeMap<String, Project>,
    #[serde(default)]
    pub queue: Option<QueueState>,
    #[serde(default)]
    pub usage: Option<UsageReport>,
    #[serde(default)]
    pub orchestrators: Vec<ModeView>,
    #[serde(default)]
    pub plans: Vec<Value>,
    #[serde(default)]
    pub issues: Vec<Value>,
    #[serde(default)]
    pub worktrees: Vec<WorktreeView>,
    #[serde(default)]
    pub errors: Vec<PortalErrorView>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorktreeView {
    pub id: String,
    pub path: PathBuf,
    pub exists: bool,
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub repository: Option<PathBuf>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EngineView {
    #[serde(default)]
    pub accounts: Vec<AccountView>,
    #[serde(default)]
    pub capabilities: EngineCapabilitiesView,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EngineCapabilitiesView {
    pub binary_available: bool,
    pub orchestrator: bool,
    pub worker: bool,
    pub multiple_profiles: bool,
    pub native_sessions: bool,
    pub usage_discovery: bool,
    #[serde(default)]
    pub model_discovery: String,
    #[serde(default)]
    pub profile_root: PathBuf,
    #[serde(default)]
    pub quota: QuotaCapabilityView,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QuotaCapabilityView {
    pub supported: bool,
    pub available: bool,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub windows: Vec<String>,
    pub reactive: bool,
}

/// The provider catalog's decision for how an account may participate in the
/// fleet. Keeping each flag explicit lets the portal distinguish a connected
/// credential from one that can actually be scheduled or rotated.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ProfileEligibilityView {
    pub credential_present: bool,
    pub authenticated: bool,
    pub worker_eligible: bool,
    pub orchestrator_eligible: bool,
    pub rotation_eligible: bool,
    pub managed_pool_eligible: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccountView {
    pub n: String,
    pub dir: PathBuf,
    pub name: String,
    pub role: String,
    #[serde(default)]
    pub reserved: bool,
    pub rotate_advised: bool,
    pub authenticated: bool,
    pub worker_eligible: bool,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub auth_method: Option<String>,
    pub live: u32,
    pub workers: u32,
    pub mains: u32,
    pub subagents: u32,
    pub agents: u32,
    pub cooldown_until: i64,
    pub paused: bool,
    pub token_expired: bool,
    #[serde(default)]
    pub eligibility: ProfileEligibilityView,
    #[serde(default)]
    pub usage: Option<Value>,
    #[serde(default)]
    pub telemetry: Option<Value>,
    #[serde(default)]
    pub capabilities: EngineCapabilitiesView,
    #[serde(default)]
    pub duplicate_of: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunView {
    pub id: String,
    pub engine: String,
    pub account: String,
    #[serde(default)]
    pub acct_no: Option<String>,
    pub status: String,
    pub prompt: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub repo: Option<String>,
    pub children: usize,
    #[serde(default)]
    pub child_list: Vec<Value>,
    #[serde(default)]
    pub effort: Option<String>,
    pub ultra: bool,
    pub opus: bool,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    pub attempt: u32,
    #[serde(default)]
    pub goal: Option<String>,
    #[serde(default)]
    pub pr_url: Option<String>,
    pub acknowledged: bool,
    #[serde(default)]
    pub worktree: Option<PathBuf>,
    #[serde(default)]
    pub worktree_state: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub files_touched: Vec<String>,
    pub started: i64,
    #[serde(default)]
    pub ended: Option<i64>,
    #[serde(default)]
    pub orch_session: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SummaryView {
    pub live_total: u32,
    pub running: usize,
    pub workers: u32,
    pub mains: u32,
    pub subagents: u32,
    pub agents_total: u32,
    pub accounts_up: usize,
    pub accounts_total: usize,
    pub cooling: usize,
    pub inbox: usize,
    pub tasks_open: usize,
    pub runs_total: usize,
    #[serde(default)]
    pub claude_7d: Value,
    #[serde(default)]
    pub codex_7d: Value,
    #[serde(default)]
    pub opencode_7d: Value,
    #[serde(default)]
    pub kimi_7d: Value,
    #[serde(default)]
    pub grok_7d: Value,
    #[serde(default)]
    pub claude_weekly_min: Option<f64>,
    #[serde(default)]
    pub codex_weekly_min: Option<f64>,
    #[serde(default)]
    pub opencode_weekly_min: Option<f64>,
    #[serde(default)]
    pub kimi_weekly_min: Option<f64>,
    #[serde(default)]
    pub grok_weekly_min: Option<f64>,
    pub claude_weekly_soft: bool,
    pub codex_weekly_soft: bool,
    pub opencode_weekly_soft: bool,
    pub kimi_weekly_soft: bool,
    pub grok_weekly_soft: bool,
    pub claude_weekly_exhausted: bool,
    pub codex_weekly_exhausted: bool,
    pub opencode_weekly_exhausted: bool,
    pub kimi_weekly_exhausted: bool,
    pub grok_weekly_exhausted: bool,
    #[serde(default)]
    pub fleet_scope: Vec<String>,
    pub orch_reserved: bool,
    #[serde(default)]
    pub auth_rotations: Vec<Value>,
    #[serde(default)]
    pub failover_events: Vec<Value>,
    #[serde(default)]
    pub rotate_advised: Vec<Value>,
    #[serde(default)]
    pub orch_accounts: BTreeMap<String, bool>,
    #[serde(default)]
    pub duplicate_accounts: Vec<String>,
    #[serde(default)]
    pub quota_capabilities: Vec<String>,
    #[serde(default)]
    pub quota_available: Vec<String>,
    #[serde(default)]
    pub quota_reactive: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModeView {
    pub id: String,
    pub title: String,
    pub cmd: String,
    #[serde(default)]
    pub orchestrator: Option<String>,
    pub workers: String,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub why: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModesResponse {
    #[serde(default)]
    pub launch_dir: Option<PathBuf>,
    #[serde(default)]
    pub modes: Vec<ModeView>,
    #[serde(default)]
    pub account_commands: Vec<AccountCommandView>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccountCommandView {
    pub what: String,
    pub cmd: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PortalErrorView {
    pub component: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PrStateView {
    pub url: String,
    pub available: bool,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(rename = "isDraft", default)]
    pub is_draft: bool,
    pub merged: bool,
    #[serde(default)]
    pub error: Option<String>,
}

impl PrStateView {
    pub fn unavailable(url: &str, error: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            available: false,
            error: Some(error.into()),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionPlanView {
    pub operation: String,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    pub destructive: bool,
    pub confirmation_required: bool,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionResponse {
    pub accepted: bool,
    pub executed: bool,
    pub operation: String,
    #[serde(default)]
    pub engine: Option<String>,
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
    pub confirmation_required: bool,
    pub message: String,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub plan: Option<ActionPlanView>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunDiff {
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub worktree: Option<PathBuf>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub patch: String,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum PortalResponse {
    Status(Box<PortalSnapshot>),
    History(Vec<HistorySummary>),
    Modes(ModesResponse),
    Usage(UsageReport),
    Sessions(Vec<SessionRecord>),
    Subagents(Vec<SessionRecord>),
    RunDiff(RunDiff),
    PrState(PrStateView),
    Action(ActionResponse),
    Json(Value),
}

impl PortalResponse {
    pub fn into_json(self) -> serde_json::Result<Value> {
        serde_json::to_value(self)
    }
}
