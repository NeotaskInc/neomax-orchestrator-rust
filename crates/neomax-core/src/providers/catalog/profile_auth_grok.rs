use std::path::Path;

use crate::Engine;
use serde_json::Value;

use super::filesystem::FileSystem;
use super::profile_auth_common::{json_file, object_has_credential, unique_methods};
use super::profile_auth_store::classify_store_entry;
use super::profiles::credential_path;
use super::types::{AuthMethod, GrokAuthIdentity};

const GROK_CREDENTIAL_FIELDS: &[&str] = &[
    "key",
    "api_key",
    "apiKey",
    "xai_api_key",
    "xaiApiKey",
    "access_token",
    "accessToken",
    "refresh_token",
    "refreshToken",
    "device_token",
    "deviceToken",
    "device_code",
    "deviceCode",
];
const MAX_IDENTITY_TEXT_CHARS: usize = 256;

pub(super) fn grok_auth(profile: &Path, filesystem: &dyn FileSystem) -> Vec<AuthMethod> {
    let Some(Value::Object(store)) = json_file(
        credential_path(Engine::Grok, profile, Path::new("")),
        filesystem,
    ) else {
        return Vec::new();
    };
    let mut methods = Vec::new();
    for value in store.values() {
        if let Some(object) = value.as_object() {
            methods.extend(classify_store_entry(object));
        }
    }
    unique_methods(methods)
}

pub(super) fn grok_auth_identity(
    profile: &Path,
    filesystem: &dyn FileSystem,
) -> Option<GrokAuthIdentity> {
    let Value::Object(store) = json_file(
        credential_path(Engine::Grok, profile, Path::new("")),
        filesystem,
    )?
    else {
        return None;
    };
    store.values().find_map(|value| {
        let object = value.as_object()?;
        object_has_credential(object, GROK_CREDENTIAL_FIELDS).then(|| {
            let first_name = safe_text(object.get("first_name"));
            let last_name = safe_text(object.get("last_name"));
            let name = match (first_name, last_name) {
                (Some(first), Some(last)) => Some(format!("{first} {last}")),
                (Some(first), None) => Some(first),
                (None, Some(last)) => Some(last),
                (None, None) => first_available_text(object, &["name", "display_name"]),
            };
            GrokAuthIdentity::new(
                auth_method(object),
                safe_text(object.get("email")),
                name,
                first_available_text(object, &["team_name", "team"]),
            )
        })
    })
}

fn auth_method(object: &serde_json::Map<String, Value>) -> String {
    let raw = ["auth_mode", "authMode", "method", "type"]
        .into_iter()
        .find_map(|key| safe_text(object.get(key)))
        .map(|value| value.to_ascii_lowercase().replace(['-', '_'], ""));
    match raw.as_deref() {
        Some("oauth" | "oidc" | "managedoauth") => "OAuth".into(),
        Some("apikey" | "api" | "key") => "API key".into(),
        Some("device" | "deviceoauth" | "deviceauth") => "device".into(),
        _ => "connected".into(),
    }
}

fn first_available_text(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| safe_text(object.get(*key)))
}

fn safe_text(value: Option<&Value>) -> Option<String> {
    let text = value?.as_str()?.trim();
    if text.is_empty()
        || text.chars().count() > MAX_IDENTITY_TEXT_CHARS
        || text.chars().any(char::is_control)
    {
        return None;
    }
    Some(text.to_owned())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn identity_reads_allowlisted_metadata_without_retaining_credentials() {
        let temp = tempfile::tempdir().unwrap();
        let profile = temp.path().join("grok");
        fs::create_dir_all(&profile).unwrap();
        fs::write(
            profile.join("auth.json"),
            br#"{"xai::oidc":{"auth_mode":"oidc","key":"raw-token","email":"person@example.test","first_name":"Ada","last_name":"Lovelace","team_name":"Analytical Engine","access_token":"second-secret"}}"#,
        )
        .unwrap();

        let identity = grok_auth_identity(&profile, &super::super::filesystem::RealFileSystem)
            .expect("identity metadata");
        assert_eq!(identity.method(), "OAuth");
        assert_eq!(identity.email(), Some("person@example.test"));
        assert_eq!(identity.name(), Some("Ada Lovelace"));
        assert_eq!(identity.team(), Some("Analytical Engine"));
        let debug = format!("{identity:?}");
        assert!(!debug.contains("raw-token"));
        assert!(!debug.contains("second-secret"));
        assert!(!debug.contains(&profile.display().to_string()));
    }

    #[test]
    fn identity_uses_connected_fallback_for_unknown_auth_modes() {
        let temp = tempfile::tempdir().unwrap();
        let profile = temp.path().join("grok");
        fs::create_dir_all(&profile).unwrap();
        fs::write(
            profile.join("auth.json"),
            br#"{"xai::credential":{"auth_mode":"future_mode","key":"raw-token","email":"person@example.test"}}"#,
        )
        .unwrap();

        let identity = grok_auth_identity(&profile, &super::super::filesystem::RealFileSystem)
            .expect("identity metadata");
        assert_eq!(identity.method(), "connected");
        assert_eq!(identity.email(), Some("person@example.test"));
    }

    #[test]
    fn identity_discards_control_text_and_oversized_fields() {
        let temp = tempfile::tempdir().unwrap();
        let profile = temp.path().join("grok");
        fs::create_dir_all(&profile).unwrap();
        let oversized = "x".repeat(MAX_IDENTITY_TEXT_CHARS + 1);
        let contents = serde_json::json!({
            "credential": {
                "auth_mode": "api_key",
                "key": "raw-token",
                "email": "person@example.test\nforged: value",
                "first_name": oversized,
                "last_name": "Visible",
                "team_name": "Team"
            }
        });
        fs::write(
            profile.join("auth.json"),
            serde_json::to_vec(&contents).unwrap(),
        )
        .unwrap();

        let identity = grok_auth_identity(&profile, &super::super::filesystem::RealFileSystem)
            .expect("identity metadata");
        assert_eq!(identity.method(), "API key");
        assert_eq!(identity.email(), None);
        assert_eq!(identity.name(), Some("Visible"));
        assert_eq!(identity.team(), Some("Team"));
    }

    #[cfg(unix)]
    #[test]
    fn identity_does_not_follow_an_auth_file_symlink() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let profile = temp.path().join("grok");
        let outside = temp.path().join("outside-auth.json");
        fs::create_dir_all(&profile).unwrap();
        fs::write(
            &outside,
            br#"{"credential":{"auth_mode":"api_key","key":"raw-token","email":"person@example.test"}}"#,
        )
        .unwrap();
        symlink(&outside, profile.join("auth.json")).unwrap();

        assert!(grok_auth_identity(&profile, &super::super::filesystem::RealFileSystem).is_none());
    }

    #[test]
    fn identity_does_not_read_an_oversized_auth_file() {
        let temp = tempfile::tempdir().unwrap();
        let profile = temp.path().join("grok");
        fs::create_dir_all(&profile).unwrap();
        let mut contents = br#"{"credential":{"auth_mode":"api_key","key":"raw-token","email":"person@example.test","padding":""}}"#.to_vec();
        contents.extend(std::iter::repeat_n(b'x', 2 * 1024 * 1024));
        fs::write(profile.join("auth.json"), contents).unwrap();

        assert!(grok_auth_identity(&profile, &super::super::filesystem::RealFileSystem).is_none());
    }
}
