use crate::{Error, Result};

use super::super::state::PartState;
use super::record::PlanRecord;
use super::types::PlanStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanTransition {
    Start {
        at: i64,
    },
    PartRunning {
        part_id: String,
        run_id: String,
        branch: Option<String>,
        profile: Option<String>,
        at: i64,
    },
    PartDone {
        part_id: String,
        at: i64,
    },
    PartFailed {
        part_id: String,
        error: String,
        at: i64,
    },
    Done {
        at: i64,
    },
    Failed {
        error: String,
        at: i64,
    },
    Interrupted {
        error: Option<String>,
        at: i64,
    },
    Killed {
        error: Option<String>,
        at: i64,
    },
    Recover {
        at: i64,
    },
    RequestCleanup {
        at: i64,
    },
    CompleteCleanup {
        at: i64,
    },
    CleanupFailed {
        error: String,
        at: i64,
    },
}

pub fn apply_transition(record: &mut PlanRecord, transition: PlanTransition) -> Result<()> {
    match transition {
        PlanTransition::Start { at } => start(record, at),
        PlanTransition::PartRunning {
            part_id,
            run_id,
            branch,
            profile,
            at,
        } => part_running(record, &part_id, run_id, branch, profile, at),
        PlanTransition::PartDone { part_id, at } => part_done(record, &part_id, at),
        PlanTransition::PartFailed { part_id, error, at } => {
            part_failed(record, &part_id, error, at)
        }
        PlanTransition::Done { at } => complete(record, at),
        PlanTransition::Failed { error, at } => fail(record, error, at),
        PlanTransition::Interrupted { error, at } => interrupt(record, error, at),
        PlanTransition::Killed { error, at } => kill(record, error, at),
        PlanTransition::Recover { at } => recover(record, at),
        PlanTransition::RequestCleanup { at } => request_cleanup(record, at),
        PlanTransition::CompleteCleanup { at } => complete_cleanup(record, at),
        PlanTransition::CleanupFailed { error, at } => cleanup_failed(record, error, at),
    }
}

fn start(record: &mut PlanRecord, at: i64) -> Result<()> {
    timestamp(record, at)?;
    match record.status {
        PlanStatus::Pending | PlanStatus::Running => {
            record.status = PlanStatus::Running;
            record.started_at.get_or_insert(at);
            record.updated_at = at;
            Ok(())
        }
        status => Err(terminal_error(record, status)),
    }
}

fn part_running(
    record: &mut PlanRecord,
    part_id: &str,
    run_id: String,
    branch: Option<String>,
    profile: Option<String>,
    at: i64,
) -> Result<()> {
    start(record, at)?;
    record
        .state
        .mark_running(part_id, run_id, branch, profile, at as f64)
        .map_err(|error| part_error(record, part_id, error))?;
    Ok(())
}

fn part_done(record: &mut PlanRecord, part_id: &str, at: i64) -> Result<()> {
    timestamp(record, at)?;
    ensure_live(record)?;
    record
        .state
        .mark_done(part_id)
        .map_err(|error| part_error(record, part_id, error))?;
    record.updated_at = at;
    Ok(())
}

fn part_failed(record: &mut PlanRecord, part_id: &str, error: String, at: i64) -> Result<()> {
    timestamp(record, at)?;
    ensure_live(record)?;
    record
        .state
        .mark_failed(part_id)
        .map_err(|state_error| part_error(record, part_id, state_error))?;
    record.record_error(format!("part {part_id}: {error}"));
    record.updated_at = at;
    Ok(())
}

fn complete(record: &mut PlanRecord, at: i64) -> Result<()> {
    timestamp(record, at)?;
    if record
        .state
        .states
        .values()
        .any(|state| *state != PartState::Done)
    {
        return Err(Error::Conflict(format!(
            "scheduler plan {} has unfinished or failed parts",
            record.plan_id
        )));
    }
    record.status = PlanStatus::Done;
    record.ended_at = Some(at);
    record.updated_at = at;
    Ok(())
}

