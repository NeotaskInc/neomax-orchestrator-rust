use serde::{Deserialize, Serialize};

use crate::{Engine, WorkerScope};

pub mod auth;
pub mod commands;
pub mod continuation;
pub mod handoff;
pub mod registry;
pub mod rotation;
pub mod selection;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mode {
    pub id: String,
    pub title: String,
    pub command: String,
    pub orchestrator: Option<Engine>,
    pub workers: String,
}

pub fn provider_mode(engine: Engine, scope: WorkerScope) -> Mode {
    let command = match engine {
        Engine::Claude => "cmax",
        Engine::Codex => "cdxmax",
        Engine::Opencode => "ocmax",
        Engine::Kimi => "kmax",
        Engine::Grok => "gmax",
    };
    Mode {
        id: format!("{}-pinned", engine.as_str()),
        title: format!("{} orchestrator", engine.as_str()),
        command: command.into(),
        orchestrator: Some(engine),
        workers: scope.csv(),
    }
}

pub fn universal_mode() -> Mode {
    Mode {
        id: "neomax".into(),
        title: "Dynamic Neomax orchestration".into(),
        command: "neomax".into(),
        orchestrator: None,
        workers: "all eligible connected providers".into(),
    }
}
