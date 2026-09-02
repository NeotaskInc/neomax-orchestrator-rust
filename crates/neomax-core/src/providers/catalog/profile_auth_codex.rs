use std::fmt::Write;
use std::path::Path;

use crate::Engine;
use base64::{engine::general_purpose::URL_SAFE, Engine as _};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::filesystem::FileSystem;
use super::profile_auth_common::{json_file, unique_methods};
use super::profiles::credential_path;
use super::types::{AuthMethod, CodexAuthIdentity};

const TOKEN_FIELDS: &[&str] = &["access_token", "id_token", "refresh_token"];
const IDENTITY_FIELDS: &[&str] = &[
    "chatgpt_account_id",
    "chatgptAccountId",
    "account_id",
    "accountId",
    "chatgpt_user_id",
    "chatgptUserId",
    "user_id",
    "userId",
];

pub(super) fn codex_auth(profile: &Path, filesystem: &dyn FileSystem) -> Vec<AuthMethod> {
    let Some(value) = json_file(
        credential_path(Engine::Codex, profile, Path::new("")),
        filesystem,
    ) else {
        return Vec::new();
    };
    let object = value.as_object();
    let Some(object) = object else {
        return Vec::new();
    };
    let mut methods = Vec::new();
    if object.get("OPENAI_API_KEY").is_some_and(nonempty_string)
        || object.get("CODEX_API_KEY").is_some_and(nonempty_string)
    {
        methods.push(AuthMethod::ApiKey);
    }
    if object
        .get("tokens")
        .and_then(Value::as_object)
        .is_some_and(|tokens| {
            TOKEN_FIELDS
                .iter()
                .any(|field| tokens.get(*field).is_some_and(nonempty_string))
        })
    {
        methods.push(AuthMethod::OAuth);
    }
    unique_methods(methods)
}

pub(super) fn codex_auth_identity(
    profile: &Path,
    filesystem: &dyn FileSystem,
) -> Option<CodexAuthIdentity> {
    let value = json_file(
        credential_path(Engine::Codex, profile, Path::new("")),
        filesystem,
    )?;
    let root = value.as_object()?;
    let tokens = root.get("tokens")?.as_object()?;
    if !TOKEN_FIELDS
        .iter()
        .any(|field| tokens.get(*field).is_some_and(nonempty_string))
    {
        return None;
    }

    let mut plan = None;
    let mut identity = first_identity(root).or_else(|| first_identity(tokens));
    for field in TOKEN_FIELDS {
        let Some(token) = tokens.get(*field).and_then(Value::as_str) else {
            continue;
        };
        let Some(payload) = jwt_payload(token) else {
            continue;
        };
        if plan.is_none() {
            plan = payload_plan(&payload);
        }
        if identity.is_none() {
            identity = first_identity_from_payload(&payload);
        }
        if identity.is_none() {
            identity = nonempty_string_value(payload.get("email"));
        }
        if identity.is_some() && plan.is_some() {
            break;
        }
    }

    let identity = identity?;
    let canonical = identity.trim().to_ascii_lowercase();
    if canonical.is_empty() {
        return None;
    }
    let digest = Sha256::digest(canonical.as_bytes());
    let mut label = String::from("acct-");
    for byte in digest.iter().take(8) {
        let _ = write!(label, "{byte:02x}");
    }
    Some(CodexAuthIdentity::new(label, plan))
}

fn nonempty_string(value: &Value) -> bool {
    value.as_str().is_some_and(|value| !value.trim().is_empty())
}

