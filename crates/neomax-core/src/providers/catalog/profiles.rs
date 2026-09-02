use std::fmt::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::io::path_to_utf8;
use crate::{Engine, Error, Result};

use super::environment::Environment;
use super::filesystem::FileSystem;
use super::profile_paths::{derived_child, normalize_explicit_path};
use super::specs::spec;
use super::types::{
    AuthMethod, AuthStatus, CodexAuthIdentity, GrokAuthIdentity, ProfileEligibility,
    ProfileSelector, ProfileSnapshot,
};

/// Resolve an account selector using the same roots used by catalog discovery.
/// The caller owns the environment so tests never consult the process home.
pub fn resolve_profile_path(
    engine: Engine,
    selector: ProfileSelector,
    environment: &dyn Environment,
) -> Result<PathBuf> {
    let provider = spec(engine);
    let home = environment
        .home_dir()
        .ok_or_else(|| Error::InvalidArgument("HOME or USERPROFILE is not set".into()))?;
    match selector {
        ProfileSelector::Orchestrator => orchestrator_path(engine, &home, environment),
        ProfileSelector::Number(number) => {
            if number == 0 {
                return Err(Error::InvalidArgument(
                    "account number must be greater than zero".into(),
                ));
            }
            if let Some(paths) = configured_profile_paths(engine, environment) {
                let paths = paths?;
                let index = usize::try_from(number - 1).map_err(|_| {
                    Error::InvalidArgument(format!("account number {number} is too large"))
                })?;
                if let Some(path) = paths.iter().find(|path| {
                    account_number(path, &provider.account_prefix) == Some(number)
                        || (number == 1
                            && path.as_path() == home.join(&provider.default_profile_dir).as_path())
                }) {
                    return Ok(path.clone());
                }
                if let Some(path) = paths.get(index) {
                    if account_number(path, &provider.account_prefix).is_none() {
                        return Ok(path.clone());
                    }
                }
                let root = paths
                    .first()
                    .map(|path| profile_root(path, &provider))
                    .unwrap_or(&home);
                return derived_child(
                    root,
                    &format!("{}{}", provider.account_prefix, number),
                    "account",
                );
            }
            if number == 1 {
                return Ok(home.join(&provider.default_profile_dir));
            }
            derived_child(
                &home,
                &format!("{}{}", provider.account_prefix, number),
                "account",
            )
        }
    }
}

