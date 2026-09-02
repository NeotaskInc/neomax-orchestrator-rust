use std::collections::BTreeMap;
use std::ffi::OsStr;

use crate::EffectiveSettings;
use crate::Result;
use crate::agent_tools::{
    EnvironmentInput, LaunchRole, NEOMAX_ALLOW_FULL_TOOL_POLICY_ENV, NEOMAX_TOOL_POLICY_ENV,
    RecursionGuard, ToolPolicy, build_environment, resolve_executable,
};
use crate::providers::WorkerRequest;

use super::PreparedWorkerTools;
use super::manifest;
use super::types::manifest_path;

pub fn prepare_worker_tools(
    input: super::types::WorkerToolingInput<'_>,
) -> Result<PreparedWorkerTools> {
    let super::types::WorkerToolingInput {
        paths,
        settings,
        request,
        executable_inputs,
        ambient_path,
        inherited_depth,
        inherited_max_depth,
    } = input;
    let path = manifest_path(paths);
    let _manifest = manifest::ensure_private_canonical(&path)?;
    let executable = resolve_executable(&executable_inputs)?;
    let guard = recursion_guard(settings, request, inherited_depth, inherited_max_depth)?;
    let existing_path = request
        .agent_environment
        .get("PATH")
        .map(OsStr::new)
        .or(ambient_path.as_deref());
    let install_bin = executable_inputs
        .install_bin
        .as_deref()
        .filter(|candidate| candidate.is_absolute());
    let environment = build_environment(EnvironmentInput {
        executable: &executable.path,
        manifest_path: &path,
        install_bin,
        existing_path,
        guard,
        role: request.launch_role(),
    })?;
    let policy = resolve_policy(settings, request)?;
    let mut variables = BTreeMap::new();
    environment.extend_into(&mut variables);
    variables.insert(NEOMAX_TOOL_POLICY_ENV.into(), policy.as_name().into());
    if policy.is_full() {
        variables.insert(NEOMAX_ALLOW_FULL_TOOL_POLICY_ENV.into(), "1".into());
    }
    Ok(PreparedWorkerTools::new(
        request.launch_role(),
        policy,
        variables,
    ))
}

fn resolve_policy(settings: &EffectiveSettings, request: &WorkerRequest) -> Result<ToolPolicy> {
    let mut configured = settings.agent_environment();
    configured.extend(request.agent_environment.clone());
    resolve_policy_from_sources(
        &configured,
        std::env::var(NEOMAX_TOOL_POLICY_ENV).ok().as_deref(),
        std::env::var(NEOMAX_ALLOW_FULL_TOOL_POLICY_ENV)
            .ok()
            .as_deref(),
        request.launch_role(),
    )
}

fn resolve_policy_from_sources(
    configured: &BTreeMap<String, String>,
    ambient_policy: Option<&str>,
    ambient_allow_full: Option<&str>,
    role: LaunchRole,
) -> Result<ToolPolicy> {
    if configured.contains_key(NEOMAX_TOOL_POLICY_ENV) {
        let mut values = configured.clone();
        if !values.contains_key(NEOMAX_ALLOW_FULL_TOOL_POLICY_ENV) {
            if let Some(value) = ambient_allow_full {
                values.insert(NEOMAX_ALLOW_FULL_TOOL_POLICY_ENV.into(), value.into());
            }
        }
        return ToolPolicy::from_environment(&values, role);
    }

    match ambient_policy {
        Some("full") => {
            let mut values = BTreeMap::from([(NEOMAX_TOOL_POLICY_ENV.into(), "full".into())]);
            if let Some(value) = ambient_allow_full {
                values.insert(NEOMAX_ALLOW_FULL_TOOL_POLICY_ENV.into(), value.into());
            }
            ToolPolicy::from_environment(&values, role)
        }
        Some("worker" | "orchestrator") | None => Ok(ToolPolicy::for_role(role)),
        Some(other) => ToolPolicy::from_name_with_full(other, false),
    }
}

#[cfg(test)]
pub(crate) fn resolve_policy_for_test(
    configured: &BTreeMap<String, String>,
    ambient_policy: Option<&str>,
    ambient_allow_full: Option<&str>,
    role: LaunchRole,
) -> Result<ToolPolicy> {
    resolve_policy_from_sources(configured, ambient_policy, ambient_allow_full, role)
}

fn recursion_guard(
    settings: &EffectiveSettings,
    request: &WorkerRequest,
    inherited_depth: Option<String>,
    inherited_max_depth: Option<String>,
) -> Result<RecursionGuard> {
    let mut values = settings.agent_environment();
    values.extend(request.agent_environment.clone());
    let depth = configured_value(&values, crate::agent_tools::NEOMAX_TOOL_DEPTH_ENV);
    let max_depth = configured_value(&values, crate::agent_tools::NEOMAX_TOOL_MAX_DEPTH_ENV);
    let depth = depth.or(inherited_depth);
    let max_depth = max_depth.or(inherited_max_depth);
    RecursionGuard::from_environment(depth.as_deref(), max_depth.as_deref())
}

fn configured_value(values: &BTreeMap<String, String>, key: &str) -> Option<String> {
    values.get(key).cloned()
}
