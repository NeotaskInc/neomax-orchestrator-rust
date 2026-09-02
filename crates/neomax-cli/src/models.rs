use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use neomax_core::Engine;
pub use neomax_core::settings::{EffectiveModel, ModelOverrides};

pub fn config_path(settings_path: &Path) -> PathBuf {
    neomax_core::settings::model_config_path(settings_path)
}

pub fn parse_engine(value: &str) -> Result<Engine> {
    value
        .parse::<Engine>()
        .map_err(|error| anyhow::anyhow!(error))
}

pub fn validate_model(value: String) -> Result<String> {
    let model = value.trim().to_owned();
    if model.is_empty() {
        bail!("model must not be empty");
    }
    if model
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        bail!("model must not contain whitespace or control characters");
    }
    Ok(model)
}

pub fn validate_model_for_engine(engine: Engine, value: String) -> Result<String> {
    let model = validate_model(value)?;
    Ok(neomax_core::settings::resolve_explicit_model(
        engine, &model,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_arbitrary_provider_model_ids() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("models.toml");
        let mut overrides = ModelOverrides::default();
        overrides.set(Engine::Opencode, "local/custom/model:latest".into());
        overrides.save(&path).unwrap();
        let loaded = ModelOverrides::load(&path).unwrap();
        assert_eq!(
            loaded.get(Engine::Opencode),
            Some("local/custom/model:latest")
        );
        assert_eq!(
            loaded
                .effective_model(Engine::Opencode, None)
                .unwrap()
                .source,
            "config"
        );
    }

    #[test]
    fn argv_model_wins_without_changing_persistent_settings() {
        let overrides = ModelOverrides {
            kimi: Some("kimi-code/k3".into()),
            ..ModelOverrides::default()
        };
        let selected = overrides
            .effective_model(Engine::Kimi, Some("k2.7"))
            .unwrap();
        assert_eq!(selected.model, "kimi-code/kimi-for-coding");
        assert_eq!(selected.source, "argv");
    }

    #[test]
    fn engine_validation_canonicalizes_aliases_and_rejects_unqualified_opencode() {
        assert_eq!(
            validate_model_for_engine(Engine::Kimi, "k2.7".into()).unwrap(),
            "kimi-code/kimi-for-coding"
        );
        assert!(validate_model_for_engine(Engine::Opencode, "big-pickle".into()).is_err());
        assert_eq!(
            validate_model_for_engine(Engine::Grok, "local/grok-model".into()).unwrap(),
            "local/grok-model"
        );
    }
}
