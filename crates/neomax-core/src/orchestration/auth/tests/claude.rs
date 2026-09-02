use std::path::Path;

use serde_json::Value;

use super::fixture::claude_profile;
use crate::orchestration::auth::{CredentialWriter, FsCredentialWriter, claude};
use crate::{Error, Result};

#[test]
fn copy_preserves_destination_settings_and_source_credential_fields() {
    let temp = tempfile::tempdir().unwrap();
    let source = claude_profile(temp.path(), ".claude-1", "source", "source-uuid");
    let destination = claude_profile(temp.path(), ".claude-2", "destination", "destination-uuid");
    let plan = claude::prepare_copy(&FsCredentialWriter, &destination, &source).unwrap();
    assert_eq!(
        plan.from_email.as_deref(),
        Some("destination-uuid@example.test")
    );
    assert_eq!(plan.to_email.as_deref(), Some("source-uuid@example.test"));
    assert_eq!(plan.mutations.len(), 2);
    let identity = plan.mutations[1].bytes.as_ref().unwrap();
    let value: Value = serde_json::from_slice(identity).unwrap();
    assert_eq!(value["settings"]["keep"], true);
    assert_eq!(value["oauthAccount"]["accountUuid"], "source-uuid");
    assert!(
        String::from_utf8(plan.mutations[0].bytes.clone().unwrap())
            .unwrap()
            .contains("preserve")
    );
}

#[test]
fn same_account_uuid_is_rejected_before_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let source = claude_profile(temp.path(), ".claude-1", "source", "same");
    let destination = claude_profile(temp.path(), ".claude-2", "destination", "same");
    assert!(matches!(
        claude::prepare_copy(&FsCredentialWriter, &destination, &source),
        Err(Error::Conflict(_))
    ));
}

#[test]
fn default_profile_uses_global_claude_identity_file() {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join(".claude");
    assert_eq!(
        claude::identity_path_for_home(&profile, root.path()),
        root.path().join(".claude.json")
    );
    assert_ne!(
        claude::identity_path_for_home(&profile, root.path()),
        profile.join(".claude.json")
    );
    assert_eq!(
        claude::identity_path_for_home(&profile, Path::new("/another-home")),
        profile.join(".claude.json")
    );
}

#[test]
fn swap_exchanges_credentials_and_identities_without_duplicate_uuid() {
    let temp = tempfile::tempdir().unwrap();
    let first = claude_profile(temp.path(), ".claude-1", "first", "first");
    let second = claude_profile(temp.path(), ".claude-2", "second", "second");
    let plan = claude::prepare_swap(&FsCredentialWriter, &first, &second).unwrap();
    assert_eq!(plan.mutations.len(), 4);
    let first_identity: Value =
        serde_json::from_slice(plan.mutations[2].bytes.as_ref().unwrap()).unwrap();
    let second_identity: Value =
        serde_json::from_slice(plan.mutations[3].bytes.as_ref().unwrap()).unwrap();
    assert_eq!(first_identity["oauthAccount"]["accountUuid"], "second");
    assert_eq!(second_identity["oauthAccount"]["accountUuid"], "first");
}

struct ReadOnlyWriter;

impl CredentialWriter for ReadOnlyWriter {
    fn read_optional(&self, path: &std::path::Path) -> Result<Option<Vec<u8>>> {
        Ok(std::fs::read(path).ok())
    }

    fn write_atomic(&self, _path: &std::path::Path, _bytes: &[u8]) -> Result<()> {
        Err(Error::Message("write not expected".into()))
    }

    fn remove(&self, _path: &std::path::Path) -> Result<()> {
        Err(Error::Message("remove not expected".into()))
    }
}

#[test]
fn malformed_identity_fails_closed_before_credential_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let source = claude_profile(temp.path(), ".claude-1", "source", "source");
    let destination = claude_profile(temp.path(), ".claude-2", "destination", "destination");
    std::fs::write(claude::identity_path(&destination), b"not-json").unwrap();
    assert!(matches!(
        claude::prepare_copy(&ReadOnlyWriter, &destination, &source),
        Err(Error::InvalidState { .. })
    ));
}
