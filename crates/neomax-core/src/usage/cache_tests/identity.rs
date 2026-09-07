use std::path::PathBuf;

use super::super::{MAX_CACHE_BYTES, UsageCacheStore, legacy_path, write_json_atomic};
use super::fixtures::cache;
use crate::Engine;

#[test]
fn read_only_legacy_cache_inspection_does_not_create_or_rewrite_files() {
    let temp = tempfile::tempdir().unwrap();
    let store = UsageCacheStore::new(temp.path().join("usage"));
    let profile = temp.path().join("profile");
    let legacy = legacy_path(&store.directory, Engine::Codex, &profile);
    write_json_atomic(&legacy, &cache(27.0)).unwrap();
    let original = std::fs::read(&legacy).unwrap();
    let loaded = store.load_read_only(Engine::Codex, &profile).unwrap();
    assert_eq!(loaded.five_hour.used_percent, Some(27.0));
    assert_eq!(std::fs::read(&legacy).unwrap(), original);
    assert!(!store.path(Engine::Codex, &profile).exists());
}

#[test]
fn oversized_cache_is_ignored_before_deserialization() {
    let temp = tempfile::tempdir().unwrap();
    let store = UsageCacheStore::new(temp.path());
    let profile = PathBuf::from("/profiles/.claude");
    let path = store.path(Engine::Claude, &profile);
    std::fs::File::create(path)
        .unwrap()
        .set_len(MAX_CACHE_BYTES as u64 + 1)
        .unwrap();
    assert!(store.load(Engine::Claude, &profile).is_none());
}

#[test]
fn profiles_with_the_same_basename_have_distinct_private_cache_identity() {
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("first").join(".claude");
    let second = temp.path().join("second").join(".claude");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    let store = UsageCacheStore::new(temp.path().join("usage"));

    let first_path = store.path(Engine::Claude, &first);
    let second_path = store.path(Engine::Claude, &second);
    assert_ne!(first_path, second_path);
    assert!(
        !first_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(".claude")
    );
    assert!(
        !second_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(".claude")
    );

    store.save(Engine::Claude, &first, &cache(12.0)).unwrap();
    store.save(Engine::Claude, &second, &cache(87.0)).unwrap();
    assert_eq!(
        store
            .load(Engine::Claude, &first)
            .unwrap()
            .five_hour
            .used_percent,
        Some(12.0)
    );
    assert_eq!(
        store
            .load(Engine::Claude, &second)
            .unwrap()
            .five_hour
            .used_percent,
        Some(87.0)
    );

    let legacy = legacy_path(store.directory.as_path(), Engine::Claude, &second);
    let legacy_value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(legacy).unwrap()).unwrap();
    assert_eq!(
        legacy_value["five_hour"]["used_percent"],
        serde_json::json!(87.0)
    );
    assert!(legacy_value["neomax_profile_identity"].is_string());

    let alias = first.join("..").join(".claude");
    assert_eq!(
        store.path(Engine::Claude, &first),
        store.path(Engine::Claude, &alias)
    );
}

#[test]
fn legacy_identity_tag_never_crosses_hashed_profile_boundaries() {
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("first").join(".claude");
    let second = temp.path().join("second").join(".claude");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    let store = UsageCacheStore::new(temp.path().join("usage"));

    store.save(Engine::Claude, &first, &cache(12.0)).unwrap();
    store.save(Engine::Claude, &second, &cache(87.0)).unwrap();

    assert_eq!(
        store
            .load(Engine::Claude, &first)
            .unwrap()
            .five_hour
            .used_percent,
        Some(12.0)
    );
    assert_eq!(
        store
            .load(Engine::Claude, &second)
            .unwrap()
            .five_hour
            .used_percent,
        Some(87.0)
    );
}

#[test]
fn newer_legacy_cache_is_promoted_back_to_the_native_identity_file() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profile").join(".claude");
    std::fs::create_dir_all(&profile).unwrap();
    let store = UsageCacheStore::new(temp.path().join("usage"));
    store.save(Engine::Claude, &profile, &cache(12.0)).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(20));
    let legacy = legacy_path(store.directory.as_path(), Engine::Claude, &profile);
    write_json_atomic(&legacy, &cache(87.0)).unwrap();

    assert_eq!(
        store
            .load(Engine::Claude, &profile)
            .unwrap()
            .five_hour
            .used_percent,
        Some(87.0)
    );
    let native: serde_json::Value =
        serde_json::from_slice(&std::fs::read(store.path(Engine::Claude, &profile)).unwrap())
            .unwrap();
    assert_eq!(native["five_hour"]["used_percent"], serde_json::json!(87.0));
    assert!(native["neomax_profile_identity"].is_string());
}

#[test]
fn legacy_basename_cache_is_read_and_migrated_to_private_identity() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("legacy").join(".claude");
    std::fs::create_dir_all(&profile).unwrap();
    let store = UsageCacheStore::new(temp.path().join("usage"));
    let legacy = legacy_path(store.directory.as_path(), Engine::Claude, &profile);
    let expected = cache(64.0);
    write_json_atomic(&legacy, &expected).unwrap();

    let migrated_path = store.path(Engine::Claude, &profile);
    assert_ne!(legacy, migrated_path);
    assert!(!migrated_path.exists());
    assert_eq!(store.load(Engine::Claude, &profile), Some(expected.clone()));
    assert!(migrated_path.is_file());
    let migrated_legacy: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&legacy).unwrap()).unwrap();
    assert!(migrated_legacy["neomax_profile_identity"].is_string());
    assert_eq!(store.load(Engine::Claude, &profile), Some(expected));

    let other_profile = temp.path().join("other").join(".claude");
    std::fs::create_dir_all(&other_profile).unwrap();
    assert!(store.load(Engine::Claude, &other_profile).is_none());
}
