use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};
use neomax_core::accounts::{AccountControlStore, AccountInventory, AccountSnapshot};
use neomax_core::providers::catalog::{AuthMethod, AuthStatus, ProfileSnapshot};
use neomax_core::providers::runtime::ProviderRuntime;
use neomax_core::runs::{RunLiveWorkSource, RunStore, SystemProcessProbe};
use neomax_core::sessions::PortalSnapshot;
use neomax_core::usage::UsageCacheStore;
use neomax_core::{Engine, WorkerScope};

use super::runs::live_child_count;
use super::safety::{account_label, model_label};
use super::types::{AccountView, ProviderView, QuotaView};
use crate::context::RuntimeContext;

pub(super) fn views(
    context: &RuntimeContext,
    runtime: &ProviderRuntime,
    runs: &RunStore,
    probe: &SystemProcessProbe,
    session_snapshot: &PortalSnapshot,
) -> Result<Vec<AccountView>> {
    let controls = AccountControlStore::new(&context.paths.cooldowns, &context.paths.paused);
    let quota = UsageCacheStore::new(&context.paths.usage);
    let live_work = RunLiveWorkSource::with_system(runs, probe);
    let inventory = AccountInventory::from_runtime(runtime, &quota, &controls, &live_work);
    let snapshots = inventory.snapshots(&WorkerScope::all(), datetime(context.now))?;
    let by_profile = snapshots
        .into_iter()
        .map(|snapshot| ((snapshot.engine, snapshot.profile.clone()), snapshot))
        .collect::<HashMap<_, _>>();
    let child_counts = runs
        .all()?
        .iter()
        .filter_map(|run| {
            let count = live_child_count(run, probe) as u32;
            (count != 0).then_some(((run.engine, run.profile.clone()), count))
        })
        .fold(
            HashMap::<(Engine, std::path::PathBuf), u32>::new(),
            |mut counts, (key, count)| {
                *counts.entry(key).or_default() += count;
                counts
            },
        );
    let mut output = Vec::new();
    for engine in Engine::ALL {
        let Some(provider) = runtime.catalog().providers.get(&engine) else {
            continue;
        };
        for profile in &provider.profiles {
            output.push(view(AccountViewInput {
                profile,
                snapshot: by_profile.get(&(engine, profile.path.clone())),
                binary_available: provider.binary.available,
                default_model: provider.spec.default_model.clone(),
                models: provider.models.clone(),
                now: context.now,
                run_subagents: child_counts
                    .get(&(engine, profile.path.clone()))
                    .copied()
                    .unwrap_or(0),
                session_snapshot,
            }));
        }
    }
    Ok(output)
}

pub(super) fn provider_views(
    runtime: &ProviderRuntime,
    accounts: Vec<AccountView>,
) -> std::collections::BTreeMap<Engine, ProviderView> {
    let by_engine = accounts.into_iter().fold(
        std::collections::BTreeMap::<Engine, Vec<AccountView>>::new(),
        |mut grouped, account| {
            grouped.entry(account.engine).or_default().push(account);
            grouped
        },
    );
    Engine::ALL
        .into_iter()
        .filter_map(|engine| {
            let provider = runtime.catalog().providers.get(&engine)?;
            Some((
                engine,
                ProviderView {
                    engine,
                    binary: super::safety::program_label(&provider.binary.program),
                    binary_available: provider.binary.available,
                    version: provider.binary.version.clone(),
                    connected: provider.connected(),
                    orchestrator_eligible: provider.eligible_for_orchestrator(),
                    worker_eligible: provider.eligible_for_workers(),
                    default_model: model_label(&provider.spec.default_model),
                    available_models: provider
                        .models
                        .iter()
                        .map(|model| model_label(model))
                        .collect(),
                    accounts: by_engine.get(&engine).cloned().unwrap_or_default(),
                },
            ))
        })
        .collect()
}

