use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Utc};
use neomax_core::accounts::{AccountSnapshot, QuotaSupport, quota_support, rotation_advice};
use neomax_core::config::Engine;
use neomax_core::providers::ProviderProfile;
use neomax_core::providers::catalog::{AuthMethod, Environment, ProfileEligibility};
use neomax_core::runs::{RunRecord, RunStatus, effective_status};
use neomax_core::sessions::SessionRecord;
use neomax_core::usage::UsageCacheStore;
use serde_json::Value;

use super::capabilities::capabilities_for;
use super::identity::identity_for;
use super::state::ControlState;
use super::telemetry::{is_live_main, is_working_subagent, telemetry_for};
use crate::model::{EngineCapabilitiesView, ProfileEligibilityView};

pub(crate) struct AccountContext<'a> {
    pub(crate) home: &'a Path,
    pub(crate) environment: &'a dyn Environment,
    pub(crate) controls: &'a ControlState,
    pub(crate) usage: &'a UsageCacheStore,
    pub(crate) runs: &'a [RunRecord],
    pub(crate) sessions: &'a [SessionRecord],
    pub(crate) probe: &'a neomax_core::runs::SystemProcessProbe,
    pub(crate) now: DateTime<Utc>,
    pub(crate) session_window_days: u32,
}

pub(crate) struct AccountProfile {
    pub(crate) account: String,
    pub(crate) path: PathBuf,
    pub(crate) reserved: bool,
    pub(crate) rotate_advised: bool,
    pub(crate) authenticated: bool,
    pub(crate) worker_eligible: bool,
    pub(crate) eligibility: ProfileEligibilityView,
    pub(crate) email: Option<String>,
    pub(crate) plan: Option<String>,
    pub(crate) display_name: Option<String>,
    pub(crate) auth_method: Option<String>,
    pub(crate) live: u32,
    pub(crate) workers: u32,
    pub(crate) mains: u32,
    pub(crate) subagents: u32,
    pub(crate) cooldown_until: i64,
    pub(crate) paused: bool,
    pub(crate) token_expired: bool,
    pub(crate) usage: Option<Value>,
    pub(crate) telemetry: Option<Value>,
    pub(crate) capabilities: EngineCapabilitiesView,
}

pub(crate) fn inspect_profile(
    profile: &ProviderProfile,
    engine: Engine,
    eligibility: Option<ProfileEligibility>,
    auth_methods: Option<&[AuthMethod]>,
    binary_available: bool,
    context: &AccountContext<'_>,
) -> Result<AccountProfile> {
    let auth_method = (engine == Engine::Kimi)
        .then(|| {
            auth_methods.and_then(|methods| {
                methods.iter().find_map(|method| match method {
                    AuthMethod::OAuth => Some("OAuth".to_string()),
                    AuthMethod::ApiKey => Some("API key".to_string()),
                    AuthMethod::Device | AuthMethod::LocalCredential => None,
                })
            })
        })
        .flatten();
    let detected_authenticated = eligibility
        .map(|value| value.authenticated)
        .unwrap_or_else(|| auth_method.is_some());
    let eligibility = eligibility.unwrap_or(ProfileEligibility {
        credential_present: detected_authenticated,
        authenticated: detected_authenticated,
        worker_eligible: detected_authenticated,
        orchestrator_eligible: detected_authenticated,
        rotation_eligible: false,
        managed_pool_eligible: detected_authenticated,
    });
    let eligibility_view = ProfileEligibilityView {
        credential_present: eligibility.credential_present,
        authenticated: eligibility.authenticated,
        worker_eligible: eligibility.worker_eligible,
        orchestrator_eligible: eligibility.orchestrator_eligible,
        rotation_eligible: eligibility.rotation_eligible,
        managed_pool_eligible: eligibility.managed_pool_eligible,
    };
    let worker_eligible = eligibility_view.worker_eligible && binary_available;
    let mut eligibility_view = eligibility_view;
    eligibility_view.worker_eligible = worker_eligible;
    eligibility_view.orchestrator_eligible &= binary_available;
    let authenticated = eligibility_view.authenticated;
    let mut snapshot = AccountSnapshot {
        engine,
        account: profile.account.clone(),
        profile: profile.path.clone(),
        binary_available,
        authenticated,
        rotation_eligible: eligibility_view.rotation_eligible,
        paused: context.controls.is_paused(&profile.path),
        reserved: profile.reserved,
        live_workers: 0,
        five_hour_percent: None,
        weekly_percent: None,
        cooldown_until: context
            .controls
            .cooldown_until(&profile.path, context.now.timestamp() as f64)
            .and_then(|seconds| DateTime::from_timestamp(seconds as i64, 0)),
        five_hour_reset_at: None,
        weekly_reset_at: None,
    };
    let cache = context.usage.load(engine, &profile.path);
    context.usage.hydrate(&mut snapshot, context.now);

    let workers = context
        .runs
        .iter()
        .filter(|run| run.profile == profile.path)
        .filter(|run| {
            matches!(
                effective_status(run, context.probe),
                RunStatus::Running | RunStatus::Orphaned
            )
        })
        .count() as u32;
    snapshot.live_workers = workers;
    let account_sessions = context
        .sessions
        .iter()
        .filter(|row| row.engine == engine && row.account == profile.account);
    let mains = account_sessions
        .clone()
        .filter(|row| is_live_main(row, context.now.timestamp()))
        .count() as u32;
    let subagents = account_sessions
        .filter(|row| is_working_subagent(row, context.now.timestamp()))
        .count() as u32;
    let advice = rotation_advice(
        engine,
        snapshot.five_hour_at(context.now),
        snapshot.weekly_at(context.now),
    );
    let cooldown_until = snapshot
        .cooldown_until
        .map(|value| value.timestamp())
        .unwrap_or_default();
    let usage = matches!(quota_support(engine), QuotaSupport::Numeric)
        .then(|| cache.as_ref())
        .flatten()
        .and_then(|value| serde_json::to_value(value).ok());
    let telemetry = telemetry_for(
        engine,
        &profile.account,
        context.sessions,
        context.session_window_days,
    );
    let token_expired = cache.as_ref().is_some_and(|value| value.expired);
    let live = workers.saturating_add(mains);
    let (email, plan, display_name) = identity_for(engine, &profile.path, context.home);
    let capabilities = capabilities_for(
        engine,
        context.home,
        &usage,
        telemetry.as_ref().unwrap_or(&Value::Null),
        context.environment,
    );

    Ok(AccountProfile {
        account: profile.account.clone(),
        path: profile.path.clone(),
        reserved: profile.reserved,
        rotate_advised: advice.rotate,
        authenticated,
        worker_eligible,
        eligibility: eligibility_view,
        email,
        plan,
        display_name,
        auth_method,
        live,
        workers,
        mains,
        subagents,
        cooldown_until,
        paused: snapshot.paused,
        token_expired,
        usage,
        telemetry,
        capabilities,
    })
}
