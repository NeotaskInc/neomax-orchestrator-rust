use std::collections::BTreeMap;

use serde_json::Value;

use crate::accounts::AccountControlStore;
use crate::runs::{EventStore, HistoryStore, RunEvent, RunRecord, RunStatus, RunStore};
use crate::{Error, Result};

use super::cooldown::record_limit_cooldown;
use super::pull_request::{request_for_run, PullRequestFinalizer};
use super::types::{exit_code, Finalization, FinalizeOptions};
use super::worktree::WorktreeFinalizer;

pub struct RunFinalizer<'a> {
    pub runs: &'a RunStore,
    pub events: &'a EventStore,
    pub history: &'a HistoryStore,
    pub controls: &'a AccountControlStore,
    pub worktrees: &'a dyn WorktreeFinalizer,
    pub pull_requests: Option<&'a dyn PullRequestFinalizer>,
}

impl RunFinalizer<'_> {
    pub fn finish(
        &self,
        run: &mut RunRecord,
        status: RunStatus,
        options: FinalizeOptions,
    ) -> Result<Finalization> {
        if !status.is_terminal() {
            return Err(Error::InvalidArgument(format!(
                "cannot finalize run {} as {}",
                run.id,
                status.as_str()
            )));
        }
        run.status = status;
        run.ended = Some(options.now.timestamp());
        run.acknowledged = Some(false);

        let mut warnings = Vec::new();
        let cooldown_until = match record_limit_cooldown(
            self.controls,
            run,
            status,
            options.now,
            options.default_cooldown,
        ) {
            Ok(value) => value,
            Err(error) => {
                warnings.push(format!("account cooldown: {error}"));
                None
            }
        };
        let committed = self.runs.update(&run.id, |persisted| {
            let mut candidate = run.clone();
            merge_control_markers(&mut candidate, persisted);
            if let Err(error) = self.worktrees.record_outcome(&mut candidate) {
                warnings.push(format!("worktree outcome: {error}"));
            }
            if should_open_pull_request(&candidate) {
                if let Some(pull_requests) = self.pull_requests {
                    if let Some(request) = request_for_run(&candidate) {
                        match pull_requests.open(&request) {
                            Ok(Some(url)) => candidate.pr_url = Some(url),
                            Ok(None) => {}
                            Err(error) => warnings.push(format!("pull request: {error}")),
                        }
                    } else {
                        warnings.push(format!(
                            "pull request: run {} has no repository or branch",
                            candidate.id
                        ));
                    }
                }
            }
            *persisted = candidate.clone();
            Ok(())
        })?;
        *run = committed;
        let status = run.status;
        let archive = match self.history.archive_or_spill(
            run,
            options.account_number,
            options.now.timestamp(),
        ) {
            Ok(value) => Some(value),
            Err(error) => {
                warnings.push(format!("history archive: {error}"));
                None
            }
        };
        if let Err(error) = self
            .events
            .append(&finished_event(run, &options), options.now)
        {
            warnings.push(format!("event journal: {error}"));
        }
        Ok(Finalization {
            status,
            exit_code: exit_code(status),
            cooldown_until,
            archive,
            warnings,
        })
    }
}

fn should_open_pull_request(run: &RunRecord) -> bool {
    run.status == RunStatus::Done
        && run.open_pull_request
        && run.pr_url.is_none()
        && run.worktree_state.as_deref() == Some("has_changes")
}

fn merge_control_markers(candidate: &mut RunRecord, persisted: &RunRecord) {
    candidate.killed |= persisted.killed;
    if persisted.status.is_interruption() && !candidate.status.is_interruption() {
        candidate.status = persisted.status;
        candidate.ended = persisted.ended.or(candidate.ended);
        candidate.acknowledged = persisted.acknowledged.or(candidate.acknowledged);
    }
}

fn finished_event(run: &RunRecord, options: &FinalizeOptions) -> RunEvent {
    let mut extra = BTreeMap::from([
        (
            "result_status".into(),
            Value::String(run.status.as_str().into()),
        ),
        (
            "worktree_state".into(),
            run.worktree_state
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        ),
        (
            "pr_url".into(),
            run.pr_url.clone().map(Value::String).unwrap_or(Value::Null),
        ),
        ("children".into(), Value::from(run.children.len() as u64)),
    ]);
    if let Some(signal) = run.interrupt_signal {
        extra.insert("signal".into(), Value::from(signal));
    }
    RunEvent {
        ts: options.now.timestamp(),
        run: run.id.clone(),
        event: "finished".into(),
        engine: run.engine,
        account: Some(run.account()),
        status: Some(run.status),
        attempt: Some(run.attempt),
        extra,
    }
}
