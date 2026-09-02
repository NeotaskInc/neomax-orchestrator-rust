use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use neomax_core::Engine;
use neomax_core::providers::ProviderProfile;
use neomax_core::providers::catalog::{
    self, AuthMethod, AuthStatus, MapEnvironment, ProfileSelector, ProfileSnapshot, RealFileSystem,
};

use super::super::request::AccountSelector;
use super::types::{DetectedAuth, ManagedProfile};

pub(super) fn discover(engine: Engine, home: &Path, cwd: &Path) -> Result<Vec<ManagedProfile>> {
    let environment = MapEnvironment::new(std::env::vars().collect::<BTreeMap<_, _>>())
        .with_home(home)
        .with_current_dir(cwd);
    let snapshots = catalog::discover_profile_snapshots(engine, &environment, &RealFileSystem)
        .with_context(|| format!("could not discover {engine} account profiles"))?;
    Ok(snapshots
        .into_iter()
        .map(|snapshot| managed_profile_with_home(snapshot, home))
        .collect())
}

pub(super) fn ensure(
    engine: Engine,
    account: &AccountSelector,
    home: &Path,
    cwd: &Path,
) -> Result<ManagedProfile> {
    let path = profile_path_at(engine, account, home, cwd)?;
    fs::create_dir_all(&path)
        .with_context(|| format!("could not create provider profile {}", path.display()))?;
    seed_config(engine, &path, home, cwd)?;
    neomax_core::installation::ensure_profile_workflows(engine, &path, home)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("could not seed Neomax workflows in {}", path.display()))?;
    let profile = inspect(engine, account.label(), path, home);
    Ok(profile)
}

#[cfg(test)]
pub(crate) fn profile_path(
    engine: Engine,
    account: &AccountSelector,
    home: &Path,
) -> Result<PathBuf> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| home.to_path_buf());
    profile_path_at(engine, account, home, &cwd)
}

pub(crate) fn profile_path_at(
    engine: Engine,
    account: &AccountSelector,
    home: &Path,
    cwd: &Path,
) -> Result<PathBuf> {
    let environment = process_environment(home, cwd);
    catalog::resolve_profile_path(engine, selector(account), &environment)
        .map_err(anyhow::Error::from)
}

pub(crate) fn profile_for(
    profiles: &[ManagedProfile],
    account: &AccountSelector,
) -> Result<ManagedProfile> {
    let requested = account.label();
    profiles
        .iter()
        .find(|profile| profile.account().eq_ignore_ascii_case(&requested))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("account {requested} does not exist; run login first"))
}

pub(super) fn inspect(
    engine: Engine,
    account: String,
    path: PathBuf,
    home: &Path,
) -> ManagedProfile {
    let snapshot =
        catalog::inspect_profile_snapshot(engine, account, path, false, home, &RealFileSystem);
    managed_profile_with_home(snapshot, home)
}

fn managed_profile_with_home(snapshot: ProfileSnapshot, _home: &Path) -> ManagedProfile {
    managed_profile(snapshot)
}

fn managed_profile(snapshot: ProfileSnapshot) -> ManagedProfile {
    let auth = detected_auth(snapshot.engine, &snapshot.auth);
    ManagedProfile {
        profile: ProviderProfile {
            engine: snapshot.engine,
            account: snapshot.account,
            path: snapshot.path,
            reserved: snapshot.reserved,
        },
        auth,
    }
}

fn detected_auth(engine: Engine, auth: &AuthStatus) -> Option<DetectedAuth> {
    let AuthStatus::Authenticated { methods } = auth else {
        return None;
    };
    methods.iter().find_map(|method| match method {
        AuthMethod::OAuth => Some(DetectedAuth::OAuth),
        AuthMethod::ApiKey => Some(DetectedAuth::ApiKey),
        AuthMethod::Device => Some(DetectedAuth::Device),
        AuthMethod::LocalCredential => (engine == Engine::Claude).then_some(DetectedAuth::Unknown),
    })
}

fn seed_config(engine: Engine, profile: &Path, home: &Path, cwd: &Path) -> Result<()> {
    let source = if engine == Engine::Codex {
        let environment = process_environment(home, cwd);
        Some(
            catalog::resolve_profile_path(engine, ProfileSelector::Number(1), &environment)?
                .join("config.toml"),
        )
    } else {
        None
    };
    let Some(source) = source else {
        return Ok(());
    };
    let target = profile.join("config.toml");
    if source != target && source.is_file() && !target.exists() {
        fs::copy(source, target).context("could not seed provider profile configuration")?;
    }
    Ok(())
}

fn selector(account: &AccountSelector) -> ProfileSelector {
    match account {
        AccountSelector::Number(number) => ProfileSelector::Number(*number),
        AccountSelector::Orchestrator => ProfileSelector::Orchestrator,
    }
}

fn process_environment(home: &Path, cwd: &Path) -> MapEnvironment {
    MapEnvironment::new(std::env::vars().collect::<BTreeMap<_, _>>())
        .with_home(home)
        .with_current_dir(cwd)
}
