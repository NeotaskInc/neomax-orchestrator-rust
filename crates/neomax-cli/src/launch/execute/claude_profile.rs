use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use neomax_core::Engine;
use neomax_core::accounts::AccountSnapshot;
use neomax_core::orchestration::commands::Launcher;
use neomax_core::providers::catalog::{self, Environment, MapEnvironment, ProfileSelector};

use crate::context::RuntimeContext;

use super::super::types::LaunchOptions;

pub(super) fn prepare_pinned_profile(
    launcher: Launcher,
    options: &LaunchOptions,
    context: &RuntimeContext,
    accounts: &mut Vec<AccountSnapshot>,
) -> Result<()> {
    if !matches!(launcher, Launcher::ProviderOrchestrator(Engine::Claude))
        || options.worker_dispatch
        || options.dedicated
    {
        return Ok(());
    }
    let Some(account) = options.account.as_deref() else {
        return Ok(());
    };
    let (label, path) = profile_at(account, &context.paths.home, &context.cwd)?;
    let binary_available = context
        .provider_runtime()?
        .registry()
        .binary_available(Engine::Claude);
    fs::create_dir_all(&path)
        .with_context(|| format!("could not create Claude profile {}", path.display()))?;
    neomax_core::installation::ensure_profile_workflows(Engine::Claude, &path, &context.paths.home)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("could not seed Neomax workflows in {}", path.display()))?;
    if !accounts.iter().any(|candidate| {
        candidate.engine == Engine::Claude
            && candidate.account.eq_ignore_ascii_case(&label)
            && candidate.profile == path
    }) {
        accounts.push(AccountSnapshot {
            engine: Engine::Claude,
            account: label,
            profile: path,
            binary_available,
            authenticated: false,
            rotation_eligible: false,
            paused: false,
            reserved: false,
            live_workers: 0,
            five_hour_percent: None,
            weekly_percent: None,
            cooldown_until: None,
            five_hour_reset_at: None,
            weekly_reset_at: None,
        });
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn profile(account: &str, home: &Path) -> Result<(String, PathBuf)> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| home.to_path_buf());
    profile_at(account, home, &cwd)
}

pub(super) fn profile_at(account: &str, home: &Path, cwd: &Path) -> Result<(String, PathBuf)> {
    let (label, selector) =
        if account.eq_ignore_ascii_case("orch") || account.eq_ignore_ascii_case("orchestrator") {
            ("orch".to_owned(), ProfileSelector::Orchestrator)
        } else {
            let number = account
                .parse::<u32>()
                .ok()
                .filter(|number| *number > 0)
                .ok_or_else(|| anyhow::anyhow!("cmax account must be a positive number or orch"))?;
            (number.to_string(), ProfileSelector::Number(number))
        };
    let environment = MapEnvironment::new(std::env::vars().collect::<BTreeMap<_, _>>())
        .with_home(home)
        .with_current_dir(cwd);
    let path = resolve_profile_with_environment(selector, &environment)?;
    Ok((label, path))
}

fn resolve_profile_with_environment(
    selector: ProfileSelector,
    environment: &dyn Environment,
) -> Result<PathBuf> {
    catalog::resolve_profile_path(Engine::Claude, selector, environment)
        .map_err(anyhow::Error::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canonical_expected(path: &Path) -> PathBuf {
        let mut missing = Vec::new();
        let mut existing = path.to_path_buf();
        while !existing.exists() {
            missing.push(existing.file_name().unwrap().to_os_string());
            existing = existing.parent().unwrap().to_path_buf();
        }
        let mut resolved = fs::canonicalize(existing).unwrap();
        for component in missing.iter().rev() {
            resolved.push(component);
        }
        resolved
    }

    #[test]
    fn profile_paths_match_the_claude_cli_layout() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        assert_eq!(
            profile("1", &home).unwrap(),
            ("1".into(), home.join(".claude"))
        );
        assert_eq!(
            profile("2", &home).unwrap(),
            ("2".into(), home.join(".claude-acct2"))
        );
        assert_eq!(
            profile("orchestrator", &home).unwrap(),
            ("orch".into(), home.join(".claude-orch"))
        );
        assert!(profile("default", &home).is_err());
        assert!(profile("../outside", &home).is_err());
    }

    #[test]
    fn custom_profile_and_orchestrator_roots_are_used_for_pinned_creation() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("fixture-home");
        let cwd = temp.path().join("workspace");
        let first = temp.path().join("claude-one");
        let second = temp.path().join("claude-two");
        let orchestrator = temp.path().join("claude-orch");
        let environment = MapEnvironment::new([
            (
                "NEOMAX_PROFILES".into(),
                std::env::join_paths([&first, &second])
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            ),
            (
                "NEOMAX_CLAUDE_ORCH".into(),
                orchestrator.to_string_lossy().into_owned(),
            ),
        ])
        .with_home(&home)
        .with_current_dir(&cwd);
        let first_path =
            resolve_profile_with_environment(ProfileSelector::Number(1), &environment).unwrap();
        let orch_path =
            resolve_profile_with_environment(ProfileSelector::Orchestrator, &environment).unwrap();
        assert_eq!(first_path, canonical_expected(&first));
        assert_eq!(orch_path, canonical_expected(&orchestrator));
        assert!(!home.exists());
    }
}
