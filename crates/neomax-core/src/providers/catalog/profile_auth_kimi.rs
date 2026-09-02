use std::path::Path;

use crate::Engine;
use serde_json::Value;

use super::filesystem::FileSystem;
use super::profile_auth_common::{json_file, read_utf8, unique_methods};
use super::profiles::credential_path;
use super::types::AuthMethod;

const KIMI_API_KEY_ENV_NAMES: &[&str] = &[
    "KIMI_API_KEY",
    "KIMI_MODEL_API_KEY",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GOOGLE_API_KEY",
    "VERTEXAI_API_KEY",
];

pub(super) fn kimi_auth(profile: &Path, filesystem: &dyn FileSystem) -> Vec<AuthMethod> {
    let mut methods = Vec::new();
    if let Some(Value::Object(credentials)) = json_file(
        credential_path(Engine::Kimi, profile, Path::new("")),
        filesystem,
    ) {
        if ["access_token", "refresh_token"]
            .iter()
            .any(|key| credentials.get(*key).is_some_and(json_token))
        {
            methods.push(AuthMethod::OAuth);
        }
    }
    if read_utf8(profile.join("config.toml"), filesystem)
        .and_then(|bytes| toml::from_str::<toml::Value>(&bytes).ok())
        .is_some_and(|config| kimi_config_has_api_key(&config))
    {
        methods.push(AuthMethod::ApiKey);
    }
    unique_methods(methods)
}

fn kimi_config_has_api_key(config: &toml::Value) -> bool {
    if config
        .get("api_key")
        .and_then(toml::Value::as_str)
        .is_some_and(|key| !key.trim().is_empty())
    {
        return true;
    }
    config
        .get("providers")
        .and_then(toml::Value::as_table)
        .is_some_and(|providers| providers.values().any(provider_config_has_api_key))
}

fn provider_config_has_api_key(provider: &toml::Value) -> bool {
    if provider
        .get("api_key")
        .and_then(toml::Value::as_str)
        .is_some_and(non_empty)
    {
        return true;
    }
    provider
        .get("env")
        .and_then(toml::Value::as_table)
        .is_some_and(|environment| {
            environment.iter().any(|(name, value)| {
                KIMI_API_KEY_ENV_NAMES.contains(&name.as_str())
                    && value.as_str().is_some_and(non_empty)
            })
        })
}

fn non_empty(value: &str) -> bool {
    !value.trim().is_empty()
}

fn json_token(value: &Value) -> bool {
    matches!(value, Value::String(token) if non_empty(token))
}
