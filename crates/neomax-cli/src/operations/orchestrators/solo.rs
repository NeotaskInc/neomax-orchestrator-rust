use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use neomax_core::Engine;
use neomax_core::WorkerScope;
use neomax_core::accounts::{AccountControlStore, AccountInventory};
use neomax_core::orchestration::commands::Launcher;
use neomax_core::orchestration::selection::{
    OrchestratorPolicy, ProviderSelectionRequest, choose_provider_orchestrator,
};
use neomax_core::providers::{ProviderProfile, ProviderRuntime, catalog};
use neomax_core::runs::{RunLiveWorkSource, RunStore, SystemProcessProbe};
use neomax_core::usage::UsageCacheStore;
use neomax_core::{
    atomic::write_bytes_atomic,
    io::{LocalFileSource, ReadLimits, read_file},
};
use serde::Serialize;

use crate::context::RuntimeContext;
use crate::output;

const CREDENTIAL_MAX_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SoloSetupResult {
    pub(crate) source_account: String,
    pub(crate) source_profile: PathBuf,
    pub(crate) destination_profile: PathBuf,
    pub(crate) credential_file: PathBuf,
}

pub(crate) fn execute(
    _launcher: Launcher,
    args: &[String],
    context: &RuntimeContext,
) -> Result<()> {
    let (account, json) = parse_args(args)?;
    let result = setup_profile(context, account.as_deref())?;
    if json {
        output::json(&result)
    } else {
        println!(
            "solo profile seeded from Claude account {} ({})",
            result.source_account,
            result.source_profile.display()
        );
        Ok(())
    }
}

pub(crate) fn setup_profile(
    context: &RuntimeContext,
    account: Option<&str>,
) -> Result<SoloSetupResult> {
    let runtime = context.provider_runtime()?;
    let source = select_source(&runtime, context, account)?;
    let credential = catalog::credential_path(Engine::Claude, &source.path, &context.paths.home);
    let limits = ReadLimits::new(CREDENTIAL_MAX_BYTES, Duration::from_secs(5))
        .map_err(|error| anyhow::anyhow!(error))?;
    let bytes = read_file(&LocalFileSource, &credential, limits).with_context(|| {
        format!(
            "could not read Claude credentials from {}",
            credential.display()
        )
    })?;
    let destination = context.paths.home.join(".claude-solo");
    neomax_core::installation::ensure_profile_workflows(
        Engine::Claude,
        &destination,
        &context.paths.home,
    )
    .map_err(anyhow::Error::from)
    .with_context(|| {
        format!(
            "could not seed Claude solo workflows in {}",
            destination.display()
        )
    })?;
    let destination_credential =
        catalog::credential_path(Engine::Claude, &destination, &context.paths.home);
    write_bytes_atomic(&destination_credential, &bytes).with_context(|| {
        format!(
            "could not write solo credentials to {}",
            destination_credential.display()
        )
    })?;
    let result = SoloSetupResult {
        source_account: source.account,
        source_profile: source.path,
        destination_profile: destination,
        credential_file: destination_credential,
    };
    Ok(result)
}

fn select_source(
    runtime: &ProviderRuntime,
    context: &RuntimeContext,
    account: Option<&str>,
) -> Result<ProviderProfile> {
    let profiles = runtime.registry().profiles_for(Engine::Claude)?;
    if let Some(account) = account {
        let profile = profiles
            .iter()
            .find(|profile| {
                profile.account.eq_ignore_ascii_case(account)
                    || (account.eq_ignore_ascii_case("orch") && profile.reserved)
                    || profile
                        .path
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(account))
            })
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Claude account {account:?} was not discovered"))?;
        if profile.reserved {
            bail!("solo-setup cannot copy from the reserved orchestrator profile");
        }
        if !runtime.registry().managed_pool_eligible(&profile) {
            bail!("Claude account {} is not authenticated", profile.account);
        }
        if !runtime.registry().rotation_eligible(&profile) {
            bail!(
                "Claude account {} does not have OAuth or device credentials available for credential copying",
                profile.account
            );
        }
        return Ok(profile);
    }
    let snapshots = account_snapshots(context, runtime)?;
    let eligible = snapshots
        .iter()
        .filter(|account| !account.reserved)
        .filter(|account| {
            profiles.iter().any(|profile| {
                profile.path == account.profile && runtime.registry().rotation_eligible(profile)
            })
        })
        .cloned()
        .collect::<Vec<_>>();
    let selected = choose_provider_orchestrator(&ProviderSelectionRequest {
        accounts: &eligible,
        orchestrators: &[],
        engine: Engine::Claude,
        dedicated: false,
        current_session: None,
        now: Utc::now(),
        policy: &OrchestratorPolicy::default(),
    })
    .ok_or_else(|| anyhow::anyhow!("no logged-in, uncooled Claude account is available"))?;
    profiles
        .into_iter()
        .find(|profile| profile.path == selected.profile)
        .ok_or_else(|| anyhow::anyhow!("selected Claude profile disappeared during solo setup"))
}

fn account_snapshots(
    context: &RuntimeContext,
    runtime: &ProviderRuntime,
) -> Result<Vec<neomax_core::accounts::AccountSnapshot>> {
    let controls = AccountControlStore::new(&context.paths.cooldowns, &context.paths.paused);
    let usage = UsageCacheStore::new(&context.paths.usage);
    let runs = RunStore::new(&context.paths.runs);
    let probe = SystemProcessProbe;
    let live_work = RunLiveWorkSource::with_system(&runs, &probe);
    let inventory = AccountInventory {
        providers: runtime.registry(),
        quota: &usage,
        controls: &controls,
        live_work: &live_work,
    };
    inventory
        .routing_snapshots(&WorkerScope::only(Engine::Claude), Utc::now())
        .map_err(Into::into)
}

fn parse_args(args: &[String]) -> Result<(Option<String>, bool)> {
    let mut account = None;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => index += 1,
            "--account" => {
                let value = args.get(index + 1).context("--account requires a value")?;
                account = Some(value.clone());
                index += 2;
            }
            value if value.starts_with("--account=") => {
                let value = value.trim_start_matches("--account=");
                if value.is_empty() {
                    bail!("--account requires a value");
                }
                account = Some(value.to_owned());
                index += 1;
            }
            value if value.starts_with('-') => bail!("unknown solo-setup option {value}"),
            value => bail!("unexpected solo-setup argument {value}"),
        }
        if args
            .get(index.saturating_sub(1))
            .is_some_and(|value| value == "--json")
        {
            json = true;
        }
    }
    Ok((account, json))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_solo_setup_options() {
        assert_eq!(
            parse_args(&["--account=2".into(), "--json".into()]).unwrap(),
            (Some("2".into()), true)
        );
    }

    #[test]
    fn rejects_unknown_solo_setup_flags() {
        assert!(parse_args(&["--provider".into()]).is_err());
    }
}
