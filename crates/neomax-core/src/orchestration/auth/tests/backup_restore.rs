use serde_json::Value;
use std::fs;

use super::fixture::claude_profile;
use crate::orchestration::auth::limits::MAX_BACKUP_BYTES;
use crate::orchestration::auth::{
    types::FileSnapshot, BackupStore, RotationPaths, RotationService,
};
use crate::Engine;
use crate::Error;

#[test]
fn backup_round_trip_preserves_all_bytes_and_extra_fields() {
    let temp = tempfile::tempdir().unwrap();
    let store = BackupStore::new(temp.path().join("backups"));
    let snapshot = FileSnapshot {
        credential: Some(br#"{"claudeAiOauth":{"accessToken":"fixture"},"extra":true}"#.to_vec()),
        identity: Some(br#"{"oauthAccount":{"accountUuid":"U"},"settings":{"x":1}}"#.to_vec()),
        auth: None,
    };
    let profile = temp.path().join(".claude-1");
    let path = store.save(Engine::Claude, &profile, &snapshot, 10).unwrap();
    let document = store.load(&path).unwrap();
    assert_eq!(document.snapshot().unwrap(), snapshot);
    assert_eq!(document.purpose, "rotation");
    assert!(store.latest(Engine::Claude, &profile).unwrap().is_some());
}

#[test]
fn rotation_backup_emits_legacy_python_fields_and_filename() {
    let temp = tempfile::tempdir().unwrap();
    let store = BackupStore::new(temp.path().join("backups"));
    let profile = temp.path().join(".claude-legacy");
    let snapshot = FileSnapshot {
        credential: Some(br#"{"claudeAiOauth":{"accessToken":"fixture"}}"#.to_vec()),
        identity: Some(br#"{"oauthAccount":{"accountUuid":"fixture-account"}}"#.to_vec()),
        auth: None,
    };

    let path = store.save(Engine::Claude, &profile, &snapshot, 42).unwrap();
    assert!(path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .ends_with("-.claude-legacy.json"));
    let value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(value["engine"], "claude");
    assert_eq!(value["ts"], 42);
    assert_eq!(
        value["blob"],
        String::from_utf8(snapshot.credential.unwrap()).unwrap()
    );
    assert_eq!(value["oauth_account"]["accountUuid"], "fixture-account");
}

#[test]
fn restore_safety_backup_does_not_enter_legacy_restore_pool() {
    let temp = tempfile::tempdir().unwrap();
    let store = BackupStore::new(temp.path().join("backups"));
    let profile = temp.path().join(".claude-safety");
    let snapshot = FileSnapshot {
        credential: Some(br#"{"credential":"fixture"}"#.to_vec()),
        identity: None,
        auth: None,
    };

    let path = store
        .save_safety(Engine::Claude, &profile, &snapshot, 43)
        .unwrap();
    let value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert!(value.get("blob").is_none());
    assert!(value.get("ts").is_none());
    assert!(store.latest(Engine::Claude, &profile).unwrap().is_none());
}

#[test]
fn legacy_python_claude_backup_loads_and_binds_by_profile_name() {
    let temp = tempfile::tempdir().unwrap();
    let store = BackupStore::new(temp.path().join("backups"));
    let path = temp.path().join("backups").join("100-.claude-legacy.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        br#"{"engine":"claude","blob":"{\"claudeAiOauth\":{\"accessToken\":\"fixture\"}}","oauth_account":{"accountUuid":"fixture-account"},"ts":100}"#,
    )
    .unwrap();

    let profile = temp.path().join(".claude-legacy");
    let document = store.load_for_profile(&path, &profile).unwrap();
    assert!(document.is_legacy());
    assert_eq!(document.profile, profile);
    assert!(store
        .load_for_profile(&path, &temp.path().join(".claude-other"))
        .is_err());
    let snapshot = document.snapshot().unwrap();
    assert_eq!(
        snapshot.credential,
        Some(br#"{"claudeAiOauth":{"accessToken":"fixture"}}"#.to_vec())
    );
    assert!(String::from_utf8(snapshot.identity.unwrap())
        .unwrap()
        .contains("fixture-account"));
    assert_eq!(
        store
            .latest(Engine::Claude, &profile)
            .unwrap()
            .unwrap()
            .1
            .timestamp,
        100
    );
}

#[test]
fn legacy_python_codex_backup_loads_without_oauth_data() {
    let temp = tempfile::tempdir().unwrap();
    let store = BackupStore::new(temp.path().join("backups"));
    let path = temp.path().join("backups").join("101-.codex-2.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        br#"{"engine":"codex","blob":"{\"tokens\":{\"access_token\":\"fixture\"}}","ts":101}"#,
    )
    .unwrap();

    let profile = temp.path().join(".codex-2");
    let document = store.load_for_profile(&path, &profile).unwrap();
    assert!(document.is_legacy());
    let snapshot = document.snapshot().unwrap();
    assert_eq!(
        snapshot.auth,
        Some(br#"{"tokens":{"access_token":"fixture"}}"#.to_vec())
    );
    assert!(snapshot.credential.is_none());
    assert!(snapshot.identity.is_none());
}

#[test]
fn legacy_backup_without_blob_is_rejected_when_restoring_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let store = BackupStore::new(temp.path().join("backups"));
    let path = temp.path().join("backups").join("102-.claude-empty.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        br#"{"engine":"claude","blob":"","oauth_account":{},"ts":102}"#,
    )
    .unwrap();

    let document = store.load(&path).unwrap();
    assert!(document.snapshot().is_err());
}

#[test]
fn explicit_legacy_restore_is_bound_to_the_requested_profile() {
    let temp = tempfile::tempdir().unwrap();
    let destination = claude_profile(
        temp.path(),
        ".claude-destination",
        "current-credential",
        "current-account",
    );
    let other = claude_profile(
        temp.path(),
        ".claude-other",
        "other-credential",
        "other-account",
    );
    let backup_dir = temp.path().join("backups");
    let backup = backup_dir.join("100-.claude-destination.json");
    fs::create_dir_all(&backup_dir).unwrap();
    fs::write(
        &backup,
        br#"{"engine":"claude","blob":"{\"claudeAiOauth\":{\"accessToken\":\"legacy-credential\"}}","oauth_account":{"accountUuid":"legacy-account"},"ts":100}"#,
    )
    .unwrap();
    let service = RotationService::filesystem(RotationPaths::new(
        backup_dir,
        temp.path().join("rotations.jsonl"),
    ));

    service
        .restore(Engine::Claude, &destination, Some(&backup), 200, None)
        .unwrap();
    assert!(fs::read_to_string(destination.join(".credentials.json"))
        .unwrap()
        .contains("legacy-credential"));
    assert!(fs::read_to_string(destination.join(".claude.json"))
        .unwrap()
        .contains("legacy-account"));

    let other_before = fs::read(other.join(".credentials.json")).unwrap();
    assert!(service
        .restore(Engine::Claude, &other, Some(&backup), 300, None)
        .is_err());
    assert_eq!(
        fs::read(other.join(".credentials.json")).unwrap(),
        other_before
    );
}

#[cfg(unix)]
#[test]
fn explicit_legacy_restore_rejects_a_backup_symlink() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let destination = claude_profile(
        temp.path(),
        ".claude-destination",
        "current-credential",
        "current-account",
    );
    let backup_dir = temp.path().join("backups");
    let real_backup = backup_dir.join("100-.claude-destination.json");
    let linked_backup = backup_dir.join("101-.claude-destination.json");
    fs::create_dir_all(&backup_dir).unwrap();
    fs::write(
        &real_backup,
        br#"{"engine":"claude","blob":"{\"claudeAiOauth\":{\"accessToken\":\"legacy-credential\"}}","oauth_account":{"accountUuid":"legacy-account"},"ts":100}"#,
    )
    .unwrap();
    symlink(&real_backup, &linked_backup).unwrap();
    let service = RotationService::filesystem(RotationPaths::new(
        backup_dir,
        temp.path().join("rotations.jsonl"),
    ));

    assert!(service
        .restore(
            Engine::Claude,
            &destination,
            Some(&linked_backup),
            200,
            None,
        )
        .is_err());
    assert!(fs::read_to_string(destination.join(".credentials.json"))
        .unwrap()
        .contains("current-credential"));
}

#[test]
fn corrupt_backup_is_rejected_without_a_default_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let store = BackupStore::new(temp.path().join("backups"));
    let path = temp.path().join("backups").join("corrupt.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"not-json").unwrap();

    assert!(matches!(store.load(&path), Err(Error::InvalidState { .. })));
}

#[test]
fn oversized_backup_is_rejected_before_json_parsing() {
    let temp = tempfile::tempdir().unwrap();
    let store = BackupStore::new(temp.path().join("backups"));
    let path = temp.path().join("oversized.json");
    fs::write(&path, vec![b'x'; MAX_BACKUP_BYTES + 1]).unwrap();

    assert!(store.load(&path).is_err());
}

#[test]
fn restore_uses_rotation_backup_not_restore_safety_backup() {
    let temp = tempfile::tempdir().unwrap();
    let source = claude_profile(temp.path(), ".claude-1", "source", "source");
    let destination = claude_profile(temp.path(), ".claude-2", "destination", "destination");
    let service = RotationService::filesystem(RotationPaths::new(
        temp.path().join("backups"),
        temp.path().join("rotations.jsonl"),
    ));
    service
        .copy(Engine::Claude, &destination, &source, 100, None)
        .unwrap();
    service
        .restore(
            Engine::Claude,
            &destination,
            None,
            200,
            Some("restore".into()),
        )
        .unwrap();
    assert!(fs::read_to_string(destination.join(".credentials.json"))
        .unwrap()
        .contains("destination"));
    service
        .restore(Engine::Claude, &destination, None, 300, None)
        .unwrap();
    assert!(fs::read_to_string(destination.join(".credentials.json"))
        .unwrap()
        .contains("destination"));
}
