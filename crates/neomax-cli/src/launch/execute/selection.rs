use anyhow::Result;
use chrono::Utc;
use std::collections::{BTreeMap, BTreeSet};

use neomax_core::accounts::{AccountSelector, AccountSnapshot, SelectionPolicy, select_account};
use neomax_core::orchestration::registry::OrchestratorRecord;
use neomax_core::orchestration::selection::{
    NeomaxSelectionRequest, OrchestratorPolicy, ProviderSelectionRequest,
    choose_neomax_orchestrator, choose_provider_orchestrator, engine_priority,
};
use neomax_core::{Engine, WorkerScope};

use crate::context::RuntimeContext;
use crate::models::EffectiveModel;

use super::super::types::LaunchOptions;

pub(super) fn choose_target(
    launcher: neomax_core::orchestration::commands::Launcher,
    options: &LaunchOptions,
    context: &RuntimeContext,
    accounts: &[AccountSnapshot],
    orchestrators: &[OrchestratorRecord],
    scope: &WorkerScope,
) -> Result<AccountSnapshot> {
    let pinned = match launcher {
        neomax_core::orchestration::commands::Launcher::ProviderOrchestrator(engine)
        | neomax_core::orchestration::commands::Launcher::AccountHelper(engine) => Some(engine),
        neomax_core::orchestration::commands::Launcher::Universal => None,
    };
    let requested_engine = options.engine.or(pinned);
    if options.worker_dispatch && requested_engine.is_some_and(|engine| !scope.contains(engine)) {
        let engine = requested_engine.expect("checked above");
        return Err(anyhow::anyhow!(
            "engine {engine} is out of the inherited NEOMAX_FLEET scope ({})",
            scope.csv()
        ));
    }
    let restrict_to_scope = options.worker_dispatch || requested_engine.is_none();
    if let Some(account) = options.account.as_deref() {
        let matches = accounts
            .iter()
            .filter(|candidate| {
                candidate.account.eq_ignore_ascii_case(account)
                    && requested_engine.is_none_or(|engine| candidate.engine == engine)
                    && (!restrict_to_scope || scope.contains(candidate.engine))
            })
            .cloned()
            .collect::<Vec<_>>();
        if matches.is_empty() {
            return Err(anyhow::anyhow!("no account {account} is available"));
        }
        if options.worker_dispatch && requested_engine.is_none() {
            let environment_priority = std::env::var("NEOMAX_ENGINE_PRIORITY").ok();
            let priority = engine_priority(
                options
                    .priority
                    .as_deref()
                    .or(environment_priority.as_deref()),
            )?;
            let choice = choose_neomax_orchestrator(NeomaxSelectionRequest {
                accounts: &matches,
                orchestrators,
                priority: &priority,
                forced_engine: None,
                cwd: context.cwd.clone(),
                resume: false,
                dedicated: false,
                previous_engine: None,
                current_session: options.session_id.as_deref(),
                now: Utc::now(),
                policy: &OrchestratorPolicy::default(),
            })
            .ok_or_else(|| {
                anyhow::anyhow!("account {account} has no eligible authenticated provider")
            })?;
            return matches
                .into_iter()
                .find(|candidate| {
                    candidate.engine == choice.engine && candidate.profile == choice.profile
                })
                .ok_or_else(|| anyhow::anyhow!("selected account {account} disappeared"));
        }
        let candidate = matches.first().expect("account matches were checked above");
        if !candidate.binary_available {
            return Err(anyhow::anyhow!(
                "account {account} provider executable is unavailable"
            ));
        }
        if options.resume {
            if !candidate.authenticated {
                return Err(anyhow::anyhow!(
                    "account {account} is no longer authenticated for resume"
                ));
            }
            if candidate.paused {
                return Err(anyhow::anyhow!(
                    "account {account} is paused and cannot resume this session"
                ));
            }
            return Ok(candidate.clone());
        }
        let explicit_orchestrator = matches!(
            launcher,
            neomax_core::orchestration::commands::Launcher::Universal
                | neomax_core::orchestration::commands::Launcher::ProviderOrchestrator(_)
        ) && options.dedicated
            && candidate.reserved
            && candidate.account.eq_ignore_ascii_case("orch");
        if matches!(
            launcher,
            neomax_core::orchestration::commands::Launcher::ProviderOrchestrator(Engine::Claude)
        ) && !options.worker_dispatch
            && !options.dedicated
        {
            return Ok(candidate.clone());
        }
        if !explicit_orchestrator {
            let policy = SelectionPolicy::from_settings(&context.settings);
            select_account(
                &matches,
                &AccountSelector::Account(account.to_owned()),
                &BTreeSet::new(),
                &BTreeMap::new(),
                Utc::now(),
                &policy,
            )
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        }
        let selected = if explicit_orchestrator {
            choose_provider_orchestrator(&ProviderSelectionRequest {
                accounts: &matches,
                orchestrators,
                engine: candidate.engine,
                dedicated: true,
                current_session: options.session_id.as_deref(),
                now: Utc::now(),
                policy: &OrchestratorPolicy::default(),
            })
            .ok_or_else(|| anyhow::anyhow!("reserved account {account} is not eligible"))?
        } else {
            let policy = OrchestratorPolicy::default();
            choose_provider_orchestrator(&ProviderSelectionRequest {
                accounts: &matches,
                orchestrators,
                engine: candidate.engine,
                dedicated: false,
                current_session: options.session_id.as_deref(),
                now: Utc::now(),
                policy: &policy,
            })
            .ok_or_else(|| anyhow::anyhow!("account {account} is not eligible for new work"))?
        };
        return Ok(selected);
    }
    let now = Utc::now();
    let policy = OrchestratorPolicy::default();
    let eligible_accounts = if !restrict_to_scope {
        accounts.to_vec()
    } else {
        accounts
            .iter()
            .filter(|account| scope.contains(account.engine))
            .cloned()
            .collect::<Vec<_>>()
    };
    if let Some(engine) = requested_engine {
        return choose_provider_orchestrator(&ProviderSelectionRequest {
            accounts: &eligible_accounts,
            orchestrators,
            engine,
            dedicated: options.dedicated,
            current_session: options.session_id.as_deref(),
            now,
            policy: &policy,
        })
        .ok_or_else(|| anyhow::anyhow!("no eligible authenticated {engine} orchestrator account"));
    }
    let environment_priority = std::env::var("NEOMAX_ENGINE_PRIORITY").ok();
    let priority = engine_priority(
        options
            .priority
            .as_deref()
            .or(environment_priority.as_deref()),
    )?;
    let previous = neomax_core::orchestration::selection::SelectionStateStore::new(
        &context.paths.orchestrator_selection,
    )
    .previous_engine(&context.cwd);
    let choice = choose_neomax_orchestrator(NeomaxSelectionRequest {
        accounts: &eligible_accounts,
        orchestrators,
        priority: &priority,
        forced_engine: None,
        cwd: context.cwd.clone(),
        resume: options.resume,
        dedicated: options.dedicated,
        previous_engine: previous,
        current_session: options.session_id.as_deref(),
        now,
        policy: &policy,
    })
    .ok_or_else(|| {
        anyhow::anyhow!("no eligible authenticated orchestrator account is available")
    })?;
    eligible_accounts
        .iter()
        .find(|account| account.engine == choice.engine && account.profile == choice.profile)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("selected orchestrator profile disappeared during launch"))
}

