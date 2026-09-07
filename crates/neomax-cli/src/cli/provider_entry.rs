use anyhow::{Context, Result, bail};
use neomax_core::Engine;
use neomax_core::orchestration::commands::Launcher;
use neomax_core::providers::catalog::{
    MapEnvironment, RealFileSystem, discover_profile_snapshots, resolve_profile_entry,
    verify_profile_entry_identity,
};

use crate::{context::RuntimeContext, launch, operations, output};

pub(super) fn run(engine: Engine, args: &[String], context: &RuntimeContext) -> Result<()> {
    if args.get(1).is_some_and(|arg| arg == "rotate")
        && args.first().is_some_and(|arg| !arg.starts_with('-'))
    {
        return operations::rotate_account(engine, &args[0], &args[2..], context);
    }
    if args.first().is_some_and(|arg| {
        matches!(
            arg.as_str(),
            "login" | "logout" | "status" | "whoami" | "models"
        )
    }) {
        let selected =
            super::account_selectors::normalize(Launcher::AccountHelper(engine), args, context)?;
        let args = selected.as_deref().unwrap_or(args);
        if args.iter().any(|arg| arg == "--dry-run") {
            return launch::run(Launcher::AccountHelper(engine), args, context);
        }
        return operations::provider_operation(engine, args, context);
    }
    let launcher = Launcher::ProviderOrchestrator(engine);
    let mut normalized = args.to_vec();
    if normalized.first().is_some_and(|arg| !arg.starts_with('-')) {
        let account = normalized.remove(0);
        normalized.splice(0..0, ["--account".into(), account]);
    }
    let options = launch::LaunchOptions::parse(launcher, &normalized)?;
    launch::build_plan(launcher, options.clone(), context)?;
    if options.engine.is_some_and(|requested| requested != engine) {
        bail!("neomax {engine} cannot select another provider with --engine");
    }
    if options.worker_dispatch {
        bail!("use `neomax dispatch --engine {engine}` for guarded worker dispatch");
    }
    let environment = MapEnvironment::new(std::env::vars())
        .with_home(&context.paths.home)
        .with_current_dir(&context.cwd);
    let profiles = discover_profile_snapshots(engine, &environment, &RealFileSystem)?;
    let entry = resolve_profile_entry(
        engine,
        &profiles,
        options.account.as_deref(),
        &environment,
        &RealFileSystem,
    )?;
    let Some(entry) = entry else {
        return launch::run(launcher, &normalized, context);
    };
    if options.resume && entry.login_required {
        bail!(
            "account {} needs login before it can resume; open `neomax {engine} {}` first",
            entry.account,
            entry.account
        );
    }
    if entry.login_required && options.dry_run {
        let report = serde_json::json!({"engine":engine,"account":entry.account,"login_required":true,"will_launch_after_login":true,"dry_run":true});
        return if args.iter().any(|arg| arg == "--json") {
            output::json(&report)
        } else {
            println!(
                "{engine} account {}: login required, then launch (dry run)",
                entry.account
            );
            Ok(())
        };
    }
    let mut launch_args = Vec::new();
    let mut index = 0;
    while index < normalized.len() {
        let arg = &normalized[index];
        if arg == "--" {
            launch_args.extend_from_slice(&normalized[index..]);
            break;
        }
        if arg == "--account" {
            index += 2;
            continue;
        }
        if !arg.starts_with("--account=") {
            launch_args.push(arg.clone());
        }
        index += 1;
    }
    launch_args.splice(0..0, ["--account".into(), entry.account.clone()]);
    if entry.login_required {
        if entry.create_new {
            if let Some(parent) = entry.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(&entry.path).context("the new account slot is no longer empty; retry without changing the existing profile")?;
        }
        eprintln!(
            "Signing in to {engine} account {}. The session will open after login succeeds.",
            entry.account
        );
        operations::login_profile(engine, &entry.account, context)?;
        verify_profile_entry_identity(engine, &entry, &environment, &RealFileSystem)?;
        let fresh = discover_profile_snapshots(engine, &environment, &RealFileSystem)?;
        let canonical = std::fs::canonicalize(&entry.path)?;
        if !fresh.iter().any(|profile| {
            profile.account == entry.account
                && profile.eligibility.authenticated
                && std::fs::canonicalize(&profile.path).ok().as_ref() == Some(&canonical)
        }) {
            bail!(
                "login did not produce an authenticated {engine} profile; no session was launched"
            );
        }
        return launch::run(launcher, &launch_args, &context.after_login()?);
    }
    verify_profile_entry_identity(engine, &entry, &environment, &RealFileSystem)?;
    launch::run(launcher, &launch_args, context)
}
