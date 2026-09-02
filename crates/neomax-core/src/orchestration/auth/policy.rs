use crate::{Engine, Error, Result};

pub fn copy_allowed(engine: Engine) -> Result<()> {
    match engine {
        Engine::Claude | Engine::Codex => Ok(()),
        Engine::Opencode | Engine::Kimi | Engine::Grok => Err(Error::InvalidArgument(format!(
            "{engine} credential copying is not supported; profiles are isolated, use handoff"
        ))),
    }
}

pub fn handoff_required(engine: Engine) -> bool {
    matches!(engine, Engine::Opencode | Engine::Kimi | Engine::Grok)
}
