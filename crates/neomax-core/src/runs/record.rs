use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::agent_tools::LaunchRole;
use crate::Engine;

mod wire;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Running,
    Done,
    Limit,
    Error,
    Aborted,
    Stalled,
    Timeout,
    Interrupted,
    Orphaned,
    Integrated,
    #[default]
    #[serde(other)]
    Unknown,
}

impl RunStatus {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Done
                | Self::Limit
                | Self::Error
                | Self::Aborted
                | Self::Stalled
                | Self::Timeout
                | Self::Interrupted
                | Self::Integrated
        )
    }

    pub const fn is_interruption(self) -> bool {
        matches!(self, Self::Aborted | Self::Interrupted)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Done => "done",
            Self::Limit => "limit",
            Self::Error => "error",
            Self::Aborted => "aborted",
            Self::Stalled => "stalled",
            Self::Timeout => "timeout",
            Self::Interrupted => "interrupted",
            Self::Orphaned => "orphaned",
            Self::Integrated => "integrated",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionHistoryEntry {
    pub session: String,
    pub account: String,
    #[serde(default)]
    pub attempt: Option<u32>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct RunRecord {
    pub id: String,
    pub engine: Engine,
    pub model: String,
    pub prompt: String,
    pub prompt_to_send: Option<String>,
    pub profile: PathBuf,
    pub workdir: PathBuf,
    pub cwd: Option<PathBuf>,
    pub repo: Option<PathBuf>,
    pub worktree: Option<PathBuf>,
    pub branch: Option<String>,
    pub base: Option<String>,
    pub base_ref: Option<String>,
    pub tag: Option<String>,
    pub effort: Option<String>,
    pub goal: Option<String>,
    pub max_turns: Option<u32>,
    pub session: Option<String>,
    pub resume_session: Option<String>,
    pub session_history: Vec<SessionHistoryEntry>,
    pub project: Option<String>,
    pub orch_session: Option<String>,
    pub worker_pid: Option<u32>,
    pub supervisor_pid: Option<u32>,
    pub interrupt_signal: Option<i32>,
    pub attempt: u32,
    pub status: RunStatus,
    pub started: i64,
    pub ended: Option<i64>,
    pub wall_min: Option<f64>,
    pub stall_min: Option<f64>,
    pub ultra: bool,
    pub opus: bool,
    pub plan_mode: bool,
    pub open_pull_request: bool,
    pub no_failover: bool,
    pub resumed: bool,
    pub killed: bool,
    pub acknowledged: Option<bool>,
    pub tried: Vec<PathBuf>,
    pub log: Option<PathBuf>,
    pub result_text: Option<String>,
    pub usage: Option<serde_json::Value>,
    pub resets_at: Option<f64>,
    pub limit_window: Option<String>,
    pub error_detail: Option<String>,
    pub worktree_state: Option<String>,
    pub files_touched: Vec<String>,
    pub pr_url: Option<String>,
    pub children: Vec<serde_json::Value>,
    pub environment: BTreeMap<String, String>,
    pub launch_role: LaunchRole,
    pub extra: BTreeMap<String, serde_json::Value>,
    unknown_status: Option<String>,
}

impl RunRecord {
    pub fn new(
        id: impl Into<String>,
        engine: Engine,
        model: impl Into<String>,
        prompt: impl Into<String>,
        profile: impl Into<PathBuf>,
        workdir: impl Into<PathBuf>,
        started: i64,
    ) -> Self {
        Self {
            id: id.into(),
            engine,
            model: model.into(),
            prompt: prompt.into(),
            prompt_to_send: None,
            profile: profile.into(),
            workdir: workdir.into(),
            cwd: None,
            repo: None,
            worktree: None,
            branch: None,
            base: None,
            base_ref: None,
            tag: None,
            effort: None,
            goal: None,
            max_turns: None,
            session: None,
            resume_session: None,
            session_history: Vec::new(),
            project: None,
            orch_session: None,
            worker_pid: None,
            supervisor_pid: None,
            interrupt_signal: None,
            attempt: 1,
            status: RunStatus::Running,
            started,
            ended: None,
            wall_min: None,
            stall_min: None,
            ultra: false,
            opus: false,
            plan_mode: false,
            open_pull_request: false,
            no_failover: false,
            resumed: false,
            killed: false,
            acknowledged: None,
            tried: Vec::new(),
            log: None,
            result_text: None,
            usage: None,
            resets_at: None,
            limit_window: None,
            error_detail: None,
            worktree_state: None,
            files_touched: Vec::new(),
            pr_url: None,
            children: Vec::new(),
            environment: BTreeMap::new(),
            launch_role: LaunchRole::Worker,
            extra: BTreeMap::new(),
            unknown_status: None,
        }
    }

    pub fn prompt_for_attempt(&self) -> &str {
        self.prompt_to_send.as_deref().unwrap_or(&self.prompt)
    }

    pub fn account(&self) -> String {
        let name = self
            .profile
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_owned();
        if name == crate::providers::catalog::spec(self.engine).orchestrator_dir {
            "orch".into()
        } else {
            name
        }
    }

    pub fn is_acknowledged(&self) -> bool {
        self.acknowledged.unwrap_or(true)
    }

    pub fn remember_session(&mut self) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let entry = SessionHistoryEntry {
            session,
            account: self.account(),
            attempt: Some(self.attempt),
            extra: BTreeMap::new(),
        };
        if !self.session_history.iter().any(|existing| {
            existing.session == entry.session
                && existing.account == entry.account
                && existing.attempt == entry.attempt
        }) {
            self.session_history.push(entry);
        }
    }
}

fn default_engine() -> Engine {
    Engine::Claude
}

fn default_attempt() -> u32 {
    1
}

pub fn run_id(now: DateTime<Utc>, pid: u32) -> String {
    format!("{}-{pid}", now.format("%Y%m%d-%H%M%S"))
}

pub fn worktree_path(state: &Path, id: &str) -> PathBuf {
    state.join("worktrees").join(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_existing_record_shape_and_preserves_unknown_fields() {
        let value = serde_json::json!({
            "id":"run-1",
            "engine":"codex",
            "model":"gpt-5.6-sol",
            "prompt":"work",
            "_prompt_to_send":"continue",
            "profile":"/profiles/.codex1",
            "workdir":"/workspace",
            "pid":123,
            "attempt":2,
            "status":"running",
            "started":100,
            "future_field":"kept"
        });
        let record: RunRecord = serde_json::from_value(value).unwrap();
        assert_eq!(record.supervisor_pid, Some(123));
        assert_eq!(record.prompt_for_attempt(), "continue");
        assert_eq!(record.extra.get("future_field").unwrap(), "kept");
        assert!(serde_json::to_value(record).unwrap().get("pid").is_some());
    }

    #[test]
    fn preserves_unknown_status_and_session_history_fields() {
        let value = serde_json::json!({
            "id": "run-future",
            "status": "provider_review",
            "started": 1,
            "session_history": [{
                "session": "session-1",
                "account": "account-1",
                "attempt": 2,
                "future_session_field": {"keep": true}
            }],
            "future_record_field": ["keep"]
        });
        let record: RunRecord = serde_json::from_value(value).unwrap();
        assert_eq!(record.status, RunStatus::Unknown);
        assert_eq!(
            record.session_history[0].extra["future_session_field"]["keep"],
            true
        );
        let serialized = serde_json::to_value(record).unwrap();
        assert_eq!(serialized["status"], "provider_review");
        assert_eq!(serialized["future_record_field"][0], "keep");
        assert_eq!(
            serialized["session_history"][0]["future_session_field"]["keep"],
            true
        );
    }

    #[test]
    fn remember_session_does_not_duplicate_a_future_extended_entry() {
        let mut record: RunRecord = serde_json::from_value(serde_json::json!({
            "id": "run-future",
            "status": "running",
            "started": 1,
            "profile": "/profiles/account-1",
            "session": "session-1",
            "attempt": 2,
            "session_history": [{
                "session": "session-1",
                "account": "account-1",
                "attempt": 2,
                "future_session_field": true
            }]
        }))
        .unwrap();
        record.remember_session();
        assert_eq!(record.session_history.len(), 1);
        assert_eq!(
            record.session_history[0].extra["future_session_field"],
            true
        );
    }

    #[test]
    fn missing_acknowledgement_is_legacy_acknowledged() {
        let value = serde_json::json!({
            "id":"run-1", "status":"done", "started":1
        });
        let record: RunRecord = serde_json::from_value(value).unwrap();
        assert!(record.is_acknowledged());
    }

    #[test]
    fn preserves_the_scheduler_integrated_status() {
        let record: RunRecord = serde_json::from_value(serde_json::json!({
            "id":"run-1", "status":"integrated", "started":1
        }))
        .unwrap();
        assert_eq!(record.status, RunStatus::Integrated);
        assert_eq!(
            serde_json::to_value(record).unwrap()["status"],
            "integrated"
        );
    }

    #[test]
    fn preserves_an_interrupt_signal_without_requiring_it_in_legacy_records() {
        let mut record: RunRecord = serde_json::from_value(serde_json::json!({
            "id": "run-1",
            "status": "aborted",
            "started": 1,
            "interrupt_signal": 15
        }))
        .unwrap();
        assert_eq!(record.interrupt_signal, Some(15));
        record.interrupt_signal = None;
        assert!(serde_json::to_value(record)
            .unwrap()
            .get("interrupt_signal")
            .is_none());
    }

    #[test]
    fn names_an_orchestrator_profile_with_its_logical_account_selector() {
        let record = RunRecord::new(
            "run-1",
            Engine::Claude,
            "claude-fable-5[1m]",
            "work",
            "/profiles/.claude-orch",
            "/workspace",
            1,
        );
        assert_eq!(record.account(), "orch");
    }
}
