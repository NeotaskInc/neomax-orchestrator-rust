use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::agent_tools::LaunchRole;
use crate::Engine;

use super::{default_attempt, default_engine, RunRecord, RunStatus, SessionHistoryEntry};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RunRecordWire {
    id: String,
    #[serde(default = "default_engine")]
    engine: Engine,
    #[serde(default)]
    model: String,
    #[serde(default)]
    prompt: String,
    #[serde(default, rename = "_prompt_to_send")]
    prompt_to_send: Option<String>,
    #[serde(default)]
    profile: PathBuf,
    #[serde(default)]
    workdir: PathBuf,
    #[serde(default)]
    cwd: Option<PathBuf>,
    #[serde(default)]
    repo: Option<PathBuf>,
    #[serde(default)]
    worktree: Option<PathBuf>,
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    base: Option<String>,
    #[serde(default)]
    base_ref: Option<String>,
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    effort: Option<String>,
    #[serde(default)]
    goal: Option<String>,
    #[serde(default)]
    max_turns: Option<u32>,
    #[serde(default)]
    session: Option<String>,
    #[serde(
        default,
        rename = "_resume_session",
        skip_serializing_if = "Option::is_none"
    )]
    resume_session: Option<String>,
    #[serde(default)]
    session_history: Vec<SessionHistoryEntry>,
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    orch_session: Option<String>,
    #[serde(default)]
    worker_pid: Option<u32>,
    #[serde(default, rename = "pid")]
    supervisor_pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    interrupt_signal: Option<i32>,
    #[serde(default = "default_attempt")]
    attempt: u32,
    #[serde(default = "default_status")]
    status: String,
    #[serde(default)]
    started: i64,
    #[serde(default)]
    ended: Option<i64>,
    #[serde(default)]
    wall_min: Option<f64>,
    #[serde(default)]
    stall_min: Option<f64>,
    #[serde(default)]
    ultra: bool,
    #[serde(default)]
    opus: bool,
    #[serde(default)]
    plan_mode: bool,
    #[serde(default, rename = "pr")]
    open_pull_request: bool,
    #[serde(default)]
    no_failover: bool,
    #[serde(default)]
    resumed: bool,
    #[serde(default)]
    killed: bool,
    #[serde(default)]
    acknowledged: Option<bool>,
    #[serde(default)]
    tried: Vec<PathBuf>,
    #[serde(default)]
    log: Option<PathBuf>,
    #[serde(default)]
    result_text: Option<String>,
    #[serde(default)]
    usage: Option<serde_json::Value>,
    #[serde(default)]
    resets_at: Option<f64>,
    #[serde(default)]
    limit_window: Option<String>,
    #[serde(default)]
    error_detail: Option<String>,
    #[serde(default)]
    worktree_state: Option<String>,
    #[serde(default)]
    files_touched: Vec<String>,
    #[serde(default)]
    pr_url: Option<String>,
    #[serde(default)]
    children: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    environment: BTreeMap<String, String>,
    #[serde(default)]
    launch_role: LaunchRole,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

impl Serialize for RunRecord {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        RunRecordWire::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RunRecord {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        RunRecordWire::deserialize(deserializer).map(Into::into)
    }
}

