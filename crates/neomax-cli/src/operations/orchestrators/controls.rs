use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use neomax_core::Engine;
use neomax_core::accounts::AccountControlStore;
use neomax_core::io::is_rooted_but_not_absolute;
use neomax_core::orchestration::commands::Command;
use neomax_core::providers::{ProviderProfile, ProviderRuntime};
use serde::Serialize;

use crate::context::RuntimeContext;
use crate::error;
use crate::output;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ControlOptions {
    engine: Engine,
    selector: Option<String>,
    json: bool,
}

#[derive(Debug, Clone, Serialize)]
struct AccountControlResult {
    engine: Engine,
    account: String,
    profile: PathBuf,
    paused: bool,
}

#[derive(Debug, Clone, Serialize)]
struct PausedAccount {
    engine: Engine,
    account: String,
    profile: PathBuf,
}

pub(crate) fn execute(command: Command, args: &[String], context: &RuntimeContext) -> Result<()> {
    match command {
        Command::Pause => set_paused(true, args, context),
        Command::Unpause => set_paused(false, args, context),
        Command::Paused => list_paused(args, context),
        _ => bail!("unsupported account control command {command:?}"),
    }
}

fn set_paused(paused: bool, args: &[String], context: &RuntimeContext) -> Result<()> {
    let options = error::usage(parse_control_options(args, true))?;
    let selector = options.selector.as_deref().ok_or_else(|| {
        error::usage_error(anyhow::anyhow!(
            "{} requires an account selector",
            if paused { "pause" } else { "unpause" }
        ))
    })?;
    let runtime = context.provider_runtime()?;
    let profiles = runtime.registry().profiles_for(options.engine)?;
    let targets = select_profiles(
        &profiles,
        selector,
        options.engine,
        &context.paths.home,
        &runtime,
        selector.eq_ignore_ascii_case("all"),
    )?;
    if targets.is_empty() {
        bail!(
            "no {} {} account for {}",
            options.engine,
            if paused {
                "authenticated to pause"
            } else {
                "available to unpause"
            },
            selector
        );
    }
    let controls = AccountControlStore::new(&context.paths.cooldowns, &context.paths.paused);
    let mut results = Vec::with_capacity(targets.len());
    for profile in targets {
        controls.set_paused(&profile.path, paused)?;
        results.push(AccountControlResult {
            engine: options.engine,
            account: profile.account,
            profile: profile.path,
            paused,
        });
    }
    if options.json {
        return output::json(&results);
    }
    let verb = if paused { "PAUSED" } else { "UNPAUSED" };
    for result in results {
        if paused {
            println!(
                "{verb} {} account {} ({}) no tasks will route here until unpaused",
                result.engine,
                result.account,
                result.profile.display()
            );
        } else {
            println!(
                "{verb} {} account {} ({})",
                result.engine,
                result.account,
                result.profile.display()
            );
        }
    }
    Ok(())
}

fn list_paused(args: &[String], context: &RuntimeContext) -> Result<()> {
    let options = error::usage(parse_control_options(args, false))?;
    let runtime = context.provider_runtime()?;
    let controls = AccountControlStore::new(&context.paths.cooldowns, &context.paths.paused);
    let engines = if args.iter().any(|arg| arg == "--engine")
        || args.iter().any(|arg| arg.starts_with("--engine="))
    {
        vec![options.engine]
    } else {
        Engine::ALL.to_vec()
    };
    let mut rows = Vec::new();
    for engine in engines {
        for profile in runtime.registry().profiles_for(engine)? {
            if controls.is_paused(&profile.path)? {
                rows.push(PausedAccount {
                    engine,
                    account: profile.account,
                    profile: profile.path,
                });
            }
        }
    }
    if options.json {
        return output::json(&rows);
    }
    if rows.is_empty() {
        println!("no paused accounts all are eligible for dispatch");
        return Ok(());
    }
    println!("PAUSED accounts held out of auto-dispatch until unpaused:");
    for row in rows {
        println!(
            "  {} account {} ({})",
            row.engine,
            row.account,
            row.profile.display()
        );
    }
    Ok(())
}

fn parse_control_options(args: &[String], require_selector: bool) -> Result<ControlOptions> {
    let mut engine = Engine::Claude;
    let mut selector = None;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        let current = &args[index];
        if current == "--json" {
            json = true;
            index += 1;
            continue;
        }
        if current == "--engine" {
            let value = args.get(index + 1).context("--engine requires a value")?;
            engine = value.parse()?;
            index += 2;
            continue;
        }
        if let Some(value) = current.strip_prefix("--engine=") {
            if value.is_empty() {
                bail!("--engine requires a value");
            }
            engine = value.parse()?;
            index += 1;
            continue;
        }
        if current.starts_with('-') {
            bail!("unknown account control option {current}");
        }
        if selector.replace(current.clone()).is_some() {
            bail!("only one account selector is allowed");
        }
        index += 1;
    }
    if require_selector && selector.is_none() {
        bail!("account selector is required");
    }
    Ok(ControlOptions {
        engine,
        selector,
        json,
    })
}