pub fn discover_profile_snapshots(
    engine: Engine,
    environment: &dyn Environment,
    filesystem: &dyn FileSystem,
) -> Result<Vec<ProfileSnapshot>> {
    let provider = spec(engine);
    let Some(home) = environment.home_dir() else {
        return Ok(Vec::new());
    };
    let mut paths = if let Some(paths) = configured_profile_paths(engine, environment) {
        let mut paths = paths?;
        if let Some(root) = paths
            .first()
            .map(|path| profile_root(path, &provider).to_path_buf())
        {
            let mut extras = filesystem
                .children(&root)?
                .into_iter()
                .filter(|path| account_number(path, &provider.account_prefix).is_some())
                .collect::<Vec<_>>();
            extras.sort_by_key(|path| account_number(path, &provider.account_prefix).unwrap_or(0));
            for path in extras {
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
        paths
    } else {
        let mut extras = filesystem
            .children(&home)?
            .into_iter()
            .filter(|path| account_number(path, &provider.account_prefix).is_some())
            .collect::<Vec<_>>();
        extras.sort_by_key(|path| account_number(path, &provider.account_prefix).unwrap_or(0));
        let mut paths = vec![home.join(&provider.default_profile_dir)];
        paths.extend(extras);
        paths
    };
    let orchestrator = discovered_orchestrator_profile(engine, &home, environment, filesystem)?;
    if let Some(path) = orchestrator.as_ref() {
        if !paths.contains(path) {
            paths.push(path.clone());
        }
    }
    let environment_api_key = crate::providers::process_secret::process_secret_allowed(environment)
        && api_key_in_environment(engine, environment);
    let environment_profile = paths.first().cloned();
    let reserved = environment.value("NEOMAX_ORCH_RESERVED").as_deref() == Some("1");
    Ok(paths
        .into_iter()
        .enumerate()
        .map(|(index, path)| {
            let is_orchestrator = orchestrator.as_ref() == Some(&path);
            let account = if is_orchestrator {
                "orch".into()
            } else {
                account_number(&path, &provider.account_prefix)
                    .or_else(|| (path == home.join(&provider.default_profile_dir)).then_some(1))
                    .unwrap_or_else(|| u32::try_from(index + 1).unwrap_or(u32::MAX))
                    .to_string()
            };
            let mut snapshot = inspect_profile_snapshot_with_environment(
                engine,
                account,
                path,
                is_orchestrator && reserved,
                &home,
                environment,
                filesystem,
            );
            if environment_api_key && environment_profile.as_ref() == Some(&snapshot.path) {
                add_api_key_auth(&mut snapshot);
            }
            snapshot
        })
        .collect())
}

pub fn inspect_profile_snapshot(
    engine: Engine,
    account: impl Into<String>,
    path: PathBuf,
    reserved: bool,
    home: &Path,
    filesystem: &dyn FileSystem,
) -> ProfileSnapshot {
    let (auth, credential_present) =
        super::profile_auth::detect_auth(engine, &path, home, filesystem);
    profile_snapshot(engine, account, path, reserved, auth, credential_present)
}

fn inspect_profile_snapshot_with_environment(
    engine: Engine,
    account: impl Into<String>,
    path: PathBuf,
    reserved: bool,
    home: &Path,
    environment: &dyn Environment,
    filesystem: &dyn FileSystem,
) -> ProfileSnapshot {
    let (auth, credential_present) = super::profile_auth::detect_auth_with_environment(
        engine,
        &path,
        home,
        environment,
        filesystem,
    );
    profile_snapshot(engine, account, path, reserved, auth, credential_present)
}

fn profile_snapshot(
    engine: Engine,
    account: impl Into<String>,
    path: PathBuf,
    reserved: bool,
    auth: AuthStatus,
    credential_present: bool,
) -> ProfileSnapshot {
    let authenticated = auth.is_authenticated();
    let methods = match &auth {
        AuthStatus::Authenticated { methods } => methods.as_slice(),
        _ => &[],
    };
    let rotation_eligible =
        authenticated && super::profile_auth::supports_in_place_rotation(engine, methods);
    let managed_pool_eligible = authenticated;
    let eligibility = ProfileEligibility {
        credential_present,
        authenticated,
        worker_eligible: authenticated && !reserved,
        orchestrator_eligible: authenticated,
        rotation_eligible,
        managed_pool_eligible,
    };
    ProfileSnapshot {
        engine,
        account: account.into(),
        path,
        reserved,
        auth,
        eligibility,
    }
}

pub fn codex_auth_identity(
    profile: &Path,
    filesystem: &dyn FileSystem,
) -> Option<CodexAuthIdentity> {
    super::profile_auth_codex::codex_auth_identity(profile, filesystem)
}

pub fn grok_auth_identity(profile: &Path, filesystem: &dyn FileSystem) -> Option<GrokAuthIdentity> {
    super::profile_auth_grok::grok_auth_identity(profile, filesystem)
}

pub fn credential_path(engine: Engine, profile: &Path, home: &Path) -> PathBuf {
    match engine {
        Engine::Claude => profile.join(".credentials.json"),
        Engine::Codex | Engine::Grok => profile.join("auth.json"),
        Engine::Opencode => {
            let runtime = crate::runtime::RuntimeEnvironment::process();
            if runtime.home_dir().as_deref() == Some(home) {
                return runtime.opencode_auth_path(profile);
            }
            if profile == home.join(".opencode") {
                home.join(".local/share/opencode/auth.json")
            } else {
                profile.join("opencode/auth.json")
            }
        }
        Engine::Kimi => profile.join("credentials/kimi-code.json"),
    }
}

pub fn credential_path_with_environment(
    engine: Engine,
    profile: &Path,
    home: &Path,
    environment: &dyn Environment,
) -> PathBuf {
    if engine == Engine::Opencode {
        return environment.opencode_data_dir(profile).join("auth.json");
    }
    credential_path(engine, profile, home)
}

pub fn claude_keychain_service(profile: &Path, home: &Path) -> String {
    if profile == home.join(".claude") {
        return "Claude Code-credentials".into();
    }
    let hash = Sha256::digest(profile.to_string_lossy().as_bytes());
    let mut prefix = String::with_capacity(16);
    for byte in hash.iter().take(4) {
        let _ = write!(prefix, "{byte:02x}");
    }
    format!("Claude Code-credentials-{prefix}")
}

pub fn checked_claude_keychain_service(profile: &Path, home: &Path) -> Result<String> {
    path_to_utf8("Claude profile path", profile)?;
    path_to_utf8("Claude home path", home)?;
    Ok(claude_keychain_service(profile, home))
}

pub fn worker_profile_snapshots(profiles: &[ProfileSnapshot]) -> Vec<ProfileSnapshot> {
    profiles
        .iter()
        .filter(|profile| profile.eligibility.worker_eligible)
        .cloned()
        .collect()
}

fn split_profile_paths(raw: &str, environment: &dyn Environment) -> Result<Vec<PathBuf>> {
    let values = if environment.platform().is_windows() {
        raw.split(';').map(PathBuf::from).collect::<Vec<_>>()
    } else {
        std::env::split_paths(raw).collect::<Vec<_>>()
    };
    values
        .into_iter()
        .filter(|path| !path.as_os_str().is_empty())
        .map(|path| {
            let resolved = environment.resolve_path(&path.to_string_lossy());
            normalize_explicit_path(resolved, "configured profile path")
        })
        .collect()
}

fn profile_root<'a>(path: &'a Path, provider: &super::types::ProviderSpec) -> &'a Path {
    let name = path.file_name().and_then(|value| value.to_str());
    let conventional = name.is_some_and(|name| {
        name == provider.default_profile_dir
            || name
                .strip_prefix(&provider.account_prefix)
                .is_some_and(|suffix| {
                    !suffix.is_empty() && suffix.chars().all(|character| character.is_ascii_digit())
                })
    });
    if conventional {
        path.parent().unwrap_or(path)
    } else {
        path
    }
}

