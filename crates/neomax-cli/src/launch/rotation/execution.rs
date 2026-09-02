use std::time::Duration;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use neomax_core::accounts::{
    AccountControlStore, AccountInventory, RotationClaimStore, SelectionPolicy,
};
use neomax_core::orchestration::commands::Launcher;
use neomax_core::orchestration::continuation::{
    ContinuationMode, ContinuationPort, ContinuationRequest, FilesystemContinuation,
    RotationTrigger, apply_to_run_with_resolver,
};
use neomax_core::providers::ProviderRegistry;
use neomax_core::runs::execution::{AttemptSupervisor, SupervisorConfig, prepare_attempt};
use neomax_core::runs::failover::{FailoverDecision, FailoverTarget, plan_failover};
use neomax_core::runs::{RunLiveWorkSource, RunRecord, RunStatus, RunStore, SystemProcessProbe};
use neomax_core::usage::UsageCacheStore;
use neomax_core::{StatePaths, WorkerScope};

use super::options::RotationOptions;
use super::report::{RotationReport, failover_stop_message, without_target};
use crate::context::RuntimeContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RotationExecution {
    Resume,
    ModelFree,
}

pub(super) fn resume(
    launcher: Launcher,
    context: &RuntimeContext,
    args: &[String],
    trigger: RotationTrigger,
) -> Result<Vec<RotationReport>> {
    rotate_with_execution(launcher, context, args, trigger, RotationExecution::Resume)
}

pub(super) fn model_free(
    launcher: Launcher,
    context: &RuntimeContext,
    args: &[String],
    trigger: RotationTrigger,
) -> Result<Vec<RotationReport>> {
    rotate_with_execution(
        launcher,
        context,
        args,
        trigger,
        RotationExecution::ModelFree,
    )
}

fn rotate_with_execution(
    launcher: Launcher,
    context: &RuntimeContext,
    args: &[String],
    trigger: RotationTrigger,
    execution: RotationExecution,
) -> Result<Vec<RotationReport>> {
    let options = RotationOptions::parse(args)?;
    let ids = options.ids;
    let provider_runtime = context.provider_runtime()?;
    let providers = provider_runtime.registry();
    let runs = RunStore::new(&context.paths.runs);
    let usage = UsageCacheStore::new(&context.paths.usage);
    let controls = AccountControlStore::new(&context.paths.cooldowns, &context.paths.paused);
    let claims =
        RotationClaimStore::new(&context.paths.rotation_claims, &context.paths.rotation_lock);
    let continuation =
        FilesystemContinuation::in_paths(&context.paths, Some(context.paths.usage.clone()));
    let scope = super::super::worker_scope(launcher, options.scope);
    let probe = SystemProcessProbe;
    let live_work = RunLiveWorkSource::with_system(&runs, &probe);
    let inventory = AccountInventory {
        providers,
        quota: &usage,
        controls: &controls,
        live_work: &live_work,
    };
    let now = timestamp(context.now);
    let accounts = inventory.routing_snapshots(&scope, now)?;
    let selection = SelectionPolicy::from_settings(&context.settings);
    let runs_to_rotate: Vec<RunRecord> = if ids.is_empty() {
        runs.all()?
            .into_iter()
            .filter(|run| {
                scope.contains(run.engine)
                    && if trigger == RotationTrigger::Tick && !options.active {
                        matches!(run.status, RunStatus::Limit)
                    } else {
                        matches!(run.status, RunStatus::Running | RunStatus::Limit)
                    }
            })
            .collect()
    } else {
        ids.into_iter()
            .map(|id| runs.load(&id))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|run| scope.contains(run.engine))
            .collect()
    };
    if runs_to_rotate.is_empty() {
        return Ok(Vec::new());
    }

    let mut reports = Vec::with_capacity(runs_to_rotate.len());
    for run in runs_to_rotate {
        reports.push(rotate_one(
            &run,
            &runs,
            providers,
            &accounts,
            &controls,
            &claims,
            &continuation,
            &selection,
            &scope,
            &context.paths,
            &context.settings,
            trigger,
            now,
            execution,
        )?);
    }
    Ok(reports)
}

