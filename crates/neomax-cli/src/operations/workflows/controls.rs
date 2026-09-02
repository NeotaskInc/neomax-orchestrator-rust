use std::path::Path;
use std::str::FromStr;

use anyhow::{Result, bail};
use neomax_core::Engine;
use neomax_core::accounts::AccountControlStore;
use neomax_core::providers::catalog::ProfileSnapshot;
use serde::Serialize;
use serde_json::json;

use super::args;
use crate::context::RuntimeContext;
use crate::output;

const VALUE_FLAGS: &[&str] = &["--engine"];
const SWITCH_FLAGS: &[&str] = &["--json"];

#[derive(Debug, Clone, Serialize)]
struct PausedAccount {
    engine: String,
    account: String,
    path: String,
}

pub(super) fn set_paused(context: &RuntimeContext, args: &[String], paused: bool) -> Result<()> {
    let parsed = args::parse(args, VALUE_FLAGS, SWITCH_FLAGS)?;
    let selector = parsed.positional(0, if paused { "pause" } else { "unpause" })?;
    let engine = parsed
        .value("--engine")
        .map(Engine::from_str)
        .transpose()
        .map_err(|error| anyhow::anyhow!(error))?
        .unwrap_or(Engine::Claude);
    let profiles = profiles(context, engine)?;
    let targets = select_profiles(&profiles, selector, &context.paths.home)?;
    if targets.is_empty() {
        bail!(
            "no authenticated {engine} account matched {selector:?}; use {engine} login or choose another account"
        );
    }
    let controls = AccountControlStore::new(&context.paths.cooldowns, &context.paths.paused);
    let mut rows = Vec::with_capacity(targets.len());
    for profile in targets {
        controls.set_paused(&profile.path, paused)?;
        rows.push(PausedAccount {
            engine: engine.to_string(),
            account: profile.account.clone(),
            path: display_path(&profile.path, &context.paths.home),
        });
    }
    if parsed.has("--json") {
        return output::json(&json!({
            "paused": paused,
            "accounts": rows,
        }));
    }
    for row in rows {
        println!(
            "{} {} account {} ({})",
            if paused { "paused" } else { "unpaused" },
            row.engine,
            row.account,
            row.path
        );
    }
    Ok(())
}

pub(super) fn list(context: &RuntimeContext, args: &[String]) -> Result<()> {
    let parsed = args::parse(args, &[], SWITCH_FLAGS)?;
    let controls = AccountControlStore::new(&context.paths.cooldowns, &context.paths.paused);
    let mut rows = Vec::new();
    for engine in Engine::ALL {
        for profile in profiles(context, engine)? {
            if controls.is_paused(&profile.path)? {
                rows.push(PausedAccount {
                    engine: engine.to_string(),
                    account: profile.account,
                    path: display_path(&profile.path, &context.paths.home),
                });
            }
        }
    }
    if parsed.has("--json") {
        return output::json(&rows);
    }
    if rows.is_empty() {
        println!("no paused accounts; all discovered accounts are eligible for dispatch");
        return Ok(());
    }
    println!("paused accounts (excluded from automatic dispatch):");
    for row in rows {
        println!("  {} account {} ({})", row.engine, row.account, row.path);
    }
    Ok(())
}

fn profiles(context: &RuntimeContext, engine: Engine) -> Result<Vec<ProfileSnapshot>> {
    let runtime = context.provider_runtime()?;
    Ok(runtime
        .catalog()
        .providers
        .get(&engine)
        .map(|provider| provider.profiles.clone())
        .unwrap_or_default())
}

fn select_profiles<'a>(
    profiles: &'a [ProfileSnapshot],
    selector: &str,
    home: &Path,
) -> Result<Vec<&'a ProfileSnapshot>> {
    if selector.eq_ignore_ascii_case("all") {
        return Ok(profiles
            .iter()
            .filter(|profile| profile.eligibility.authenticated)
            .collect());
    }
    let target = selector.trim();
    if target.is_empty() {
        bail!("account selector cannot be empty");
    }
    let profile = profiles
        .iter()
        .find(|profile| {
            profile.eligibility.authenticated
                && (profile.account.eq_ignore_ascii_case(target)
                    || profile.path == home.join(target))
        })
        .ok_or_else(|| {
            anyhow::anyhow!("account {target:?} was not found or is not authenticated")
        })?;
    Ok(vec![profile])
}

fn display_path(path: &Path, home: &Path) -> String {
    path.strip_prefix(home).map_or_else(
        |_| path.display().to_string(),
        |relative| relative.display().to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use neomax_core::providers::catalog::{AuthStatus, ProfileEligibility};
    use std::path::PathBuf;

    fn profile(account: &str, authenticated: bool) -> ProfileSnapshot {
        ProfileSnapshot {
            engine: Engine::Kimi,
            account: account.into(),
            path: PathBuf::from(format!("/home/.kimi-code-acct{account}")),
            reserved: false,
            auth: if authenticated {
                AuthStatus::Authenticated {
                    methods: Vec::new(),
                }
            } else {
                AuthStatus::Unauthenticated
            },
            eligibility: ProfileEligibility {
                credential_present: authenticated,
                authenticated,
                worker_eligible: authenticated,
                orchestrator_eligible: authenticated,
                rotation_eligible: false,
                managed_pool_eligible: authenticated,
            },
        }
    }

    #[test]
    fn selectors_are_limited_to_authenticated_profiles() {
        let profiles = [profile("1", true), profile("2", false)];
        assert_eq!(
            select_profiles(&profiles, "1", Path::new("/home"))
                .unwrap()
                .len(),
            1
        );
        assert!(select_profiles(&profiles, "2", Path::new("/home")).is_err());
        assert_eq!(
            select_profiles(&profiles, "all", Path::new("/home"))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn display_path_hides_the_home_prefix() {
        assert_eq!(
            display_path(Path::new("/home/.kimi-code"), Path::new("/home")),
            ".kimi-code"
        );
        assert_eq!(
            display_path(Path::new("/elsewhere"), Path::new("/home")),
            "/elsewhere"
        );
    }
}
