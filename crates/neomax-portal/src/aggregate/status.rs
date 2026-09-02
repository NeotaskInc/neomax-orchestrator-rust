use std::collections::BTreeMap;

use anyhow::Result;

use neomax_core::projects::Project;
use neomax_core::queue::QueueState;
use neomax_core::runs::{SystemProcessProbe, in_inbox};
use neomax_core::tasks::TaskRegistry;

use crate::model::{PortalErrorView, PortalSnapshot};
use crate::source::{
    FilesystemPortalSource, PortalSource, events, issues, plans, runs as run_source, state,
    worktrees,
};
use crate::source::{accounts, modes};

use super::{runs, sessions, usage};

pub fn build_status(
    source: &FilesystemPortalSource,
    now: i64,
    days: u32,
) -> Result<PortalSnapshot> {
    let mut errors = Vec::new();
    let (run_records, skipped_runs) = run_source::read_records(&source.paths.runs)?;
    if skipped_runs > 0 {
        errors.push(PortalErrorView {
            component: "runs".into(),
            message: format!("{skipped_runs} oversized or malformed run record(s) omitted"),
        });
    }
    let session_records = match source.sessions(days, now) {
        Ok(value) => value,
        Err(error) => {
            errors.push(error_view("sessions", &error));
            Vec::new()
        }
    };
    let usage_report = match source.usage(days, now) {
        Ok(value) => Some(value),
        Err(error) => {
            errors.push(error_view("usage", &error));
            None
        }
    };
    let engines = match accounts::account_views(source, &run_records, &session_records, now, days) {
        Ok(value) => value,
        Err(error) => {
            errors.push(error_view("accounts", &error));
            BTreeMap::new()
        }
    };
    let projects_result = state::load::<BTreeMap<String, Project>>(&source.paths.projects);
    let projects = match projects_result {
        Ok(value) => value.unwrap_or_default(),
        Err(error) => {
            errors.push(error_view("projects", &error));
            BTreeMap::new()
        }
    };
    let tasks_result = state::load::<TaskRegistry>(&source.paths.tasks);
    let tasks_registry = match tasks_result {
        Ok(value) => value.unwrap_or_default(),
        Err(error) => {
            errors.push(error_view("tasks", &error));
            Default::default()
        }
    };
    let tasks = tasks_registry
        .tasks
        .values()
        .filter_map(|task| serde_json::to_value(task).ok())
        .collect::<Vec<_>>();
    let (plans, skipped_plans) = match plans::read_plans(source) {
        Ok(value) => value,
        Err(error) => {
            errors.push(error_view("plans", &error));
            (Vec::new(), 0)
        }
    };
    if skipped_plans > 0 {
        errors.push(PortalErrorView {
            component: "plans".into(),
            message: format!("{skipped_plans} malformed plan record(s) omitted"),
        });
    }
    let (issues, skipped_issues) = match issues::read_issues(source) {
        Ok(value) => value,
        Err(error) => {
            errors.push(error_view("issues", &error));
            (Vec::new(), 0)
        }
    };
    if skipped_issues > 0 {
        errors.push(PortalErrorView {
            component: "issues".into(),
            message: format!("{skipped_issues} malformed issue record(s) omitted"),
        });
    }
    let queue = match state::load::<QueueState>(&source.paths.agent_queue) {
        Ok(value) => value,
        Err(error) => {
            errors.push(error_view("queue", &error));
            None
        }
    };
    let (ambient, _) = sessions::ambient_records(session_records);
    let probe = SystemProcessProbe;
    let runs = runs::run_views(&run_records, &projects, &probe);
    let (worktrees, skipped_worktrees) =
        match worktrees::read_worktrees(source, &run_records, &projects) {
            Ok(value) => value,
            Err(error) => {
                errors.push(error_view("worktrees", &error));
                (Vec::new(), 0)
            }
        };
    if skipped_worktrees > 0 {
        errors.push(PortalErrorView {
            component: "worktrees".into(),
            message: format!("{skipped_worktrees} unreadable worktree record(s) omitted"),
        });
    }
    let inbox = run_records
        .iter()
        .filter(|run| in_inbox(run, &probe))
        .count();
    let orchestrators = modes::available_modes(source);
    let mut summary = usage::build_summary(&engines, &runs, &tasks, inbox, now);
    match usage::recent_rotations(source, now) {
        Ok(rotations) => summary.auth_rotations = rotations,
        Err(error) => errors.push(error_view("auth rotations", &error)),
    }
    match events::read_failover_events(source, now) {
        Ok(failovers) => summary.failover_events = failovers,
        Err(error) => errors.push(error_view("failover events", &error)),
    }
    Ok(PortalSnapshot {
        now,
        engines,
        runs,
        inbox,
        ambient,
        summary,
        tasks,
        projects,
        queue,
        usage: usage_report,
        orchestrators: orchestrators.modes,
        plans,
        issues,
        worktrees,
        errors,
    })
}

fn error_view(component: &str, error: &impl std::fmt::Display) -> PortalErrorView {
    crate::security::log_internal(component, error);
    PortalErrorView {
        component: component.into(),
        message: "data unavailable".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FilesystemPortalSource;

    #[test]
    fn empty_state_still_returns_a_complete_snapshot_shape() {
        let temp = tempfile::tempdir().unwrap();
        let source = FilesystemPortalSource::new(temp.path(), temp.path().join("state"));
        let snapshot = build_status(&source, 1_800_000_000, 3).unwrap();
        assert_eq!(snapshot.now, 1_800_000_000);
        assert_eq!(snapshot.inbox, 0);
        assert!(snapshot.summary.fleet_scope.len() <= 5);
    }
}
