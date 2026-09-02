use crate::{Engine, WorkerScope};

pub fn cross_provider_order(engine: Engine, scope: &WorkerScope) -> Vec<Engine> {
    let order: &[Engine] = match engine {
        Engine::Claude => &[Engine::Opencode, Engine::Grok, Engine::Kimi, Engine::Codex],
        Engine::Codex => &[Engine::Opencode, Engine::Grok, Engine::Kimi, Engine::Claude],
        Engine::Opencode => &[Engine::Grok, Engine::Kimi, Engine::Codex, Engine::Claude],
        Engine::Kimi => &[
            Engine::Opencode,
            Engine::Grok,
            Engine::Codex,
            Engine::Claude,
        ],
        Engine::Grok => &[
            Engine::Opencode,
            Engine::Kimi,
            Engine::Codex,
            Engine::Claude,
        ],
    };
    order
        .iter()
        .copied()
        .filter(|engine| scope.contains(*engine))
        .collect()
}