struct AccountViewInput<'a> {
    profile: &'a ProfileSnapshot,
    snapshot: Option<&'a AccountSnapshot>,
    binary_available: bool,
    default_model: String,
    models: Vec<String>,
    now: i64,
    run_subagents: u32,
    session_snapshot: &'a PortalSnapshot,
}

fn view(input: AccountViewInput<'_>) -> AccountView {
    let AccountViewInput {
        profile,
        snapshot,
        binary_available,
        default_model,
        models,
        now,
        run_subagents,
        session_snapshot,
    } = input;
    let eligibility = profile.eligibility;
    let (authenticated, auth_status, methods) = match &profile.auth {
        AuthStatus::Authenticated { methods } => (
            true,
            "authenticated".to_owned(),
            methods.iter().map(auth_method_name).collect(),
        ),
        AuthStatus::Unauthenticated => (false, "unauthenticated".to_owned(), Vec::new()),
        AuthStatus::Unknown => (false, "unknown".to_owned(), Vec::new()),
    };
    let snapshot = snapshot.cloned().unwrap_or_else(|| AccountSnapshot {
        engine: profile.engine,
        account: profile.account.clone(),
        profile: profile.path.clone(),
        binary_available,
        authenticated,
        rotation_eligible: eligibility.rotation_eligible,
        paused: false,
        reserved: profile.reserved,
        live_workers: 0,
        five_hour_percent: None,
        weekly_percent: None,
        cooldown_until: None,
        five_hour_reset_at: None,
        weekly_reset_at: None,
    });
    let mains = session_snapshot
        .sessions
        .iter()
        .filter(|session| {
            session.engine == profile.engine && session.account == profile.account && session.active
        })
        .count() as u32;
    let native_subagents = session_snapshot
        .subagents
        .iter()
        .filter(|session| {
            session.engine == profile.engine && session.account == profile.account && session.active
        })
        .count() as u32;
    let subagents = run_subagents.saturating_add(native_subagents);
    let account = account_label(&profile.account);
    let quota = QuotaView {
        five_hour_percent: snapshot.five_hour_percent,
        weekly_percent: snapshot.weekly_percent,
        five_hour_reset_at: snapshot.five_hour_reset_at.map(|value| value.timestamp()),
        weekly_reset_at: snapshot.weekly_reset_at.map(|value| value.timestamp()),
        cooldown_until: snapshot.cooldown_until.map(|value| value.timestamp()),
        hard_wall: snapshot.at_hard_wall(datetime(now)),
    };
    AccountView {
        engine: profile.engine,
        account: account.clone(),
        identity: format!("{}:{account}", profile.engine),
        role: if profile.reserved {
            "orchestrator"
        } else {
            "worker"
        }
        .to_owned(),
        auth_status,
        auth_methods: methods,
        credential_present: eligibility.credential_present,
        authenticated,
        worker_eligible: eligibility.worker_eligible && binary_available,
        orchestrator_eligible: eligibility.orchestrator_eligible && binary_available,
        rotation_eligible: eligibility.rotation_eligible,
        managed_pool_eligible: eligibility.managed_pool_eligible,
        reserved: profile.reserved,
        paused: snapshot.paused,
        live_workers: snapshot.live_workers,
        mains,
        subagents,
        live: snapshot.live_workers + mains + subagents,
        agents: snapshot.live_workers + mains + subagents,
        default_model: model_label(&default_model),
        available_models: models.iter().map(|model| model_label(model)).collect(),
        quota,
    }
}

fn datetime(value: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(value, 0)
        .unwrap_or_else(|| DateTime::from_timestamp(0, 0).expect("unix epoch is valid"))
}

fn auth_method_name(method: &AuthMethod) -> String {
    match method {
        AuthMethod::OAuth => "oauth",
        AuthMethod::ApiKey => "api_key",
        AuthMethod::Device => "device",
        AuthMethod::LocalCredential => "local_credential",
    }
    .to_owned()
}
