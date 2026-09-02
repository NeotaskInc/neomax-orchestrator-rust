use std::collections::BTreeMap;
use std::env;
use std::path::Path;

use crate::providers::ProviderProfile;
use crate::{Engine, Error, Result};

use super::environment::{Environment, MapEnvironment, ProcessEnvironment};
use super::filesystem::RealFileSystem;
use super::profiles::discover_profile_snapshots;
use super::specs::spec;
use super::types::{ProfileSnapshot, ProviderSnapshot};

pub fn current_binary(engine: Engine) -> String {
    let provider = spec(engine);
    ProcessEnvironment
        .value(&provider.binary_env)
        .unwrap_or(provider.default_binary)
}

pub fn current_profiles(engine: Engine) -> Result<Vec<ProviderProfile>> {
    let environment = ProcessEnvironment;
    let home = environment
        .home_dir()
        .ok_or_else(|| Error::InvalidArgument("HOME is not set".into()))?;
    let values = env::vars().collect::<BTreeMap<_, _>>();
    discover_profiles(engine, &home, &values)
}

pub fn discover_profiles(
    engine: Engine,
    home: &Path,
    environment: &BTreeMap<String, String>,
) -> Result<Vec<ProviderProfile>> {
    let current_dir = env::current_dir().unwrap_or_default();
    let environment = MapEnvironment::new(environment.clone())
        .with_home(home)
        .with_current_dir(current_dir);
    let profiles = discover_profile_snapshots(engine, &environment, &RealFileSystem)?;
    Ok(profiles
        .into_iter()
        .map(|profile| ProviderProfile {
            engine: profile.engine,
            account: profile.account,
            path: profile.path,
            reserved: profile.reserved,
        })
        .collect())
}

pub fn worker_profiles(profiles: &[ProviderProfile]) -> Vec<ProviderProfile> {
    profiles
        .iter()
        .filter(|profile| !profile.reserved)
        .cloned()
        .collect()
}

pub fn provider_profiles(snapshot: &ProviderSnapshot) -> Vec<ProviderProfile> {
    snapshot.profiles.iter().map(provider_profile).collect()
}

fn provider_profile(profile: &ProfileSnapshot) -> ProviderProfile {
    ProviderProfile {
        engine: profile.engine,
        account: profile.account.clone(),
        path: profile.path.clone(),
        reserved: profile.reserved,
    }
}

pub fn profile_account_number(engine: Engine, path: &Path) -> Option<u32> {
    let provider = spec(engine);
    let name = path.file_name()?.to_str()?;
    if name == provider.default_profile_dir {
        Some(1)
    } else {
        name.strip_prefix(&provider.account_prefix)?.parse().ok()
    }
}