fn nonempty_string_value(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn first_identity(object: &serde_json::Map<String, Value>) -> Option<String> {
    IDENTITY_FIELDS
        .iter()
        .find_map(|field| nonempty_string_value(object.get(*field)))
}

fn first_identity_from_payload(payload: &Value) -> Option<String> {
    payload
        .get("https://api.openai.com/auth")
        .and_then(Value::as_object)
        .and_then(first_identity)
        .or_else(|| payload.as_object().and_then(first_identity))
        .or_else(|| nonempty_string_value(payload.get("sub")))
}

fn payload_plan(payload: &Value) -> Option<String> {
    let value = payload
        .get("https://api.openai.com/auth")
        .and_then(Value::as_object)
        .and_then(|auth| auth.get("chatgpt_plan_type"))
        .or_else(|| payload.get("plan_type"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let mut plan = String::with_capacity(value.len().min(32));
    for character in value.chars().take(32) {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
            plan.push(character.to_ascii_lowercase());
        } else {
            return None;
        }
    }
    (!plan.is_empty()).then_some(plan)
}

fn jwt_payload(token: &str) -> Option<Value> {
    let mut pieces = token.split('.');
    let _header = pieces.next()?;
    let payload = pieces.next()?;
    let _signature = pieces.next()?;
    if pieces.next().is_some() || payload.trim().is_empty() {
        return None;
    }
    let mut encoded = payload.as_bytes().to_vec();
    while encoded.len() % 4 != 0 {
        encoded.push(b'=');
    }
    let bytes = URL_SAFE.decode(encoded).ok()?;
    let value = serde_json::from_slice::<Value>(&bytes).ok()?;
    value.is_object().then_some(value)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn jwt(payload: &str) -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        format!(
            "{}.{}.signature",
            URL_SAFE_NO_PAD.encode(br#"{}"#),
            URL_SAFE_NO_PAD.encode(payload.as_bytes())
        )
    }

    #[test]
    fn malformed_and_wrong_type_codex_credentials_are_not_authenticated() {
        for bytes in [
            br#"{"tokens":{"access_token":"   "}}"#.as_slice(),
            br#"{"tokens":{"access_token":true}}"#.as_slice(),
            br#"{"tokens":[]}"#.as_slice(),
            br#"{"OPENAI_API_KEY":true}"#.as_slice(),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let profile = temp.path().join("codex");
            fs::create_dir_all(&profile).unwrap();
            fs::write(profile.join("auth.json"), bytes).unwrap();
            assert!(codex_auth(&profile, &super::super::filesystem::RealFileSystem).is_empty());
        }
    }

    #[test]
    fn identity_is_opaque_and_stable_for_same_account_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let profile = temp.path().join("codex");
        fs::create_dir_all(&profile).unwrap();
        let token = jwt(
            r#"{"email":"person@example.test","https://api.openai.com/auth":{"chatgpt_account_id":"acct-123","chatgpt_plan_type":"Plus"}}"#,
        );
        fs::write(
            profile.join("auth.json"),
            serde_json::to_vec(&serde_json::json!({
                "tokens": {"id_token": token, "refresh_token": "refresh"}
            }))
            .unwrap(),
        )
        .unwrap();
        let identity =
            codex_auth_identity(&profile, &super::super::filesystem::RealFileSystem).unwrap();
        assert!(identity.label().starts_with("acct-"));
        assert!(!identity.label().contains("person"));
        assert!(!identity.label().contains("123"));
        assert_eq!(identity.plan(), Some("plus"));
    }

    #[test]
    fn identity_falls_back_to_sanitized_email_hash_without_printing_email() {
        let temp = tempfile::tempdir().unwrap();
        let profile = temp.path().join("codex");
        fs::create_dir_all(&profile).unwrap();
        let token = jwt(r#"{"email":"person@example.test"}"#);
        fs::write(
            profile.join("auth.json"),
            serde_json::to_vec(&serde_json::json!({"tokens": {"id_token": token}})).unwrap(),
        )
        .unwrap();
        let identity =
            codex_auth_identity(&profile, &super::super::filesystem::RealFileSystem).unwrap();
        assert!(identity.label().starts_with("acct-"));
        assert!(!identity.label().contains("person"));
        assert!(identity.plan().is_none());
    }
}
