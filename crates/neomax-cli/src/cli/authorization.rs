use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use neomax_core::agent_tools::{
    CommandClass, ManifestStore, NEOMAX_BIN_ENV, NEOMAX_TOOL_DEPTH_ENV, NEOMAX_TOOL_MANIFEST_ENV,
    NEOMAX_TOOL_MAX_DEPTH_ENV, NEOMAX_TOOL_POLICY_ENV, RecursionGuard, ToolPolicy,
    resolve_agent_command,
};

/// Authorize calls made through the canonical tool environment.
///
/// Human shell invocations do not carry any of these variables and retain the
/// normal CLI behavior. Once an agent marker is present, every part of the
/// contract is required and the private canonical manifest is checked before
/// the command is dispatched.
pub fn authorize_agent_invocation(args: &[String]) -> Result<()> {
    let has_agent_marker = [
        NEOMAX_BIN_ENV,
        NEOMAX_TOOL_MANIFEST_ENV,
        NEOMAX_TOOL_POLICY_ENV,
        NEOMAX_TOOL_DEPTH_ENV,
        NEOMAX_TOOL_MAX_DEPTH_ENV,
        "NEOMAX_ROLE",
        "NEOMAX_WORKER",
        "NEOMAX_ORCHESTRATOR",
    ]
    .into_iter()
    .any(|name| env::var_os(name).is_some());
    if !has_agent_marker {
        return Ok(());
    }

    required_absolute_file(NEOMAX_BIN_ENV)?;
    let manifest_path = required_absolute_path(NEOMAX_TOOL_MANIFEST_ENV)?;
    let policy_name = env::var(NEOMAX_TOOL_POLICY_ENV)
        .with_context(|| format!("{NEOMAX_TOOL_POLICY_ENV} is required for agent calls"))?;
    let policy = ToolPolicy::from_name(&policy_name)
        .map_err(|error| anyhow::anyhow!("invalid {NEOMAX_TOOL_POLICY_ENV}: {error}"))?;
    validate_launch_role_marker(&policy_name, policy.is_full())?;
    let command = resolve_agent_command(args)?;
    let guard = RecursionGuard::from_environment(
        env::var(NEOMAX_TOOL_DEPTH_ENV).ok().as_deref(),
        env::var(NEOMAX_TOOL_MAX_DEPTH_ENV).ok().as_deref(),
    )
    .map_err(|error| anyhow::anyhow!("invalid agent recursion environment: {error}"))?;
    let manifest = ManifestStore::new(manifest_path)
        .read_private_canonical()
        .map_err(|error| anyhow::anyhow!("invalid {NEOMAX_TOOL_MANIFEST_ENV}: {error}"))?;
    let authorized = policy.authorize(&manifest, command).map_err(|error| {
        anyhow::anyhow!("agent tool authorization failed for {command}: {error}")
    })?;
    if authorized.command().class == CommandClass::External {
        guard.enter().map_err(|error| {
            anyhow::anyhow!(
                "agent command {command} would exceed the configured tool recursion limit: {error}"
            )
        })?;
    }

    Ok(())
}

fn validate_launch_role_marker(policy_name: &str, full_policy: bool) -> Result<()> {
    let worker = env::var("NEOMAX_WORKER").ok().as_deref() == Some("1");
    let orchestrator = env::var("NEOMAX_ORCHESTRATOR").ok().as_deref() == Some("1");
    if worker && orchestrator {
        bail!("agent launch cannot be marked as both worker and orchestrator");
    }
    if worker && policy_name != "worker" && !full_policy {
        bail!("NEOMAX_TOOL_POLICY does not match the worker launch marker");
    }
    if orchestrator && policy_name != "orchestrator" && !full_policy {
        bail!("NEOMAX_TOOL_POLICY does not match the orchestrator launch marker");
    }
    Ok(())
}

fn required_absolute_path(name: &str) -> Result<PathBuf> {
    let value = env::var_os(name).with_context(|| format!("{name} is required for agent calls"))?;
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        bail!(
            "{name} must be an absolute path for agent calls: {}",
            path.display()
        );
    }
    Ok(path)
}

fn required_absolute_file(name: &str) -> Result<PathBuf> {
    let path = required_absolute_path(name)?;
    if !path.is_file() {
        bail!(
            "{name} must identify an installed executable: {}",
            path.display()
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path)?.permissions().mode();
        if mode & 0o111 == 0 {
            bail!("{name} is not executable: {}", path.display());
        }
    }
    Ok(path)
}

#[cfg(test)]
pub(crate) fn resolved_agent_command(args: &[String]) -> neomax_core::Result<&'static str> {
    resolve_agent_command(args)
}
