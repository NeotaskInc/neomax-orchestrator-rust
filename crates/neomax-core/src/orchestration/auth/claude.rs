use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::runtime::RuntimeEnvironment;
use crate::{Error, Result};

use super::transaction::{Mutation, snapshot_paths};
use super::types::{FileSnapshot, absolute_path, same_profile};
use super::writer::CredentialWriter;

pub const CREDENTIAL_FILE: &str = ".credentials.json";
pub const IDENTITY_FILE: &str = ".claude.json";

#[derive(Debug, Clone)]
pub struct ClaudeCopyPlan {
    pub destination: PathBuf,
    pub source: PathBuf,
    pub destination_before: FileSnapshot,
    pub mutations: Vec<Mutation>,
    pub from_email: Option<String>,
    pub to_email: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ClaudeSwapPlan {
    pub first: PathBuf,
    pub second: PathBuf,
    pub first_before: FileSnapshot,
    pub second_before: FileSnapshot,
    pub mutations: Vec<Mutation>,
    pub first_email: Option<String>,
    pub second_email: Option<String>,
}

pub fn credential_path(profile: &Path) -> PathBuf {
    profile.join(CREDENTIAL_FILE)
}

pub fn identity_path(profile: &Path) -> PathBuf {
    let Some(home) = RuntimeEnvironment::process().home_dir() else {
        return profile.join(IDENTITY_FILE);
    };
    identity_path_for_home(profile, &home)
}

pub fn identity_path_for_home(profile: &Path, home: &Path) -> PathBuf {
    if absolute_path(profile) == absolute_path(&home.join(".claude")) {
        return home.join(IDENTITY_FILE);
    }
    profile.join(IDENTITY_FILE)
}

pub fn prepare_copy<W: CredentialWriter>(
    writer: &W,
    destination: &Path,
    source: &Path,
) -> Result<ClaudeCopyPlan> {
    reject_same_profile(destination, source)?;
    let source_credential = required_credential(writer, source)?;
    let source_identity = read_identity(writer, source)?;
    let destination_identity = read_identity(writer, destination)?;
    reject_same_account(&destination_identity, &source_identity)?;
    let destination_before = read_snapshot(writer, destination)?;
    let mut mutations = vec![Mutation::write(
        credential_path(destination),
        source_credential,
    )];
    if let Some(identity) =
        merged_identity(destination_identity.as_ref(), source_identity.as_ref())?
    {
        mutations.push(Mutation::write(identity_path(destination), identity));
    }
    Ok(ClaudeCopyPlan {
        destination: destination.to_path_buf(),
        source: source.to_path_buf(),
        destination_before,
        mutations,
        from_email: email(destination_identity.as_ref()),
        to_email: email(source_identity.as_ref()),
    })
}

pub fn prepare_swap<W: CredentialWriter>(
    writer: &W,
    first: &Path,
    second: &Path,
) -> Result<ClaudeSwapPlan> {
    reject_same_profile(first, second)?;
    let first_credential = required_credential(writer, first)?;
    let second_credential = required_credential(writer, second)?;
    let first_identity = read_identity(writer, first)?;
    let second_identity = read_identity(writer, second)?;
    reject_same_account(&first_identity, &second_identity)?;
    let first_before = read_snapshot(writer, first)?;
    let second_before = read_snapshot(writer, second)?;
    let mut mutations = vec![
        Mutation::write(credential_path(first), second_credential),
        Mutation::write(credential_path(second), first_credential),
    ];
    if let Some(identity) = merged_identity(first_identity.as_ref(), second_identity.as_ref())? {
        mutations.push(Mutation::write(identity_path(first), identity));
    }
    if let Some(identity) = merged_identity(second_identity.as_ref(), first_identity.as_ref())? {
        mutations.push(Mutation::write(identity_path(second), identity));
    }
    Ok(ClaudeSwapPlan {
        first: first.to_path_buf(),
        second: second.to_path_buf(),
        first_before,
        second_before,
        mutations,
        first_email: email(first_identity.as_ref()),
        second_email: email(second_identity.as_ref()),
    })
}

pub fn restore_mutations(snapshot: &FileSnapshot, profile: &Path) -> Vec<Mutation> {
    vec![
        mutation_for(credential_path(profile), snapshot.credential.clone()),
        mutation_for(identity_path(profile), snapshot.identity.clone()),
    ]
}

pub fn account_email(snapshot: &FileSnapshot) -> Option<String> {
    identity_value(snapshot.identity.as_deref()).and_then(|value| email(Some(&value)))
}

pub(crate) fn read_snapshot<W: CredentialWriter>(
    writer: &W,
    profile: &Path,
) -> Result<FileSnapshot> {
    let files = snapshot_paths(
        writer,
        vec![credential_path(profile), identity_path(profile)],
    )?;
    Ok(FileSnapshot {
        credential: files[0].bytes.clone(),
        identity: files[1].bytes.clone(),
        auth: None,
    })
}

fn required_credential<W: CredentialWriter>(writer: &W, profile: &Path) -> Result<Vec<u8>> {
    let Some(bytes) = writer.read_optional(&credential_path(profile))? else {
        return Err(Error::NotFound(format!(
            "Claude credentials are missing for profile {}",
            profile.display()
        )));
    };
    if bytes.is_empty() {
        return Err(Error::InvalidArgument(format!(
            "Claude credentials are empty for profile {}",
            profile.display()
        )));
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| Error::InvalidState {
        path: credential_path(profile),
        message: "Claude credentials are not valid JSON".into(),
    })?;
    let valid = value
        .get("claudeAiOauth")
        .and_then(Value::as_object)
        .and_then(|oauth| oauth.get("accessToken"))
        .is_some_and(json_truthy);
    if !valid {
        return Err(Error::InvalidArgument(format!(
            "Claude profile {} has no usable access token",
            profile.display()
        )));
    }
    Ok(bytes)
}

fn read_identity<W: CredentialWriter>(writer: &W, profile: &Path) -> Result<Option<Value>> {
    let Some(bytes) = writer.read_optional(&identity_path(profile))? else {
        return Ok(None);
    };
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| Error::InvalidState {
        path: identity_path(profile),
        message: "Claude identity is not valid JSON".into(),
    })?;
    if !value.is_object() {
        return Err(Error::InvalidState {
            path: identity_path(profile),
            message: "Claude identity must be a JSON object".into(),
        });
    }
    Ok(Some(value))
}

