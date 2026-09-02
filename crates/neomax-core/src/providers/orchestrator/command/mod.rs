use std::ffi::OsStr;

use crate::providers::ProviderCommand;
use crate::{Engine, Result};

use super::types::OrchestratorRequest;
use super::validation::validate_request;

pub(crate) mod environment;
pub(crate) mod instructions;
pub(crate) mod root;
pub(crate) mod solo;

/// Build one provider's interactive root or isolated solo command.
pub fn build(
    engine: Engine,
    binary: &OsStr,
    request: &OrchestratorRequest,
) -> Result<ProviderCommand> {
    validate_request(engine, request)?;
    let model = request
        .model
        .as_deref()
        .unwrap_or_else(|| crate::providers::catalog::default_model_id(engine));
    let command = if request.solo {
        solo::build(engine, binary, request, model)?
    } else {
        root::build(engine, binary, request, model)?
    };
    Ok(command.inherit_stdin())
}

/// Build an optional non-interactive bootstrap for providers whose interactive
/// session cannot accept an initial task on its command line.
pub fn build_bootstrap(
    engine: Engine,
    binary: &OsStr,
    request: &OrchestratorRequest,
) -> Result<Option<ProviderCommand>> {
    validate_request(engine, request)?;
    if request
        .initial_task
        .as_deref()
        .is_none_or(|task| task.trim().is_empty())
    {
        return Ok(None);
    }
    let model = request
        .model
        .as_deref()
        .unwrap_or_else(|| crate::providers::catalog::default_model_id(engine));
    let command = if request.solo {
        environment::solo_base_command(engine, binary, request, model)?
    } else {
        environment::base_command(engine, binary, request, model)?
    };
    match engine {
        Engine::Kimi if request.solo => Ok(Some(solo::kimi::bootstrap(command, request, model))),
        Engine::Kimi => Ok(Some(root::kimi::bootstrap(command, request, model))),
        Engine::Claude | Engine::Codex | Engine::Opencode | Engine::Grok => Ok(None),
    }
}
