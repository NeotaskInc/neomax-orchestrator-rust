use crate::context::RuntimeContext;
use anyhow::{Context, Result, bail};
use neomax_core::accounts::{AccountControlStore, AccountInventory, RotationClaimStore};
use neomax_core::orchestration::auth::RotationPaths;
use neomax_core::orchestration::handoff::TargetPolicy;
use neomax_core::orchestration::rotation::accounts::{choose_rotation_target, swap_account_auth};
use neomax_core::providers::catalog::{self, MapEnvironment, RealFileSystem};
use neomax_core::runs::{RunLiveWorkSource, RunStore, SystemProcessProbe};
use neomax_core::usage::UsageCacheStore;
use neomax_core::{Engine, WorkerScope};

pub(crate) fn execute(
    engine: Engine,
    selector: &str,
    args: &[String],
    context: &RuntimeContext,
) -> Result<()> {
    let mut target = None;
    let mut dry_run = false;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--dry-run" => dry_run = true,
            "--json" => json = true,
            "--with" | "--from" => {
                index += 1;
                let value = args
                    .get(index)
                    .context("rotate --with requires an account number or email")?;
                if value.starts_with('-') || target.replace(value.as_str()).is_some() {
                    bail!("rotate accepts one --with account");
                }
            }
            _ => {
                bail!("usage: neomax {engine} ACCOUNT rotate [--with ACCOUNT] [--dry-run] [--json]")
            }
        }
        index += 1;
    }
    let runtime = context.provider_runtime()?;
    let environment = MapEnvironment::new(std::env::vars())
        .with_home(&context.paths.home)
        .with_current_dir(&context.cwd);
    let profiles = catalog::discover_profile_snapshots(engine, &environment, &RealFileSystem)?;
    let resolve = |selector: &str| {
        catalog::resolve_profile_selector(
            engine,
            &profiles,
            selector,
            &context.paths.home,
            &environment,
            &RealFileSystem,
        )
    };
    let source_profile = resolve(selector)?;
    let targets = target
        .map(|value| resolve(value).map(|profile| vec![profile.account.clone()]))
        .transpose()?
        .unwrap_or_default();
    let controls = AccountControlStore::new(&context.paths.cooldowns, &context.paths.paused);
    let quota = UsageCacheStore::new(&context.paths.usage);
    let runs = RunStore::new(&context.paths.runs);
    let probe = SystemProcessProbe;
    let live = RunLiveWorkSource::with_system(&runs, &probe);
    let inventory = AccountInventory::from_runtime(&runtime, &quota, &controls, &live);
    let now = chrono::DateTime::from_timestamp(context.now, 0).context("invalid clock")?;
    let accounts = inventory.routing_snapshots(&WorkerScope::only(engine), now)?;
    let source = accounts
        .iter()
        .find(|account| account.profile == source_profile.path)
        .context("rotation source is not available")?;
    let email = |path: &std::path::Path| {
        catalog::profile_email_with_environment(
            engine,
            path,
            &context.paths.home,
            &environment,
            &RealFileSystem,
        )
    };
    let source_email = email(&source.profile);
    let candidates = accounts
        .iter()
        .filter(|account| {
            source_email
                .as_ref()
                .is_none_or(|source_email| email(&account.profile).as_ref() != Some(source_email))
        })
        .filter(|account| {
            profiles
                .iter()
                .find(|profile| profile.path == account.profile)
                .is_some_and(|profile| {
                    !matches!(
                        catalog::credential_evidence(
                            profile,
                            &context.paths.home,
                            &environment,
                            &RealFileSystem,
                            context.now
                        )
                        .health,
                        catalog::CredentialHealth::LoginRequired
                            | catalog::CredentialHealth::Missing
                    )
                })
        })
        .cloned()
        .collect::<Vec<_>>();
    let chosen = choose_rotation_target(
        &candidates,
        source,
        &targets,
        now,
        &TargetPolicy::from_settings(&context.settings),
    )?;
    let target_email = email(&chosen.profile);
    let mut report = serde_json::json!({"engine":engine,"source_account":source.account,"replacement_account":chosen.account,"source_email_before":source_email,"replacement_email_before":target_email,"dry_run":dry_run,"status":"planned","session_restarted":false});
    if !dry_run {
        let claims =
            RotationClaimStore::new(&context.paths.rotation_claims, &context.paths.rotation_lock);
        if !claims.try_claim(&source.profile, now)? {
            bail!("the source account is already being rotated; retry after it finishes");
        }
        let result = (|| -> Result<()> {
            if !claims.try_claim(&chosen.profile, now)? {
                bail!("the replacement account is already being rotated; retry selection");
            }
            let operation = swap_account_auth(
                engine,
                &source.profile,
                &chosen.profile,
                RotationPaths::new(&context.paths.auth_backups, &context.paths.auth_rotations)
                    .with_usage_cache_dir(&context.paths.usage),
                &controls,
                context.now,
            );
            let release = claims.release(&chosen.profile);
            operation?;
            release?;
            Ok(())
        })();
        let release = claims.release(&source.profile);
        result?;
        release?;
        report["status"] = "credentials_swapped".into();
        report["source_email_after"] = serde_json::to_value(email(&source.profile))?;
        report["replacement_email_after"] = serde_json::to_value(email(&chosen.profile))?;
    }
    if json {
        return crate::output::json(&report);
    }
    println!(
        "{engine} {} {} {engine} {}",
        source.account,
        if dry_run {
            "would swap credentials with"
        } else {
            "swapped credentials with"
        },
        chosen.account
    );
    if let Some(email) = source_email {
        println!(
            "{email} {} account {}",
            if dry_run {
                "would move to"
            } else {
                "is now in"
            },
            chosen.account
        );
    }
    println!(
        "Session files stay in their original slots. No session was restarted; an open provider may need to reload authentication."
    );
    Ok(())
}