pub(super) fn selected_model(
    context: &RuntimeContext,
    options: &LaunchOptions,
    engine: Engine,
) -> Result<EffectiveModel> {
    let overrides = context.model_overrides()?;
    let explicit = options
        .provider_models
        .get(&engine)
        .map(String::as_str)
        .or_else(|| {
            if options.opus && engine == Engine::Claude {
                Some("claude-opus-5[1m]")
            } else {
                options.model.as_deref()
            }
        });
    Ok(overrides.effective_model(engine, explicit)?)
}

pub(super) fn worker_models(
    context: &RuntimeContext,
    options: &LaunchOptions,
    orchestrator: Engine,
) -> Result<BTreeMap<Engine, String>> {
    let overrides = context.model_overrides()?;
    Engine::ALL
        .into_iter()
        .map(|engine| {
            let explicit = options
                .provider_models
                .get(&engine)
                .map(String::as_str)
                .or_else(|| {
                    if engine == orchestrator {
                        if options.opus && engine == Engine::Claude {
                            Some("claude-opus-5[1m]")
                        } else {
                            options.model.as_deref()
                        }
                    } else {
                        None
                    }
                });
            overrides
                .effective_model(engine, explicit)
                .map(|model| (engine, model.model))
                .map_err(Into::into)
        })
        .collect()
}
