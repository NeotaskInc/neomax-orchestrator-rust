use std::collections::BTreeMap;

use neomax_core::{Engine, WorkerScope};
use serde::Serialize;

use crate::models::EffectiveModel;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LaunchMode {
    Dynamic,
    ProviderPinned,
    AccountHelper,
    Solo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EnvironmentPlan {
    pub source: String,
    pub role: String,
    pub policy: String,
    pub variables: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdapterPlan {
    pub provider: String,
    pub executable: String,
    pub role: String,
    pub execution: String,
    pub environment: EnvironmentPlan,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LaunchPlan {
    pub invocation: String,
    pub mode: LaunchMode,
    pub orchestrator: Option<String>,
    pub worker_engines: Vec<String>,
    pub routing: String,
    pub account: Option<String>,
    pub operation: Option<String>,
    pub operation_args: Vec<String>,
    pub initial_task: Option<String>,
    pub goal: Option<String>,
    pub base: Option<String>,
    pub run_id: Option<String>,
    pub tag: Option<String>,
    pub session_id: Option<String>,
    pub max_turns: Option<u32>,
    pub priority: Option<String>,
    pub effort: Option<String>,
    pub wall_min: Option<f64>,
    pub stall_min: Option<f64>,
    pub no_failover: bool,
    pub no_worktree: bool,
    pub plan_mode: bool,
    pub open_pull_request: bool,
    pub brief: bool,
    pub ultra: bool,
    pub opus: bool,
    pub resume: bool,
    pub dedicated: bool,
    pub detach: bool,
    pub foreground: bool,
    pub worker_dispatch: bool,
    pub solo: bool,
    pub models: BTreeMap<String, EffectiveModel>,
    pub adapters: Vec<AdapterPlan>,
    pub dry_run: bool,
    pub provider_execution: String,
}

#[derive(Debug, Clone, Default)]
pub struct LaunchOptions {
    pub(crate) dry_run: bool,
    pub(crate) engine: Option<Engine>,
    pub(crate) model: Option<String>,
    pub(crate) provider_models: BTreeMap<Engine, String>,
    pub(crate) worker_scope: Option<WorkerScope>,
    pub(crate) positionals: Vec<String>,
    pub(crate) helper_command: Option<String>,
    pub(crate) helper_args: Vec<String>,
    pub(crate) routing: String,
    pub(crate) account: Option<String>,
    pub(crate) goal: Option<String>,
    pub(crate) base: Option<String>,
    pub(crate) run_id: Option<String>,
    pub(crate) tag: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) max_turns: Option<u32>,
    pub(crate) priority: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) wall_min: Option<f64>,
    pub(crate) stall_min: Option<f64>,
    pub(crate) no_failover: bool,
    pub(crate) no_worktree: bool,
    pub(crate) plan_mode: bool,
    pub(crate) open_pull_request: bool,
    pub(crate) brief: bool,
    pub(crate) ultra: bool,
    pub(crate) opus: bool,
    pub(crate) resume: bool,
    pub(crate) dedicated: bool,
    pub(crate) detach: bool,
    pub(crate) foreground: bool,
    pub(crate) worker_dispatch: bool,
    pub(crate) solo: bool,
    pub(crate) routing_allowed: bool,
}
