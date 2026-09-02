use std::time::Duration;

use crate::accounts::{AccountControlStore, AccountInventory, RotationClaimStore, SelectionPolicy};
use crate::orchestration::continuation::{
    ContinuationMode, ContinuationPort, ContinuationRequest, RotationTrigger,
    apply_to_run_with_resolver,
};
use crate::runs::coordinator::events::{append_attempt, append_failover_with_strategy};
use crate::runs::failover::{
    FailoverDecision, ModelResolver, NoModelOverrides, apply_failover_with_resolver, plan_failover,
};
use crate::runs::lifecycle::{
    Finalization, FinalizeOptions, RunFinalizer, exit_code, mark_attempt_started,
};
use crate::runs::{EventStore, RunRecord, RunStatus, RunStore};
use crate::{Result, WorkerScope};

use super::attempt::AttemptRunner;
use super::clock::RunClock;

pub struct RunCoordinator<'a> {
    pub attempts: &'a dyn AttemptRunner,
    pub inventory: &'a AccountInventory<'a>,
    pub runs: &'a RunStore,
    pub events: &'a EventStore,
    pub controls: &'a AccountControlStore,
    pub finalizer: &'a RunFinalizer<'a>,
    pub scope: &'a WorkerScope,
    pub selection: SelectionPolicy,
    pub clock: &'a dyn RunClock,
    pub default_cooldown: Duration,
    pub continuation: Option<&'a dyn ContinuationPort>,
    pub claims: Option<&'a RotationClaimStore>,
}

