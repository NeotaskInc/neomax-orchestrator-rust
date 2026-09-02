use std::collections::BTreeMap;

use anyhow::Result;
use neomax_core::Engine;
use neomax_core::accounts::AccountControlStore;
use neomax_core::providers::catalog;
use neomax_core::runs::{RunStatus, RunStore};

use crate::context::RuntimeContext;
use crate::output;

use super::super::commands::provider_auth_methods;
use super::super::profiles::{AuthPort, ManagedProfile};
use super::super::render::{ProfileView, StatusReport, display_path, helper_name};
use super::super::request::{AccountHelperRequest, AccountSelector};
use super::identity::codex_identity;

pub(super) fn execute(
    request: &AccountHelperRequest,
    context: &RuntimeContext,
    auth: &dyn AuthPort,
) -> Result<()> {
    let profiles = status_profiles(
        request.engine,
        auth.profiles(request.engine, &context.paths.home, &context.cwd)?,
        &context.paths.home,
        &context.cwd,
    )?;
    let warnings = duplicate_codex_warnings(request.engine, &profiles);
    let runs = RunStore::new(&context.paths.runs).all()?;
    let controls = AccountControlStore::new(&context.paths.cooldowns, &context.paths.paused);
    let views = profiles
        .iter()
        .map(|profile| {
            let live_workers = runs
                .iter()
                .filter(|run| {
                    run.engine == request.engine
                        && run.status == RunStatus::Running
                        && run.profile == profile.profile.path
                })
                .count() as u32;
            let cooldown_until = controls
                .cooldown_until(&profile.profile.path, context.now as f64)
                .ok()
                .flatten()
                .map(|value| value as i64);
            ProfileView {
                account: profile.account().to_owned(),
                path: display_path(&profile.profile.path, &context.paths.home),
                authenticated: profile.authenticated(),
                auth_method: profile.auth,
                identity: codex_identity(profile).map(|identity| identity.label().to_owned()),
                live_workers,
                cooldown_until,
            }
        })
        .collect::<Vec<_>>();
    let report = StatusReport {
        engine: request.engine.to_string(),
        default_model: catalog::default_model_id(request.engine).into(),
        auth_methods: provider_auth_methods(request.engine),
        profiles: views,
        warnings,
    };
    if request.json {
        return output::json(&report);
    }
    if report.profiles.is_empty() {
        println!(
            "{}: no profiles discovered; run {} login 1",
            report.engine,
            helper_name(request.engine)
        );
        return Ok(());
    }
    for profile in report.profiles {
        let auth = profile
            .auth_method
            .map_or("not authenticated".into(), |method| {
                format!("authenticated via {}", method.label())
            });
        let cooldown = profile
            .cooldown_until
            .map_or_else(String::new, |until| format!(" cooldown_until={until}"));
        println!(
            "{} account {} {} live_workers={}{}",
            report.engine, profile.account, auth, profile.live_workers, cooldown
        );
    }
    for warning in report.warnings {
        eprintln!("{warning}");
    }
    Ok(())
}

pub(super) fn duplicate_codex_warnings(engine: Engine, profiles: &[ManagedProfile]) -> Vec<String> {
    if engine != Engine::Codex {
        return Vec::new();
    }
    let mut identities = BTreeMap::<String, Vec<String>>::new();
    for profile in profiles.iter().filter(|profile| profile.authenticated()) {
        let Some(identity) = codex_identity(profile) else {
            continue;
        };
        identities
            .entry(identity.label().to_owned())
            .or_default()
            .push(profile.account().to_owned());
    }
    identities
        .into_iter()
        .filter_map(|(identity, mut accounts)| {
            (accounts.len() > 1).then(|| {
                accounts.sort();
                format!(
                    "warning: Codex accounts {} share authenticated identity {}; separate refresh-token families are required because refreshing one profile can invalidate the other",
                    accounts.join(", "),
                    identity
                )
            })
        })
        .collect()
}

pub(super) fn status_profiles(
    engine: Engine,
    profiles: Vec<ManagedProfile>,
    home: &std::path::Path,
    cwd: &std::path::Path,
) -> anyhow::Result<Vec<ManagedProfile>> {
    if engine != Engine::Codex {
        return Ok(profiles);
    }

    let mut profiles = profiles;
    for number in 1..=3 {
        let account = number.to_string();
        if profiles.iter().any(|profile| profile.account() == account) {
            continue;
        }
        let Ok(path) = super::super::profiles::profile_path_at(
            engine,
            &AccountSelector::Number(number),
            home,
            cwd,
        ) else {
            continue;
        };
        profiles.push(ManagedProfile {
            profile: neomax_core::providers::ProviderProfile {
                engine,
                account,
                path,
                reserved: false,
            },
            auth: None,
        });
    }
    profiles.sort_by_key(|profile| profile.account().parse::<u32>().unwrap_or(u32::MAX));
    Ok(profiles)
}