fn orchestrator_path(
    engine: Engine,
    home: &Path,
    environment: &dyn Environment,
) -> Result<PathBuf> {
    let provider = spec(engine);
    environment
        .value(&provider.orchestrator_env)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|path| {
            let resolved = environment.resolve_path(&path.to_string_lossy());
            normalize_explicit_path(resolved, "configured orchestrator path")
        })
        .unwrap_or_else(|| Ok(home.join(&provider.orchestrator_dir)))
}

fn discovered_orchestrator_profile(
    engine: Engine,
    home: &Path,
    environment: &dyn Environment,
    filesystem: &dyn FileSystem,
) -> Result<Option<PathBuf>> {
    let provider = spec(engine);
    if environment
        .value(&provider.orchestrator_env)
        .is_some_and(|value| !value.is_empty())
    {
        return orchestrator_path(engine, home, environment).map(Some);
    }
    let path = orchestrator_path(engine, home, environment)?;
    Ok(filesystem.is_dir(&path).then_some(path))
}

fn configured_profile_paths(
    engine: Engine,
    environment: &dyn Environment,
) -> Option<Result<Vec<PathBuf>>> {
    let provider = spec(engine);
    environment.value(&provider.profile_env).map(|raw| {
        let paths = split_profile_paths(&raw, environment)?;
        if paths.is_empty() {
            Err(Error::InvalidArgument(format!(
                "{} must contain at least one profile path",
                provider.profile_env
            )))
        } else {
            Ok(paths)
        }
    })
}

fn api_key_in_environment(engine: Engine, environment: &dyn Environment) -> bool {
    crate::providers::process_secret::supported_environment_keys(engine)
        .iter()
        .any(|key| {
            environment
                .value(key)
                .is_some_and(|value| !value.trim().is_empty())
        })
}

fn add_api_key_auth(snapshot: &mut ProfileSnapshot) {
    match &mut snapshot.auth {
        AuthStatus::Authenticated { methods } => {
            if !methods.contains(&AuthMethod::ApiKey) {
                methods.push(AuthMethod::ApiKey);
            }
        }
        _ => {
            snapshot.auth = AuthStatus::Authenticated {
                methods: vec![AuthMethod::ApiKey],
            };
        }
    }
    snapshot.eligibility.authenticated = true;
    snapshot.eligibility.worker_eligible = !snapshot.reserved;
    snapshot.eligibility.orchestrator_eligible = true;
    let methods: &[AuthMethod] = match &snapshot.auth {
        AuthStatus::Authenticated { methods } => methods.as_slice(),
        _ => &[],
    };
    snapshot.eligibility.rotation_eligible =
        super::profile_auth::supports_in_place_rotation(snapshot.engine, methods);
    snapshot.eligibility.managed_pool_eligible = true;
}

fn account_number(path: &Path, prefix: &str) -> Option<u32> {
    path.file_name()?
        .to_str()?
        .strip_prefix(prefix)?
        .parse()
        .ok()
}
