use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::Engine;

use super::environment::{Environment, ProcessEnvironment};
use super::filesystem::FileSystem;
use super::profile_auth_common::json_file;
use super::profile_auth_codex;
use super::profile_auth_grok;
use super::profiles::credential_path;

const EMAIL_FIELDS: &[&str] = &[
    "email",
    "emailAddress",
    "email_address",
    "user_email",
    "userEmail",
    "account_email",
    "accountEmail",
];
const IDENTITY_CONTAINERS: &[&str] = &[
    "account",
    "identity",
    "oauthAccount",
    "claudeAiOauth",
    "profile",
    "user",
    "userinfo",
];
const MAX_EMAIL_CHARS: usize = 320;

/// Return the account email exposed by a locally stored provider credential.
/// Missing or malformed metadata is intentionally treated as no email.
pub fn profile_email(
    engine: Engine,
    profile: &Path,
    home: &Path,
    filesystem: &dyn FileSystem,
) -> Option<String> {
    profile_email_with_environment(
        engine,
        profile,
        home,
        &ProcessEnvironment,
        filesystem,
    )
}

/// Environment-aware email lookup used by discovery and hermetic tests.
pub fn profile_email_with_environment(
    engine: Engine,
    profile: &Path,
    home: &Path,
    environment: &dyn Environment,
    filesystem: &dyn FileSystem,
) -> Option<String> {
    match engine {
        Engine::Codex => profile_auth_codex::codex_auth_identity(profile, filesystem)
            .and_then(|identity| identity.email().map(ToOwned::to_owned))
            .and_then(|email| canonical_email(&email)),
        Engine::Grok => profile_auth_grok::grok_auth_identity(profile, filesystem)
            .and_then(|identity| identity.email().map(ToOwned::to_owned))
            .and_then(|email| canonical_email(&email)),
        Engine::Claude => claude_email(profile, filesystem),
        Engine::Kimi => json_email(
            credential_path(Engine::Kimi, profile, Path::new("")),
            filesystem,
        ),
        Engine::Opencode => {
            let path = environment.opencode_data_dir(profile).join("auth.json");
            opencode_email(&path, filesystem)
        }
    }
    .or_else(|| {
        if matches!(engine, Engine::Opencode) {
            let path = credential_path(engine, profile, home);
            opencode_email(&path, filesystem)
        } else {
            None
        }
    })
}

fn claude_email(profile: &Path, filesystem: &dyn FileSystem) -> Option<String> {
    [
        credential_path(Engine::Claude, profile, Path::new("")),
        profile.join("settings.json"),
        profile.join(".claude.json"),
    ]
    .into_iter()
    .find_map(|path| json_email(path, filesystem))
}

fn opencode_email(path: &Path, filesystem: &dyn FileSystem) -> Option<String> {
    let value = json_file(path.to_path_buf(), filesystem)?;
    let mut emails = BTreeSet::new();
    collect_emails(&value, 0, &mut emails);
    (emails.len() == 1).then(|| emails.into_iter().next().expect("one email"))
}

fn collect_emails(value: &Value, depth: usize, emails: &mut BTreeSet<String>) {
    let Value::Object(object) = value else {
        return;
    };
    for field in EMAIL_FIELDS {
        if let Some(email) = object.get(*field).and_then(Value::as_str).and_then(canonical_email) {
            emails.insert(email);
        }
    }
    if depth >= 4 {
        return;
    }
    for nested in object.values() {
        if nested.is_object() {
            collect_emails(nested, depth + 1, emails);
        }
    }
}

fn json_email(path: PathBuf, filesystem: &dyn FileSystem) -> Option<String> {
    let value = json_file(path, filesystem)?;
    first_email(&value)
}

fn first_email(value: &Value) -> Option<String> {
    first_email_at(value, 0)
}

fn first_email_at(value: &Value, depth: usize) -> Option<String> {
    let Value::Object(object) = value else {
        return None;
    };
    if let Some(email) = EMAIL_FIELDS
        .iter()
        .find_map(|field| object.get(*field).and_then(Value::as_str))
        .and_then(canonical_email)
    {
        return Some(email);
    }
    if depth >= 4 {
        return None;
    }
    IDENTITY_CONTAINERS.iter().find_map(|field| {
        object
            .get(*field)
            .and_then(|nested| first_email_at(nested, depth + 1))
    })
}

pub(crate) fn canonical_email(value: &str) -> Option<String> {
    let email = value.trim();
    if email.is_empty()
        || email.chars().count() > MAX_EMAIL_CHARS
        || email.chars().any(|character| character.is_control() || character.is_whitespace())
        || email.matches('@').count() != 1
    {
        return None;
    }
    let (local, domain) = email.split_once('@')?;
    if local.is_empty() || domain.is_empty() {
        return None;
    }
    Some(email.to_ascii_lowercase())
}

#[cfg(test)]
#[path = "profile_identity_tests.rs"]
mod tests;
