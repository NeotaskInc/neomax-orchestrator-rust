use std::collections::BTreeMap;

use crate::providers::catalog;
use crate::{Engine, Result};

// Compatibility facade. New code should import model policy from providers::catalog.
pub use catalog::{
    CLAUDE_DEFAULT_MODEL, CLAUDE_OPUS_MODEL, CLAUDE_OPUS_MODEL_1M, CODEX_DEFAULT_MODEL,
    CODEX_SERVICE_TIER, GROK_DEFAULT_MODEL, KIMI_DEFAULT_MODEL, OPENCODE_DEFAULT_MODEL,
};

pub fn resolve_model(
    engine: Engine,
    explicit: Option<&str>,
    environment: &BTreeMap<String, String>,
) -> Result<String> {
    let environment = catalog::MapEnvironment::new(environment.clone());
    catalog::resolve_model(engine, explicit, &environment).map(|model| model.id)
}

pub fn codex_model_tier(model: &str) -> Option<&'static str> {
    match model {
        "gpt-5.6-sol" => Some("sol"),
        "gpt-5.6-terra" => Some("terra"),
        "gpt-5.6-luna" => Some("luna"),
        _ => None,
    }
}
pub fn default_model(engine: Engine) -> &'static str {
    catalog::default_model_id(engine)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_defaults_and_provider_aliases_compatible() {
        let environment = BTreeMap::new();
        assert_eq!(
            resolve_model(Engine::Claude, None, &environment).unwrap(),
            CLAUDE_DEFAULT_MODEL
        );
        assert_eq!(
            resolve_model(Engine::Codex, Some("TERRA"), &environment).unwrap(),
            "gpt-5.6-terra"
        );
        assert_eq!(
            resolve_model(Engine::Kimi, Some("2.7"), &environment).unwrap(),
            "kimi-code/kimi-for-coding"
        );
        assert_ne!(CLAUDE_DEFAULT_MODEL, CLAUDE_OPUS_MODEL_1M);
    }

    #[test]
    fn preserves_the_legacy_claude_default_override() {
        let environment =
            BTreeMap::from([("NEOMAX_DEFAULT_MODEL".into(), "claude-sonnet-local".into())]);
        assert_eq!(
            resolve_model(Engine::Claude, None, &environment).unwrap(),
            "claude-sonnet-local[1m]"
        );
    }

    #[test]
    fn passes_supported_local_models_through() {
        let environment = BTreeMap::new();
        assert_eq!(
            resolve_model(Engine::Claude, Some("custom-local-claude"), &environment).unwrap(),
            "custom-local-claude"
        );
        assert_eq!(
            resolve_model(Engine::Opencode, Some("opencode/big-pickle"), &environment).unwrap(),
            "opencode/big-pickle"
        );
        assert!(resolve_model(Engine::Opencode, Some("big-pickle"), &environment).is_err());
    }
}
