use crate::runs::{ProcessProbe, RunRecord, RunStatus, RunStore, effective_status};
use crate::{Error, Result};

use super::store::SelfHealStore;
use super::types::{
    HealDecision, HealResult, HealSkip, HealSkipReason, ReconcileCandidate, ReconcileClass,
    ReconcileReport, ReconcileRequest, RepairPlan,
};

pub trait RepairExecutor: Send + Sync {
    fn execute(&self, plan: &RepairPlan, run: &RunRecord) -> Result<()>;
}

pub struct ReconciliationService<'a, P: ?Sized> {
    runs: &'a RunStore,
    probe: &'a P,
    self_heal: &'a SelfHealStore,
}

impl<'a, P> ReconciliationService<'a, P>
where
    P: ProcessProbe + ?Sized,
{
    pub fn new(runs: &'a RunStore, probe: &'a P, self_heal: &'a SelfHealStore) -> Self {
        Self {
            runs,
            probe,
            self_heal,
        }
    }

    pub fn candidates(&self) -> Result<Vec<ReconcileCandidate>> {
        Ok(self
            .runs
            .all()?
            .iter()
            .map(|run| candidate(run, self.probe))
            .collect())
    }

    pub fn reconcile<E: RepairExecutor>(
        &self,
        request: &ReconcileRequest,
        executor: Option<&E>,
    ) -> Result<ReconcileReport> {
        let runs = self.runs.all()?;
        let mut report = ReconcileReport {
            candidates: runs.iter().map(|run| candidate(run, self.probe)).collect(),
            ..ReconcileReport::default()
        };
        if executor.is_none() {
            return Ok(report);
        }
        let mut used = 0_usize;
        for run in runs {
            let class = classify(&run, self.probe);
            let Some(action) = class.action() else {
                if class == ReconcileClass::Running {
                    report.skipped.push(HealSkip {
                        run_id: run.id,
                        reason: HealSkipReason::LiveWorker,
                        action: None,
                    });
                }
                continue;
            };
            if request.excluded_run_ids.contains(&run.id) {
                report.skipped.push(HealSkip {
                    run_id: run.id,
                    reason: HealSkipReason::Excluded,
                    action: Some(action),
                });
                continue;
            }
            if class != ReconcileClass::Orphaned
                && matches!(effective_status(&run, self.probe), RunStatus::Running)
            {
                report.skipped.push(HealSkip {
                    run_id: run.id,
                    reason: HealSkipReason::LiveWorker,
                    action: Some(action),
                });
                continue;
            }
            if request.policy.max_batch == 0 || used >= request.policy.max_batch {
                report.skipped.push(HealSkip {
                    run_id: run.id,
                    reason: HealSkipReason::CapReached,
                    action: Some(action),
                });
                continue;
            }
            if is_too_old(&run, request.now, request.policy.max_age) {
                report.skipped.push(HealSkip {
                    run_id: run.id,
                    reason: HealSkipReason::TooOld,
                    action: Some(action),
                });
                continue;
            }
            let decision = self.self_heal.reserve(
                &run.id,
                action,
                request.now,
                &request.policy,
                request.allow_repeat,
            )?;
            let HealDecision::Eligible { attempt, next_at } = decision else {
                report.skipped.push(HealSkip {
                    run_id: run.id,
                    reason: skip_reason(decision),
                    action: Some(action),
                });
                continue;
            };
            let plan = RepairPlan {
                run_id: run.id.clone(),
                class,
                action,
                status: effective_status(&run, self.probe).as_str().to_owned(),
                attempt,
                next_at,
            };
            let executor = executor.expect("executor checked above");
            match executor.execute(&plan, &run) {
                Ok(()) => {
                    self.self_heal
                        .complete(&run.id, action, request.now, "completed")?;
                    report.healed.push(HealResult {
                        run_id: run.id,
                        action,
                        attempt,
                        completed: true,
                        diagnostic: None,
                    });
                    used += 1;
                }
                Err(error) => {
                    let diagnostic = safe_diagnostic(&error);
                    self.self_heal
                        .complete(&run.id, action, request.now, "failed")?;
                    report.healed.push(HealResult {
                        run_id: run.id,
                        action,
                        attempt,
                        completed: false,
                        diagnostic: Some(diagnostic),
                    });
                    used += 1;
                }
            }
        }
        Ok(report)
    }
}

