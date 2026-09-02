use crate::runs::{RunRecord, RunStatus};

pub fn mark_attempt_started(run: &mut RunRecord, supervisor_pid: u32) {
    if run.tried.last() != Some(&run.profile) {
        run.tried.push(run.profile.clone());
    }
    run.status = RunStatus::Running;
    run.supervisor_pid = Some(supervisor_pid);
    run.worker_pid = None;
    run.ended = None;
}