fn select_profiles(
    profiles: &[ProviderProfile],
    selector: &str,
    engine: Engine,
    home: &Path,
    runtime: &ProviderRuntime,
    authenticated_only: bool,
) -> Result<Vec<ProviderProfile>> {
    if selector.eq_ignore_ascii_case("all") {
        return Ok(profiles
            .iter()
            .filter(|profile| {
                (!authenticated_only || runtime.registry().managed_pool_eligible(profile))
                    && !profile.reserved
            })
            .cloned()
            .collect());
    }
    let selector_path = Path::new(selector);
    if is_rooted_but_not_absolute(selector_path) {
        bail!(
            "account selector path must not be rooted without an absolute prefix: {}",
            selector_path.display()
        );
    }
    let absolute_selector = if selector_path.is_absolute() {
        selector_path.to_path_buf()
    } else {
        if is_rooted_but_not_absolute(home) {
            bail!(
                "account profile home must not be rooted without an absolute prefix: {}",
                home.display()
            );
        }
        home.join(selector_path)
    };
    let found = profiles.iter().find(|profile| {
        profile.account.eq_ignore_ascii_case(selector)
            || (selector.eq_ignore_ascii_case("orch") && profile.reserved)
            || profile.path == absolute_selector
            || profile
                .path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(selector))
    });
    let profile = found.ok_or_else(|| {
        anyhow::anyhow!(
            "cannot resolve {engine} account {selector:?}; use a discovered account number, orch, or profile path"
        )
    })?;
    Ok(vec![profile.clone()])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(account: &str, reserved: bool, home: &Path) -> ProviderProfile {
        ProviderProfile {
            engine: Engine::Claude,
            account: account.into(),
            path: home.join(format!(".claude{account}")),
            reserved,
        }
    }

    #[test]
    fn parse_control_options_requires_a_single_selector() {
        let parsed = parse_control_options(
            &["2".into(), "--engine=codex".into(), "--json".into()],
            true,
        )
        .unwrap();
        assert_eq!(parsed.engine, Engine::Codex);
        assert_eq!(parsed.selector.as_deref(), Some("2"));
        assert!(parsed.json);
        assert!(parse_control_options(&["1".into(), "2".into()], true).is_err());
    }

    #[test]
    fn select_profiles_resolves_numbers_reserved_profiles_and_all() {
        let temp = tempfile::tempdir().expect("temporary root");
        let home = temp.path().join("home");
        std::fs::create_dir_all(&home).expect("fixture home");
        let profiles = [profile("1", false, &home), profile("orch", true, &home)];
        let runtime =
            ProviderRuntime::from_catalog(neomax_core::providers::catalog::CatalogSnapshot {
                providers: std::collections::BTreeMap::new(),
            });
        assert_eq!(
            select_profiles(&profiles, "1", Engine::Claude, &home, &runtime, false,)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            select_profiles(&profiles, "orch", Engine::Claude, &home, &runtime, false,).unwrap()[0]
                .account,
            "orch"
        );
    }

    #[cfg(windows)]
    #[test]
    fn select_profiles_rejects_partial_root_selectors() {
        let profiles = [profile("1", false, Path::new(r"C:\fixture"))];
        let runtime =
            ProviderRuntime::from_catalog(neomax_core::providers::catalog::CatalogSnapshot {
                providers: std::collections::BTreeMap::new(),
            });

        for selector in [r"\fixture\.claude1", r"C:fixture\.claude1"] {
            let error = select_profiles(
                &profiles,
                selector,
                Engine::Claude,
                Path::new(r"C:\fixture"),
                &runtime,
                false,
            )
            .unwrap_err();
            assert!(error.to_string().contains("must not be rooted"));
        }
    }

    #[cfg(windows)]
    #[test]
    fn select_profiles_rejects_a_partial_root_home() {
        let profiles = [profile("1", false, Path::new(r"C:\fixture"))];
        let runtime =
            ProviderRuntime::from_catalog(neomax_core::providers::catalog::CatalogSnapshot {
                providers: std::collections::BTreeMap::new(),
            });

        let error = select_profiles(
            &profiles,
            "missing-profile",
            Engine::Claude,
            Path::new(r"\fixture"),
            &runtime,
            false,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("profile home must not be rooted")
        );
    }
}
