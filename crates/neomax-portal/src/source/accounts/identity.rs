use std::path::Path;

use base64::{Engine as _, engine::general_purpose::URL_SAFE};
use neomax_core::config::Engine;
use neomax_core::io::{LocalFileSource, ReadLimits, read_file};
use serde_json::Value;

pub(crate) fn identity_for(
    engine: Engine,
    profile: &Path,
    home: &Path,
) -> (Option<String>, Option<String>, Option<String>) {
    match engine {
        Engine::Claude => {
            let path = if profile == home.join(".claude") {
                home.join(".claude.json")
            } else {
                profile.join(".claude.json")
            };
            let Some(Value::Object(root)) = read_json(&path) else {
                return (None, None, None);
            };
            let account = root.get("oauthAccount").and_then(Value::as_object);
            (
                account
                    .and_then(|value| value.get("emailAddress"))
                    .and_then(Value::as_str)
                    .map(Into::into),
                account
                    .and_then(|value| value.get("organizationRateLimitTier"))
                    .and_then(Value::as_str)
                    .map(|value| value.replace("default_claude_", "").replace('_', " ")),
                account
                    .and_then(|value| value.get("displayName"))
                    .and_then(Value::as_str)
                    .map(Into::into),
            )
        }
        Engine::Codex => {
            let Some(Value::Object(root)) = read_json(&profile.join("auth.json")) else {
                return (None, None, None);
            };
            let Some(token) = root
                .get("tokens")
                .and_then(Value::as_object)
                .and_then(|tokens| tokens.get("id_token"))
                .and_then(Value::as_str)
            else {
                return (None, None, None);
            };
            let Some(payload) = token.split('.').nth(1).and_then(decode_json) else {
                return (None, None, None);
            };
            let plan = payload
                .get("https://api.openai.com/auth")
                .and_then(|value| value.get("chatgpt_plan_type"))
                .and_then(Value::as_str)
                .map(Into::into);
            (
                payload.get("email").and_then(Value::as_str).map(Into::into),
                plan,
                payload.get("name").and_then(Value::as_str).map(Into::into),
            )
        }
        Engine::Opencode => (None, Some("Go".into()), Some("OX Alpha Free".into())),
        Engine::Kimi => (None, Some("Kimi Code".into()), Some("K3 / K2.7".into())),
        Engine::Grok => {
            let Some(Value::Object(root)) = read_json(&profile.join("auth.json")) else {
                return (None, None, None);
            };
            let item = root.values().find_map(Value::as_object);
            (
                item.and_then(|value| value.get("email"))
                    .and_then(Value::as_str)
                    .map(Into::into),
                item.and_then(|value| value.get("team_name"))
                    .and_then(Value::as_str)
                    .map(Into::into),
                item.and_then(|value| value.get("first_name"))
                    .and_then(Value::as_str)
                    .map(Into::into),
            )
        }
    }
}

fn read_json(path: &Path) -> Option<Value> {
    const MAX_IDENTITY_BYTES: usize = 512 * 1024;
    let bytes = read_file(
        &LocalFileSource,
        path,
        ReadLimits::new(MAX_IDENTITY_BYTES, std::time::Duration::from_secs(2)).ok()?,
    )
    .ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn decode_json(value: &str) -> Option<Value> {
    let mut bytes = value.as_bytes().to_vec();
    while bytes.len() % 4 != 0 {
        bytes.push(b'=');
    }
    let bytes = URL_SAFE.decode(bytes).ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn reads_safe_display_identity_without_exposing_credentials() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join(".claude.json"),
            r#"{"oauthAccount":{"emailAddress":"dev@example.test","displayName":"Dev","organizationRateLimitTier":"default_claude_max"}}"#,
        )
        .unwrap();
        let identity = identity_for(Engine::Claude, &temp.path().join(".claude"), temp.path());
        assert_eq!(identity.0.as_deref(), Some("dev@example.test"));
        assert_eq!(identity.1.as_deref(), Some("max"));
        assert_eq!(identity.2.as_deref(), Some("Dev"));
    }

    #[test]
    fn oversized_identity_files_are_ignored() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(".claude.json");
        fs::write(&path, vec![b'x'; 512 * 1024 + 1]).unwrap();
        assert_eq!(
            identity_for(Engine::Claude, &temp.path().join(".claude"), temp.path()),
            (None, None, None)
        );
    }
}
