use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use neomax_core::Engine;
use neomax_core::accounts::{AccountControlStore, AccountSnapshot, engine_has_five_hour};
use neomax_core::orchestration::continuation::{
    ContinuationMode, ContinuationRequest, ContinuationService, CredentialRotationPort,
    RotationTrigger, apply_to_run_with_resolver,
};
use neomax_core::orchestration::handoff::{AccountId, HandoffBaton, HandoffStore};
use neomax_core::runs::{EventStore, RunEvent, RunRecord, RunStatus, RunStore};
use neomax_core::{Error, Result as CoreResult};
use serde_json::json;

use super::super::options::HandoffOptions;
use super::super::selection::{HandoffSelection, context_time};
use crate::context::RuntimeContext;

pub(crate) fn find_current_run(
    runs: &RunStore,
    options: &HandoffOptions,
    selection: &HandoffSelection,
    context: &RuntimeContext,
) -> Result<Option<RunRecord>> {
    if options.interactive_only {
        return Ok(None);
    }
    if let Some(id) = options.run_id.as_deref() {
        let run = runs
            .load(id)
            .with_context(|| format!("could not load run {id}"))?;
        if run.engine != selection.engine {
            bail!(
                "run {id} belongs to {}, not {}",
                run.engine,
                selection.engine
            );
        }
        return Ok(Some(run));
    }
    let session = options
        .session
        .clone()
        .or_else(|| std::env::var("NEOMAX_ORCH_SESSION").ok())
        .filter(|value| !value.trim().is_empty());
    let mut candidates = runs
        .all()?
        .into_iter()
        .filter(|run| {
            matches!(
                run.status,
                RunStatus::Running | RunStatus::Orphaned | RunStatus::Limit
            )
        })
        .filter(|run| run.engine == selection.engine)
        .filter(|run| same_path(&run.profile, &selection.current_profile))
        .filter(|run| {
            session.as_deref().is_none_or(|session| {
                run.session.as_deref() == Some(session)
                    || run.orch_session.as_deref() == Some(session)
                    || run.id == session
            })
        })
        .filter(|run| {
            run.cwd
                .as_deref()
                .or(Some(run.workdir.as_path()))
                .is_some_and(|path| same_path(path, &context.cwd) || same_path(path, &options.cwd))
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|run| std::cmp::Reverse(run.started));
    Ok(candidates.into_iter().next())
}

pub(crate) fn continue_tracked_run(
    original: &RunRecord,
    options: &HandoffOptions,
    selection: &HandoffSelection,
    target: AccountSnapshot,
    context: &RuntimeContext,
    runs: &RunStore,
    trigger: RotationTrigger,
) -> Result<ContinuationMode> {
    let mut run = runs.load(&original.id)?;
    let resets_at = run.resets_at;
    let mut request = ContinuationRequest::from_run_with_source_eligibility(
        &run,
        target.clone(),
        trigger,
        context_time(context),
        selection.source.rotation_eligible,
    )
    .with_observed_quota(Some(&selection.source));
    request.source_profile = selection.current_profile.clone();
    request.source_account = selection.source.account.clone();
    request.reason = options.reason.clone();
    request.cwd = run.cwd.clone().unwrap_or_else(|| options.cwd.clone());
    request.target = target.clone();

    let handoff = HandoffStore::at_state_dir(&context.paths.state);
    let rotation = NoCredentialRotation;
    let service = ContinuationService {
        rotation: &rotation,
        handoff: &handoff,
    };
    let outcome = service
        .continue_run(&request)
        .map_err(|error| anyhow::anyhow!(error))?;
    let source_engine = run.engine;
    apply_to_run_with_resolver(
        &mut run,
        &neomax_core::runs::failover::FailoverTarget {
            account: target.clone(),
            crosses_provider: target.engine != source_engine,
        },
        &outcome,
        &options.model_overrides,
    );
    run.supervisor_pid = None;
    runs.save_preserving_control_markers(&run)?;

    let controls = AccountControlStore::new(&context.paths.cooldowns, &context.paths.paused);
    controls.set_cooldown(
        &outcome.cooldown_profile,
        resets_at,
        context.now as f64,
        Duration::from_secs(30 * 60).as_secs_f64(),
    )?;
    append_event(context, &run, &selection.source, &target, &options.reason)?;
    Ok(outcome.mode)
}

pub(super) fn save_untracked_baton(
    options: &HandoffOptions,
    selection: &HandoffSelection,
    context: &RuntimeContext,
) -> Result<()> {
    let target = selection
        .target
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no eligible handoff target is available"))?;
    let mut extra = BTreeMap::new();
    extra.insert("source_profile".into(), json!(selection.current_profile));
    extra.insert("target_profile".into(), json!(target.account.profile));
    extra.insert("worker_scope".into(), json!(options.worker_scope));
    extra.insert("session".into(), json!(options.session));
    extra.insert("run_id".into(), json!(options.run_id));
    extra.insert("kickoff".into(), json!(options.kickoff));
    extra.insert("prompt".into(), json!(options.kickoff));
    for key in ["NEOMAX_PROJECT", "NEOMAX_BRANCH_PREFIX"] {
        if let Some(value) = options.environment.values.get(key) {
            extra.insert(key.to_ascii_lowercase(), json!(value));
        }
    }
    extra.insert(
        "project".into(),
        options
            .environment
            .values
            .get("NEOMAX_PROJECT")
            .cloned()
            .map_or(serde_json::Value::Null, serde_json::Value::String),
    );
    extra.insert(
        "branch_prefix".into(),
        options
            .environment
            .values
            .get("NEOMAX_BRANCH_PREFIX")
            .cloned()
            .map_or(serde_json::Value::Null, serde_json::Value::String),
    );
    let baton = HandoffBaton {
        ts: context.now,
        engine: selection.engine,
        from_account: AccountId::from(selection.source.account.clone()),
        to_account: Some(AccountId::from(target.account.account.clone())),
        reason: options.reason.clone(),
        cwd: options.cwd.clone(),
        five_hour: engine_has_five_hour(selection.source.engine)
            .then_some(selection.source.five_hour_percent)
            .flatten()
            .filter(|value| value.is_finite()),
        seven_day: selection
            .source
            .weekly_percent
            .filter(|value| value.is_finite()),
        extra,
    };
    HandoffStore::at_state_dir(&context.paths.state).save(&baton)?;
    Ok(())
}

fn append_event(
    context: &RuntimeContext,
    run: &RunRecord,
    source: &AccountSnapshot,
    target: &AccountSnapshot,
    reason: &str,
) -> Result<()> {
    let event = RunEvent {
        ts: context.now,
        run: run.id.clone(),
        event: "handoff".into(),
        engine: run.engine,
        account: Some(source.account.clone()),
        status: Some(run.status),
        attempt: Some(run.attempt),
        extra: BTreeMap::from([
            ("from_account".into(), json!(source.account)),
            ("to_account".into(), json!(target.account)),
            ("target_engine".into(), json!(target.engine)),
            ("reason".into(), json!(reason)),
        ]),
    };
    EventStore::with_legacy_directory(&context.paths.run_events, &context.paths.events)
        .append(&event, context_time(context))?;
    Ok(())
}

fn same_path(left: &Path, right: &Path) -> bool {
    normalize(left) == normalize(right)
}

fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::RootDir
            | std::path::Component::Prefix(_)
            | std::path::Component::Normal(_) => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

#[derive(Debug, Clone, Copy)]
struct NoCredentialRotation;

impl CredentialRotationPort for NoCredentialRotation {
    fn supports(&self, _engine: Engine) -> bool {
        false
    }

    fn swap(
        &self,
        _engine: Engine,
        _destination: &Path,
        _source: &Path,
        _timestamp: i64,
        _reason: Option<String>,
    ) -> CoreResult<neomax_core::orchestration::auth::RotationEffects> {
        Err(Error::InvalidArgument(
            "interactive handoff does not exchange credentials".into(),
        ))
    }
}