fn merged_identity(destination: Option<&Value>, source: Option<&Value>) -> Result<Option<Vec<u8>>> {
    let Some(source) = source else {
        return Ok(None);
    };
    let Some(account) = source.get("oauthAccount").filter(|value| value.is_object()) else {
        return Ok(None);
    };
    let mut destination = destination
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()));
    let object = destination
        .as_object_mut()
        .ok_or_else(|| Error::InvalidArgument("Claude identity must be a JSON object".into()))?;
    object.insert("oauthAccount".into(), account.clone());
    let mut bytes = serde_json::to_vec_pretty(&destination)?;
    bytes.push(b'\n');
    Ok(Some(bytes))
}

fn reject_same_account(destination: &Option<Value>, source: &Option<Value>) -> Result<()> {
    let Some(destination_uuid) = account_uuid(destination.as_ref()) else {
        return Ok(());
    };
    let Some(source_uuid) = account_uuid(source.as_ref()) else {
        return Ok(());
    };
    if destination_uuid == source_uuid {
        return Err(Error::Conflict(
            "Claude destination and source already identify the same account".into(),
        ));
    }
    Ok(())
}

fn account_uuid(value: Option<&Value>) -> Option<&str> {
    value?
        .get("oauthAccount")?
        .get("accountUuid")?
        .as_str()
        .filter(|value| !value.is_empty())
}

fn email(value: Option<&Value>) -> Option<String> {
    value?
        .get("oauthAccount")?
        .get("emailAddress")?
        .as_str()
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn identity_value(bytes: Option<&[u8]>) -> Option<Value> {
    serde_json::from_slice(bytes?).ok()
}

fn mutation_for(path: PathBuf, bytes: Option<Vec<u8>>) -> Mutation {
    match bytes {
        Some(bytes) => Mutation::write(path, bytes),
        None => Mutation::remove(path),
    }
}

fn reject_same_profile(destination: &Path, source: &Path) -> Result<()> {
    if same_profile(destination, source) {
        return Err(Error::Conflict(format!(
            "destination and source are the same profile: {}",
            absolute_path(destination).display()
        )));
    }
    Ok(())
}

fn json_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64() != Some(0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}
