use std::fs;

use super::fixture::{FailOnceWriter, claude_profile, codex_profile};
use crate::Engine;
use crate::orchestration::auth::{RotationPaths, RotationService};
use crate::usage::UsageCacheStore;

#[test]
fn copy_failure_on_identity_write_restores_the_original_profile() {
    let temp = tempfile::tempdir().unwrap();
    let source = claude_profile(temp.path(), ".claude-1", "source", "source");
    let destination = claude_profile(temp.path(), ".claude-2", "destination", "destination");
    let original_credential = fs::read(destination.join(".credentials.json")).unwrap();
    let original_identity = fs::read(destination.join(".claude.json")).unwrap();
    let writer = FailOnceWriter::new(destination.join(".claude.json"));
    let paths = RotationPaths::new(
        temp.path().join("backups"),
        temp.path().join("rotations.jsonl"),
    );
    let service = RotationService::new(writer, paths);
    assert!(
        service
            .copy(Engine::Claude, &destination, &source, 10, None)
            .is_err()
    );
    assert_eq!(
        fs::read(destination.join(".credentials.json")).unwrap(),
        original_credential
    );
    assert_eq!(
        fs::read(destination.join(".claude.json")).unwrap(),
        original_identity
    );
}

#[test]
fn copy_returns_cache_invalidation_effects_without_deleting_cache_files() {
    let temp = tempfile::tempdir().unwrap();
    let source = claude_profile(temp.path(), ".claude-1", "source", "source");
    let destination = claude_profile(temp.path(), ".claude-2", "destination", "destination");
    let cache = temp.path().join("usage");
    fs::create_dir_all(&cache).unwrap();
    let cache_paths = UsageCacheStore::new(&cache).cache_paths(Engine::Claude, &destination);
    for path in &cache_paths {
        fs::write(path, b"cache").unwrap();
    }
    let service = RotationService::filesystem(
        RotationPaths::new(
            temp.path().join("backups"),
            temp.path().join("rotations.jsonl"),
        )
        .with_usage_cache_dir(&cache),
    );
    let effects = service
        .copy(
            Engine::Claude,
            &destination,
            &source,
            100,
            Some("test".into()),
        )
        .unwrap();
    assert_eq!(effects.invalidated_cache_paths, cache_paths);
    assert!(
        effects
            .invalidated_cache_paths
            .iter()
            .all(|path| path.exists())
    );
    assert_eq!(effects.backup_paths.len(), 1);
    assert_eq!(
        service.recent_rotations(10).unwrap()[0].destination,
        ".claude-2"
    );
}

#[test]
fn codex_copy_returns_hashed_and_legacy_cache_invalidation_paths() {
    let temp = tempfile::tempdir().unwrap();
    let source = codex_profile(temp.path(), ".codex-1", "source", 1);
    let destination = codex_profile(temp.path(), ".codex-2", "destination", 2);
    let cache = temp.path().join("usage");
    let expected = UsageCacheStore::new(&cache).cache_paths(Engine::Codex, &destination);
    let service = RotationService::filesystem(
        RotationPaths::new(
            temp.path().join("backups"),
            temp.path().join("rotations.jsonl"),
        )
        .with_usage_cache_dir(&cache),
    );

    let effects = service
        .copy(
            Engine::Codex,
            &destination,
            &source,
            100,
            Some("test".into()),
        )
        .unwrap();

    assert_eq!(effects.invalidated_cache_paths, expected);
}

#[test]
fn isolated_providers_are_rejected_without_touching_profiles() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&destination).unwrap();
    let service = RotationService::filesystem(RotationPaths::new(
        temp.path().join("backups"),
        temp.path().join("rotations.jsonl"),
    ));
    assert!(
        service
            .copy(Engine::Opencode, &destination, &source, 1, None)
            .is_err()
    );
    assert!(!temp.path().join("backups").exists());
}
