use std::path::Path;
use std::str::FromStr;

use serde_json::{Map, Value};

use crate::{Engine, Error, Result};

use super::document::{BackupDocument, BACKUP_VERSION};
use super::super::types::FileSnapshot;

pub(super) fn write_legacy_fields(
    value: &mut Value,
    engine: Engine,
    snapshot: &FileSnapshot,
    timestamp: i64,
) {
    value["ts"] = Value::from(timestamp);
    let blob = match engine {
        Engine::Claude => legacy_text(snapshot.credential.as_deref()),
        Engine::Codex => legacy_text(snapshot.auth.as_deref()),
        _ => None,
    };
    if let Some(blob) = blob {
        value["blob"] = Value::String(blob);
    }
    if engine == Engine::Claude {
        if let Some(account) = oauth_account(snapshot.identity.as_deref()) {
            value["oauth_account"] = account;
        }
    }
}

pub(super) fn parse_document(path: &Path, bytes: &[u8]) -> Result<BackupDocument> {
    let value: Value = serde_json::from_slice(bytes).map_err(|error| Error::InvalidState {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    if value.get("version").is_some() {
        let document: BackupDocument =
            serde_json::from_value(value).map_err(|error| Error::InvalidState {
                path: path.to_path_buf(),
                message: error.to_string(),
            })?;
        if document.version != BACKUP_VERSION {
            return Err(Error::InvalidState {
                path: path.to_path_buf(),
                message: format!(
                    "unsupported credential backup version {}",
                    document.version
                ),
            });
        }
        return Ok(document);
    }
    parse_legacy_document(path, value)
}

fn parse_legacy_document(path: &Path, value: Value) -> Result<BackupDocument> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid_legacy(path, "record must be an object"))?;
    let engine = object
        .get("engine")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_legacy(path, "missing engine"))
        .and_then(|value| {
            Engine::from_str(value).map_err(|_| invalid_legacy(path, "invalid engine"))
        })?;
    let timestamp = object
        .get("ts")
        .or_else(|| object.get("timestamp"))
        .and_then(Value::as_i64)
        .ok_or_else(|| invalid_legacy(path, "missing integer ts"))?;
    let blob = object
        .get("blob")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_legacy(path, "missing string blob"))?;
    let profile_name = legacy_profile_name(path)
        .ok_or_else(|| invalid_legacy(path, "legacy filename does not identify a profile"))?;
    let oauth_account = match object.get("oauth_account") {
        Some(value) if value.is_object() => Some(value.clone()),
        Some(_) => return Err(invalid_legacy(path, "oauth_account must be an object")),
        None => None,
    };
    let identity = oauth_account.as_ref().map(|account| {
        let mut identity = Map::new();
        identity.insert("oauthAccount".into(), account.clone());
        let mut bytes = serde_json::to_vec(&Value::Object(identity)).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    });
    let (credential, auth) = match engine {
        Engine::Claude => (Some(blob.as_bytes().to_vec()), None),
        Engine::Codex => (None, Some(blob.as_bytes().to_vec())),
        _ => {
            return Err(invalid_legacy(
                path,
                "legacy backups support only Claude and Codex",
            ));
        }
    };
    Ok(BackupDocument::from_legacy(
        engine,
        profile_name,
        timestamp,
        credential,
        identity,
        auth,
        blob.is_empty(),
    ))
}

fn invalid_legacy(path: &Path, message: &str) -> Error {
    Error::InvalidState {
        path: path.to_path_buf(),
        message: format!("invalid legacy credential backup: {message}"),
    }
}

fn legacy_profile_name(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let (timestamp, profile) = stem.split_once('-')?;
    timestamp
        .parse::<i64>()
        .ok()
        .filter(|_| !profile.is_empty())?;
    Some(profile.to_owned())
}

fn legacy_text(bytes: Option<&[u8]>) -> Option<String> {
    match bytes {
        Some(bytes) => std::str::from_utf8(bytes).ok().map(ToOwned::to_owned),
        None => Some(String::new()),
    }
}

fn oauth_account(identity: Option<&[u8]>) -> Option<Value> {
    serde_json::from_slice::<Value>(identity?)
        .ok()?
        .get("oauthAccount")
        .filter(|value| value.is_object())
        .cloned()
}
