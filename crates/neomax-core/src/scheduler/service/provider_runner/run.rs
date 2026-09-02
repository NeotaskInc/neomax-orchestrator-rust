use super::super::super::runtime::WorkerOutcome;
use super::ProviderExecution;
use crate::accounts::{AccountControlStore, AccountInventory, RotationClaimStore};
use crate::orchestration::continuation::FilesystemContinuation;
use crate::runs::coordinator::{NativeAttemptRunner, RunCoordinator};
use crate::runs::lifecycle::{ManagedRunWorktreeFinalizer, RunFinalizer};
use crate::runs::{
    EventStore, HistoryStore, RunLiveWorkSource, RunRecord, RunStatus, RunStore, SystemProcessProbe,
};
use crate::usage::UsageCacheStore;
use crate::{Error, Result};

impl ProviderExecution {
    pub(super) fn execute_run(
        &self,
        scheduler_run_id: &str,
        run: RunRecord,
    ) -> Result<WorkerOutcome> {
        let outcome = self.execute_run_inner(scheduler_run_id, run);
        let released = self.inner.admission.release(scheduler_run_id);
        match (outcome, released) {
            (Ok(value), Ok(_)) => Ok(value),
            (Err(error), Ok(_)) => Err(error),
            (Ok(_), Err(error)) => Err(Error::Message(format!(
                "worker completed but dispatch lease release failed: {error}"
            ))),
            (Err(error), Err(release_error)) => Err(Error::Message(format!(
                "worker execution failed: {error}; dispatch lease release failed: {release_error}"
            ))),
        }
    }

    fn execute_run_inner(
        &self,
        scheduler_run_id: &str,
        mut run: RunRecord,
    ) -> Result<WorkerOutcome> {
        let runs = RunStore::new(self.inner.paths.runs.clone());
        let usage = UsageCacheStore::new(self.inner.paths.usage.clone());
        let controls = AccountControlStore::new(
            self.inner.paths.cooldowns.clone(),
            self.inner.paths.paused.clone(),
        );
        let claims = RotationClaimStore::new(
            self.inner.paths.rotation_claims.clone(),
            self.inner.paths.rotation_lock.clone(),
        );
        let continuation = FilesystemContinuation::in_paths(
            &self.inner.paths,
            Some(self.inner.paths.usage.clone()),
        );
        let probe = SystemProcessProbe;
        let live_work = RunLiveWorkSource::with_system(&runs, &probe);
        let inventory = AccountInventory {
            providers: self.inner.providers.as_ref(),
            quota: &usage,
            controls: &controls,
            live_work: &live_work,
        };
        let events = EventStore::with_legacy_directory(
            self.inner.paths.run_events.clone(),
            self.inner.paths.events.clone(),
        );
        let history = HistoryStore::new(
            self.inner.paths.history_db.clone(),
            self.inner.paths.logs.clone(),
            self.inner.paths.history_logs.clone(),
            self.inner.paths.history_pending.clone(),
        );
        let worktrees = ManagedRunWorktreeFinalizer::new(&self.inner.paths.worktrees);
        let pull_requests = crate::git::pull_request::GitHubPullRequestAdapter::default();
        let finalizer = RunFinalizer {
            runs: &runs,
            events: &events,
            history: &history,
            controls: &controls,
            worktrees: &worktrees,
            pull_requests: Some(&pull_requests),
        };
        let attempts = NativeAttemptRunner {
            providers: self.inner.providers.as_ref(),
            settings: &self.inner.settings,
            paths: &self.inner.paths,
            runs: &runs,
            quota: &usage,
        };
        let coordinator = RunCoordinator {
            attempts: &attempts,
            inventory: &inventory,
            runs: &runs,
            events: &events,
            controls: &controls,
            finalizer: &finalizer,
            scope: &self.inner.scope,
            selection: self.inner.selection.clone(),
            clock: self.inner.clock.as_ref(),
            default_cooldown: self.inner.default_cooldown,
            continuation: Some(&continuation),
            claims: Some(&claims),
        };
        let model_overrides =
            crate::settings::process_environment_model_overrides(&self.inner.settings.config_path)?;
        let finalization = match coordinator.execute_with_model_resolver(&mut run, &model_overrides)
        {
            Ok(finalization) => finalization,
            Err(error) => {
                let detail = error.to_string();
                if let Err(persist_error) = runs.update(&run.id, |record| {
                    record.status = RunStatus::Error;
                    record.error_detail = Some(detail.clone());
                    record.ended = Some(self.inner.clock.now().timestamp());
                    Ok(())
                }) {
                    return Err(Error::Message(format!(
                        "worker execution failed: {detail}; durable error update failed: {persist_error}"
                    )));
                }
                return Err(error);
            }
        };
        Ok(super::outcome::outcome_for_status(
            scheduler_run_id,
            &run,
            finalization.status,
        ))
    }
}
