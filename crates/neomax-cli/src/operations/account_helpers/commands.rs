use std::ffi::OsString;
use std::path::Path;

use anyhow::{Result, bail};
use neomax_core::Engine;
use neomax_core::providers::catalog::{self, AuthMethod};

use super::process::ProcessInvocation;
use super::profiles::{DetectedAuth, ManagedProfile};
use super::request::{AccountHelperRequest, AccountOperation, AuthMode};

pub(crate) fn login_command(
    request: &AccountHelperRequest,
    profile: &ManagedProfile,
    home: &Path,
    cwd: &Path,
) -> Result<ProcessInvocation> {
    let AccountOperation::Login {
        auth_mode,
        provider,
        region,
    } = &request.operation
    else {
        bail!("login command requires a login operation")
    };
    let mut command = base_command(request.engine, profile, home, cwd)?.interactive();
    match request.engine {
        Engine::Claude => {
            command = command.arg("auth").arg("login");
        }
        Engine::Codex => {
            command = command.arg("login");
            match auth_mode {
                AuthMode::Device => command = command.arg("--device-auth"),
                AuthMode::ApiKey => command = command.arg("--with-api-key"),
                AuthMode::AccessToken => command = command.arg("--with-access-token"),
                AuthMode::Choose | AuthMode::OAuth => {}
            }
        }
        Engine::Opencode => {
            command = command
                .arg("auth")
                .arg("login")
                .arg("--provider")
                .arg(provider.as_deref().unwrap_or("opencode-go"));
            match auth_mode {
                AuthMode::ApiKey => {
                    command = command.arg("--method").arg("api");
                }
                AuthMode::OAuth => {
                    command = command.arg("--method").arg("oauth");
                }
                AuthMode::Choose => {}
                AuthMode::Device => unreachable!("OpenCode device login is rejected by validation"),
                AuthMode::AccessToken => {
                    unreachable!("Codex access-token login is rejected by validation")
                }
            }
        }
        Engine::Kimi => match auth_mode {
            AuthMode::OAuth | AuthMode::Device => {
                command = command.arg("login");
                if let Some(region) = region {
                    command = command.arg("--region").arg(region);
                }
            }
            AuthMode::ApiKey => {}
            AuthMode::Choose => {}
            AuthMode::AccessToken => {
                unreachable!("Codex access-token login is rejected by validation")
            }
        },
        Engine::Grok => match auth_mode {
            AuthMode::OAuth => command = command.arg("login").arg("--oauth"),
            AuthMode::Device => command = command.arg("login").arg("--device-auth"),
            AuthMode::ApiKey => {
                bail!("Grok API-key login is handled by the profile credential store")
            }
            AuthMode::Choose => {
                bail!("Grok authentication selection must be resolved before command construction")
            }
            AuthMode::AccessToken => {
                bail!("Grok access-token login is not supported; use OAuth, device, or API key")
            }
        },
    }
    Ok(command.args(request.forwarded.clone()))
}

pub(crate) fn logout_command(
    request: &AccountHelperRequest,
    profile: &ManagedProfile,
    home: &Path,
    cwd: &Path,
) -> Result<ProcessInvocation> {
    let command = base_command(request.engine, profile, home, cwd)?.interactive();
    let command = match request.engine {
        Engine::Claude => command.arg("auth").arg("logout"),
        Engine::Opencode => command.arg("auth").arg("logout"),
        _ => command.arg("logout"),
    };
    Ok(command.args(request.forwarded.clone()))
}

