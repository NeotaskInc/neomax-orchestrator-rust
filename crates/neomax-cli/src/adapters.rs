use neomax_core::Engine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderAdapter {
    pub engine: Engine,
    pub executable: &'static str,
    pub label: &'static str,
}

impl ProviderAdapter {
    pub const fn for_engine(engine: Engine) -> Self {
        match engine {
            Engine::Claude => Self {
                engine,
                executable: "claude",
                label: "Claude",
            },
            Engine::Codex => Self {
                engine,
                executable: "codex",
                label: "Codex",
            },
            Engine::Opencode => Self {
                engine,
                executable: "opencode",
                label: "OpenCode",
            },
            Engine::Kimi => Self {
                engine,
                executable: "kimi",
                label: "Kimi",
            },
            Engine::Grok => Self {
                engine,
                executable: "grok",
                label: "Grok",
            },
        }
    }

    pub const fn all() -> [Self; 5] {
        [
            Self::for_engine(Engine::Claude),
            Self::for_engine(Engine::Codex),
            Self::for_engine(Engine::Opencode),
            Self::for_engine(Engine::Kimi),
            Self::for_engine(Engine::Grok),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_metadata_covers_every_provider_without_executing_it() {
        let adapters = ProviderAdapter::all();
        assert_eq!(adapters.len(), Engine::ALL.len());
        assert_eq!(
            ProviderAdapter::for_engine(Engine::Opencode).executable,
            "opencode"
        );
        assert_eq!(ProviderAdapter::for_engine(Engine::Grok).label, "Grok");
    }
}
