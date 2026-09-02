use std::ffi::OsStr;

use crate::providers::{ProviderCommand, catalog};
use crate::{Engine, Result};

use super::super::types::{ORCHESTRATOR_INSTRUCTION_ENV, OrchestratorRequest};
use super::instructions::startup_instruction;

pub(crate) const COMMON_SECRET_ENVIRONMENT_KEYS: [&str; 17] = [
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CODEX_API_KEY",
    "NEOMAX_GROK_API_KEY",
    "GROK_API_KEY",
    "GROK_DEPLOYMENT_KEY",
    "KIMI_API_KEY",
    "KIMI_MODEL_API_KEY",
    "OPENAI_API_KEY",
    "OPENCODE_API_KEY",
    "OPENCODE_ZEN_API_KEY",
    "OPENCODE_AUTH_CONTENT",
    "XAI_API_KEY",
    "KIMI_MODEL_BASE_URL",
    "KIMI_CODE_BASE_URL",
    "KIMI_BASE_URL",
];

pub(crate) fn solo_base_command(
    engine: Engine,
    binary: &OsStr,
    request: &OrchestratorRequest,
    selected_model: &str,
) -> Result<ProviderCommand> {
    let provider = catalog::spec(engine);
    let mut command = ProviderCommand::new(binary, request.cwd.clone()).scrub_ambient_secrets();
    for key in [
        "NEOMAX_WORKER",
        "NEOMAX_ORCHESTRATOR",
        "NEOMAX_ORCH_PID",
        "NEOMAX_ORCH_RESERVED",
        "NEOMAX_ROLE",
        "NEOMAX_ENGINE",
        "NEOMAX_FLEET",
        "NEOMAX_WORKERS",
        "NEOMAX_PROJECT_ROOT",
        "NEOMAX_ORCH_SESSION",
        "NEOMAX_BIN",
        "NEOMAX_DEFAULT_MODEL",
        "NEOMAX_MODEL",
        "NEOMAX_ORCHESTRATOR_INSTRUCTION",
        "NEOMAX_ORCHESTRATOR_ORIENTATION",
        "NEOMAX_TOOL_MANIFEST",
        "NEOMAX_TOOL_POLICY",
        "NEOMAX_TOOL_INSTRUCTION",
        "NEOMAX_TOOL_DEPTH",
        "NEOMAX_TOOL_MAX_DEPTH",
    ] {
        command = command.remove_env(key);
    }
    for secret in COMMON_SECRET_ENVIRONMENT_KEYS
        .into_iter()
        .chain(provider.scrub.iter().map(String::as_str))
    {
        command = command.remove_env(secret);
    }
    command = command
        .env("NEOMAX_MODE", "solo")
        .env("NEOMAX_MODEL", selected_model)
        .env("NEOMAX_PROFILE", request.profile.path.as_os_str())
        .env(&provider.model_env, selected_model);
    let default_profile = request.home.join(&provider.default_profile_dir);
    command = command
        .inject_process_secret(engine, request.process_secret.as_ref())
        .env(
            super::super::super::process_secret::PROCESS_SECRET_BOUNDARY_ENV,
            "1",
        );
    if provider.default_unsets_config_env && request.profile.path == default_profile {
        command = command.remove_env(&provider.config_env);
    } else {
        command = command.env(&provider.config_env, request.profile.path.as_os_str());
    }
    Ok(command)
}

pub(crate) fn base_command(
    engine: Engine,
    binary: &OsStr,
    request: &OrchestratorRequest,
    selected_model: &str,
) -> Result<ProviderCommand> {
    let provider = catalog::spec(engine);
    let mut command = ProviderCommand::new(binary, request.cwd.clone()).scrub_ambient_secrets();

    for key in [
        "NEOMAX_WORKER",
        "NEOMAX_ORCHESTRATOR",
        "NEOMAX_ORCH_PID",
        "NEOMAX_ORCH_RESERVED",
    ] {
        command = command.remove_env(key);
    }

    for secret in COMMON_SECRET_ENVIRONMENT_KEYS
        .into_iter()
        .chain(provider.scrub.iter().map(String::as_str))
    {
        command = command.remove_env(secret);
    }

    for (key, value) in &request.environment.variables {
        if !is_secret_environment_key(key) {
            command = command.env(key, value);
        }
    }

    let role = engine.to_string();
    let fleet = request.environment.fleet.csv();
    command = command
        .env("NEOMAX_ROLE", &role)
        .env("NEOMAX_ENGINE", &role)
        .env("NEOMAX_MODE", "orchestrator")
        .env("NEOMAX_ORCHESTRATOR", "1")
        .env(ORCHESTRATOR_INSTRUCTION_ENV, startup_instruction(request))
        .env("NEOMAX_FLEET", &fleet)
        .env("NEOMAX_WORKERS", &fleet)
        .env("NEOMAX_PROJECT_ROOT", request.cwd.as_os_str())
        .env("NEOMAX_ORCH_SESSION", &request.environment.session)
        .env("NEOMAX_DEFAULT_MODEL", selected_model)
        .env("NEOMAX_MODEL", selected_model);

    for candidate in Engine::ALL {
        let spec = catalog::spec(candidate);
        let model = if candidate == engine {
            selected_model
        } else {
            request
                .environment
                .worker_models
                .get(&candidate)
                .map(String::as_str)
                .unwrap_or(spec.default_model.as_str())
        };
        command = command.env(&spec.model_env, model);
    }

    if let Some(pid) = request.environment.pid {
        command = command.env("NEOMAX_ORCH_PID", pid.to_string());
    }
    if request.environment.reserved || request.profile.reserved {
        command = command.env("NEOMAX_ORCH_RESERVED", "1");
    }

    let default_profile = request.home.join(&provider.default_profile_dir);
    command = command
        .inject_process_secret(engine, request.process_secret.as_ref())
        .env(
            super::super::super::process_secret::PROCESS_SECRET_BOUNDARY_ENV,
            "1",
        );
    if provider.default_unsets_config_env && request.profile.path == default_profile {
        command = command.remove_env(&provider.config_env);
    } else {
        command = command.env(&provider.config_env, request.profile.path.as_os_str());
    }

    Ok(command)
}

pub(crate) fn is_secret_environment_key(key: &str) -> bool {
    super::super::super::process_secret::is_secret_environment_key(key)
}
