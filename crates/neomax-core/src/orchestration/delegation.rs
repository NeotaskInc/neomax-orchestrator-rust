//! Delegation defaults that belong to the orchestrating engine.
//!
//! An Antigravity (Gemini 4 Neon) orchestrator delegates to its own subagents
//! first, then to Antigravity (Neon) workers, then to Claude Code workers on
//! Opus at effort high; it never starts a Fable model.

use crate::accounts::claude_model_family;
use crate::{Engine, Error, Result, WorkerScope};

pub const ANTIGRAVITY_CLAUDE_WORKER_EFFORT: &str = "high";

pub const ANTIGRAVITY_DELEGATION: &str = "Delegation order for this Antigravity orchestrator: 1) your own subagents and parallel threads (invoke_subagent) for bounded side work such as research, edits and checks; 2) Antigravity (Gemini 4 Neon) workers through NEOMAX_BIN dispatch with engine antigravity for units that own a task, branch or pull request; 3) Claude Code workers through NEOMAX_BIN dispatch with engine claude, which run Claude Opus at effort high. Reviewers stay Claude. Never select, request or start a Fable model.";

/// The worker pool an orchestrator uses when the launch names none.
pub fn default_worker_scope(orchestrator: Engine) -> WorkerScope {
    match orchestrator {
        Engine::Antigravity => WorkerScope::of([Engine::Antigravity, Engine::Claude]),
        _ => WorkerScope::all(),
    }
}

/// Refuse a worker model the orchestrating engine never delegates to.
pub fn check_worker_model(orchestrator: Engine, worker: Engine, model: &str) -> Result<()> {
    if orchestrator == Engine::Antigravity
        && worker == Engine::Claude
        && claude_model_family(model) == Some("fable")
    {
        return Err(Error::InvalidArgument(format!(
            "an Antigravity orchestrator never starts Fable workers ({model}); use Claude Opus"
        )));
    }
    Ok(())
}

/// The effort a worker gets when its dispatch names none.
pub fn default_worker_effort(orchestrator: Engine, worker: Engine) -> Option<&'static str> {
    (orchestrator == Engine::Antigravity && worker == Engine::Claude)
        .then_some(ANTIGRAVITY_CLAUDE_WORKER_EFFORT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neon_refuses_fable_and_keeps_opus() {
        for model in ["claude-fable-5", "claude-fable-5-2[1m]", "fable"] {
            assert!(check_worker_model(Engine::Antigravity, Engine::Claude, model).is_err());
            assert!(check_worker_model(Engine::Claude, Engine::Claude, model).is_ok());
        }
        assert!(
            check_worker_model(Engine::Antigravity, Engine::Claude, "claude-opus-5-5[1m]").is_ok()
        );
    }
}
