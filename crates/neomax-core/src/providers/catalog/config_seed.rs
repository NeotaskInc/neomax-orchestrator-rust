use crate::{Error, Result};

pub fn codex_config_seed(source: &str) -> Result<String> {
    let mut config: toml::Table = toml::from_str(source)
        .map_err(|_| Error::InvalidArgument("Codex seed configuration is invalid TOML".into()))?;
    config
        .entry("model")
        .or_insert_with(|| super::CODEX_DEFAULT_MODEL.into());
    // An explicitly restricted seed remains restricted, including one-sided policies.
    if !config.contains_key("approval_policy")
        && !config.contains_key("sandbox_mode")
        && !config.contains_key("permissions")
        && !config.contains_key("default_permissions")
    {
        config.insert("approval_policy".into(), "never".into());
        config.insert("sandbox_mode".into(), "danger-full-access".into());
    }
    let features = config
        .entry("features")
        .or_insert_with(|| toml::Value::Table(Default::default()))
        .as_table_mut()
        .ok_or_else(|| Error::InvalidArgument("Codex features must be a table".into()))?;
    let context = features
        .entry("context_management")
        .or_insert_with(|| toml::Value::Table(Default::default()))
        .as_table_mut()
        .ok_or_else(|| Error::InvalidArgument("Codex context_management must be a table".into()))?;
    context.entry("experimental_mode").or_insert(true.into());
    toml::to_string_pretty(&config)
        .map_err(|_| Error::Message("could not encode Codex configuration".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_defaults_enable_requested_context_and_permissions() {
        let config: toml::Value = toml::from_str(&codex_config_seed("").unwrap()).unwrap();
        assert_eq!(
            config["model"].as_str(),
            Some(super::super::CODEX_DEFAULT_MODEL)
        );
        assert_eq!(config["approval_policy"].as_str(), Some("never"));
        assert_eq!(config["sandbox_mode"].as_str(), Some("danger-full-access"));
        assert_eq!(
            config["features"]["context_management"]["experimental_mode"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn explicit_settings_and_restricted_seeds_survive() {
        let source = "model = 'custom'\nsandbox_mode = 'read-only'\nmodel_context_window = 12345\n[features.context_management]\nexperimental_mode = false\n[profiles.plan]\nsandbox_mode = 'read-only'\n";
        let config: toml::Value = toml::from_str(&codex_config_seed(source).unwrap()).unwrap();
        assert_eq!(config["model"].as_str(), Some("custom"));
        assert_eq!(config["sandbox_mode"].as_str(), Some("read-only"));
        assert!(config.get("approval_policy").is_none());
        assert_eq!(
            config["features"]["context_management"]["experimental_mode"].as_bool(),
            Some(false)
        );
        assert_eq!(config["model_context_window"].as_integer(), Some(12345));
        assert_eq!(
            config["profiles"]["plan"]["sandbox_mode"].as_str(),
            Some("read-only")
        );
        assert!(codex_config_seed("not toml").is_err());
    }
}
