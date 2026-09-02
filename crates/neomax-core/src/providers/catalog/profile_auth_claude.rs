use std::path::Path;

use crate::Engine;
use serde_json::Value;

use super::filesystem::FileSystem;
use super::profile_auth_common::{json_file, read_utf8, unique_methods, value_has_credential_key};
use super::profiles::credential_path;
use super::types::AuthMethod;
pub(super) fn claude_auth(profile: &Path, filesystem: &dyn FileSystem) -> Vec<AuthMethod> {
    let credentials_path = credential_path(Engine::Claude, profile, Path::new(""));
    let mut methods = Vec::new();
    if let Some(value) = json_file(credentials_path.clone(), filesystem) {
        if claude_value_has_credential(
            &value,
            &[
                "accessToken",
                "access_token",
                "refreshToken",
                "refresh_token",
            ],
        ) {
            methods.push(AuthMethod::OAuth);
        }
        if claude_value_has_credential(
            &value,
            &[
                "apiKey",
                "api_key",
                "ANTHROPIC_API_KEY",
                "ANTHROPIC_AUTH_TOKEN",
            ],
        ) {
            methods.push(AuthMethod::ApiKey);
        }
    }

    for path in [profile.join("settings.json"), profile.join(".claude.json")] {
        if let Some(value) = json_file(path, filesystem) {
            if claude_value_has_credential(
                &value,
                &[
                    "accessToken",
                    "access_token",
                    "refreshToken",
                    "refresh_token",
                ],
            ) {
                methods.push(AuthMethod::OAuth);
            }
            if claude_value_has_credential(
                &value,
                &[
                    "apiKey",
                    "api_key",
                    "ANTHROPIC_API_KEY",
                    "ANTHROPIC_AUTH_TOKEN",
                ],
            ) {
                methods.push(AuthMethod::ApiKey);
            }
        }
    }
    if let Some(dotenv) = read_utf8(profile.join(".env"), filesystem) {
        if dotenv.lines().any(|line| {
            let (key, value) = line.split_once('=').unwrap_or(("", ""));
            matches!(key.trim(), "ANTHROPIC_API_KEY" | "ANTHROPIC_AUTH_TOKEN")
                && !value.trim().is_empty()
        }) {
            methods.push(AuthMethod::ApiKey);
        }
    }
    unique_methods(methods)
}

fn claude_value_has_credential(value: &Value, keys: &[&str]) -> bool {
    value_has_credential_key(value, keys)
        || value
            .get("claudeAiOauth")
            .is_some_and(|value| value_has_credential_key(value, keys))
        || value
            .get("env")
            .is_some_and(|value| value_has_credential_key(value, keys))
}

pub(super) fn claude_credential_present(profile: &Path, filesystem: &dyn FileSystem) -> bool {
    filesystem.is_file(&credential_path(Engine::Claude, profile, Path::new("")))
        || filesystem.is_file(&profile.join("settings.json"))
        || filesystem.is_file(&profile.join(".claude.json"))
        || filesystem.is_file(&profile.join(".env"))
}