impl From<&RunRecord> for RunRecordWire {
    fn from(record: &RunRecord) -> Self {
        Self {
            id: record.id.clone(),
            engine: record.engine,
            model: record.model.clone(),
            prompt: record.prompt.clone(),
            prompt_to_send: record.prompt_to_send.clone(),
            profile: record.profile.clone(),
            workdir: record.workdir.clone(),
            cwd: record.cwd.clone(),
            repo: record.repo.clone(),
            worktree: record.worktree.clone(),
            branch: record.branch.clone(),
            base: record.base.clone(),
            base_ref: record.base_ref.clone(),
            tag: record.tag.clone(),
            effort: record.effort.clone(),
            goal: record.goal.clone(),
            max_turns: record.max_turns,
            session: record.session.clone(),
            resume_session: record.resume_session.clone(),
            session_history: record.session_history.clone(),
            project: record.project.clone(),
            orch_session: record.orch_session.clone(),
            worker_pid: record.worker_pid,
            supervisor_pid: record.supervisor_pid,
            interrupt_signal: record.interrupt_signal,
            attempt: record.attempt,
            status: record
                .unknown_status
                .as_deref()
                .filter(|_| record.status == RunStatus::Unknown)
                .unwrap_or_else(|| record.status.as_str())
                .to_owned(),
            started: record.started,
            ended: record.ended,
            wall_min: record.wall_min,
            stall_min: record.stall_min,
            ultra: record.ultra,
            opus: record.opus,
            plan_mode: record.plan_mode,
            open_pull_request: record.open_pull_request,
            no_failover: record.no_failover,
            resumed: record.resumed,
            killed: record.killed,
            acknowledged: record.acknowledged,
            tried: record.tried.clone(),
            log: record.log.clone(),
            result_text: record.result_text.clone(),
            usage: record.usage.clone(),
            resets_at: record.resets_at,
            limit_window: record.limit_window.clone(),
            error_detail: record.error_detail.clone(),
            worktree_state: record.worktree_state.clone(),
            files_touched: record.files_touched.clone(),
            pr_url: record.pr_url.clone(),
            children: record.children.clone(),
            environment: record.environment.clone(),
            launch_role: record.launch_role,
            extra: record.extra.clone(),
        }
    }
}

impl From<RunRecordWire> for RunRecord {
    fn from(wire: RunRecordWire) -> Self {
        let (status, unknown_status) = parse_status(wire.status);
        Self {
            id: wire.id,
            engine: wire.engine,
            model: wire.model,
            prompt: wire.prompt,
            prompt_to_send: wire.prompt_to_send,
            profile: wire.profile,
            workdir: wire.workdir,
            cwd: wire.cwd,
            repo: wire.repo,
            worktree: wire.worktree,
            branch: wire.branch,
            base: wire.base,
            base_ref: wire.base_ref,
            tag: wire.tag,
            effort: wire.effort,
            goal: wire.goal,
            max_turns: wire.max_turns,
            session: wire.session,
            resume_session: wire.resume_session,
            session_history: wire.session_history,
            project: wire.project,
            orch_session: wire.orch_session,
            worker_pid: wire.worker_pid,
            supervisor_pid: wire.supervisor_pid,
            interrupt_signal: wire.interrupt_signal,
            attempt: wire.attempt,
            status,
            started: wire.started,
            ended: wire.ended,
            wall_min: wire.wall_min,
            stall_min: wire.stall_min,
            ultra: wire.ultra,
            opus: wire.opus,
            plan_mode: wire.plan_mode,
            open_pull_request: wire.open_pull_request,
            no_failover: wire.no_failover,
            resumed: wire.resumed,
            killed: wire.killed,
            acknowledged: wire.acknowledged,
            tried: wire.tried,
            log: wire.log,
            result_text: wire.result_text,
            usage: wire.usage,
            resets_at: wire.resets_at,
            limit_window: wire.limit_window,
            error_detail: wire.error_detail,
            worktree_state: wire.worktree_state,
            files_touched: wire.files_touched,
            pr_url: wire.pr_url,
            children: wire.children,
            environment: wire.environment,
            launch_role: wire.launch_role,
            extra: wire.extra,
            unknown_status,
        }
    }
}

fn default_status() -> String {
    "unknown".into()
}

fn parse_status(value: String) -> (RunStatus, Option<String>) {
    let status = match value.as_str() {
        "running" => RunStatus::Running,
        "done" => RunStatus::Done,
        "limit" => RunStatus::Limit,
        "error" => RunStatus::Error,
        "aborted" => RunStatus::Aborted,
        "stalled" => RunStatus::Stalled,
        "timeout" => RunStatus::Timeout,
        "interrupted" => RunStatus::Interrupted,
        "orphaned" => RunStatus::Orphaned,
        "integrated" => RunStatus::Integrated,
        "unknown" => RunStatus::Unknown,
        _ => RunStatus::Unknown,
    };
    let raw = (status == RunStatus::Unknown && value != "unknown").then_some(value);
    (status, raw)
}