#[allow(clippy::too_many_arguments)]
fn rotate_one(
    original: &RunRecord,
    runs: &RunStore,
    providers: &ProviderRegistry,
    accounts: &[neomax_core::accounts::AccountSnapshot],
    controls: &AccountControlStore,
    claims: &RotationClaimStore,
    continuation: &FilesystemContinuation,
    selection: &SelectionPolicy,
    scope: &WorkerScope,
    paths: &StatePaths,
    settings: &neomax_core::EffectiveSettings,
    trigger: RotationTrigger,
    now: DateTime<Utc>,
    execution: RotationExecution,
) -> Result<RotationReport> {
    if matches!(original.status, RunStatus::Error | RunStatus::Done) {
        bail!(
            "run {} has status {}; only running or quota-limited runs can rotate",
            original.id,
            original.status.as_str()
        );
    }
    let mut run = runs.load(&original.id)?;
    let source_engine = run.engine;
    let source_account = run.account();
    let target = loop {
        let decision = plan_failover(&run, RunStatus::Limit, accounts, scope, now, selection);
        let target = match decision {
            FailoverDecision::Continue(target) => target,
            FailoverDecision::Stop(stop) => {
                return Ok(without_target(&run, failover_stop_message(stop)));
            }
        };
        if !claims.try_claim(&target.account.profile, now)? {
            if !run.tried.contains(&target.account.profile) {
                run.tried.push(target.account.profile);
            }
            continue;
        }
        if target.crosses_provider && !trigger.allows_cross_provider() {
            let _ = claims.release(&target.account.profile);
            return Ok(without_target(
                &run,
                "cross-provider continuation is not allowed for manual rotation".into(),
            ));
        }
        break target;
    };
    let claim_profile = target.account.profile.clone();
    let resets_at = run.resets_at;
    let original_worker_pid = run.worker_pid;
    let original_supervisor_pid = run.supervisor_pid;
    if execution == RotationExecution::Resume {
        if let Some(pid) = run.worker_pid.take() {
            if let Err(error) = crate::process::terminate_worker(pid) {
                let _ = claims.release(&claim_profile);
                return Err(error);
            }
        }
    }
    run.status = RunStatus::Limit;
    let source_rotation_eligible = accounts
        .iter()
        .find(|account| account.profile == run.profile)
        .is_some_and(|account| account.rotation_eligible);
    let source_quota = accounts
        .iter()
        .find(|account| account.engine == run.engine && account.profile == run.profile);
    let request = ContinuationRequest::from_run_with_source_eligibility(
        &run,
        target.account.clone(),
        trigger,
        now,
        source_rotation_eligible,
    )
    .with_observed_quota(source_quota);
    let outcome = match continuation.continue_after_limit(&request) {
        Ok(outcome) => outcome,
        Err(error) => {
            let _ = claims.release(&claim_profile);
            run.error_detail = Some(error.to_string());
            run.ended = Some(now.timestamp());
            let _ = runs.save_preserving_control_markers(&run);
            return Err(error.into());
        }
    };
    let crosses_provider = outcome.target_engine != source_engine;
    let model_overrides =
        neomax_core::settings::process_environment_model_overrides(&settings.config_path)
            .map_err(|error| anyhow::anyhow!(error))?;
    apply_to_run_with_resolver(
        &mut run,
        &FailoverTarget {
            account: target.account.clone(),
            crosses_provider,
        },
        &outcome,
        &model_overrides,
    );
    if execution == RotationExecution::ModelFree
        && outcome.mode != ContinuationMode::InPlaceAuthRotation
    {
        if let Some(pid) = original_worker_pid {
            if let Err(error) = crate::process::terminate_worker(pid) {
                let _ = claims.release(&claim_profile);
                return Err(error);
            }
        }
        run.worker_pid = None;
    }
    run.supervisor_pid = if execution == RotationExecution::ModelFree {
        if outcome.mode == ContinuationMode::InPlaceAuthRotation {
            original_supervisor_pid
        } else {
            None
        }
    } else {
        Some(std::process::id())
    };
    runs.save_preserving_control_markers(&run)?;
    let cooldown_profile = outcome.cooldown_profile.clone();
    controls.set_cooldown(
        &cooldown_profile,
        resets_at,
        now.timestamp_millis() as f64 / 1000.0,
        Duration::from_secs(30 * 60).as_secs_f64(),
    )?;
    if execution == RotationExecution::ModelFree {
        if outcome.mode == ContinuationMode::InPlaceAuthRotation {
            run.supervisor_pid = original_supervisor_pid;
            run.worker_pid = original_worker_pid;
        } else {
            run.supervisor_pid = None;
            run.worker_pid = None;
        }
        run.status = RunStatus::Running;
        run.ended = None;
        runs.save_preserving_control_markers(&run)?;
        let _ = claims.release(&claim_profile);
        return Ok(RotationReport {
            run_id: run.id,
            status: "continued (model-free)".into(),
            source_engine: source_engine.to_string(),
            source_account,
            target_engine: Some(outcome.target_engine.to_string()),
            target_account: Some(target.account.account),
            attempt: run.attempt,
            crosses_provider,
        });
    }
    let provider = providers
        .get(run.engine)
        .with_context(|| format!("provider adapter {} is not registered", run.engine))?;
    let resume_session = neomax_core::providers::catalog::supports_native_resume(run.engine)
        .then_some(run.resume_session.as_deref())
        .flatten();
    let resumed = resume_session.is_some();
    let prepared = match prepare_attempt(provider, &run, settings, paths, resume_session) {
        Ok(prepared) => prepared,
        Err(error) => {
            let _ = claims.release(&claim_profile);
            run.status = RunStatus::Error;
            run.error_detail = Some(error.to_string());
            run.ended = Some(now.timestamp());
            runs.save_preserving_control_markers(&run)?;
            return Err(error.into());
        }
    };
    let supervisor = AttemptSupervisor::new(provider, SupervisorConfig::for_run(&run)?);
    let outcome_run = match supervisor.run(prepared, &mut run, &paths.logs, resumed, |spawned| {
        runs.save_preserving_control_markers(spawned).map(|_| ())
    }) {
        Ok(outcome) => outcome,
        Err(error) => {
            let _ = claims.release(&claim_profile);
            run.status = RunStatus::Error;
            run.error_detail = Some(error.to_string());
            run.ended = Some(now.timestamp());
            runs.save_preserving_control_markers(&run)?;
            return Err(error.into());
        }
    };
    run.resume_session = None;
    run.status = outcome_run.status;
    run.ended = Some(now.timestamp());
    run.worker_pid = None;
    runs.save_preserving_control_markers(&run)?;
    let _ = claims.release(&claim_profile);
    Ok(RotationReport {
        run_id: run.id,
        status: run.status.as_str().into(),
        source_engine: source_engine.to_string(),
        source_account,
        target_engine: Some(outcome.target_engine.to_string()),
        target_account: Some(target.account.account),
        attempt: run.attempt,
        crosses_provider,
    })
}

fn timestamp(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(seconds, 0).unwrap_or_else(Utc::now)
}