pub fn classify<P: ProcessProbe + ?Sized>(run: &RunRecord, probe: &P) -> ReconcileClass {
    let status = effective_status(run, probe);
    if status == RunStatus::Orphaned {
        return ReconcileClass::Orphaned;
    }
    if matches!(status, RunStatus::Running) {
        return ReconcileClass::Running;
    }
    if matches!(
        status,
        RunStatus::Interrupted | RunStatus::Aborted | RunStatus::Stalled | RunStatus::Timeout
    ) {
        return ReconcileClass::NeedsResume;
    }
    if matches!(status, RunStatus::Error | RunStatus::Limit) {
        return ReconcileClass::NeedsRetry;
    }
    if run.worktree_state.as_deref() == Some("has_changes") {
        return ReconcileClass::HasChanges;
    }
    if status.is_terminal() && !run.is_acknowledged() {
        return ReconcileClass::Resolved;
    }
    ReconcileClass::Resolved
}

fn candidate<P: ProcessProbe + ?Sized>(run: &RunRecord, probe: &P) -> ReconcileCandidate {
    let class = classify(run, probe);
    ReconcileCandidate {
        run_id: run.id.clone(),
        class,
        action: class.action(),
        status: effective_status(run, probe).as_str().to_owned(),
        started: run.started,
        ended: run.ended,
        age_reference: run.ended.unwrap_or(run.started),
    }
}

fn is_too_old(run: &RunRecord, now: i64, max_age: std::time::Duration) -> bool {
    let reference = run.ended.unwrap_or(run.started);
    now.saturating_sub(reference) > i64::try_from(max_age.as_secs()).unwrap_or(i64::MAX)
}

fn skip_reason(decision: HealDecision) -> HealSkipReason {
    match decision {
        HealDecision::AlreadyHealed => HealSkipReason::AlreadyHealed,
        HealDecision::Backoff { .. } => HealSkipReason::Backoff,
        HealDecision::CapReached => HealSkipReason::CapReached,
        HealDecision::Eligible { .. } => HealSkipReason::NoAction,
    }
}