pub(crate) fn run_command(
    request: &AccountHelperRequest,
    profile: &ManagedProfile,
    model: &str,
    home: &Path,
    cwd: &Path,
) -> Result<ProcessInvocation> {
    let mut command = base_command(request.engine, profile, home, cwd)?.interactive();
    match request.engine {
        Engine::Claude => {
            command = command
                .arg("-p")
                .arg("--dangerously-skip-permissions")
                .arg("--model")
                .arg(model);
        }
        Engine::Codex => {
            command = command
                .arg("-m")
                .arg(model)
                .arg("-c")
                .arg("service_tier=fast")
                .arg("-a")
                .arg("never")
                .arg("-s")
                .arg("danger-full-access");
        }
        Engine::Opencode => {
            let policy = neomax_core::providers::opencode_policy::content(model)
                .map_err(|error| anyhow::anyhow!(error))?;
            command = command
                .arg(cwd.as_os_str())
                .arg("--model")
                .arg(model)
                .arg("--agent")
                .arg("build")
                .arg("--auto")
                .env("OPENCODE_CONFIG_CONTENT", policy)
                .env("OPENCODE_DISABLE_AUTOUPDATE", "1")
                .env("OPENCODE_DISABLE_SHARE", "1")
                .env("OPENCODE_AUTO_SHARE", "false");
        }
        Engine::Kimi => {
            if !matches!(profile.auth, Some(DetectedAuth::ApiKey)) || request.model.is_some() {
                command = command.arg("-m").arg(model);
            }
        }
        Engine::Grok => {
            command = command.arg("--no-auto-update").arg("--model").arg(model);
        }
    }
    Ok(command.args(request.forwarded.clone()))
}

pub(crate) fn models_command(
    request: &AccountHelperRequest,
    profile: &ManagedProfile,
    home: &Path,
    cwd: &Path,
) -> Option<ProcessInvocation> {
    let command = base_command(request.engine, profile, home, cwd).ok()?;
    match request.engine {
        Engine::Claude | Engine::Codex => None,
        Engine::Opencode => Some(command.arg("models").args(request.forwarded.clone())),
        Engine::Kimi => Some(
            command
                .arg("provider")
                .arg("list")
                .arg("--json")
                .args(request.forwarded.clone()),
        ),
        Engine::Grok => Some(command.arg("models").args(request.forwarded.clone())),
    }
}

pub(crate) fn whoami_command(
    request: &AccountHelperRequest,
    profile: &ManagedProfile,
    home: &Path,
    cwd: &Path,
) -> Option<ProcessInvocation> {
    let command = base_command(request.engine, profile, home, cwd).ok()?;
    match request.engine {
        Engine::Claude => Some(command.arg("auth").args(request.forwarded.clone())),
        Engine::Codex => Some(command.arg("login").arg("status")),
        Engine::Opencode => Some(command.arg("auth").arg("list")),
        Engine::Kimi => Some(
            command
                .arg("provider")
                .arg("list")
                .args(request.forwarded.clone()),
        ),
        Engine::Grok => None,
    }
}

fn base_command(
    engine: Engine,
    profile: &ManagedProfile,
    home: &Path,
    cwd: &Path,
) -> Result<ProcessInvocation> {
    let spec = catalog::spec(engine);
    let binary =
        std::env::var_os(&spec.binary_env).unwrap_or_else(|| OsString::from(spec.default_binary));
    let mut command = ProcessInvocation::new(binary, cwd);
    for key in spec.scrub {
        command = command.remove_env(key);
    }
    let default_profile = home.join(&spec.default_profile_dir);
    if engine == Engine::Opencode {
        if profile.profile.path == default_profile {
            command = command.remove_env("XDG_DATA_HOME");
        } else {
            command = command.env("XDG_DATA_HOME", profile.profile.path.clone());
        }
    } else if profile.profile.path == default_profile && spec.default_unsets_config_env {
        command = command.remove_env(spec.config_env);
    } else {
        command = command.env(spec.config_env, profile.profile.path.clone());
    }
    Ok(command)
}

pub(crate) fn provider_supports_models(engine: Engine) -> bool {
    matches!(engine, Engine::Opencode | Engine::Kimi | Engine::Grok)
}

pub(crate) fn provider_auth_methods(engine: Engine) -> Vec<&'static str> {
    catalog::spec(engine)
        .capabilities
        .auth_methods
        .iter()
        .map(|method| match method {
            AuthMethod::OAuth => "oauth",
            AuthMethod::ApiKey => "api-key",
            AuthMethod::Device => "device",
            AuthMethod::LocalCredential => "local",
        })
        .collect()
}

#[cfg(test)]
mod tests;
