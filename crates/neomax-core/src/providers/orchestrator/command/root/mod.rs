pub(crate) mod claude;
pub(crate) mod codex;
pub(crate) mod grok;
pub(crate) mod kimi;
pub(crate) mod opencode;

use std::ffi::OsStr;

use crate::providers::ProviderCommand;
use crate::{Engine, Result};

use super::super::types::OrchestratorRequest;
use super::environment::base_command;

pub(crate) fn build(
    engine: Engine,
    binary: &OsStr,
    request: &OrchestratorRequest,
    model: &str,
) -> Result<ProviderCommand> {
    let command = base_command(engine, binary, request, model)?;
    match engine {
        Engine::Claude => Ok(claude::build(command, request, model)),
        Engine::Codex => Ok(codex::build(command, request, model)),
        Engine::Opencode => opencode::build(command, request, model),
        Engine::Kimi => Ok(kimi::build(command, request, model)),
        Engine::Grok => Ok(grok::build(command, request, model)),
    }
}