fn safe_diagnostic(error: &Error) -> String {
    match error {
        Error::InvalidArgument(_) => "invalid repair request".into(),
        Error::NotFound(_) => "run or lifecycle state was not found".into(),
        Error::Conflict(_) => "run is already owned or active".into(),
        Error::Provider { .. } => "provider repair failed".into(),
        Error::InvalidState { .. } => "durable state is invalid".into(),
        Error::Io(_) | Error::Json(_) | Error::Sql(_) => "durable state operation failed".into(),
        Error::Message(_) => "repair action failed".into(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::{Arc, Mutex};

    use super::super::{RepairAction, SelfHealPolicy};
    use super::*;
    use crate::Engine;
    use crate::runs::{ProbeState, ProcessProbe};

    #[derive(Default)]
    struct FakeProbe {
        workers: BTreeSet<u32>,
    }

    impl ProcessProbe for FakeProbe {
        fn pid_alive(&self, _pid: u32) -> bool {
            false
        }

        fn worker_alive(&self, worker_pid: u32, _engine: Engine) -> bool {
            self.workers.contains(&worker_pid)
        }

        fn worker_state(&self, worker_pid: u32, _engine: Engine) -> ProbeState {
            if self.workers.contains(&worker_pid) {
                ProbeState::Alive
            } else {
                ProbeState::Dead
            }
        }
    }

    struct FakeExecutor {
        calls: Arc<Mutex<Vec<(String, RepairAction)>>>,
        fail: bool,
    }

    impl RepairExecutor for FakeExecutor {
        fn execute(&self, plan: &RepairPlan, _run: &RunRecord) -> Result<()> {
            self.calls
                .lock()
                .unwrap()
                .push((plan.run_id.clone(), plan.action));
            if self.fail {
                Err(Error::Message("fake repair failure".into()))
            } else {
                Ok(())
            }
        }
    }

    fn run(id: &str, status: RunStatus, now: i64) -> RunRecord {
        let mut run = RunRecord::new(
            id,
            Engine::Claude,
            "model",
            "task",
            "/account",
            "/workspace",
            now - 10,
        );
        run.status = status;
        run.ended = status.is_terminal().then_some(now - 1);
        run
    }

    #[test]
    fn repeated_reconciliation_is_idempotent_and_does_not_launch_again() {
        let temp = tempfile::tempdir().unwrap();
        let runs = RunStore::new(temp.path().join("runs"));
        runs.create(&run("run-1", RunStatus::Error, 100)).unwrap();
        let probe = FakeProbe::default();
        let ledger = SelfHealStore::new(temp.path().join("self-heal.json"));
        let service = ReconciliationService::new(&runs, &probe, &ledger);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let executor = FakeExecutor {
            calls: Arc::clone(&calls),
            fail: false,
        };
        let mut request = ReconcileRequest::new(100);
        request.policy.initial_backoff = std::time::Duration::ZERO;
        request.policy.max_backoff = std::time::Duration::ZERO;
        let first = service.reconcile(&request, Some(&executor)).unwrap();
        let second = service.reconcile(&request, Some(&executor)).unwrap();
        assert_eq!(first.healed.len(), 1);
        assert!(second.healed.is_empty());
        assert_eq!(calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn persisted_reservation_survives_a_crash_without_duplicate_dispatch() {
        let temp = tempfile::tempdir().unwrap();
        let runs = RunStore::new(temp.path().join("runs"));
        runs.create(&run("run-1", RunStatus::Interrupted, 100))
            .unwrap();
        let probe = FakeProbe::default();
        let ledger = SelfHealStore::new(temp.path().join("self-heal.json"));
        let policy = SelfHealPolicy {
            initial_backoff: std::time::Duration::ZERO,
            max_backoff: std::time::Duration::ZERO,
            ..SelfHealPolicy::default()
        };
        ledger
            .reserve("run-1", RepairAction::Resume, 100, &policy, false)
            .unwrap();
        let service = ReconciliationService::new(&runs, &probe, &ledger);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let executor = FakeExecutor {
            calls: Arc::clone(&calls),
            fail: false,
        };
        let mut request = ReconcileRequest::new(100);
        request.policy = policy;
        let report = service.reconcile(&request, Some(&executor)).unwrap();
        assert!(report.healed.is_empty());
        assert!(calls.lock().unwrap().is_empty());
    }

    #[test]
    fn cap_and_batch_bound_the_number_of_repairs() {
        let temp = tempfile::tempdir().unwrap();
        let runs = RunStore::new(temp.path().join("runs"));
        for id in ["one", "two", "three"] {
            runs.create(&run(id, RunStatus::Error, 100)).unwrap();
        }
        let probe = FakeProbe::default();
        let ledger = SelfHealStore::new(temp.path().join("self-heal.json"));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let executor = FakeExecutor {
            calls: Arc::clone(&calls),
            fail: false,
        };
        let mut request = ReconcileRequest::new(100);
        request.policy.max_batch = 2;
        request.policy.initial_backoff = std::time::Duration::ZERO;
        request.policy.max_backoff = std::time::Duration::ZERO;
        let service = ReconciliationService::new(&runs, &probe, &ledger);
        let report = service.reconcile(&request, Some(&executor)).unwrap();
        assert_eq!(report.healed.len(), 2);
        assert_eq!(report.skipped.len(), 1);
    }

    #[test]
    fn orphaned_and_terminal_statuses_are_classified_without_provider_access() {
        let mut probe = FakeProbe::default();
        probe.workers.insert(42);
        let mut orphan = run("orphan", RunStatus::Running, 100);
        orphan.worker_pid = Some(42);
        assert_eq!(classify(&orphan, &probe), ReconcileClass::Orphaned);
        assert_eq!(
            classify(&run("error", RunStatus::Error, 100), &probe),
            ReconcileClass::NeedsRetry
        );
        assert_eq!(
            classify(&run("done", RunStatus::Done, 100), &probe),
            ReconcileClass::Resolved
        );
    }
}
