use crate::providers::catalog::{self, MapEnvironment};
use crate::{Engine, Error, Result};

use super::types::OrchestratorRequest;

pub(super) fn validate_request(engine: Engine, request: &OrchestratorRequest) -> Result<()> {
    if request.profile.engine != engine {
        return Err(Error::InvalidArgument(format!(
            "orchestrator profile engine {} does not match {engine}",
            request.profile.engine
        )));
    }
    if request.home.as_os_str().is_empty() {
        return Err(Error::InvalidArgument(
            "orchestrator profile home cannot be empty".into(),
        ));
    }
    if request.cwd.as_os_str().is_empty() {
        return Err(Error::InvalidArgument(
            "orchestrator working directory cannot be empty".into(),
        ));
    }
    if request.environment.session.trim().is_empty() {
        return Err(Error::InvalidArgument(
            "orchestrator session cannot be empty".into(),
        ));
    }
    if request.resume
        && request
            .session_id
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        return Err(Error::InvalidArgument(
            "orchestrator resume requires a session ID".into(),
        ));
    }
    if !request.resume && request.session_id.is_some() {
        return Err(Error::InvalidArgument(
            "orchestrator session IDs are only valid when resuming".into(),
        ));
    }
    if request
        .model
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err(Error::InvalidArgument(
            "orchestrator model cannot be empty".into(),
        ));
    }
    if let Some(model) = request.model.as_deref() {
        catalog::resolve_model(engine, Some(model), &MapEnvironment::default())?;
    }
    if request
        .effort
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err(Error::InvalidArgument(
            "orchestrator effort cannot be empty".into(),
        ));
    }
    if request
        .goal
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err(Error::InvalidArgument(
            "orchestrator goal cannot be empty".into(),
        ));
    }
    if request.max_turns == Some(0) {
        return Err(Error::InvalidArgument(
            "orchestrator max turns must be greater than zero".into(),
        ));
    }
    if request.max_turns.is_some() && request.goal.is_none() {
        return Err(Error::InvalidArgument(
            "orchestrator max turns requires a goal".into(),
        ));
    }
    if request.solo && (request.goal.is_some() || request.max_turns.is_some()) {
        return Err(Error::InvalidArgument(
            "solo orchestrators do not support a goal or max turns".into(),
        ));
    }
    if matches!(engine, Engine::Opencode | Engine::Kimi | Engine::Grok)
        && (request.effort.is_some() || request.ultra)
    {
        return Err(Error::InvalidArgument(format!(
            "{engine} orchestrators do not support effort or ultra settings"
        )));
    }
    if request.profile.path.as_os_str().is_empty() {
        return Err(Error::InvalidArgument(
            "orchestrator profile path cannot be empty".into(),
        ));
    }
    if engine == Engine::Kimi {
        if request.goal.is_some() || request.max_turns.is_some() {
            return Err(Error::InvalidArgument(
                "Kimi interactive orchestrators do not support --goal or --max-turns; start an interactive session without them".into(),
            ));
        }
        if request.resume
            && request
                .initial_task
                .as_deref()
                .is_some_and(|task| !task.trim().is_empty())
        {
            return Err(Error::InvalidArgument(
                "Kimi resume cannot combine a new initial task; resume the session, then enter the follow-up task interactively".into(),
            ));
        }
        if !request.resume
            && request
                .agent_file
                .as_deref()
                .is_some_and(|agent_file| !agent_file.is_absolute())
        {
            return Err(Error::InvalidArgument(
                "Kimi orchestrator agent file must be an absolute path".into(),
            ));
        }
    }
    Ok(())
}