impl RunCoordinator<'_> {
    pub fn execute(&self, run: &mut RunRecord) -> Result<Finalization> {
        self.execute_with_model_resolver(run, &NoModelOverrides)
    }

    pub fn execute_with_model_resolver(
        &self,
        run: &mut RunRecord,
        models: &dyn ModelResolver,
    ) -> Result<Finalization> {
        let mut warnings = Vec::new();
        loop {
            if self.refresh_control_markers(run)? {
                return self.finish(
                    run,
                    RunStatus::Aborted,
                    self.clock.now(),
                    warnings,
                );
            }
            let started_at = self.clock.now();
            mark_attempt_started(run, std::process::id());
            *run = self.runs.save_preserving_kill(run)?;
            if self.refresh_control_markers(run)? {
                return self.finish(
                    run,
                    RunStatus::Aborted,
                    self.clock.now(),
                    warnings,
                );
            }
            record_event(
                &mut warnings,
                append_attempt(self.events, run, "attempt_started", None, started_at),
            );
            if self.refresh_control_markers(run)? {
                return self.finish(
                    run,
                    RunStatus::Aborted,
                    self.clock.now(),
                    warnings,
                );
            }

            let active_attempt = run.attempt;
            let status = match self.attempts.run_attempt(run) {
                Ok(status) => status,
                Err(error) => {
                    run.error_detail = Some(error.to_string());
                    RunStatus::Error
                }
            };
            run.status = status;
            let persisted = self.runs.save_preserving_kill(run)?;
            if persisted.attempt > active_attempt {
                *run = persisted;
                warnings.push(format!(
                    "attempt {active_attempt} yielded to resumed attempt {}",
                    run.attempt
                ));
                return Ok(Finalization {
                    status: run.status,
                    exit_code: exit_code(run.status),
                    cooldown_until: None,
                    archive: None,
                    warnings,
                });
            }
            *run = persisted;
            let finished_at = self.clock.now();
            record_event(
                &mut warnings,
                append_attempt(
                    self.events,
                    run,
                    "attempt_finished",
                    Some(status),
                    finished_at,
                ),
            );

            if self.refresh_control_markers(run)? {
                return self.finish(run, RunStatus::Aborted, finished_at, warnings);
            }

            if !matches!(status, RunStatus::Limit | RunStatus::Error)
                || run.no_failover
                || run.resumed
            {
                return self.finish(run, status, finished_at, warnings);
            }

            if status == RunStatus::Error {
                return self.finish(run, status, finished_at, warnings);
            }

            if self.refresh_control_markers(run)? {
                return self.finish(run, RunStatus::Aborted, finished_at, warnings);
            }

            let accounts = match self.inventory.routing_snapshots(self.scope, finished_at) {
                Ok(accounts) => accounts,
                Err(error) => {
                    record_cooldown(
                        &mut warnings,
                        self.controls,
                        &run.profile,
                        run.resets_at,
                        finished_at,
                        self.default_cooldown,
                    );
                    warnings.push(format!("account inventory: {error}"));
                    return self.finish(run, status, finished_at, warnings);
                }
            };
            if self.refresh_control_markers(run)? {
                return self.finish(run, RunStatus::Aborted, finished_at, warnings);
            }
            let decision = match self.plan_target(run, status, &accounts, finished_at) {
                Ok(decision) => decision,
                Err(error) => {
                    warnings.push(format!("rotation claim: {error}"));
                    record_cooldown(
                        &mut warnings,
                        self.controls,
                        &run.profile,
                        run.resets_at,
                        finished_at,
                        self.default_cooldown,
                    );
                    return self.finish(run, status, finished_at, warnings);
                }
            };
            if self.refresh_control_markers(run)? {
                return self.finish(run, RunStatus::Aborted, finished_at, warnings);
            }
            match decision {
                FailoverDecision::Continue(target) => {
                    if self.refresh_control_markers(run)? {
                        self.release_claim(&target.account.profile, &mut warnings);
                        return self.finish(run, RunStatus::Aborted, finished_at, warnings);
                    }
                    let outcome = if let Some(port) = self.continuation {
                        let source_rotation_eligible = accounts
                            .iter()
                            .find(|account| account.profile == run.profile)
                            .is_some_and(|account| account.rotation_eligible);
                        let source = accounts.iter().find(|account| {
                            account.engine == run.engine && account.profile == run.profile
                        });
                        let request = ContinuationRequest::from_run_with_source_eligibility(
                            run,
                            target.account.clone(),
                            RotationTrigger::Quota,
                            finished_at,
                            source_rotation_eligible,
                        )
                        .with_observed_quota(source);
                        match port.continue_after_limit(&request) {
                            Ok(outcome) => Some(outcome),
                            Err(error) => {
                                warnings.push(format!("continuation: {error}"));
                                self.release_claim(&target.account.profile, &mut warnings);
                                record_cooldown(
                                    &mut warnings,
                                    self.controls,
                                    &run.profile,
                                    run.resets_at,
                                    finished_at,
                                    self.default_cooldown,
                                );
                                return self.finish(run, status, finished_at, warnings);
                            }
                        }
                    } else {
                        None
                    };
                    if self.refresh_control_markers(run)? {
                        self.release_claim(&target.account.profile, &mut warnings);
                        return self.finish(run, RunStatus::Aborted, finished_at, warnings);
                    }
                    let crosses_provider = outcome
                        .as_ref()
                        .is_some_and(|value| value.mode == ContinuationMode::CrossProviderHandoff)
                        || target.crosses_provider;
                    let strategy = outcome.as_ref().map_or(
                        if crosses_provider {
                            "cross_provider_handoff"
                        } else {
                            "same_provider_handoff"
                        },
                        |value| match value.mode {
                            ContinuationMode::InPlaceAuthRotation => "in_place_auth_rotation",
                            ContinuationMode::SameProviderHandoff => "same_provider_handoff",
                            ContinuationMode::CrossProviderHandoff => "cross_provider_handoff",
                        },
                    );
                    record_event(
                        &mut warnings,
                        append_failover_with_strategy(
                            self.events,
                            run,
                            status,
                            &target.account,
                            crosses_provider,
                            strategy,
                            finished_at,
                        ),
                    );
                    let cooldown_profile = outcome
                        .as_ref()
                        .map(|value| value.cooldown_profile.clone())
                        .unwrap_or_else(|| run.profile.clone());
                    let resets_at = run.resets_at;
                    if let Some(ref outcome) = outcome {
                        apply_to_run_with_resolver(run, &target, outcome, models);
                    } else {
                        record_cooldown(
                            &mut warnings,
                            self.controls,
                            &cooldown_profile,
                            resets_at,
                            finished_at,
                            self.default_cooldown,
                        );
                        apply_failover_with_resolver(run, &target, models);
                    }
                    if outcome.is_some() {
                        record_cooldown(
                            &mut warnings,
                            self.controls,
                            &cooldown_profile,
                            resets_at,
                            finished_at,
                            self.default_cooldown,
                        );
                    }
                    *run = self.runs.save_preserving_kill(run)?;
                }
                FailoverDecision::Stop(_) => {
                    record_cooldown(
                        &mut warnings,
                        self.controls,
                        &run.profile,
                        run.resets_at,
                        finished_at,
                        self.default_cooldown,
                    );
                    return self.finish(run, status, finished_at, warnings);
                }
            }
        }
    }

    fn refresh_control_markers(&self, run: &mut RunRecord) -> Result<bool> {
        let persisted = self.runs.load(&run.id)?;
        let interrupted = persisted.killed || persisted.status.is_interruption();
        if !interrupted {
            return Ok(false);
        }
        run.killed |= persisted.killed;
        if persisted.status.is_interruption() {
            run.status = persisted.status;
            if persisted.ended.is_some() {
                run.ended = persisted.ended;
            }
            if persisted.acknowledged.is_some() {
                run.acknowledged = persisted.acknowledged;
            }
            if persisted.interrupt_signal.is_some() {
                run.interrupt_signal = persisted.interrupt_signal;
            }
        }
        Ok(true)
    }

    fn plan_target(
        &self,
        run: &mut RunRecord,
        status: RunStatus,
        accounts: &[crate::accounts::AccountSnapshot],
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<FailoverDecision> {
        loop {
            let decision = plan_failover(run, status, accounts, self.scope, now, &self.selection);
            let FailoverDecision::Continue(target) = decision else {
                return Ok(decision);
            };
            let Some(claims) = self.claims else {
                return Ok(FailoverDecision::Continue(target));
            };
            if claims.try_claim(&target.account.profile, now)? {
                return Ok(FailoverDecision::Continue(target));
            }
            if !run.tried.contains(&target.account.profile) {
                run.tried.push(target.account.profile);
            }
        }
    }

    fn release_claim(&self, profile: &std::path::Path, warnings: &mut Vec<String>) {
        if let Some(claims) = self.claims {
            if let Err(error) = claims.release(profile) {
                warnings.push(format!("rotation claim release: {error}"));
            }
        }
    }

    fn finish(
        &self,
        run: &mut RunRecord,
        status: RunStatus,
        now: chrono::DateTime<chrono::Utc>,
        mut warnings: Vec<String>,
    ) -> Result<Finalization> {
        let mut result = self.finalizer.finish(
            run,
            status,
            FinalizeOptions {
                now,
                account_number: account_number(run),
                default_cooldown: self.default_cooldown,
            },
        )?;
        warnings.append(&mut result.warnings);
        result.warnings = warnings;
        Ok(result)
    }
}

fn record_cooldown(
    warnings: &mut Vec<String>,
    controls: &AccountControlStore,
    profile: &std::path::Path,
    resets_at: Option<f64>,
    now: chrono::DateTime<chrono::Utc>,
    default_cooldown: Duration,
) {
    if let Err(error) = controls.set_cooldown(
        profile,
        resets_at,
        now.timestamp_millis() as f64 / 1000.0,
        default_cooldown.as_secs_f64(),
    ) {
        warnings.push(format!("account cooldown: {error}"));
    }
}

fn record_event(warnings: &mut Vec<String>, result: Result<()>) {
    if let Err(error) = result {
        warnings.push(format!("event journal: {error}"));
    }
}

fn account_number(run: &RunRecord) -> Option<u32> {
    crate::providers::catalog::profile_account_number(run.engine, &run.profile)
}