fn fail(record: &mut PlanRecord, error: String, at: i64) -> Result<()> {
    timestamp(record, at)?;
    record.status = PlanStatus::Failed;
    record.ended_at = Some(at);
    record.record_error(error);
    record.updated_at = at;
    Ok(())
}

fn interrupt(record: &mut PlanRecord, error: Option<String>, at: i64) -> Result<()> {
    timestamp(record, at)?;
    record.status = PlanStatus::Interrupted;
    record.interrupted = true;
    record.ended_at = Some(at);
    if let Some(error) = error {
        record.record_error(error);
    }
    record.updated_at = at;
    Ok(())
}

fn kill(record: &mut PlanRecord, error: Option<String>, at: i64) -> Result<()> {
    timestamp(record, at)?;
    record.status = PlanStatus::Killed;
    record.killed = true;
    record.kill_requested = true;
    record.ended_at = Some(at);
    if let Some(error) = error {
        record.record_error(error);
    }
    record.updated_at = at;
    Ok(())
}

fn recover(record: &mut PlanRecord, at: i64) -> Result<()> {
    timestamp(record, at)?;
    if !record.status.is_terminal() {
        return Err(Error::Conflict(format!(
            "scheduler plan {} is not terminal",
            record.plan_id
        )));
    }
    record.status = PlanStatus::Running;
    record.started_at.get_or_insert(at);
    record.ended_at = None;
    record.recovery_count = record.recovery_count.saturating_add(1);
    record.cleanup_requested = false;
    record.cleanup_completed = false;
    record.cleanup_error = None;
    record.updated_at = at;
    Ok(())
}

fn request_cleanup(record: &mut PlanRecord, at: i64) -> Result<()> {
    timestamp(record, at)?;
    record.cleanup_requested = true;
    record.cleanup_completed = false;
    record.updated_at = at;
    Ok(())
}

fn complete_cleanup(record: &mut PlanRecord, at: i64) -> Result<()> {
    timestamp(record, at)?;
    if !record.cleanup_requested {
        return Err(Error::Conflict(format!(
            "scheduler plan {} has no cleanup request",
            record.plan_id
        )));
    }
    record.cleanup_completed = true;
    record.updated_at = at;
    Ok(())
}

fn cleanup_failed(record: &mut PlanRecord, error: String, at: i64) -> Result<()> {
    timestamp(record, at)?;
    record.cleanup_requested = true;
    record.cleanup_completed = false;
    record.cleanup_error = Some(error.clone());
    record.record_error(error);
    record.updated_at = at;
    Ok(())
}

fn ensure_live(record: &PlanRecord) -> Result<()> {
    match record.status {
        PlanStatus::Pending | PlanStatus::Running => Ok(()),
        status => Err(terminal_error(record, status)),
    }
}

fn timestamp(record: &PlanRecord, at: i64) -> Result<()> {
    if at < record.created_at {
        return Err(Error::InvalidArgument(format!(
            "scheduler plan timestamp {at} precedes creation time {}",
            record.created_at
        )));
    }
    Ok(())
}

fn terminal_error(record: &PlanRecord, status: PlanStatus) -> Error {
    Error::Conflict(format!(
        "scheduler plan {} is already {}",
        record.plan_id,
        status_name(status)
    ))
}

fn part_error(record: &PlanRecord, part_id: &str, error: Error) -> Error {
    Error::Conflict(format!(
        "scheduler plan {} part {part_id}: {error}",
        record.plan_id
    ))
}

fn status_name(status: PlanStatus) -> &'static str {
    match status {
        PlanStatus::Pending => "pending",
        PlanStatus::Running => "running",
        PlanStatus::Done => "done",
        PlanStatus::Failed => "failed",
        PlanStatus::Interrupted => "interrupted",
        PlanStatus::Killed => "killed",
        PlanStatus::Unknown => "unknown",
    }
}
