use super::{
    AuthStatus, Environment, FileSystem, ProfileSnapshot, credential_path_with_environment,
};
use crate::Engine;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialHealth {
    Missing,
    Unknown,
    LocallyPresent,
    Current,
    RefreshAvailable,
    LoginRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialEvidence {
    pub health: CredentialHealth,
    pub expires_at: Option<i64>,
    pub refresh_present: bool,
    pub remotely_verified: bool,
}

pub fn credential_evidence(
    profile: &ProfileSnapshot,
    home: &Path,
    environment: &dyn Environment,
    filesystem: &dyn FileSystem,
    now: i64,
) -> CredentialEvidence {
    let path = credential_path_with_environment(profile.engine, &profile.path, home, environment);
    let fallback = match profile.auth {
        AuthStatus::Authenticated { .. } => CredentialHealth::LocallyPresent,
        AuthStatus::Unauthenticated => CredentialHealth::Missing,
        AuthStatus::Unknown => CredentialHealth::Unknown,
    };
    let mut result = CredentialEvidence {
        health: fallback,
        expires_at: None,
        refresh_present: false,
        remotely_verified: false,
    };
    let value = match filesystem.read(&path) {
        Ok(Some(bytes)) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(value) => value,
            Err(_) => {
                result.health = CredentialHealth::Unknown;
                return result;
            }
        },
        Ok(None) => return result,
        Err(_) => {
            result.health = CredentialHealth::Unknown;
            return result;
        }
    };
    let token = match profile.engine {
        Engine::Claude => value.get("claudeAiOauth").unwrap_or(&value),
        Engine::Codex => value.get("tokens").unwrap_or(&value),
        Engine::Kimi => value.get("oauth").unwrap_or(&value),
        Engine::Grok => value.get("tokens").unwrap_or(&value),
        Engine::Opencode => return result,
    };
    result.refresh_present = ["refresh_token", "refreshToken"]
        .into_iter()
        .any(|key| nonempty(token.get(key)));
    result.expires_at = ["expires_at", "expiresAt", "expires", "expiration", "expiry"]
        .into_iter()
        .filter_map(|key| token.get(key).and_then(epoch))
        .min()
        .or_else(|| {
            token
                .get("access_token")
                .or_else(|| token.get("accessToken"))
                .and_then(Value::as_str)
                .and_then(jwt_expiry)
        });
    let has_access = ["access_token", "accessToken"]
        .into_iter()
        .any(|key| nonempty(token.get(key)));
    result.health = match result.expires_at {
        Some(expiry) if expiry <= now => {
            if result.refresh_present {
                CredentialHealth::RefreshAvailable
            } else {
                CredentialHealth::LoginRequired
            }
        }
        Some(_) if has_access => CredentialHealth::Current,
        _ if !has_access && result.refresh_present => CredentialHealth::RefreshAvailable,
        _ => result.health,
    };
    result
}

fn nonempty(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
}
fn epoch(value: &Value) -> Option<i64> {
    let number = value.as_i64().or_else(|| value.as_str()?.parse().ok());
    if let Some(number) = number.filter(|number| *number > 0) {
        return Some(if number > 100_000_000_000 {
            number / 1000
        } else {
            number
        });
    }
    chrono::DateTime::parse_from_rfc3339(value.as_str()?)
        .ok()
        .map(|date| date.timestamp())
}
fn jwt_expiry(token: &str) -> Option<i64> {
    let part = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(part.trim_end_matches('=')).ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    value.get("exp").and_then(epoch)
}

#[cfg(test)]
mod tests {
    use super::super::{MapEnvironment, RealFileSystem, inspect_profile_snapshot};
    use super::*;
    #[test]
    fn expired_refreshable_is_not_reported_as_verified_or_login_required() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("codex");
        std::fs::create_dir(&path).unwrap();
        for (refresh, expected) in [
            ("fixture-refresh", CredentialHealth::RefreshAvailable),
            ("", CredentialHealth::LoginRequired),
        ] {
            std::fs::write(path.join("auth.json"), serde_json::json!({"tokens":{"access_token":"fixture-access","expires_at":100,"refresh_token":refresh}}).to_string()).unwrap();
            let profile = inspect_profile_snapshot(
                Engine::Codex,
                "1",
                path.clone(),
                false,
                temp.path(),
                &RealFileSystem,
            );
            let result = credential_evidence(
                &profile,
                temp.path(),
                &MapEnvironment::default(),
                &RealFileSystem,
                200,
            );
            assert_eq!(result.health, expected);
            assert!(!result.remotely_verified);
            let json = serde_json::to_string(&result).unwrap();
            assert!(!json.contains("fixture-access"));
            assert!(!json.contains("fixture-refresh"));
        }
    }
}
