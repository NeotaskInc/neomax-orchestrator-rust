use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde_json::json;
use uuid::Uuid;

use super::super::super::runtime::{DispatchError, DispatchRequest, DispatchResult};
use super::ProviderExecution;
use crate::accounts::{select_account, AccountControlStore, AccountInventory, AccountSelector};
use crate::runs::{RunLiveWorkSource, RunRecord, RunStore, SystemProcessProbe};
use crate::usage::UsageCacheStore;
use crate::{Error, Result, WorkerScope};

impl ProviderExecution {
    pub(super) fn select_initial_profile_classified(
        &self,
        request: &DispatchRequest,
    ) -> DispatchResult<PathBuf> {
        let runs = RunStore::new(self.inner.paths.runs.clone());
        let usage = UsageCacheStore::new(self.inner.paths.usage.clone());
        let controls = AccountControlStore::new(
            self.inner.paths.cooldowns.clone(),
            self.inner.paths.paused.clone(),
        );
        let probe = SystemProcessProbe;
        let live_work = RunLiveWorkSource::with_system(&runs, &probe);
        let inventory = AccountInventory {
            providers: self.inner.providers.as_ref(),
            quota: &usage,
            controls: &controls,
            live_work: &live_work,
        };
        let now = self.inner.clock.now();
        let accounts = inventory
            .routing_snapshots(&WorkerScope::only(request.engine), now)
            .map_err(classify_selection_error)?;
        let decision = select_account(
            &accounts,
            &AccountSelector::Auto,
            &BTreeSet::new(),
            &BTreeMap::new(),
            now,
            &self.inner.selection,
        )
        .map_err(classify_selection_error)?;
        Ok(decision.account.profile.clone())
    }

    #[cfg(test)]
    pub(super) fn new_run(&self, request: &DispatchRequest) -> Result<RunRecord> {
        self.new_run_classified(request)
            .map_err(DispatchError::into_error)
    }

    pub(super) fn new_run_classified(
        &self,
        request: &DispatchRequest,
    ) -> DispatchResult<RunRecord> {
        if !valid_component(&request.plan_id) {
            return Err(DispatchError::terminal(
                "worker tag must use [A-Za-z0-9._-] without path traversal",
            ));
        }
        if !valid_component(&request.run_id) {
            return Err(DispatchError::terminal(
                "worker run id must use [A-Za-z0-9._-] without path traversal",
            ));
        }
        let model_environment = model_environment(&request.environment);
        let model = resolve_scheduler_model(
            &self.inner.settings.config_path,
            request.engine,
            request.model.as_deref(),
            &model_environment,
        )
        .map_err(|error| DispatchError::terminal(error.to_string()))?;
        let _provider = self.inner.providers.get(request.engine).ok_or_else(|| {
            DispatchError::deferred(format!(
                "provider adapter is temporarily unavailable: {}",
                request.engine
            ))
        })?;
        let profile = self.select_initial_profile_classified(request)?;
        let internal_id = format!(
            "{}-attempt-{}-{}",
            request.run_id,
            request.attempt,
            Uuid::new_v4().simple()
        );
        let mut run = RunRecord::new(
            internal_id,
            request.engine,
            model,
            request.prompt.clone(),
            profile,
            request.cwd.clone(),
            self.inner.clock.now().timestamp(),
        );
        run.cwd = Some(request.cwd.clone());
        run.repo = request.repository.clone();
        run.worktree = Some(request.cwd.clone());
        run.branch = request.branch.clone();
        run.base = request.base.clone();
        run.tag = Some(request.plan_id.clone());
        run.environment = request.environment.clone();
        run.extra
            .insert("scheduler_run_id".into(), json!(request.run_id));
        run.extra
            .insert("scheduler_plan_id".into(), json!(request.plan_id));
        run.extra
            .insert("scheduler_part_id".into(), json!(request.part_id));
        run.extra
            .insert("scheduler_attempt".into(), json!(request.attempt));
        Ok(run)
    }
}

fn classify_selection_error(error: Error) -> DispatchError {
    if selection_is_temporarily_unavailable(&error) {
        DispatchError::deferred(error.to_string())
    } else {
        DispatchError::terminal(error.to_string())
    }
}

fn selection_is_temporarily_unavailable(error: &Error) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    [
        "no authenticated account",
        "no eligible account",
        "no available account",
        "no other logged-in",
        "quota headroom",
        "usage wall",
        "capacity",
        "paused",
        "cooled",
        "temporarily unavailable",
    ]
    .iter()
    .any(|marker| message.contains(marker))
}

fn model_environment(request_environment: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut environment = std::env::vars().collect::<BTreeMap<_, _>>();
    for key in [
        "NEOMAX_DEFAULT_MODEL",
        "NEOMAX_CLAUDE_MODEL",
        "NEOMAX_CODEX_MODEL",
        "NEOMAX_OPENCODE_MODEL",
        "NEOMAX_KIMI_MODEL",
        "NEOMAX_GROK_MODEL",
    ] {
        if let Some(value) = request_environment.get(key) {
            environment.insert(key.into(), value.clone());
        }
    }
    environment
}

pub(crate) fn resolve_scheduler_model(
    config_path: &std::path::Path,
    engine: crate::Engine,
    explicit: Option<&str>,
    environment: &BTreeMap<String, String>,
) -> Result<String> {
    let model_settings =
        crate::settings::ModelOverrides::load(&crate::settings::model_config_path(config_path))?;
    Ok(model_settings
        .effective_model_with_environment(engine, explicit, environment)?
        .model)
}

fn valid_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.starts_with('.')
        && !value.ends_with('.')
        && !value.contains("..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}
