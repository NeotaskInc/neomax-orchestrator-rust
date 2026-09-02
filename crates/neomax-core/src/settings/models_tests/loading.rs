use std::collections::BTreeMap;

use super::super::{
    explicit_model_overrides, model_config_path, Engine, ModelOverrides, MAX_MODEL_SETTINGS_BYTES,
};

#[test]
fn loads_configured_models_without_returning_strict_defaults() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    std::fs::write(
        model_config_path(&config),
        "codex = 'gpt-5.6-terra'\nopencode = 'local/custom/model:latest'\n",
    )
    .unwrap();
    let overrides = explicit_model_overrides(&config, &BTreeMap::new()).unwrap();
    assert_eq!(
        overrides.get(&Engine::Codex),
        Some(&"gpt-5.6-terra".to_string())
    );
    assert_eq!(
        overrides.get(&Engine::Opencode),
        Some(&"local/custom/model:latest".to_string())
    );
    assert!(!overrides.contains_key(&Engine::Claude));
}

#[test]
fn model_settings_are_bounded_and_unknown_fields_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("models.toml");
    std::fs::write(&path, "claude = 'claude/local'\n").unwrap();
    assert_eq!(
        ModelOverrides::load(&path).unwrap().get(Engine::Claude),
        Some("claude/local")
    );
    std::fs::write(
        &path,
        "claude = 'claude/local'\nfuture_model_policy = { enabled = true, limit = 7 }\n",
    )
    .unwrap();
    let settings = ModelOverrides::load(&path).unwrap();
    assert_eq!(settings.get(Engine::Claude), Some("claude/local"));
    assert_eq!(
        settings.extra["future_model_policy"]["enabled"],
        toml::Value::Boolean(true)
    );
    let saved = temp.path().join("saved-models.toml");
    settings.save(&saved).unwrap();
    let round_trip: toml::Value = toml::from_str(&std::fs::read_to_string(saved).unwrap()).unwrap();
    assert_eq!(
        round_trip["future_model_policy"]["limit"],
        toml::Value::Integer(7)
    );

    std::fs::write(&path, "opencode = 'unqualified'\n").unwrap();
    assert!(ModelOverrides::load(&path).is_err());
    let invalid = ModelOverrides {
        opencode: Some("unqualified".into()),
        ..ModelOverrides::default()
    };
    assert!(invalid.save(&path).is_err());
    let oversized = "x".repeat(MAX_MODEL_SETTINGS_BYTES + 1);
    std::fs::write(&path, oversized).unwrap();
    assert!(ModelOverrides::load(&path).is_err());
}
